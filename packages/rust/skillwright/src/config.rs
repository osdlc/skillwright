//! Configuration.
//!
//! **The project file** is `skillwright.yaml`, committed, found by walking
//! up from the working directory — or named outright with `--config`. It
//! holds what belongs to a vault and its distribution: the vault path, the
//! frontmatter namespace, the destinations, the marketplace block.
//!
//! **The machine file** is `<config-home>/skillwright/config.yaml` in the
//! standalone tier of [`crate::dirs`], never committed, relocated by
//! `SKILLWRIGHT_MACHINE_CONFIG`. It has the same shape and stands in, per
//! top-level key, where the project file leaves that key out or empty.
//! Nested values are not merged. `paths:` merges by name instead, and the
//! machine file's entry wins when both define a name.
//!
//! Both may carry a `paths:` map, and every path-valued setting may refer to
//! an entry as `{name}`. Relative paths in either file, `paths:` entries
//! included, resolve against that file's own directory; a flag's path
//! resolves against the shell's `cwd`.
//!
//! A flag overrides the project file, which overrides the machine file —
//! except for `paths:`, where the machine file wins per name. Nothing here is
//! ever written back — this module only reads.
//!
//! This module reads shapes and nothing else. Whether `vault` names a
//! directory that exists, and whether `namespace` is a stem the core
//! accepts, are answered elsewhere — the filesystem, and
//! [`tradecraft_core::Namespace`] in `resolve.rs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use config_layers::file::{self, FileProblem};
use config_layers::machine::MachineConfigFile;
use config_layers::needs::{KeySpec, Needs, Scope};
use config_layers::paths::{self, PathNames};
use serde::{Deserialize, Serialize};

/// The tool's standalone name for its config directory segment and env stem.
pub const TOOL: &str = "skillwright";
/// The command name the configuration messages print.
const COMMAND: &str = "skillwright";
/// The committed project file, looked for from the working directory upward.
pub const PROJECT_FILE: &str = tradecraft_core::PROJECT_FILE;
/// Env var that relocates the machine file, for tests and one-off runs.
pub const MACHINE_CONFIG_ENV: &str = "SKILLWRIGHT_MACHINE_CONFIG";

/// Where `publish` writes when `--to` is not given.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DestinationConfig {
    /// The destination root directory.
    pub root: String,
    /// The target rendered into the root.
    pub target: String,
    /// `project` or `user`; informational.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Where the vault keeps its skills, references and snippets, relative to
/// the vault.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DeclaredFolders {
    /// The skills folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<String>,
    /// The references folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references: Option<String>,
    /// The snippets folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippets: Option<String>,
}

/// What `build` and `check` render; the core owns its shape so any host
/// reads the same block. See [`tradecraft_core::settings`].
pub use tradecraft_core::MarketplaceSettings;

/// The shape of both files: the committed `skillwright.yaml` and the
/// machine file.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SkillwrightConfig {
    /// Named directories; any other path setting refers to one as `{name}`.
    #[serde(default)]
    pub paths: BTreeMap<String, String>,
    /// The vault root. Relative to this file; `--vault` overrides it for
    /// one run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
    /// The frontmatter namespace as a stem — the `sw` of `sw-kind`. Absent
    /// means `sw`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Where the vault keeps skills, references and snippets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_folders: Option<DeclaredFolders>,
    /// Directories a bare `adopt` reads from.
    #[serde(default)]
    pub scan_locations: Vec<String>,
    /// Where `publish` writes when `--to` is not given.
    #[serde(default)]
    pub destinations: Vec<DestinationConfig>,
    /// The marketplace block used by `build` and `check`.
    #[serde(default)]
    pub marketplace: MarketplaceSettings,
    /// Files copied verbatim into a built plugin's own root — a CLI, a wasm
    /// artifact, a launcher: authored on disk, never as a vault note.
    #[serde(default)]
    pub assets: Vec<AssetEntry>,
    /// Canonical hooks files; see `tradecraft_core::HookFileSpec`.
    #[serde(default)]
    pub hooks: Vec<HookFileEntry>,
}

/// One `hooks:` entry, as written in the project file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct HookFileEntry {
    /// A Claude Code `hooks.json`, relative to this file's own directory —
    /// the rule [`AssetEntry::from`] follows.
    pub from: String,
    /// Which plugin's packs carry it. Absent means
    /// `marketplace.default_plugin`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

/// One `assets:` entry, as written in the project file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AssetEntry {
    /// The source file, or a directory whose whole tree lands under `to`.
    /// Relative to this file's own directory, the same rule
    /// [`resolve_configured_path`] applies to `marketplace.out`; `~` and an
    /// absolute path are honoured as written.
    pub from: String,
    /// Where it lands, relative to the built plugin's own root
    /// (`<packs>/<plugin>/`), forward-slashed.
    pub to: String,
    /// Which plugin's root this asset lands in. Absent means
    /// `marketplace.default_plugin`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

fn is_blank(config: &SkillwrightConfig) -> bool {
    config.vault.is_none()
        && config.namespace.is_none()
        && config.declared_folders.is_none()
        && config.scan_locations.is_empty()
        && config.destinations.is_empty()
        && config.marketplace.is_blank()
        && config.assets.is_empty()
        && config.hooks.is_empty()
}

/// The result of reading the configuration for one run.
#[non_exhaustive]
pub enum ConfigLoad {
    /// A configuration was read.
    Loaded {
        /// The parsed configuration.
        config: Box<SkillwrightConfig>,
        /// The configuration file.
        path: PathBuf,
        /// The directory relative paths in it resolve against.
        dir: PathBuf,
    },
    /// No project file above `cwd` and the machine file names no vault.
    /// Mode 1 (flag-only) is still possible.
    Absent {
        /// The configuration file.
        path: PathBuf,
    },
    /// `--config` named a file that isn't there — its own exit code (4),
    /// not 2.
    MissingExplicit {
        /// The configuration file.
        path: PathBuf,
    },
    /// The file exists but could not be read.
    Unreadable {
        /// The configuration file.
        path: PathBuf,
        /// What went wrong.
        detail: String,
    },
    /// The file was read but is not valid configuration.
    Malformed {
        /// The configuration file.
        path: PathBuf,
        /// What went wrong.
        detail: String,
    },
    /// The machine file's default location could not be worked out: the
    /// base directories did not resolve (no home directory and no override,
    /// or an override that is not an absolute path) and
    /// [`MACHINE_CONFIG_ENV`] did not name the file.
    MachineDirs {
        /// What went wrong.
        detail: String,
    },
}

/// A path that stands in for the machine file ahead of
/// [`MACHINE_CONFIG_ENV`]. Tests set it, because moving an environment
/// variable needs `unsafe`.
static MACHINE_CONFIG_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

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

/// `<config-home>/skillwright/config.yaml`, unless [`MACHINE_CONFIG_ENV`]
/// says otherwise. A test override, set with the `test-support` feature,
/// comes before both.
///
/// # Panics
///
/// When neither the override nor [`MACHINE_CONFIG_ENV`] names the file and
/// the base directories cannot be resolved. [`load_config`] reports that
/// case as [`ConfigLoad::MachineDirs`] first, so a caller that runs after a
/// successful load never reaches it.
pub fn machine_config_path() -> PathBuf {
    try_machine_config_path()
        .unwrap_or_else(|err| panic!("skillwright: cannot resolve the machine directories: {err}"))
}

/// [`machine_config_path`], with a base directory that does not resolve
/// returned as an error. A path the override or [`MACHINE_CONFIG_ENV`] names
/// is used without looking the directories up.
fn try_machine_config_path() -> Result<PathBuf, tiered_dirs::Error> {
    let over = MACHINE_CONFIG_OVERRIDE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if let Some(over) = over {
        return Ok(over);
    }
    // The same rule `MachineConfigFile::path` applies: a blank value means
    // unset.
    if let Some(env) = std::env::var_os(MACHINE_CONFIG_ENV)
        && !env.to_string_lossy().trim().is_empty()
    {
        return Ok(PathBuf::from(env));
    }
    let dirs = crate::dirs::system()?;
    let file: MachineConfigFile<SkillwrightConfig> = MachineConfigFile::new(TOOL, &dirs)?;
    Ok(file.with_env(MACHINE_CONFIG_ENV).path())
}

/// Resolve a flag's path as the shell would: absolute as given, `~` or a
/// `~/` prefix against the home directory, anything else relative to
/// `base`. `~name` and `~\` are not expanded: they are relative paths, as in
/// `config_layers::paths::resolve_config_path`. The rule `--vault`, `--to`,
/// `--out` and `--config` share, with `base` the working directory.
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

/// [`resolve_against_cwd`] for a value read from a config file: the same
/// rule, with `base` the file's directory, never the process's working
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

/// The [`ConfigLoad`] for a file that could not be read. `FileProblem` is
/// `#[non_exhaustive]`; a variant added later reports as malformed at `path`.
fn problem_load(problem: FileProblem, path: &Path) -> ConfigLoad {
    match problem {
        FileProblem::Unreadable { path, source } => ConfigLoad::Unreadable {
            path,
            detail: source.to_string(),
        },
        FileProblem::Malformed { path, source } => ConfigLoad::Malformed {
            path,
            detail: source.to_string(),
        },
        other => ConfigLoad::Malformed {
            path: path.to_path_buf(),
            detail: other.to_string(),
        },
    }
}

/// Resolve one path-valued setting from the machine file. A `{name}` token is
/// expanded first, so the result is judged on its final text; an undefined
/// token is left for `finish` to report. A bare word counts as relative to
/// `base`, which `paths::resolve_config_path` alone would leave untouched.
fn resolve_machine_value(raw: &str, names: &PathNames, base: &Path, home: &Path) -> String {
    let Ok(expanded) = paths::substitute_path_names(raw, names, "") else {
        return raw.to_string();
    };
    let looks_like_path =
        expanded.contains('/') || expanded.starts_with('.') || expanded.starts_with('~');
    if expanded.contains("://") || looks_like_path {
        paths::resolve_config_path(&expanded, base, home)
    } else {
        paths::resolve_config_path(&format!("./{expanded}"), base, home)
    }
}

/// Resolve the path-valued settings of the machine file's mapping against the
/// machine file's folder, using [`resolve_machine_value`].
fn resolve_machine_mapping(
    map: &mut serde_json::Map<String, serde_json::Value>,
    names: &PathNames,
    base: &Path,
    home: &Path,
) {
    let resolve = |s: &mut String| *s = resolve_machine_value(s, names, base, home);
    if let Some(serde_json::Value::String(s)) = map.get_mut("vault") {
        resolve(s);
    }
    for key in ["out", "skills_root"] {
        if let Some(serde_json::Value::Object(m)) = map.get_mut("marketplace")
            && let Some(serde_json::Value::String(s)) = m.get_mut(key)
        {
            resolve(s);
        }
    }
    if let Some(serde_json::Value::Array(seq)) = map.get_mut("scan_locations") {
        for item in seq.iter_mut() {
            if let serde_json::Value::String(s) = item {
                resolve(s);
            }
        }
    }
    for (list, key) in [
        ("destinations", "root"),
        ("assets", "from"),
        ("hooks", "from"),
    ] {
        if let Some(serde_json::Value::Array(seq)) = map.get_mut(list) {
            for item in seq.iter_mut() {
                if let serde_json::Value::Object(m) = item
                    && let Some(serde_json::Value::String(s)) = m.get_mut(key)
                {
                    resolve(s);
                }
            }
        }
    }
}

/// Top-level keys whose value is a struct, and whether the field is an
/// `Option` (so a bare `key:` reads as unset). `serde_json` reads a sequence
/// into a struct, so without this check `marketplace: []` would load.
const STRUCT_KEYS: &[(&str, bool)] = &[("declared_folders", true), ("marketplace", false)];

/// The key in [`STRUCT_KEYS`] whose value is not a mapping, if any.
fn non_mapping_struct_key(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Option<&'static str> {
    STRUCT_KEYS.iter().find_map(|&(key, optional)| {
        let v = map.get(key)?;
        let ok = v.is_object() || (optional && v.is_null());
        (!ok).then_some(key)
    })
}

/// Deserialize a merged mapping into the config, failing with the detail text
/// [`annotate_detail`] builds.
fn config_from_mapping(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<SkillwrightConfig, String> {
    let parsed =
        serde_json::from_value::<SkillwrightConfig>(serde_json::Value::Object(map.clone()));
    match parsed {
        Ok(_) if non_mapping_struct_key(map).is_some() => Err(annotate_detail(map, String::new())),
        Ok(c) => Ok(c),
        Err(err) => Err(annotate_detail(map, err.to_string())),
    }
}

fn typed_paths(map: &serde_json::Map<String, serde_json::Value>) -> BTreeMap<String, String> {
    map.get("paths")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// serde_json's struct-deserialize error carries no field path. When the
/// whole mapping fails to parse, re-check each top-level key alone against
/// its own field type and name every mismatch, joined with `; ` — so a caller
/// sees `` `vault`: invalid type: ... `` rather than a bare type-mismatch
/// message.
/// An unknown-field error already names the field on its own and is
/// returned unchanged.
fn annotate_detail(
    map: &serde_json::Map<String, serde_json::Value>,
    base_detail: String,
) -> String {
    let mut issues: Vec<String> = Vec::new();
    macro_rules! check {
        ($key:literal, $ty:ty) => {
            if let Some(v) = map.get($key)
                && let Err(err) = serde_json::from_value::<$ty>(v.clone())
            {
                issues.push(format!("`{}`: {err}", $key));
            }
        };
    }
    if let Some(key) = non_mapping_struct_key(map) {
        issues.push(format!("`{key}`: invalid type: expected a mapping"));
    }
    check!("paths", BTreeMap<String, String>);
    check!("vault", String);
    check!("namespace", String);
    check!("declared_folders", DeclaredFolders);
    check!("scan_locations", Vec<String>);
    check!("destinations", Vec<DestinationConfig>);
    check!("marketplace", MarketplaceSettings);
    check!("assets", Vec<AssetEntry>);
    check!("hooks", Vec<HookFileEntry>);
    if issues.is_empty() {
        base_detail
    } else {
        issues.join("; ")
    }
}

/// Expand every `{name}` token in the path-valued settings, so a reader
/// after this point sees plain paths. A token nothing defines is reported
/// as `Malformed`, naming the setting.
fn finish(
    mut config: SkillwrightConfig,
    dir: &Path,
    path: &Path,
    machine_paths: &BTreeMap<String, String>,
    home: &Path,
) -> ConfigLoad {
    let project_paths = config.paths.clone();
    let names: PathNames = paths::path_names(dir, home, machine_paths, &project_paths);

    macro_rules! expand {
        ($value:expr, $wher:expr) => {
            match paths::substitute_path_names($value, &names, $wher) {
                Ok(v) => v,
                Err(err) => {
                    return ConfigLoad::Malformed {
                        path: path.to_path_buf(),
                        detail: err.to_string(),
                    };
                }
            }
        };
    }

    if let Some(vault) = &config.vault {
        config.vault = Some(expand!(vault, "vault:"));
    }
    // `for` loops, not `.map()`: `expand!`'s early `return` must exit
    // `finish`, not a closure passed to `.map()`.
    let mut scan_locations = Vec::with_capacity(config.scan_locations.len());
    for s in &config.scan_locations {
        scan_locations.push(expand!(s, "scan_locations:"));
    }
    config.scan_locations = scan_locations;

    let mut destinations = Vec::with_capacity(config.destinations.len());
    for d in config.destinations {
        let root = expand!(&d.root, "destinations[].root:");
        destinations.push(DestinationConfig { root, ..d });
    }
    config.destinations = destinations;
    if let Some(out) = &config.marketplace.out {
        config.marketplace.out = Some(expand!(out, "marketplace.out:"));
    }
    if let Some(skills_root) = &config.marketplace.skills_root {
        config.marketplace.skills_root = Some(expand!(skills_root, "marketplace.skills_root:"));
    }
    let mut assets = Vec::with_capacity(config.assets.len());
    for a in config.assets {
        let from = expand!(&a.from, "assets[].from:");
        assets.push(AssetEntry { from, ..a });
    }
    config.assets = assets;
    let mut hooks = Vec::with_capacity(config.hooks.len());
    for h in config.hooks {
        let from = expand!(&h.from, "hooks[].from:");
        hooks.push(HookFileEntry { from, ..h });
    }
    config.hooks = hooks;

    ConfigLoad::Loaded {
        config: Box::new(config),
        path: path.to_path_buf(),
        dir: dir.to_path_buf(),
    }
}

/// Read the configuration for this run: the project file `--config` names
/// or the nearest one above `cwd`, over the machine file.
///
/// With no project file, the machine file alone can carry a run: its
/// `vault` and `destinations` load as the configuration, with the machine
/// file as the path relative entries resolve against. With neither, the
/// result is `Absent` and the caller decides whether flags alone suffice.
///
/// A project file that cannot be read or parsed is reported by path and
/// reason rather than degraded to defaults. The machine file is checked the
/// same way when it exists, for the same reason.
pub fn load_config(config_flag: Option<&str>, cwd: &Path) -> ConfigLoad {
    match try_machine_config_path() {
        Ok(machine_path) => load_config_at(config_flag, cwd, machine_path, &home_dir()),
        Err(err) => ConfigLoad::MachineDirs {
            detail: err.to_string(),
        },
    }
}

/// [`load_config`] with the machine file's path and the home directory
/// given, so a test needs no environment variables.
fn load_config_at(
    config_flag: Option<&str>,
    cwd: &Path,
    machine_path: PathBuf,
    home: &Path,
) -> ConfigLoad {
    // `exists()` is true for a directory too, so a path naming a directory
    // reaches the unreadable branch below rather than reading as absent.
    let machine_exists = machine_path.exists();
    let machine_mapping = if machine_exists {
        match file::read_mapping(&machine_path) {
            Ok(m) => m,
            Err(problem) => return problem_load(problem, &machine_path),
        }
    } else {
        serde_json::Map::new()
    };
    // Checked once, against the same schema the layered read uses below: a
    // typo in the machine file must not read as "no configuration, run
    // init".
    if machine_exists && let Err(detail) = config_from_mapping(&machine_mapping) {
        return ConfigLoad::Malformed {
            path: machine_path,
            detail,
        };
    }

    let explicit = config_flag.is_some();
    let found = match config_flag {
        Some(flag) => Some(resolve_against_cwd(flag, cwd)),
        None => file::find_up(cwd, PROJECT_FILE),
    };

    let Some(project_path) = found else {
        let machine_config: SkillwrightConfig = if machine_exists {
            serde_json::from_value(serde_json::Value::Object(machine_mapping.clone()))
                .unwrap_or_default()
        } else {
            SkillwrightConfig::default()
        };
        if !machine_exists || is_blank(&machine_config) {
            return ConfigLoad::Absent { path: machine_path };
        }
        let machine_dir = machine_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let machine_paths = machine_config.paths.clone();
        return finish(
            machine_config,
            &machine_dir,
            &machine_path,
            &machine_paths,
            home,
        );
    };

    if explicit && !project_path.exists() {
        return ConfigLoad::MissingExplicit { path: project_path };
    }

    let project_mapping = match file::read_mapping(&project_path) {
        Ok(m) => m,
        Err(problem) => return problem_load(problem, &project_path),
    };

    let machine_dir = machine_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let project_dir = project_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let mut machine_mapping = machine_mapping;

    // `paths:` merges by NAME (machine wins per name, in `finish`'s `path_names` call) rather
    // than as a whole top-level key, so it is carried through the overlay untouched and the
    // deserialized result is overwritten with the project's own value afterwards.
    let project_paths: BTreeMap<String, String> = project_mapping
        .get("paths")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    // A relative entry in the machine file's `paths:` resolves against the
    // machine file's directory, like every other path in that file; `finish`
    // would otherwise use the project's.
    let no_names = PathNames::new();
    let machine_paths: BTreeMap<String, String> = typed_paths(&machine_mapping)
        .into_iter()
        .map(|(name, raw)| {
            let resolved = resolve_machine_value(&raw, &no_names, &machine_dir, home);
            (name, resolved)
        })
        .collect();

    // A value taken from the project file resolves later, against the
    // project's own directory (see `resolve.rs`). A value borrowed from the
    // machine file must be resolved here, against the machine file's
    // directory, or the caller would resolve it against the wrong base.
    let names = paths::path_names(&project_dir, home, &machine_paths, &project_paths);
    resolve_machine_mapping(&mut machine_mapping, &names, &machine_dir, home);

    let merged = file::overlay_top_level(project_mapping, &machine_mapping);
    let mut config: SkillwrightConfig = match config_from_mapping(&merged) {
        Ok(c) => c,
        Err(detail) => {
            return ConfigLoad::Malformed {
                path: project_path,
                detail,
            };
        }
    };
    config.paths = project_paths;

    finish(config, &project_dir, &project_path, &machine_paths, home)
}

/// The needs check every verb asks before it starts. Each key's description is
/// the text shown when the key is missing.
pub static NEEDS: LazyLock<Needs> = LazyLock::new(|| {
    static KEYS: &[KeySpec] = &[
        KeySpec {
            key: "vault",
            scope: Scope::Both,
            description: "The vault root. Relative to this file; `--vault` overrides it for one run.",
        },
        KeySpec {
            key: "namespace",
            scope: Scope::Both,
            description: "The frontmatter namespace as a stem — the `sw` of `sw-kind`. Absent means `sw`.",
        },
        KeySpec {
            key: "declared_folders",
            scope: Scope::Both,
            description: "Where the vault keeps its skills, references and snippets, relative to the vault.",
        },
        KeySpec {
            key: "scan_locations",
            scope: Scope::Both,
            description: "Directories a bare `adopt` reads from.",
        },
        KeySpec {
            key: "destinations",
            scope: Scope::Both,
            description: "Where `publish` writes when `--to` is not given.",
        },
        KeySpec {
            key: "marketplace",
            scope: Scope::Both,
            description: "What `build` and `check` render.",
        },
        KeySpec {
            key: "paths",
            scope: Scope::Both,
            description: "Named directories; any other path setting refers to one as `{name}`.",
        },
        KeySpec {
            key: "assets",
            scope: Scope::Both,
            description: "Files copied verbatim into a built plugin's own root — a CLI, a wasm artifact, a launcher.",
        },
        KeySpec {
            key: "hooks",
            scope: Scope::Both,
            description: "Canonical hooks files, one per plugin, rendered into each harness's pack with a gap report.",
        },
        KeySpec {
            key: "marketplace.name",
            scope: Scope::Both,
            description: "The marketplace's name when the vault holds no marketplace note; a fallback.",
        },
        KeySpec {
            key: "marketplace.display_name",
            scope: Scope::Both,
            description: "Codex's `interface.displayName`.",
        },
        KeySpec {
            key: "marketplace.owner",
            scope: Scope::Both,
            description: "Claude's marketplace `owner`; free-shaped.",
        },
        KeySpec {
            key: "marketplace.author",
            scope: Scope::Both,
            description: "Default plugin author; falls back to `owner`.",
        },
        KeySpec {
            key: "marketplace.license",
            scope: Scope::Both,
            description: "Default plugin license, as an SPDX identifier; a marketplace note's `sw-license` wins over it.",
        },
        KeySpec {
            key: "marketplace.default_plugin",
            scope: Scope::Both,
            description: "The plugin a note with no `sw-plugin` belongs to. `build` and `check` need it.",
        },
        KeySpec {
            key: "marketplace.plugins",
            scope: Scope::Both,
            description: "Per-plugin manifest overrides, keyed by plugin name.",
        },
        KeySpec {
            key: "marketplace.out",
            scope: Scope::Both,
            description: "The marketplace directory — one directory holding one marketplace, which every harness reads from. `.` makes the repository itself the marketplace. `build` and `check` need it; `--out` overrides.",
        },
        KeySpec {
            key: "marketplace.skills_root",
            scope: Scope::Both,
            description: "The content root to scan and adopt under. Absent means the vault root.",
        },
        KeySpec {
            key: "marketplace.harnesses",
            scope: Scope::Both,
            description: "Which harnesses the marketplace is built for. Absent means all of them.",
        },
        KeySpec {
            key: "marketplace.require_descriptions",
            scope: Scope::Both,
            description: "Refuse `build` and `check` when a note has a `name` and no `description`. Absent or `false` publishes a filler and warns. A plugin or marketplace note whose description the configuration supplies is neither warned about nor refused.",
        },
        KeySpec {
            key: "marketplace.launcher_path",
            scope: Scope::Both,
            description: "The plugin's launcher, relative to the plugin's root. Skills call it as `${CLAUDE_PLUGIN_ROOT}/<launcher_path>`; for Codex, Cursor and Pi the build rewrites that call to the launcher's path relative to the skill's folder. There is no PATH fallback. Absent turns the rewrite off.",
        },
    ];
    Needs::new(
        COMMAND,
        PROJECT_FILE,
        KEYS,
        machine_config_path(),
        Some("the package README shows the full shape"),
    )
});

#[cfg(test)]
mod tests {
    use super::*;

    /// A `~` value borrowed from the machine file expands to the home
    /// directory, not to the machine file's directory.
    #[test]
    fn a_tilde_in_the_machine_file_expands_to_home_under_a_project_file() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let machine_path = tmp.path().join("machine").join("config.yaml");
        std::fs::create_dir_all(machine_path.parent().unwrap()).unwrap();
        std::fs::write(
            &machine_path,
            "vault: ~/notes\nscan_locations: [~/skills]\ndestinations:\n  - root: ~/out\n    target: claude\n",
        )
        .unwrap();
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(PROJECT_FILE), "namespace: sw\n").unwrap();

        let ConfigLoad::Loaded { config, .. } = load_config_at(None, &project, machine_path, &home)
        else {
            panic!("expected the layered config to load");
        };
        let as_path = |s: &str| PathBuf::from(s);
        assert_eq!(
            config.vault.as_deref().map(as_path),
            Some(home.join("notes"))
        );
        assert_eq!(
            config
                .scan_locations
                .iter()
                .map(|s| as_path(s))
                .collect::<Vec<_>>(),
            vec![home.join("skills")]
        );
        assert_eq!(as_path(&config.destinations[0].root), home.join("out"));
    }

    /// `assets[].from` and `hooks[].from` borrowed from the machine file
    /// resolve against the machine file's directory, as they do when no
    /// project file exists — not against the project's.
    #[test]
    fn machine_asset_and_hook_sources_resolve_against_the_machine_dir_under_a_project_file() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let machine_dir = tmp.path().join("machine");
        let machine_path = machine_dir.join("config.yaml");
        std::fs::create_dir_all(&machine_dir).unwrap();
        std::fs::write(
            &machine_path,
            "assets:\n  - from: ./bin/cli\n    to: cli/cli\nhooks:\n  - from: ./hooks.json\n",
        )
        .unwrap();
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(PROJECT_FILE), "namespace: sw\n").unwrap();

        let ConfigLoad::Loaded { config, dir, .. } =
            load_config_at(None, &project, machine_path, &home)
        else {
            panic!("expected the layered config to load");
        };
        assert_eq!(
            resolve_configured_path(&config.assets[0].from, &dir),
            machine_dir.join("bin/cli")
        );
        assert_eq!(
            resolve_configured_path(&config.hooks[0].from, &dir),
            machine_dir.join("hooks.json")
        );
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

    /// The same machine-file `vault: ~archive` names one directory whether
    /// or not a project file exists.
    #[test]
    fn a_tilde_name_in_the_machine_file_resolves_the_same_with_or_without_a_project_file() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let machine_dir = tmp.path().join("machine");
        let machine_path = machine_dir.join("config.yaml");
        std::fs::create_dir_all(&machine_dir).unwrap();
        std::fs::write(&machine_path, "vault: ~archive\n").unwrap();
        let bare = tmp.path().join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(PROJECT_FILE), "namespace: sw\n").unwrap();

        let vault_of = |cwd: &Path| {
            let ConfigLoad::Loaded { config, dir, .. } =
                load_config_at(None, cwd, machine_path.clone(), &home)
            else {
                panic!("expected a config to load from {}", cwd.display());
            };
            resolve_configured_path(config.vault.as_deref().unwrap(), &dir)
        };
        assert_eq!(vault_of(&bare), machine_dir.join("~archive"));
        assert_eq!(vault_of(&project), machine_dir.join("~archive"));
    }
}
