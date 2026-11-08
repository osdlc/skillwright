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

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::compile::EXCLUDED_BUNDLE_DIRECTORIES;
use crate::discover::DiscoveryWarning;
use crate::error::CoreError;
use crate::frontmatter::EntityKind;
use crate::settings::{FolderPatterns, ROOT_FILE, RootFile, parse_root_file};
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

/// The root file's folder patterns and exclusions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Patterns {
    folders: FolderPatterns,
    exclude: Vec<String>,
    /// The globs a workspace file gave this marketplace.
    inherited: BTreeSet<String>,
}

impl Patterns {
    /// The patterns in the root file at `vault_root`. A missing or malformed
    /// file has none: [`Self::load`] is what reports a malformed one.
    pub fn read(vfs: &dyn Vfs, vault_root: &Path) -> Self {
        Self::load(vfs, vault_root).unwrap_or_default()
    }

    /// The patterns in the root file at `vault_root`, over what a workspace
    /// file above it sets, key by key. None when there is no file. Globs are
    /// read from the marketplace's folder wherever they were written.
    ///
    /// # Errors
    ///
    /// `refused: malformed-root-file`: an exclusion that cannot be read must
    /// not fail open.
    pub fn load(vfs: &dyn Vfs, vault_root: &Path) -> Result<Self, CoreError> {
        let mut root = read_root(vfs, &vault_root.join(ROOT_FILE))?.unwrap_or_default();
        let mut own = Self {
            folders: root.build.patterns.clone(),
            exclude: root.build.exclude.clone(),
            inherited: BTreeSet::new(),
        };
        let written: BTreeSet<String> = own.globs().map(str::to_owned).collect();
        if !root.workspace
            && let Some(shared) = workspace_above(vfs, vault_root)?
        {
            root.inherit(&shared);
        }
        own.folders = root.build.patterns;
        own.exclude = root.build.exclude;
        own.inherited = own
            .globs()
            .filter(|glob| !written.contains(*glob))
            .map(str::to_owned)
            .collect();
        Ok(own)
    }

    /// Whether any key holds a glob. With none, [`Self::warnings_with`] has
    /// nothing to say, so a caller need not walk the vault for it.
    pub(crate) fn has_globs(&self) -> bool {
        self.globs().next().is_some()
    }

    /// Every glob, whichever key holds it.
    fn globs(&self) -> impl Iterator<Item = &str> {
        self.kinds()
            .into_iter()
            .flat_map(|(_, globs)| globs)
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
        [
            ("assets", &self.folders.assets),
            ("skills", &self.folders.skills),
            ("agents", &self.folders.agents),
            ("commands", &self.folders.commands),
            ("hooks", &self.folders.hooks),
            ("plugins", &self.folders.plugins),
        ]
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

    /// What is worth saying about these patterns over the vault's `files`,
    /// vault-relative: a glob that matches nothing, a path two kinds claim,
    /// and one plugin read from two folders. `output` is what a build owns in
    /// the vault: not source, but a fair thing for `build.exclude` to name.
    /// `siblings` are the other marketplaces' files: a glob a workspace shares
    /// need only match in one of them.
    #[cfg(test)]
    pub(crate) fn warnings(
        &self,
        files: &[String],
        output: &[String],
        siblings: &[String],
    ) -> Vec<DiscoveryWarning> {
        self.warnings_with(files, output, || siblings.to_vec())
    }

    /// [`Self::warnings`] with the other marketplaces' files read only when a
    /// shared glob matches nothing here, and then once.
    pub(crate) fn warnings_with(
        &self,
        files: &[String],
        output: &[String],
        siblings: impl FnOnce() -> Vec<String>,
    ) -> Vec<DiscoveryWarning> {
        let mut siblings = Some(siblings);
        let mut sibling_files: Option<Vec<String>> = None;
        let warn = |path: &str, reason: &str, detail: String| DiscoveryWarning {
            path: path.to_owned(),
            reason: reason.to_owned(),
            detail,
        };
        let matches_some = |files: &[String], glob: &str| {
            files
                .iter()
                .any(|file| prefixes(file).any(|prefix| glob_match(glob, prefix)))
        };
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
            let names_output = key == "build.exclude" && matches_some(output, glob);
            if names_output || matches_some(files, glob) {
                continue;
            }
            let shared = self.inherited.contains(glob)
                && matches_some(
                    sibling_files
                        .get_or_insert_with(|| siblings.take().map_or_else(Vec::new, |f| f())),
                    glob,
                );
            if !shared {
                warnings.push(warn(
                    ROOT_FILE,
                    "unmatched-glob",
                    format!("`{key}` glob `{glob}` matches no file"),
                ));
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
            let at = if parts.first() == Some(&"marketplaces") {
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
            warnings.push(warn(
                path,
                "pattern-conflict",
                format!("{} claim it; the first one wins", claims.join(" and ")),
            ));
        }
        for (conventional, folders) in folders {
            if folders.len() > 1 {
                let name = conventional.rsplit('/').next().unwrap_or(&conventional);
                let named: Vec<String> = folders.into_iter().collect();
                warnings.push(warn(
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

    /// Whether `rel`, or a folder holding it, matches `build.exclude`.
    pub fn excluded(&self, rel: &str) -> bool {
        !self.exclude.is_empty()
            && prefixes(rel).any(|prefix| self.exclude.iter().any(|g| glob_match(g, prefix)))
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
                keep_conventional(&mut out, &mut pending, &["marketplaces", "plugins"]);
                out.push(crate::model::load::ASSETS_DIR);
                settled = true;
            } else if let Some((_, kind)) = leaves.iter().find(|(globs, _)| matches(globs)) {
                keep_conventional(&mut out, &mut pending, &["marketplaces", "plugins"]);
                out.extend([*kind, segment]);
                settled = true;
            } else if matches(&self.folders.plugins) {
                keep_conventional(&mut out, &mut pending, &["marketplaces"]);
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

/// The root file at `file`, `None` when there is none.
fn read_root(vfs: &dyn Vfs, file: &Path) -> Result<Option<RootFile>, CoreError> {
    if !vfs.exists(file) {
        return Ok(None);
    }
    let malformed = |detail: String| {
        CoreError::refused(
            "malformed-root-file",
            format!("{}: {detail}. Nothing was written", file.display()),
        )
    };
    let text = String::from_utf8(vfs.read_file(file)?)
        .map_err(|_| malformed("it is not UTF-8".to_owned()))?;
    parse_root_file(&text)
        .map(Some)
        .map_err(|err| malformed(err.to_string()))
}

/// The workspace file above the marketplace at `vault_root`, when that is
/// `<workspace>/marketplaces/<m>/` and the file says `workspace: true`.
pub(crate) fn workspace_above(
    vfs: &dyn Vfs,
    vault_root: &Path,
) -> Result<Option<RootFile>, CoreError> {
    let Some(above) = vault_root
        .parent()
        .filter(|folder| {
            folder
                .file_name()
                .is_some_and(|name| name == crate::paths::WORKSPACE_DIR)
        })
        .and_then(Path::parent)
    else {
        return Ok(None);
    };
    let root = read_root(vfs, &above.join(ROOT_FILE))?;
    Ok(root.filter(|root| root.workspace))
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
            exclude: Vec::new(),
            inherited: BTreeSet::new(),
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
            inherited: BTreeSet::new(),
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
            inherited: BTreeSet::new(),
        };
        assert!(p.excluded("drafts/a/b.md"));
        assert!(p.excluded("skills/x.draft.md"));
        assert!(!p.excluded("skills/x.md"));
    }
}
