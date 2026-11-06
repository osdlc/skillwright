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
//! stays out whatever a pattern says.
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
use crate::settings::{FolderPatterns, ROOT_FILE, parse_root_file};
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

/// Whether `glob` spells out a folder that never ships.
pub(crate) fn names_a_never_ship_folder(glob: &str) -> bool {
    glob.split('/')
        .any(|segment| EXCLUDED_BUNDLE_DIRECTORIES.contains(&segment))
}

/// `*` and `?` within one path segment; `**` as a whole segment matches any
/// number of segments.
pub(crate) fn glob_match(glob: &str, path: &str) -> bool {
    let glob: Vec<&str> = glob.trim_end_matches('/').split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    segments_match(&glob, &path)
}

fn segments_match(glob: &[&str], path: &[&str]) -> bool {
    match glob.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| segments_match(rest, &path[skip..])),
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
}

impl Patterns {
    /// The patterns in the root file at `vault_root`. A missing or malformed
    /// file has none: [`Self::load`] is what reports a malformed one.
    pub fn read(vfs: &dyn Vfs, vault_root: &Path) -> Self {
        Self::load(vfs, vault_root).unwrap_or_default()
    }

    /// The patterns in the root file at `vault_root`, none when there is no
    /// file.
    ///
    /// # Errors
    ///
    /// `refused: malformed-root-file`: an exclusion that cannot be read must
    /// not fail open.
    pub fn load(vfs: &dyn Vfs, vault_root: &Path) -> Result<Self, CoreError> {
        let file = vault_root.join(ROOT_FILE);
        if !vfs.exists(&file) {
            return Ok(Self::default());
        }
        let malformed = |detail: String| {
            CoreError::refused(
                "malformed-root-file",
                format!("{}: {detail}. Nothing was written", file.display()),
            )
        };
        let text = String::from_utf8(vfs.read_file(&file)?)
            .map_err(|_| malformed("it is not UTF-8".to_owned()))?;
        let root = parse_root_file(&text).map_err(|err| malformed(err.to_string()))?;
        Ok(Self {
            folders: root.build.patterns,
            exclude: root.build.exclude,
        })
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
    /// and one plugin read from two folders.
    pub(crate) fn warnings(&self, files: &[String]) -> Vec<DiscoveryWarning> {
        let warn = |path: &str, reason: &str, detail: String| DiscoveryWarning {
            path: path.to_owned(),
            reason: reason.to_owned(),
            detail,
        };
        let matches_some = |glob: &str| {
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
            if !matches_some(glob) {
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
        for file in files {
            for prefix in prefixes(file) {
                let claims: Vec<String> = self
                    .kinds()
                    .iter()
                    .flat_map(|(key, globs)| {
                        globs
                            .iter()
                            .filter(|glob| glob_match(glob, prefix))
                            .take(1)
                            .map(move |glob| format!("`{key}: {glob}`"))
                    })
                    .collect();
                if claims.len() > 1 {
                    claimed.entry(prefix).or_insert(claims);
                }
            }
            if let Some(name) = self.canonical(file).strip_prefix("plugins/")
                && let Some((name, _)) = name.split_once('/')
            {
                let folder = self
                    .plugin_folder(file)
                    .map_or_else(|| format!("plugins/{name}"), str::to_owned);
                folders.entry(name.to_owned()).or_default().insert(folder);
            }
        }
        for (path, claims) in claimed {
            warnings.push(warn(
                path,
                "pattern-conflict",
                format!("{} all claim it; the first one wins", claims.join(" and ")),
            ));
        }
        for (name, folders) in folders {
            if folders.len() > 1 {
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
        for (prefix, segment) in prefixes(rel).zip(rel.split('/')) {
            let matches = |globs: &[String]| globs.iter().any(|g| glob_match(g, prefix));
            // A folder that never ships ends the mapping, so nothing under it
            // reads as what a pattern says.
            let never_ships = prefix != rel && EXCLUDED_BUNDLE_DIRECTORIES.contains(&segment);
            if settled || never_ships {
                pending.push(segment);
                settled = true;
            } else if matches(&self.folders.assets) {
                keep_conventional_plugin(&mut out, &mut pending);
                out.push(crate::model::load::ASSETS_DIR);
                settled = true;
            } else if let Some((_, kind)) = leaves.iter().find(|(globs, _)| matches(globs)) {
                keep_conventional_plugin(&mut out, &mut pending);
                out.extend([*kind, segment]);
                settled = true;
            } else if matches(&self.folders.plugins) {
                pending.clear();
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

/// Drops the folders a leaf match replaces, keeping a conventional
/// `plugins/<p>` at their head: the plugin folder still owns what it holds.
fn keep_conventional_plugin<'r>(out: &mut Vec<&'r str>, pending: &mut Vec<&'r str>) {
    if out.is_empty() && pending.len() >= 2 && pending[0] == "plugins" {
        out.extend(&pending[..2]);
    }
    pending.clear();
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
    fn a_deep_glob_does_not_rematch_inside_what_it_matched() {
        let p = patterns(FolderPatterns {
            skills: vec!["notes/**".to_owned()],
            ..FolderPatterns::default()
        });
        assert_eq!(p.canonical("notes/a/a.md"), "skills/notes/a/a.md");
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
        };
        assert!(p.excluded("drafts/a/b.md"));
        assert!(p.excluded("skills/x.draft.md"));
        assert!(!p.excluded("skills/x.md"));
    }
}
