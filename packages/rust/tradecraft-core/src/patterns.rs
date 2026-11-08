//! Folder patterns and exclusions: `build.patterns` and `build.exclude` in the
//! root file. Experimental in 1.0.
//!
//! A glob in `build.patterns` names the things of one kind, the way
//! `plugins/*` and `skills/*` name them by convention: `plugins: packs/*`
//! makes each folder under `packs/` a plugin, and `skills: notes/*` makes each
//! note or folder under `notes/` a skill. `assets` is the exception: its globs
//! name a plugin's assets folder, as `plugins/*/assets` does. The convention
//! keeps working beside them.
//!
//! [`Patterns::canonical`] rewrites a vault-relative path to the conventional
//! path it plays the part of, so one reading of the tree
//! ([`crate::discover`]) serves both.
//!
//! A folder that never ships (`docs/`, `tests/`, `fixtures/` and the like)
//! stays out whatever a pattern says. A plugin or skill with such a name is
//! a component, and ships.
//!
//! The root file is read here rather than passed in, the way the vault's own
//! excluded-files list is, so every entry point answers alike. A host that
//! names another file does so through [`crate::source_view::SourceView`].
//! Globs are relative to the root file's folder.
//!
//! In a workspace, `build.exclude` works like nested `.gitignore` files: each
//! glob reads from the folder of the file it is written in, and a member's
//! exclusions add to the workspace's. The workspace's reach every member;
//! a member's reach only its own folder. `build.patterns` instead inherits
//! key by key, and its globs read from each member's folder.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::compile::EXCLUDED_BUNDLE_DIRECTORIES;
use crate::discover::DiscoveryWarning;
use crate::error::CoreError;
use crate::frontmatter::EntityKind;
use crate::paths::{RootRead, WORKSPACE_DIR, read_root, read_workspace_above};
use crate::settings::{FolderPatterns, ROOT_FILE};
use crate::vfs::Vfs;

/// Why `glob` cannot be read, if it cannot. A glob is `/`-separated and
/// relative to the root file, with `*`, `?` and a whole-segment `**`.
pub(crate) fn glob_problem(glob: &str) -> Option<&'static str> {
    if glob.trim().is_empty() {
        return Some("a glob cannot be empty");
    }
    if glob.starts_with('/') || glob.contains('\\') || glob.contains(':') {
        return Some("a glob is a relative path with `/` between folders");
    }
    if glob.contains(['[', ']', '{', '}']) {
        return Some("only `*`, `?` and `**` are glob syntax here");
    }
    for segment in glob.trim_end_matches('/').split('/') {
        match segment {
            "" | "." | ".." => return Some("a glob cannot hold an empty, `.` or `..` segment"),
            "**" => {}
            other if other.contains("**") => return Some("`**` must be a whole path segment"),
            _ => {}
        }
    }
    None
}

/// Folders that never ship and are also legal component names.
const NAMEABLE_NEVER_SHIP: &[&str] = &["docs", "tests", "fixtures"];

/// The index of each folder in `folders`, a conventional path, that never
/// ships.
///
/// Position decides, not the name alone: the segment right after a kind folder
/// names a component, so `plugins/docs` and `skills/tests` ship. A kind folder
/// counts only where one can sit, so not inside a skill, command, agent or hook.
fn never_ship_folders(folders: &[&str]) -> Vec<usize> {
    let mut found = Vec::new();
    let mut holds_kinds = true;
    let mut at = 0;
    while let Some(segment) = folders.get(at) {
        match crate::discover::kind_dir(segment).filter(|_| holds_kinds) {
            // A plugin's `assets/` ships as written, whatever its folders say.
            None if holds_kinds && *segment == "assets" => break,
            Some(kind) => {
                holds_kinds = matches!(kind, EntityKind::Marketplace | EntityKind::Plugin);
                at += 1;
                if folders
                    .get(at)
                    .is_some_and(|name| !NAMEABLE_NEVER_SHIP.contains(name))
                    && EXCLUDED_BUNDLE_DIRECTORIES.contains(&folders[at])
                {
                    found.push(at);
                }
            }
            None if EXCLUDED_BUNDLE_DIRECTORIES.contains(segment) => found.push(at),
            None => {}
        }
        at += 1;
    }
    found
}

/// The folders holding `rel`.
fn folders_of(rel: &str) -> Vec<&str> {
    let mut folders: Vec<&str> = rel.split('/').collect();
    folders.pop();
    folders
}

/// Whether `glob` spells out a folder that never ships. The last segment of a
/// glob that `names_components` is a component's name, as in `packs/docs`.
pub(crate) fn names_a_never_ship_folder(glob: &str, names_components: bool) -> bool {
    let segments: Vec<&str> = glob.trim_end_matches('/').split('/').collect();
    let folders = match segments.split_last() {
        Some((_, above)) if names_components && !above.is_empty() => above,
        _ => &segments,
    };
    !never_ship_folders(folders).is_empty()
}

/// `*` and `?` within one path segment; `**` as a whole segment matches any
/// number of segments, and at least one at the end of a glob, so `notes/**`
/// names what `notes/` holds and not the folder.
pub(crate) fn glob_match(glob: &str, path: &str) -> bool {
    let glob: Vec<&str> = glob.trim_end_matches('/').split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    segments_match(&glob, &path)
}

fn segments_match(glob: &[&str], path: &[&str]) -> bool {
    match glob.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (usize::from(rest.is_empty())..=path.len())
            .any(|skip| segments_match(rest, &path[skip..])),
        Some((first, rest)) => path.split_first().is_some_and(|(segment, tail)| {
            segment_match(first.as_bytes(), segment.as_bytes()) && segments_match(rest, tail)
        }),
    }
}

fn segment_match(glob: &[u8], text: &[u8]) -> bool {
    match glob.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|skip| segment_match(rest, &text[skip..])),
        Some((b'?', rest)) => !text.is_empty() && segment_match(rest, &text[1..]),
        Some((byte, rest)) => text.first() == Some(byte) && segment_match(rest, &text[1..]),
    }
}

/// A vault's patterns and the vault root they were read from.
pub(crate) type Placed<'a> = (Patterns, &'a Path);

/// The root file's folder patterns and exclusions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Patterns {
    folders: FolderPatterns,
    /// This root file's own `build.exclude`, read from the vault root.
    exclude: Vec<String>,
    /// The workspace file's `build.exclude`, read from the workspace folder,
    /// and the vault root from that folder, `marketplaces/<m>`. `None` when
    /// no workspace holds this marketplace or it excludes nothing.
    workspace_exclude: Option<(String, Vec<String>)>,
    /// The folder-pattern globs a workspace file gave this marketplace.
    inherited: BTreeSet<String>,
}

impl Patterns {
    /// The patterns in the root file at `vault_root`, over what a workspace
    /// file above it sets. Each key of `build.patterns` inherits key by key,
    /// and its globs read from the marketplace's folder wherever they were
    /// written. `build.exclude` adds the workspace's globs to this file's, and
    /// each reads from the folder of the file it is written in.
    ///
    /// # Errors
    ///
    /// `refused: malformed-root-file` when either file's frontmatter is not a
    /// YAML mapping, or its `workspace`, `build`, `build.exclude` or
    /// `build.patterns` has the wrong shape: an exclusion that cannot be read
    /// must not fail open. A bad value in any other key is not checked here.
    pub fn load(vfs: &dyn Vfs, vault_root: &Path) -> Result<Self, CoreError> {
        let mut root =
            read_root(vfs, &vault_root.join(ROOT_FILE), RootRead::Layout)?.unwrap_or_default();
        let written: BTreeSet<String> = root.build.patterns.globs().cloned().collect();
        let mut workspace_exclude = None;
        if !root.workspace
            && let Some(shared) = read_workspace_above(vfs, vault_root, RootRead::Layout)?
        {
            root.inherit(&shared);
            if !shared.build.exclude.is_empty() {
                let name = vault_root.file_name().unwrap_or_default().to_string_lossy();
                workspace_exclude = Some((format!("{WORKSPACE_DIR}/{name}"), shared.build.exclude));
            }
        }
        let inherited = root
            .build
            .patterns
            .globs()
            .filter(|glob| !written.contains(*glob))
            .cloned()
            .collect();
        Ok(Self {
            folders: root.build.patterns,
            exclude: root.build.exclude,
            workspace_exclude,
            inherited,
        })
    }

    /// The patterns of the vault at `vault_root`, paired with it, or `None`
    /// with no vault root: what a one-note verb places its note with.
    ///
    /// # Errors
    ///
    /// As [`Self::load`].
    pub(crate) fn load_at<'r>(
        vfs: &dyn Vfs,
        vault_root: Option<&'r Path>,
    ) -> Result<Option<Placed<'r>>, CoreError> {
        vault_root
            .map(|root| Self::load(vfs, root).map(|patterns| (patterns, root)))
            .transpose()
    }

    /// Every glob this file checks, whichever key holds it: the folder
    /// patterns and this file's own exclusions. A workspace's exclusions are
    /// checked once, at the workspace ([`workspace_exclude_warnings`]).
    fn globs(&self) -> impl Iterator<Item = &str> {
        self.folders
            .globs()
            .chain(&self.exclude)
            .map(String::as_str)
    }

    /// The files of the other marketplaces beside the one at `vault_root`, for
    /// a glob a workspace shares among them. Empty when this one inherits none.
    pub(crate) fn sibling_files(&self, vfs: &dyn Vfs, vault_root: &Path) -> Vec<String> {
        let Some(folder) = vault_root.parent().filter(|_| !self.inherited.is_empty()) else {
            return Vec::new();
        };
        vfs.read_dir(folder)
            .unwrap_or_default()
            .iter()
            .map(|entry| folder.join(&entry.name))
            .filter(|path| path != vault_root && vfs.exists(&path.join(ROOT_FILE)))
            .flat_map(|path| crate::walk::glob_files(vfs, &path))
            .collect()
    }

    /// Each folder-pattern key with its globs, in the order a path is claimed.
    fn kinds(&self) -> [(&'static str, &[String]); 6] {
        self.folders.kinds()
    }

    /// The folder each pattern reads from: its glob up to the first wildcard.
    pub(crate) fn source_folders(&self) -> BTreeSet<String> {
        self.kinds()
            .iter()
            .flat_map(|(_, globs)| globs.iter())
            .map(|glob| {
                glob.split('/')
                    .take_while(|segment| !segment.contains(['*', '?']))
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .filter(|folder| !folder.is_empty())
            .collect()
    }

    /// [`Self::warnings_with`] over lists already in hand, for the tests.
    #[cfg(test)]
    pub(crate) fn warnings(
        &self,
        files: &[String],
        output: &[String],
        siblings: &[String],
    ) -> Vec<DiscoveryWarning> {
        self.warnings_with(|| files.to_vec(), output, || siblings.to_vec())
    }

    /// What is worth saying about these patterns over the vault's `files`,
    /// vault-relative: a glob that matches nothing, a path two kinds claim,
    /// and one plugin read from two folders. `output` is what a build owns in
    /// the vault: not source, but a fair thing for `build.exclude` to name.
    /// `siblings` are the other marketplaces' files: a `build.patterns` glob a
    /// workspace shares need only match in one of them.
    ///
    /// Both lists are read only when there is something to check. With no
    /// glob there is nothing to say, so neither is read; `siblings` is read
    /// only when a shared glob matches nothing here, and then once.
    pub(crate) fn warnings_with(
        &self,
        files: impl Fn() -> Vec<String>,
        output: &[String],
        siblings: impl Fn() -> Vec<String>,
    ) -> Vec<DiscoveryWarning> {
        if self.globs().next().is_none() {
            return Vec::new();
        }
        let files = files();
        let files = files.as_slice();
        let mut sibling_files: Option<Vec<String>> = None;
        let mut warnings = Vec::new();
        let keyed = self
            .kinds()
            .into_iter()
            .flat_map(|(key, globs)| {
                globs
                    .iter()
                    .map(move |glob| (format!("build.patterns.{key}"), glob))
            })
            .chain(self.exclude.iter().map(|g| ("build.exclude".to_owned(), g)));
        for (key, glob) in keyed {
            let excludes = key == "build.exclude";
            if (excludes && matches_some(output, glob)) || matches_some(files, glob) {
                continue;
            }
            let shared = !excludes
                && self.inherited.contains(glob)
                && matches_some(sibling_files.get_or_insert_with(&siblings), glob);
            if !shared {
                warnings.push(unmatched(&key, glob, ""));
            }
        }

        // By the first kind's claim on each path, once per path.
        let mut claimed: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        let mut folders: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let ships = |file: &&String| !self.never_ships(file) && !self.excluded(file);
        for file in files.iter().filter(ships) {
            // The leaf kind a shorter path claimed: a deeper claim of another
            // kind loses to it, with no two claims on one path to show it.
            let mut settled: Option<(&str, String)> = None;
            for prefix in prefixes(file) {
                let claims: Vec<(&str, String)> = self
                    .kinds()
                    .iter()
                    .flat_map(|(key, globs)| {
                        globs
                            .iter()
                            .filter(|glob| glob_match(glob, prefix))
                            .take(1)
                            .map(move |glob| (*key, format!("`{key}: {glob}`")))
                    })
                    .collect();
                let leaf = claims.iter().find(|(key, _)| *key != "plugins");
                if let (Some((first, said)), Some((key, deeper))) = (&settled, leaf)
                    && first != key
                {
                    claimed
                        .entry(prefix)
                        .or_insert_with(|| vec![said.clone(), deeper.clone()]);
                } else if claims.len() > 1 {
                    let said = claims.iter().map(|(_, said)| said.clone()).collect();
                    claimed.entry(prefix).or_insert(said);
                }
                if settled.is_none() {
                    settled = leaf.cloned();
                }
            }
            let canonical = self.canonical(file);
            let parts: Vec<&str> = canonical.split('/').collect();
            let at = if parts.first() == Some(&WORKSPACE_DIR) {
                2
            } else {
                0
            };
            if parts.get(at) == Some(&"plugins") && parts.len() > at + 2 {
                let conventional = parts[..at + 2].join("/");
                let folder = self
                    .plugin_folder(file)
                    .map_or_else(|| conventional.clone(), str::to_owned);
                folders.entry(conventional).or_default().insert(folder);
            }
        }
        for (path, claims) in claimed {
            warnings.push(DiscoveryWarning::new(
                path,
                "pattern-conflict",
                format!("{} claim it; the first one wins", claims.join(" and ")),
            ));
        }
        for (conventional, folders) in folders {
            if folders.len() > 1 {
                let name = conventional.rsplit('/').next().unwrap_or(&conventional);
                let named: Vec<String> = folders.into_iter().collect();
                warnings.push(DiscoveryWarning::new(
                    &named[0],
                    "plugin-merged",
                    format!(
                        "plugin `{name}` is read from {}, and they merge into one plugin",
                        named.join(" and ")
                    ),
                ));
            }
        }
        warnings
    }

    /// Whether `rel`, or a folder holding it, matches `build.exclude`: this
    /// file's globs from the vault root, or a workspace's from its folder.
    pub fn excluded(&self, rel: &str) -> bool {
        matches_any(&self.exclude, rel)
            || self
                .workspace_exclude
                .as_ref()
                .is_some_and(|(member, globs)| matches_any(globs, &format!("{member}/{rel}")))
    }

    /// Whether a folder that never ships holds `rel`.
    pub(crate) fn never_ships(&self, rel: &str) -> bool {
        !never_ship_folders(&folders_of(&self.canonical(rel))).is_empty()
    }

    /// Whether `rel` sits in a `docs`, `tests` or `fixtures` folder where a
    /// skill's folder would be, and no skill claimed it.
    pub(crate) fn in_a_named_never_ship_folder(&self, rel: &str) -> bool {
        let canonical = self.canonical(rel);
        folders_of(&canonical).windows(2).any(|pair| {
            NAMEABLE_NEVER_SHIP.contains(&pair[1])
                && crate::discover::kind_dir(pair[0]).is_some_and(|kind| !kind.holds_components())
        })
    }

    /// Whether a link from the file `from` to the file `to` goes into a folder
    /// that never ships. Inside such a folder, its own files link freely.
    pub(crate) fn links_into_never_ship(&self, from: &str, to: &str) -> bool {
        let (from, to) = (self.canonical(from), self.canonical(to));
        let (above, folders) = (folders_of(&from), folders_of(&to));
        let shared = above
            .iter()
            .zip(&folders)
            .take_while(|(a, b)| a == b)
            .count();
        never_ship_folders(&folders)
            .into_iter()
            .any(|at| at >= shared)
    }

    /// The conventional path `rel` plays the part of.
    pub fn canonical(&self, rel: &str) -> String {
        if self.folders.is_empty() {
            return rel.to_owned();
        }
        let leaves = [
            (&self.folders.skills, "skills"),
            (&self.folders.agents, "agents"),
            (&self.folders.commands, "commands"),
            (&self.folders.hooks, "hooks"),
        ];
        // The path so far, and the folders passed since the last match, which
        // a match replaces and the end of the path keeps.
        let mut out: Vec<&str> = Vec::new();
        let mut pending: Vec<&str> = Vec::new();
        let mut settled = false;
        for (depth, (prefix, segment)) in prefixes(rel).zip(rel.split('/')).enumerate() {
            let matches = |globs: &[String]| globs.iter().any(|g| glob_match(g, prefix));
            // A pattern match below the root names a component, as the segment
            // after a kind folder does. Any other folder that never ships ends
            // the mapping, so nothing under it reads as what a pattern says.
            let names_a_component = depth > 0
                && (matches(&self.folders.plugins)
                    || leaves.iter().any(|(globs, _)| matches(globs)));
            let never_ships = prefix != rel && !names_a_component && {
                let path: Vec<&str> = out
                    .iter()
                    .chain(&pending)
                    .chain([&segment])
                    .copied()
                    .collect();
                never_ship_folders(&path).contains(&(path.len() - 1))
            };
            if settled || never_ships {
                pending.push(segment);
                settled = true;
            } else if matches(&self.folders.assets) {
                keep_conventional(&mut out, &mut pending, &[WORKSPACE_DIR, "plugins"]);
                out.push(crate::model::load::ASSETS_DIR);
                settled = true;
            } else if let Some((_, kind)) = leaves.iter().find(|(globs, _)| matches(globs)) {
                keep_conventional(&mut out, &mut pending, &[WORKSPACE_DIR, "plugins"]);
                out.extend([*kind, segment]);
                settled = true;
            } else if matches(&self.folders.plugins) {
                keep_conventional(&mut out, &mut pending, &[WORKSPACE_DIR]);
                out.extend(["plugins", segment]);
            } else {
                pending.push(segment);
            }
        }
        out.extend(pending);
        out.join("/")
    }

    /// The folder of the plugin holding `rel`, when a pattern makes one.
    pub fn plugin_folder<'r>(&self, rel: &'r str) -> Option<&'r str> {
        prefixes(rel).find(|prefix| self.folders.plugins.iter().any(|g| glob_match(g, prefix)))
    }

    /// The plugin assets folder under `folder` a pattern names, by trying each
    /// entry of the folder.
    pub fn assets_folder(&self, vfs: &dyn Vfs, vault_root: &Path, folder: &str) -> Option<String> {
        if self.folders.assets.is_empty() {
            return None;
        }
        let mut entries = vfs.read_dir(&vault_root.join(folder)).ok()?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
            .into_iter()
            .filter(|entry| {
                entry.is_dir && !EXCLUDED_BUNDLE_DIRECTORIES.contains(&entry.name.as_str())
            })
            .map(|entry| format!("{folder}/{}", entry.name))
            .find(|rel| self.folders.assets.iter().any(|g| glob_match(g, rel)))
    }
}

/// Drops the folders a match replaces, keeping each conventional `<kind>/<name>`
/// of `kinds` at their head: a container still owns what it holds.
fn keep_conventional<'r>(out: &mut Vec<&'r str>, pending: &mut Vec<&'r str>, kinds: &[&str]) {
    if out.is_empty() {
        let mut kept = 0;
        for kind in kinds {
            if pending.len() >= kept + 2 && pending[kept] == *kind {
                kept += 2;
            }
        }
        out.extend(&pending[..kept]);
    }
    pending.clear();
}

/// Whether `rel`, or a folder holding it, matches one of `globs`.
fn matches_any(globs: &[String], rel: &str) -> bool {
    !globs.is_empty() && prefixes(rel).any(|prefix| globs.iter().any(|g| glob_match(g, prefix)))
}

/// Whether `glob` matches one of `files`, or a folder holding one.
fn matches_some(files: &[String], glob: &str) -> bool {
    files
        .iter()
        .any(|file| prefixes(file).any(|prefix| glob_match(glob, prefix)))
}

/// The `unmatched-glob` warning for `key`'s `glob`, of this folder's root file.
fn unmatched(key: &str, glob: &str, under: &str) -> DiscoveryWarning {
    DiscoveryWarning::new(
        ROOT_FILE,
        "unmatched-glob",
        format!("`{key}` glob `{glob}` matches no file{under}"),
    )
}

/// What is worth saying about `globs`, the `build.exclude` of the workspace
/// file at `workspace_root`: a glob that matches nothing anywhere under the
/// workspace folder, members included. Said once, of the workspace file,
/// since each member's build checks only its own globs.
pub fn workspace_exclude_warnings(
    vfs: &dyn Vfs,
    workspace_root: &Path,
    globs: &[String],
) -> Vec<DiscoveryWarning> {
    if globs.is_empty() {
        return Vec::new();
    }
    // A plain walk, so what a build wrote counts, as it does for a member.
    let files: Vec<String> = crate::walk::readable_files(
        vfs,
        workspace_root,
        crate::compile::EXCLUDED_VAULT_DIRECTORIES,
    )
    .into_iter()
    .map(|entry| entry.rel)
    .collect();
    globs
        .iter()
        .filter(|glob| !matches_some(&files, glob))
        .map(|glob| unmatched("build.exclude", glob, " under the workspace"))
        .collect()
}

/// Each `/`-bounded prefix of `rel`, shortest first, `rel` itself last.
fn prefixes(rel: &str) -> impl Iterator<Item = &str> {
    rel.match_indices('/')
        .map(|(at, _)| &rel[..at])
        .chain(std::iter::once(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(folders: FolderPatterns) -> Patterns {
        Patterns {
            folders,
            ..Patterns::default()
        }
    }

    #[test]
    fn a_glob_matches_within_a_segment_and_across_them_only_with_two_stars() {
        assert!(glob_match("packs/*", "packs/review"));
        assert!(!glob_match("packs/*", "packs/review/skills"));
        assert!(glob_match("docs/**", "docs/a/b.md"));
        assert!(glob_match("**/*.draft.md", "a/b/c.draft.md"));
        assert!(glob_match("**/*.draft.md", "c.draft.md"));
        assert!(glob_match("v?.md", "v1.md"));
        assert!(!glob_match("v?.md", "v12.md"));
    }

    #[test]
    fn a_plugin_pattern_keeps_the_convention_inside_the_plugin() {
        let p = patterns(FolderPatterns {
            plugins: vec!["packs/*".to_owned()],
            ..FolderPatterns::default()
        });
        assert_eq!(
            p.canonical("packs/review/skills/lint.md"),
            "plugins/review/skills/lint.md"
        );
        assert_eq!(
            p.canonical("packs/review/review.md"),
            "plugins/review/review.md"
        );
        assert_eq!(p.canonical("elsewhere/note.md"), "elsewhere/note.md");
        assert_eq!(
            p.plugin_folder("packs/review/skills/a.md"),
            Some("packs/review")
        );
    }

    #[test]
    fn a_skill_pattern_names_notes_and_folders_at_any_height() {
        let p = patterns(FolderPatterns {
            plugins: vec!["packs/*".to_owned()],
            skills: vec!["notes/*".to_owned(), "packs/*/guides/*".to_owned()],
            ..FolderPatterns::default()
        });
        assert_eq!(p.canonical("notes/a.md"), "skills/a.md");
        assert_eq!(p.canonical("notes/a/a.md"), "skills/a/a.md");
        assert_eq!(p.canonical("notes/a/refs/x.md"), "skills/a/refs/x.md");
        assert_eq!(p.canonical("packs/p/guides/a.md"), "plugins/p/skills/a.md");
    }

    #[test]
    fn a_deep_glob_names_what_the_folder_holds_and_does_not_rematch_inside_it() {
        let p = patterns(FolderPatterns {
            skills: vec!["notes/**".to_owned()],
            ..FolderPatterns::default()
        });
        assert_eq!(p.canonical("notes/a.md"), "skills/a.md");
        assert_eq!(p.canonical("notes/a/a.md"), "skills/a/a.md");
        assert_eq!(p.canonical("notes/a/refs/x.md"), "skills/a/refs/x.md");
    }

    #[test]
    fn a_leaf_pattern_under_plugins_keeps_the_note_in_its_plugin() {
        let p = patterns(FolderPatterns {
            skills: vec!["plugins/*/guides/*".to_owned()],
            assets: vec!["plugins/*/static".to_owned()],
            ..FolderPatterns::default()
        });
        assert_eq!(
            p.canonical("plugins/rev/guides/g.md"),
            "plugins/rev/skills/g.md"
        );
        assert_eq!(
            p.canonical("plugins/rev/static/bin/tool"),
            "plugins/rev/assets/bin/tool"
        );
    }

    #[test]
    fn a_folder_that_never_ships_is_not_mapped_whatever_a_pattern_says() {
        let p = patterns(FolderPatterns {
            plugins: vec!["docs/*".to_owned(), "packs/*".to_owned()],
            skills: vec!["tests/*".to_owned(), "plugins/rev/docs/*".to_owned()],
            ..FolderPatterns::default()
        });
        assert_eq!(p.canonical("docs/p/skills/a.md"), "docs/p/skills/a.md");
        assert_eq!(p.canonical("tests/t.md"), "tests/t.md");
        assert_eq!(
            p.canonical("plugins/rev/docs/d.md"),
            "plugins/rev/docs/d.md"
        );
    }

    #[test]
    fn a_component_named_for_a_never_ship_folder_is_mapped_and_ships() {
        let p = patterns(FolderPatterns {
            plugins: vec!["packs/*".to_owned()],
            skills: vec!["notes/*".to_owned()],
            ..FolderPatterns::default()
        });
        assert_eq!(
            p.canonical("packs/docs/skills/a.md"),
            "plugins/docs/skills/a.md"
        );
        assert_eq!(p.canonical("notes/tests/r.md"), "skills/tests/r.md");
        assert!(!p.never_ships("packs/docs/skills/a.md"));
        assert!(p.never_ships("packs/docs/docs/a.md"));
    }

    #[test]
    fn a_never_ship_folder_is_a_position_and_not_a_name() {
        let p = Patterns::default();
        for ships in [
            "plugins/docs/skills/a.md",
            "plugins/tests/z.md",
            "plugins/docs/misplaced/y.md",
            "plugins/rev/skills/tests/t.md",
            "marketplaces/fixtures/plugins/docs/skills/tests/tests.md",
        ] {
            assert!(!p.never_ships(ships), "{ships}");
        }
        for stays in [
            "docs/a.md",
            "plugins/rev/docs/x.md",
            "docs/plugins/q/skills/w.md",
            "plugins/rev/skills/a/tests/t.md",
            "plugins/docs/docs/x.md",
            "skills/a/skills/docs/x.md",
            "plugins/.git/x.md",
            "plugins/node_modules/x.md",
        ] {
            assert!(p.never_ships(stays), "{stays}");
        }
    }

    #[test]
    fn a_note_inside_a_never_ship_folder_links_within_it_and_nothing_links_in() {
        let p = Patterns::default();
        assert!(!p.links_into_never_ship("plugins/rev/docs/a.md", "plugins/rev/docs/b.md"));
        assert!(p.links_into_never_ship("plugins/rev/skills/a.md", "plugins/rev/docs/b.md"));
        assert!(p.links_into_never_ship("docs/x/a.md", "docs/x/tests/b.md"));
        assert!(
            !p.links_into_never_ship("plugins/rev/skills/a.md", "plugins/docs/skills/guide.md")
        );
    }

    #[test]
    fn only_a_glob_above_the_component_names_a_never_ship_folder() {
        assert!(names_a_never_ship_folder("docs/*", true));
        assert!(names_a_never_ship_folder("docs", true));
        assert!(names_a_never_ship_folder("plugins/rev/docs/*", true));
        assert!(!names_a_never_ship_folder("packs/docs", true));
        assert!(names_a_never_ship_folder("plugins/*/docs", false));
    }

    #[test]
    fn a_pattern_under_a_marketplace_stays_in_that_marketplace() {
        let p = patterns(FolderPatterns {
            plugins: vec!["marketplaces/a/packs/*".to_owned()],
            skills: vec!["marketplaces/*/guides/*".to_owned()],
            ..FolderPatterns::default()
        });
        assert_eq!(
            p.canonical("marketplaces/a/packs/rev/skills/s.md"),
            "marketplaces/a/plugins/rev/skills/s.md"
        );
        assert_eq!(
            p.canonical("marketplaces/b/guides/g.md"),
            "marketplaces/b/skills/g.md"
        );
    }

    #[test]
    fn a_shorter_claim_of_another_kind_is_a_conflict() {
        let p = patterns(FolderPatterns {
            skills: vec!["notes/*".to_owned()],
            agents: vec!["notes/bots/*".to_owned()],
            ..FolderPatterns::default()
        });
        let warnings = p.warnings(&["notes/bots/b.md".to_owned()], &[], &[]);
        let conflict = warnings
            .iter()
            .find(|warning| warning.reason == "pattern-conflict")
            .expect("a conflict");
        assert_eq!(conflict.path, "notes/bots/b.md");
        assert!(
            conflict.detail.contains("`skills: notes/*`"),
            "{conflict:?}"
        );
    }

    #[test]
    fn a_plugin_folder_inside_a_skill_pattern_is_no_conflict() {
        let p = patterns(FolderPatterns {
            plugins: vec!["packs/*".to_owned()],
            skills: vec!["packs/*/guides/*".to_owned()],
            ..FolderPatterns::default()
        });
        let warnings = p.warnings(&["packs/p/guides/g.md".to_owned()], &[], &[]);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn an_excluded_folder_merges_no_plugin() {
        let mut p = patterns(FolderPatterns {
            plugins: vec!["new/*".to_owned(), "old/*".to_owned()],
            ..FolderPatterns::default()
        });
        let files = [
            "new/rev/skills/a.md".to_owned(),
            "old/rev/skills/b.md".to_owned(),
        ];
        let merged = |p: &Patterns| {
            p.warnings(&files, &[], &[])
                .iter()
                .any(|warning| warning.reason == "plugin-merged")
        };
        assert!(merged(&p));
        p.exclude = vec!["old".to_owned()];
        assert!(!merged(&p));
    }

    #[test]
    fn excluding_the_output_is_no_unmatched_glob() {
        let mut p = patterns(FolderPatterns::default());
        p.exclude = vec!["dist".to_owned(), "dist/**".to_owned()];
        let output = ["dist/.claude-plugin/packs".to_owned()];
        assert!(p.warnings(&[], &output, &[]).is_empty());
        assert_eq!(p.warnings(&[], &[], &[]).len(), 2);
    }

    #[test]
    fn a_glob_that_cannot_be_read_says_why() {
        for bad in [
            "",
            " ",
            "/abs/*",
            "../x/*",
            "a/../b",
            "./a",
            "a//b",
            "notes/[a",
            "notes/{a,b}/*",
            "a**/b",
            "a\\b",
            "C:/x",
        ] {
            assert!(glob_problem(bad).is_some(), "{bad:?}");
        }
        for good in ["notes/*", "**/*.draft.md", "docs/**", "drafts/", "v?.md"] {
            assert_eq!(glob_problem(good), None, "{good:?}");
        }
    }

    #[test]
    fn warnings_name_a_dead_glob_a_contested_path_and_a_merged_plugin() {
        let p = Patterns {
            folders: FolderPatterns {
                plugins: vec!["packs/*".to_owned()],
                skills: vec!["notes/*".to_owned()],
                agents: vec!["notes/*".to_owned(), "none/*".to_owned()],
                ..FolderPatterns::default()
            },
            exclude: vec!["gone/**".to_owned()],
            ..Patterns::default()
        };
        let files = [
            "notes/a.md",
            "packs/rev/skills/x.md",
            "plugins/rev/skills/y.md",
        ]
        .map(str::to_owned);
        let warnings = p.warnings(&files, &[], &[]);
        let detail = |reason: &str| -> Vec<&str> {
            warnings
                .iter()
                .filter(|w| w.reason == reason)
                .map(|w| w.detail.as_str())
                .collect()
        };
        assert_eq!(detail("unmatched-glob").len(), 2, "{warnings:?}");
        let [contested] = detail("pattern-conflict")[..] else {
            panic!("{warnings:?}");
        };
        assert!(contested.contains("`skills: notes/*`"), "{contested}");
        assert!(contested.contains("`agents: notes/*`"), "{contested}");
        let [merged] = detail("plugin-merged")[..] else {
            panic!("{warnings:?}");
        };
        assert!(merged.contains("packs/rev and plugins/rev"), "{merged}");
    }

    #[test]
    fn an_assets_pattern_names_the_plugins_assets_folder() {
        let p = patterns(FolderPatterns {
            plugins: vec!["packs/*".to_owned()],
            assets: vec!["packs/*/static".to_owned()],
            ..FolderPatterns::default()
        });
        assert_eq!(
            p.canonical("packs/p/static/bin/tool"),
            "plugins/p/assets/bin/tool"
        );
    }

    #[test]
    fn exclusion_covers_what_a_matched_folder_holds() {
        let p = Patterns {
            folders: FolderPatterns::default(),
            exclude: vec!["drafts".to_owned(), "**/*.draft.md".to_owned()],
            ..Patterns::default()
        };
        assert!(p.excluded("drafts/a/b.md"));
        assert!(p.excluded("skills/x.draft.md"));
        assert!(!p.excluded("skills/x.md"));
    }

    /// The patterns of the member `name` of a workspace at `/ws`, each root
    /// file holding `build.exclude: <globs>`.
    fn member_excluding(workspace: &str, name: &str, own: &str) -> Patterns {
        let vfs = crate::vfs::MemVfs::new();
        let file =
            |globs: &str, extra: &str| format!("---\n{extra}build:\n  exclude: {globs}\n---\n");
        vfs.insert("/ws/skillwright.md", file(workspace, "workspace: true\n"))
            .unwrap();
        vfs.insert(
            format!("/ws/marketplaces/{name}/skillwright.md"),
            file(own, ""),
        )
        .unwrap();
        Patterns::load(&vfs, Path::new(&format!("/ws/marketplaces/{name}"))).unwrap()
    }

    #[test]
    fn a_members_exclusions_add_to_the_workspaces() {
        let p = member_excluding("['**/secrets/**']", "acme", "['drafts/**']");
        assert!(p.excluded("secrets/key.md"));
        assert!(p.excluded("plugins/p/secrets/key.md"));
        assert!(p.excluded("drafts/d.md"));
        assert!(!p.excluded("skills/s.md"));
    }

    #[test]
    fn a_workspace_glob_reads_from_the_workspace_folder() {
        let hidden = |glob: &str| member_excluding(glob, "acme", "[]").excluded("secrets/key.md");
        assert!(!hidden("'secrets/**'"));
        assert!(hidden("'**/secrets/**'"));
        assert!(hidden("'marketplaces/*/secrets/**'"));
        assert!(hidden("'marketplaces/acme/secrets/**'"));
        assert!(hidden("'marketplaces/acme'"));
        assert!(!hidden("'marketplaces/other/secrets/**'"));
    }

    #[test]
    fn a_members_glob_does_not_reach_a_sibling() {
        let acme = member_excluding("[]", "acme", "['drafts/**']");
        let beta = member_excluding("[]", "beta", "[]");
        assert!(acme.excluded("drafts/d.md"));
        assert!(!beta.excluded("drafts/d.md"));
    }

    #[test]
    fn a_workspace_glob_that_matches_nothing_under_it_warns_once() {
        let vfs = crate::vfs::MemVfs::new();
        vfs.insert(
            "/ws/skillwright.md",
            "---\nworkspace: true\nbuild:\n  exclude: ['marketplaces/*/skills/a.md', 'skills/a.md', 'shared/**']\n---\n",
        )
        .unwrap();
        vfs.insert("/ws/shared/x.md", "x").unwrap();
        vfs.insert("/ws/marketplaces/one/skillwright.md", "---\n---\n")
            .unwrap();
        vfs.insert("/ws/marketplaces/one/skills/a.md", "x").unwrap();
        let globs = crate::paths::read_workspace_above(
            &vfs,
            Path::new("/ws/marketplaces/one"),
            RootRead::Layout,
        )
        .unwrap()
        .unwrap()
        .build
        .exclude;
        let warnings = workspace_exclude_warnings(&vfs, Path::new("/ws"), &globs);
        let [warning] = &warnings[..] else {
            panic!("{warnings:?}");
        };
        assert_eq!(warning.path, ROOT_FILE);
        assert_eq!(warning.reason, "unmatched-glob");
        assert!(warning.detail.contains("`skills/a.md`"), "{warning:?}");
        // A member's own build does not check the workspace's globs again.
        let member = Patterns::load(&vfs, Path::new("/ws/marketplaces/one")).unwrap();
        assert!(member.warnings(&[], &[], &[]).is_empty());
    }
}
