//! Obsidian's side of `author`: where `obsidian.json` lives, which vaults it
//! lists, the `obsidian://` links that open them, and [`VaultList::add`],
//! the one write. That write is the caller's to gate: only on a person's
//! explicit choice, and only while Obsidian is closed. The file's format is
//! Obsidian-internal, not a public API.

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

/// The `pgrep` commands that find a running Obsidian. Each exits 0 when it
/// finds one and 1 when it finds none.
pub fn running_checks(os: Os) -> Vec<Vec<String>> {
    let argv = |args: &[&str]| args.iter().map(|a| (*a).to_string()).collect();
    match os {
        Os::Mac => vec![argv(&["pgrep", "-x", "Obsidian"])],
        Os::Linux => vec![
            argv(&["pgrep", "-x", "obsidian"]),
            argv(&["pgrep", "-f", FLATPAK_ID]),
        ],
    }
}

/// `obsidian.json` as read, kept so that adding a vault can refuse to
/// overwrite a file that changed since.
#[derive(Debug, Clone)]
pub struct VaultList {
    /// The file to write: the config, or its target when it is a symlink.
    path: PathBuf,
    /// The bytes as read.
    bytes: Vec<u8>,
    /// The etag of `bytes`.
    etag: String,
    /// The whole file as a JSON value. Its `vaults` is an object.
    value: serde_json::Value,
}

/// What [`VaultList::add`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    /// The new vault's id.
    pub id: String,
    /// The file it was added to.
    pub config: PathBuf,
    /// The copy of the file as it was before.
    pub backup: PathBuf,
}

impl VaultList {
    /// Read `config` for [`VaultList::add`].
    ///
    /// # Errors
    ///
    /// When it cannot be read, or is not the shape [`parse_vaults`] accepts.
    pub fn read(config: &Path) -> Result<Self, String> {
        let path = canonical(config);
        let bytes =
            std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        parse_vaults(&text)
            .map_err(|e| format!("unexpected content in {}: {e}", path.display()))?;
        let value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        Ok(Self {
            etag: etag_write::etag(&bytes),
            path,
            bytes,
            value,
        })
    }

    /// The file a write goes to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `obsidian.json.bak`, beside the file.
    pub fn backup_path(&self) -> PathBuf {
        let mut name = self.path.file_name().unwrap_or_default().to_os_string();
        name.push(".bak");
        self.path.with_file_name(name)
    }

    /// A vault id not yet in the file: 16 lowercase hex characters, the only
    /// form Obsidian keeps.
    ///
    /// # Errors
    ///
    /// When `random` fails, or keeps returning ids already taken.
    pub fn new_id(
        &self,
        random: &mut dyn FnMut() -> Result<[u8; 8], String>,
    ) -> Result<String, String> {
        let taken = self.vaults();
        for _ in 0..16 {
            let id: String = random()?.iter().map(|b| format!("{b:02x}")).collect();
            if !taken.contains_key(&id) {
                return Ok(id);
            }
        }
        Err("could not pick an unused vault id".to_string())
    }

    fn vaults(&self) -> &serde_json::Map<String, serde_json::Value> {
        static EMPTY: std::sync::LazyLock<serde_json::Map<String, serde_json::Value>> =
            std::sync::LazyLock::new(serde_json::Map::new);
        self.value
            .get("vaults")
            .and_then(serde_json::Value::as_object)
            .unwrap_or(&EMPTY)
    }

    /// The file with `vaults.<id> = {"path": folder, "ts": ts}` added and
    /// everything else as it was.
    pub fn with_vault(&self, id: &str, folder: &Path, ts: i64) -> Vec<u8> {
        let mut value = self.value.clone();
        if let Some(vaults) = value
            .get_mut("vaults")
            .and_then(serde_json::Value::as_object_mut)
        {
            vaults.insert(
                id.to_string(),
                serde_json::json!({"path": folder.to_string_lossy(), "ts": ts}),
            );
        }
        value.to_string().into_bytes()
    }

    /// Add `folder` as a vault with a new id and `ts` (milliseconds since the
    /// epoch). The file is first copied to [`VaultList::backup_path`].
    ///
    /// # Errors
    ///
    /// When the file changed since [`VaultList::read`] (nothing is written),
    /// or a read or write fails.
    pub fn add(
        &self,
        folder: &Path,
        ts: i64,
        random: &mut dyn FnMut() -> Result<[u8; 8], String>,
    ) -> Result<Added, String> {
        let shown = self.path.display();
        let changed = || format!("{shown} changed since skillwright read it; nothing written");
        let current = std::fs::read(&self.path).map_err(|e| format!("cannot read {shown}: {e}"))?;
        if etag_write::etag(&current) != self.etag {
            return Err(changed());
        }
        let id = self.new_id(random)?;
        let backup = self.backup_path();
        etag_write::write_atomic(&backup, &self.bytes)
            .map_err(|e| format!("writing {}: {e}", backup.display()))?;
        match etag_write::write_cas(
            &self.path,
            &self.with_vault(&id, folder, ts),
            Some(&self.etag),
        ) {
            Ok(_) => Ok(Added {
                id,
                config: self.path.clone(),
                backup,
            }),
            Err(etag_write::WriteError::Conflict { .. }) => Err(changed()),
            Err(err) => Err(format!("writing {shown}: {err}")),
        }
    }
}

/// 8 random bytes from `/dev/urandom`.
///
/// # Errors
///
/// When it cannot be read.
pub fn random_bytes() -> Result<[u8; 8], String> {
    use std::io::Read;
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("cannot read /dev/urandom: {e}"))?;
    Ok(bytes)
}
