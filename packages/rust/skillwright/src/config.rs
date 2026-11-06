//! Configuration: the root file, `skillwright.md`.
//!
//! The root file is a note at the top of the vault. Its frontmatter's
//! top-level keys are the marketplace's fields; settings for how skillwright
//! runs sit under `build:`. The folder holding it is the vault. The core
//! parses the file ([`tradecraft_core::parse_root_file`]); this module finds
//! it.
//!
//! It is found by walking up from the working directory, the way git finds a
//! repository, or named with `--config`. `--vault` names the folder instead,
//! and its `skillwright.md` is read when there is one. With no file at all, a
//! folder holding `plugins/` or `skills/` builds with every default, given
//! `--out`.
//!
//! A root file with `workspace: true` holds shared defaults for every
//! `marketplaces/<m>/skillwright.md` below it, and `build` and `check` run
//! over each of them.
//!
//! Relative paths in the file resolve against the file's own folder; a flag's
//! path resolves against the shell's working directory. There is no machine
//! file: personal defaults come from the environment. Nothing here is ever
//! written back.

use std::path::{Path, PathBuf};

pub use tradecraft_core::{
    AssetSetting, BuildSettings, DestinationSetting, HookSetting, ROOT_FILE, RootFile,
    normalize_join, relative_path, resolve_path,
};

/// The folder a workspace keeps its marketplaces in.
pub const WORKSPACE_DIR: &str = "marketplaces";

/// The folders whose presence makes a folder a vault with no root file.
const CONVENTION_DIRS: &[&str] = &["plugins", "skills"];

/// The result of reading one root file.
#[non_exhaustive]
#[derive(Debug)]
pub enum RootLoad {
    /// The file was read.
    Loaded(Box<RootFile>),
    /// The file exists but could not be read.
    Unreadable {
        /// What went wrong.
        detail: String,
    },
    /// The file was read but is not a valid root file.
    Malformed {
        /// What went wrong.
        detail: String,
    },
}

/// Read and parse the root file at `path`.
pub fn read_root_file(path: &Path) -> RootLoad {
    match std::fs::read_to_string(path) {
        Ok(text) => match tradecraft_core::parse_root_file(&text) {
            Ok(root) => RootLoad::Loaded(Box::new(root)),
            Err(err) => RootLoad::Malformed {
                detail: err.to_string(),
            },
        },
        Err(err) => RootLoad::Unreadable {
            detail: err.to_string(),
        },
    }
}

/// Whether `dir` holds a folder the convention reads (`plugins/`,
/// `skills/`), so it builds with no root file.
pub fn is_convention_vault(dir: &Path) -> bool {
    CONVENTION_DIRS.iter().any(|name| dir.join(name).is_dir())
}

/// The workspace root file above the marketplace whose root is `dir`, when
/// `dir` is `<workspace>/marketplaces/<m>/` and that file says
/// `workspace: true`. A file that cannot be read is returned too: an exclusion
/// it holds must not fail open, so loading it refuses the run.
pub fn workspace_above(dir: &Path) -> Option<PathBuf> {
    let parent = dir.parent()?;
    if parent.file_name()? != WORKSPACE_DIR {
        return None;
    }
    let file = parent.parent()?.join(ROOT_FILE);
    if !file.exists() {
        return None;
    }
    match read_root_file(&file) {
        RootLoad::Loaded(root) if !root.workspace => None,
        _ => Some(file),
    }
}

/// Each `marketplaces/<m>/skillwright.md` under a workspace root, by name.
pub fn workspace_members(workspace_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(workspace_dir.join(WORKSPACE_DIR)) else {
        return Vec::new();
    };
    let mut members: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(ROOT_FILE))
        .filter(|file| file.is_file())
        .collect();
    members.sort();
    members
}

/// The folder a harness loads a user's own skills from, under `$HOME`.
pub fn user_skills_dir(relative: &str) -> PathBuf {
    home_dir().join(relative)
}

/// Resolve a flag's path as the shell would: absolute as given, `~` or a
/// `~/` prefix against the home directory, anything else relative to
/// `base`. The rule `--vault`, `--to`, `--out` and `--config` share, with
/// `base` the working directory.
pub fn resolve_against_cwd(path: &str, base: &Path) -> PathBuf {
    resolve_path(path, base, &home_dir())
}

/// [`resolve_against_cwd`] for a value read from the root file: the same
/// rule, with `base` the file's folder, never the process's working
/// directory. The second name marks that intent at the call site.
pub fn resolve_configured_path(path: &str, base: &Path) -> PathBuf {
    resolve_against_cwd(path, base)
}

/// The user's home directory, read directly from `$HOME`. Falls back to `/`
/// rather than panicking when unset.
pub(crate) fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Print each warning the root file at `path` carries to `err`.
pub fn print_warnings(root: &RootFile, path: &Path, err: &mut dyn std::io::Write) {
    for warning in &root.warnings {
        let _ = writeln!(err, "warning: {}: {warning}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marketplace_under_a_workspace_finds_its_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let member = tmp.path().join("marketplaces/one");
        std::fs::create_dir_all(&member).unwrap();
        std::fs::write(member.join(ROOT_FILE), "---\nname: one\n---\n").unwrap();
        assert_eq!(workspace_above(&member), None);
        std::fs::write(tmp.path().join(ROOT_FILE), "---\nworkspace: true\n---\n").unwrap();
        assert_eq!(workspace_above(&member), Some(tmp.path().join(ROOT_FILE)));
        assert_eq!(workspace_members(tmp.path()), vec![member.join(ROOT_FILE)]);
    }

    /// Only `~` and `~/` name the home directory. `~alice/notes` and
    /// `~notes` are ordinary relative paths.
    #[test]
    fn a_tilde_that_is_not_followed_by_a_separator_is_a_relative_path() {
        let home = home_dir();
        assert_eq!(
            resolve_against_cwd("~alice/notes", Path::new("/work")),
            PathBuf::from("/work/~alice/notes")
        );
        assert_eq!(
            resolve_configured_path("~notes", Path::new("/proj")),
            PathBuf::from("/proj/~notes")
        );
        assert_eq!(resolve_against_cwd("~", Path::new("/work")), home);
        assert_eq!(
            resolve_against_cwd("~/notes", Path::new("/work")),
            home.join("notes")
        );
    }
}
