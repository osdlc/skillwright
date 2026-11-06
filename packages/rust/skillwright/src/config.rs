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

/// The nearest root file at or above `start`.
///
/// A `skillwright.md` that sits where a component note sits —
/// `<kind>/skillwright/skillwright.md`, a plugin or skill named
/// `skillwright` — is that component's note, not a root file, and the walk
/// goes past it.
pub fn find_root_file(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(here) = dir {
        let candidate = here.join(ROOT_FILE);
        if candidate.is_file() && !is_component_note(&candidate) {
            return Some(candidate);
        }
        dir = here.parent();
    }
    None
}

/// Whether `path` is `<kind>/skillwright/skillwright.md`, a component note
/// and not a root file. `marketplaces/skillwright/skillwright.md` is not one:
/// that is the root file of a workspace member named `skillwright`.
fn is_component_note(path: &Path) -> bool {
    let folder = path.parent();
    let named = folder
        .and_then(Path::file_name)
        .is_some_and(|name| name == "skillwright");
    let in_kind = folder
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name != "marketplaces"
                && tradecraft_core::KIND_DIRS
                    .iter()
                    .any(|(dir, _)| *dir == name)
        });
    named && in_kind
}

/// Whether `dir` holds a folder the convention reads (`plugins/`,
/// `skills/`), so it builds with no root file.
pub fn is_convention_vault(dir: &Path) -> bool {
    CONVENTION_DIRS.iter().any(|name| dir.join(name).is_dir())
}

/// The workspace root file above the marketplace whose root is `dir`, when
/// `dir` is `<workspace>/marketplaces/<m>/` and that file says
/// `workspace: true`.
pub fn workspace_above(dir: &Path) -> Option<PathBuf> {
    let parent = dir.parent()?;
    if parent.file_name()? != WORKSPACE_DIR {
        return None;
    }
    let file = parent.parent()?.join(ROOT_FILE);
    match read_root_file(&file) {
        RootLoad::Loaded(root) if root.workspace => Some(file),
        _ => None,
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
/// `base`. `~name` and `~\` are not expanded: they are relative paths. The
/// rule `--vault`, `--to`, `--out` and `--config` share, with `base` the
/// working directory.
pub fn resolve_against_cwd(path: &str, base: &Path) -> PathBuf {
    if path == "~" || path == "~/" {
        return home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        normalize_join(base, p)
    }
}

/// [`resolve_against_cwd`] for a value read from the root file: the same
/// rule, with `base` the file's folder, never the process's working
/// directory. The second name marks that intent at the call site.
pub fn resolve_configured_path(path: &str, base: &Path) -> PathBuf {
    resolve_against_cwd(path, base)
}

/// The user's home directory, read directly from `$HOME`. Falls back to `/`
/// rather than panicking when unset.
fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Join `rel` onto `base` and lexically collapse `.`/`..`, without touching
/// the filesystem — the Rust equivalent of Node's `path.resolve`.
pub fn normalize_join(base: &Path, rel: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in base.join(rel).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `to` expressed relative to `from`, lexically — the Rust equivalent of
/// Node's `path.relative`. Both sides are collapsed first, and a target
/// outside `from` comes back with the `..` segments that reach it. An empty
/// result means the two are the same path.
pub fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from = normalize_join(from, Path::new(""));
    let to = normalize_join(to, Path::new(""));
    let mut from = from.components().peekable();
    let mut to = to.components().peekable();
    while from.peek().is_some() && from.peek() == to.peek() {
        from.next();
        to.next();
    }
    let mut out = PathBuf::new();
    for _ in from {
        out.push("..");
    }
    for component in to {
        out.push(component.as_os_str());
    }
    out
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
    fn the_walk_up_finds_the_nearest_root_file() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("a/plugins/p/skills");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(tmp.path().join("a").join(ROOT_FILE), "---\n---\n").unwrap();
        assert_eq!(
            find_root_file(&nested),
            Some(tmp.path().join("a").join(ROOT_FILE))
        );
    }

    #[test]
    fn a_component_named_skillwright_is_not_a_root_file() {
        let tmp = tempfile::tempdir().unwrap();
        let plugin = tmp.path().join("plugins/skillwright");
        std::fs::create_dir_all(plugin.join("skills")).unwrap();
        std::fs::write(plugin.join(ROOT_FILE), "---\nname: skillwright\n---\n").unwrap();
        std::fs::write(tmp.path().join(ROOT_FILE), "---\nname: osdlc\n---\n").unwrap();
        assert_eq!(
            find_root_file(&plugin.join("skills")),
            Some(tmp.path().join(ROOT_FILE))
        );
    }

    #[test]
    fn a_member_named_skillwright_keeps_its_root_file() {
        let tmp = tempfile::tempdir().unwrap();
        let member = tmp.path().join("marketplaces/skillwright");
        std::fs::create_dir_all(&member).unwrap();
        std::fs::write(member.join(ROOT_FILE), "---\nname: skillwright\n---\n").unwrap();
        std::fs::write(tmp.path().join(ROOT_FILE), "---\nworkspace: true\n---\n").unwrap();
        assert_eq!(find_root_file(&member), Some(member.join(ROOT_FILE)));
    }

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
