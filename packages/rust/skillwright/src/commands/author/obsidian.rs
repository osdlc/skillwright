//! Obsidian's side of `author`: where `obsidian.json` lives, which vaults it
//! lists, and the `obsidian://` links that open them. Read only: nothing here
//! writes Obsidian's files.

use std::path::{Path, PathBuf};

use super::{EnvFacts, Os};

const FLATPAK_ID: &str = "md.obsidian.Obsidian";

/// Where Obsidian keeps `obsidian.json` on this OS. On Linux the flatpak
/// location is used when only it exists.
pub fn config_path(facts: &EnvFacts) -> Option<PathBuf> {
    let home = facts.home();
    match facts.os {
        Os::Mac => home.map(|h| h.join("Library/Application Support/obsidian/obsidian.json")),
        Os::Linux => {
            let native = facts
                .var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join(".config")))?
                .join("obsidian/obsidian.json");
            let flatpak = home.map(|h| {
                h.join(".var/app")
                    .join(FLATPAK_ID)
                    .join("config/obsidian/obsidian.json")
            });
            match flatpak {
                Some(f) if !native.exists() && f.exists() => Some(f),
                _ => Some(native),
            }
        }
    }
}

/// Whether Obsidian knows a folder as a vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Registration {
    /// This Obsidian vault is the folder or one of its ancestors.
    Registered {
        /// The vault's folder, as `obsidian.json` stores it.
        vault: PathBuf,
        /// The target under that stored path: what Obsidian can match.
        path: PathBuf,
    },
    /// `obsidian.json` was read and no vault holds the folder.
    NotRegistered,
    /// `obsidian.json` could not be read; treated as not registered.
    Unknown {
        /// Why.
        reason: String,
    },
}

/// The vault folders `obsidian.json` lists. Its shape is
/// `{"vaults": {"<id>": {"path": "...", ...}}}`.
///
/// # Errors
///
/// When the text is not JSON or not that shape.
pub fn parse_vaults(json: &str) -> Result<Vec<PathBuf>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let vaults = value
        .get("vaults")
        .and_then(serde_json::Value::as_object)
        .ok_or("no `vaults` object")?;
    vaults
        .values()
        .map(|entry| {
            entry
                .get("path")
                .and_then(serde_json::Value::as_str)
                .map(PathBuf::from)
                .ok_or_else(|| "a vault entry has no `path` string".to_string())
        })
        .collect()
}

/// The innermost vault that holds `target`, comparing canonical paths, and
/// `target` rewritten under that vault's stored path.
pub fn owning_vault(vaults: &[PathBuf], target: &Path) -> Option<(PathBuf, PathBuf)> {
    let target = canonical(target);
    vaults
        .iter()
        .filter_map(|vault| {
            let rest = target.strip_prefix(canonical(vault)).ok()?;
            Some((vault, rest.to_path_buf()))
        })
        .min_by_key(|(_, rest)| rest.components().count())
        .map(|(vault, rest)| {
            // `join("")` would add a trailing separator.
            let path = if rest.as_os_str().is_empty() {
                vault.clone()
            } else {
                vault.join(rest)
            };
            (vault.clone(), path)
        })
}

/// Read `config` (Obsidian's `obsidian.json`) and say whether `target` is
/// inside a registered vault. Never fails: a problem is
/// [`Registration::Unknown`].
pub fn registration(config: &Path, target: &Path) -> Registration {
    let text = match std::fs::read_to_string(config) {
        Ok(text) => text,
        Err(err) => {
            return Registration::Unknown {
                reason: format!("cannot read {}: {err}", config.display()),
            };
        }
    };
    match parse_vaults(&text) {
        Ok(vaults) => match owning_vault(&vaults, target) {
            Some((vault, path)) => Registration::Registered { vault, path },
            None => Registration::NotRegistered,
        },
        Err(err) => Registration::Unknown {
            reason: format!("unexpected content in {}: {err}", config.display()),
        },
    }
}

pub(super) fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// `text` percent-encoded the way JavaScript's `encodeURIComponent` does it,
/// minus its `!*'()`: everything but `A-Z a-z 0-9 - _ . ~` is encoded.
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `obsidian://open?path=<path>`, the link that opens a folder or note in
/// the vault holding it.
pub fn open_uri(path: &Path) -> String {
    format!(
        "obsidian://open?path={}",
        percent_encode(&path.to_string_lossy())
    )
}

/// Obsidian's vault picker, where a person adds a folder as a vault.
pub const PICKER_URI: &str = "obsidian://choose-vault";
