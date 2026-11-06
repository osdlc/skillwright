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
//! The root file is read here rather than passed in, the way the vault's own
//! excluded-files list is, so every entry point answers alike. Globs are
//! relative to the root file's folder.

use std::path::Path;

use crate::settings::{FolderPatterns, ROOT_FILE, parse_root_file};
use crate::vfs::Vfs;

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
    /// file has none: the commands that load it report what is wrong with it.
    pub fn read(vfs: &dyn Vfs, vault_root: &Path) -> Self {
        let Ok(bytes) = vfs.read_file(&vault_root.join(ROOT_FILE)) else {
            return Self::default();
        };
        let Some(root) = String::from_utf8(bytes)
            .ok()
            .and_then(|text| parse_root_file(&text).ok())
        else {
            return Self::default();
        };
        let mut exclude = root.build.exclude;
        // A moved packs folder holds built copies of the vault's notes.
        exclude.extend(root.marketplace.blocks.values().filter_map(|block| {
            block
                .get(crate::harness::manifest::PACKS_KEY)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        }));
        Self {
            folders: root.build.patterns,
            exclude,
        }
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
            if settled {
                pending.push(segment);
            } else if matches(&self.folders.assets) {
                pending.clear();
                out.push(crate::model::load::ASSETS_DIR);
                settled = true;
            } else if let Some((_, kind)) = leaves.iter().find(|(globs, _)| matches(globs)) {
                pending.clear();
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
            .filter(|entry| entry.is_dir)
            .map(|entry| format!("{folder}/{}", entry.name))
            .find(|rel| self.folders.assets.iter().any(|g| glob_match(g, rel)))
    }
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
