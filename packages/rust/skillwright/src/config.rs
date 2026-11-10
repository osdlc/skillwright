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
//! path resolves against the shell's working directory. The root file is
//! never written back by this module.
//!
//! The one per-user setting is the editor `author` opens. It lives in a small
//! machine file, not in the root file, and [`MachineAuthor::save`] is the only
//! write here.

use std::path::{Path, PathBuf};

pub use tradecraft_core::paths::{normalize_join, relative_path};
pub use tradecraft_core::{
    AssetSetting, BuildSettings, DestinationSetting, HookSetting, ROOT_FILE, RootFile,
    WORKSPACE_DIR, resolve_path,
};

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

/// [`tradecraft_core::workspace_above`] over the real filesystem.
pub fn workspace_above(dir: &Path) -> Option<PathBuf> {
    tradecraft_core::workspace_above(&tradecraft_core::NativeVfs, dir)
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

/// The editors `author` can open the vault in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Editor {
    /// Obsidian, through an `obsidian://` link.
    Obsidian,
    /// VS Code, through its `code` command.
    Code,
    /// The terminal editor `$VISUAL` or `$EDITOR` names.
    Editor,
    /// The system's default opener: `open` or `xdg-open`.
    System,
}

impl Editor {
    /// The spelling `--with` and the machine file use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Obsidian => "obsidian",
            Self::Code => "code",
            Self::Editor => "editor",
            Self::System => "system",
        }
    }
}

/// Env var that relocates the file holding the saved `author` editor, for
/// tests and one-off runs.
pub const MACHINE_CONFIG_ENV: &str = "SKILLWRIGHT_MACHINE_CONFIG";

/// A path that stands in for the machine file ahead of
/// [`MACHINE_CONFIG_ENV`]. Tests set it, because moving an environment
/// variable needs `unsafe`.
static MACHINE_CONFIG_OVERRIDE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Set or clear the machine-file override for this process and return the
/// previous one. For tests only, behind the `test-support` feature: a test
/// that sets it must hold its own lock for as long as the override is in
/// place. Not part of the API.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn replace_machine_config_override(path: Option<PathBuf>) -> Option<PathBuf> {
    let mut slot = MACHINE_CONFIG_OVERRIDE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    std::mem::replace(&mut *slot, path)
}

/// Where the saved `author` editor lives: `$SKILLWRIGHT_MACHINE_CONFIG`
/// when set and not blank, else `config.yaml` in the standalone config home
/// of [`crate::dirs`].
///
/// # Errors
///
/// When the base directories cannot be resolved (no home directory and no
/// override).
pub fn machine_config_path() -> Result<PathBuf, String> {
    if let Some(over) = MACHINE_CONFIG_OVERRIDE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
    {
        return Ok(over);
    }
    if let Some(over) = std::env::var_os(MACHINE_CONFIG_ENV)
        && !over.to_string_lossy().trim().is_empty()
    {
        return Ok(PathBuf::from(over));
    }
    crate::dirs::system()
        .and_then(|dirs| Ok(dirs.standalone("skillwright")?.config().into_path()))
        .map(|dir| dir.join("config.yaml"))
        .map_err(|err| format!("cannot resolve the machine directories: {err}"))
}

/// The machine file's `author:` block, read leniently, and what saving a
/// new editor into the file needs.
#[derive(Debug)]
pub struct MachineAuthor {
    /// The saved editor, when the file names a valid one.
    pub editor: Option<Editor>,
    /// What in the file was ignored, and why. Each one is a warning.
    pub warnings: Vec<String>,
    /// The file to write: the machine file, or its target when it is a
    /// symlink.
    path: PathBuf,
    /// The file's bytes as read; empty when it does not exist.
    source: Vec<u8>,
    /// The etag of `source`, or `None` when the file does not exist.
    etag: Option<String>,
    /// Why the file cannot be saved into, when it could not be read.
    unreadable: Option<String>,
}

impl MachineAuthor {
    /// No saved editor, and nowhere to save one: `reason` is the warning,
    /// and the error from [`MachineAuthor::save`].
    pub fn unavailable(reason: String) -> Self {
        Self {
            editor: None,
            warnings: vec![reason.clone()],
            path: PathBuf::new(),
            source: Vec::new(),
            etag: None,
            unreadable: Some(reason),
        }
    }

    /// Read the `author:` block of the machine file at `path`. Never fails: a
    /// file that cannot be read, or a bad value, is a warning and reads as
    /// no saved editor.
    pub fn read(path: &Path) -> Self {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let mut author = Self {
            editor: None,
            warnings: Vec::new(),
            path,
            source: Vec::new(),
            etag: None,
            unreadable: None,
        };
        match std::fs::read(&author.path) {
            Ok(bytes) => {
                author.etag = Some(etag_write::etag(&bytes));
                author.source = bytes;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return author,
            Err(err) => {
                let reason = format!("cannot read {}: {err}", author.path.display());
                author.warnings.push(reason.clone());
                author.unreadable = Some(reason);
                return author;
            }
        }
        let shown = author.path.display().to_string();
        let block =
            match yaml_splice::parse::parse_mapping(&String::from_utf8_lossy(&author.source)) {
                Ok(mapping) => mapping.get("author").cloned(),
                Err(err) => {
                    author.warnings.push(format!("cannot parse {shown}: {err}"));
                    return author;
                }
            };
        match block {
            None | Some(yaml_splice::Value::Null) => {}
            Some(yaml_splice::Value::Map(block)) => {
                for (key, value) in block.iter() {
                    let editor = value
                        .as_str()
                        .and_then(|v| <Editor as clap::ValueEnum>::from_str(v, false).ok());
                    match (key, editor) {
                        ("editor", Some(editor)) => author.editor = Some(editor),
                        ("editor", None) => author.warnings.push(format!(
                            "`author.editor` in {shown} is not obsidian, code, editor or system; ignoring it"
                        )),
                        (other, _) => author.warnings.push(format!(
                            "unknown key `author.{other}` in {shown}; ignoring it"
                        )),
                    }
                }
            }
            Some(_) => author
                .warnings
                .push(format!("`author` in {shown} is not a mapping; ignoring it")),
        }
        author
    }

    /// Replace the file's `author:` block with `author: {editor: ...}` and
    /// return the path written. Every other key, and every comment, stays as
    /// written. A symlinked file is written through to its target.
    ///
    /// # Errors
    ///
    /// When the file changed since [`MachineAuthor::read`] (nothing is
    /// written), or it cannot be parsed or written.
    pub fn save(&self, editor: Editor) -> Result<PathBuf, String> {
        use yaml_splice::{FieldEdit, Mapping, Value};

        if let Some(reason) = &self.unreadable {
            return Err(reason.clone());
        }
        let shown = self.path.display();
        let mut block = Mapping::new();
        block.insert("editor", Value::Str(editor.as_str().to_string()));
        let edit = FieldEdit::Set {
            key: "author".to_string(),
            value: Value::Map(block),
        };
        let outcome = yaml_splice::edit_yaml(&self.source, &[edit])
            .map_err(|err| format!("editing {shown}: {err}"))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("creating {}: {err}", parent.display()))?;
        }
        match etag_write::write_cas(&self.path, &outcome.bytes, self.etag.as_deref()) {
            Ok(_) => Ok(self.path.clone()),
            Err(
                etag_write::WriteError::Conflict { .. }
                | etag_write::WriteError::ExistsWithoutEtag { .. },
            ) => Err(format!(
                "{shown} changed since skillwright read it; not saved"
            )),
            Err(err) => Err(format!("writing {shown}: {err}")),
        }
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
            resolve_against_cwd("~notes", Path::new("/proj")),
            PathBuf::from("/proj/~notes")
        );
        assert_eq!(resolve_against_cwd("~", Path::new("/work")), home);
        assert_eq!(
            resolve_against_cwd("~/notes", Path::new("/work")),
            home.join("notes")
        );
    }
}
