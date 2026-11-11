//! `skillwright.md`, the root file: one note that is both the marketplace's
//! own note and the project's configuration.
//!
//! Its frontmatter's top-level keys are the marketplace's fields. Settings for
//! how skillwright runs sit under `build:`. The body is ignored. The folder
//! holding the file is the vault, and the build writes beside it.
//!
//! The CLI and other hosts read the file through [`parse_root_file`], so they
//! share one shape and one mapping onto a build's [`MarketplaceConfig`].
//! Finding the file stays with the caller: this module parses text and never
//! touches a filesystem.
//!
//! An unknown key is a warning, never a refusal, and is otherwise ignored. A
//! known key with a value of the wrong shape is refused, naming the key.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::export::MarketplaceConfig;
use crate::frontmatter::ContainerIdentity;
use crate::skill::split_frontmatter;

/// The root file's name.
pub const ROOT_FILE: &str = "skillwright.md";

/// The marketplace's own fields: the root file's top-level keys.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MarketplaceFields {
    /// The marketplace's name. Absent means the root folder's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// One line on what the marketplace holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The marketplace's manifest fields, flattened: `owner`, `author`,
    /// `display_name`, `license`, `version`, `repository` and `homepage` sit
    /// at the top level, as the root file writes them. Deserializing refuses
    /// `keywords` and `category`, which a marketplace does not read.
    #[serde(flatten, deserialize_with = "crate::export::marketplace_identity")]
    pub identity: ContainerIdentity,
    /// Which harnesses get what the marketplace holds. Inherited by every
    /// plugin and member with none of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<String>>,
    /// The harness blocks (`claude:`, `codex:`, `cursor:`, `pi:`), keyed by
    /// harness. Inherited by every plugin, key by key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub blocks: BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
}

/// Where `publish` writes when `--to` is not given.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DestinationSetting {
    /// The destination root directory, relative to the root file.
    pub root: String,
    /// The target rendered into the root.
    pub target: String,
    /// `project` or `user`. Informational: nothing reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// One `build.assets` entry: a file or folder copied into a plugin's root.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AssetSetting {
    /// The source file or folder, relative to the root file.
    pub from: String,
    /// Where it lands, relative to the built plugin's own root.
    pub to: String,
    /// Which plugin it lands in. Absent means the marketplace's own plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

/// One `build.hooks` entry: a Claude Code `hooks.json` rendered per harness.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HookSetting {
    /// The file, relative to the root file.
    pub from: String,
    /// Which plugin's packs carry it. Absent means the marketplace's own
    /// plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

/// The `build:` block: how skillwright runs, never what lands in a manifest.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BuildSettings {
    /// Where the harness trees are written, relative to the root file.
    /// Required, here or as `--out`: there is no default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out: Option<String>,
    /// Which harnesses to build. Absent means all of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harnesses: Option<Vec<String>>,
    /// The frontmatter namespace stem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// The plugin's launcher, relative to the plugin's root. See
    /// [`MarketplaceConfig::launcher_path`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher_path: Option<String>,
    /// How `bump` writes a version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<BumpSettings>,
    /// Where `publish` writes when `--to` is not given. Empty means each
    /// harness's user skill folder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub destinations: Vec<DestinationSetting>,
    /// Folders a bare `adopt` reads from. Empty means each harness's user
    /// skill folder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scan_locations: Vec<String>,
    /// Files copied into a plugin's root besides each plugin's `assets/`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<AssetSetting>,
    /// Canonical hooks files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<HookSetting>,
    /// Globs of what never ships, relative to the root file. Only this
    /// file's own: a workspace's globs are not inherited into this list, but
    /// added to it, read from the workspace folder. Experimental.
    #[serde(
        default,
        deserialize_with = "one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub exclude: Vec<String>,
    /// Globs that stand in for the conventional folders. Experimental.
    #[serde(default, skip_serializing_if = "FolderPatterns::is_empty")]
    pub patterns: FolderPatterns,
}

/// `build.patterns`: for each conventional folder, the globs that name what
/// it holds. See [`crate::patterns`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderPatterns {
    /// Each match is a plugin folder.
    #[serde(
        deserialize_with = "one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub plugins: Vec<String>,
    /// Each match is a skill: a note, or a folder holding its entry.
    #[serde(
        deserialize_with = "one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub skills: Vec<String>,
    /// Each match is an agent.
    #[serde(
        deserialize_with = "one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub agents: Vec<String>,
    /// Each match is a command.
    #[serde(
        deserialize_with = "one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub commands: Vec<String>,
    /// Each match is a hook note.
    #[serde(
        deserialize_with = "one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub hooks: Vec<String>,
    /// Each match is a plugin's assets folder.
    #[serde(
        deserialize_with = "one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub assets: Vec<String>,
}

impl FolderPatterns {
    /// Whether no pattern is set.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Each key with its globs, in the order a path is claimed.
    pub(crate) fn kinds(&self) -> [(&'static str, &[String]); 6] {
        [
            ("assets", &self.assets),
            ("skills", &self.skills),
            ("agents", &self.agents),
            ("commands", &self.commands),
            ("hooks", &self.hooks),
            ("plugins", &self.plugins),
        ]
    }

    /// Every glob, whichever key holds it.
    pub(crate) fn globs(&self) -> impl Iterator<Item = &String> {
        self.kinds().into_iter().flat_map(|(_, globs)| globs)
    }

    /// Fill each key this leaves unset from `shared`.
    fn inherit(&mut self, shared: &Self) {
        let keys = [
            (&mut self.plugins, &shared.plugins),
            (&mut self.skills, &shared.skills),
            (&mut self.agents, &shared.agents),
            (&mut self.commands, &shared.commands),
            (&mut self.hooks, &shared.hooks),
            (&mut self.assets, &shared.assets),
        ];
        for (own, shared) in keys {
            if own.is_empty() {
                own.clone_from(shared);
            }
        }
    }
}

/// A string or a list of strings, as a list.
fn one_or_many<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(glob) => vec![glob],
        OneOrMany::Many(globs) => globs,
    })
}

/// `build.bump`: how `bump` writes a version.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BumpSettings {
    /// How a bump writes the patch number.
    #[serde(default)]
    pub patch: PatchScheme,
}

/// How `bump` writes the patch number of the version it sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatchScheme {
    /// Semver: a patch bump adds one, a minor or major bump resets it to 0.
    #[default]
    Increment,
    /// Every bump sets the patch to the UTC time as `YYYYMMDDHHMMSS`, or to
    /// one more than the current patch when that is larger.
    Timestamp,
}

/// A parsed root file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RootFile {
    /// The marketplace's own fields.
    #[serde(default)]
    pub marketplace: MarketplaceFields,
    /// `workspace: true`: this file holds shared defaults for every
    /// `marketplaces/<m>/skillwright.md` below it and builds them all.
    #[serde(default)]
    pub workspace: bool,
    /// The `build:` block.
    #[serde(default)]
    pub build: BuildSettings,
    /// One line per key the file carries that skillwright does not read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// A key whose value has the wrong shape, named.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{key}` is not {what}: {detail}")]
pub struct SettingsError {
    /// The key.
    pub key: String,
    /// What the key must be.
    pub what: &'static str,
    /// Why the value does not fit.
    pub detail: String,
}

/// The marketplace fields the root file reads: the key table's marketplace
/// keys, less the harness blocks, in table order.
pub fn marketplace_keys() -> impl Iterator<Item = &'static str> {
    crate::keys::KEYS
        .iter()
        .filter(|spec| {
            spec.kinds
                .contains(&crate::frontmatter::EntityKind::Marketplace)
                && spec.shape != crate::keys::Shape::HarnessBlock
        })
        .map(|spec| spec.key)
}

/// The keys of the `build:` block.
pub const BUILD_KEYS: &[&str] = &[
    "out",
    "harnesses",
    "namespace",
    "launcher_path",
    "bump",
    "destinations",
    "scan_locations",
    "assets",
    "hooks",
    "exclude",
    "patterns",
];

/// A YAML value read as `T` through its JSON equivalent, so free-shaped
/// fields come out as the JSON a manifest carries.
fn from_yaml<T: DeserializeOwned>(value: &serde_yaml::Value) -> Result<T, serde_json::Error> {
    serde_json::to_value(value).and_then(serde_json::from_value)
}

/// A new root file naming the marketplace and its output folder, the file
/// `skillwright init` writes.
#[must_use]
pub fn compose_root_file(name: &str, out: &str) -> String {
    use serde_yaml::{Mapping, Value};

    let string = |value: &str| Value::String(value.to_owned());
    let mut build = Mapping::new();
    build.insert(string("out"), string(out));
    let mut map = Mapping::new();
    map.insert(string("name"), string(name));
    map.insert(string("build"), Value::Mapping(build));
    let yaml = serde_yaml::to_string(&map).expect("a mapping of strings serializes");
    format!("---\n{yaml}---\n")
}

/// Whether a file split into `frontmatter` and `body` opened frontmatter it
/// never closed: read as a body, its settings would silently be lost.
fn unterminated(frontmatter: &str, body: &str) -> bool {
    let first = body.trim_start_matches('\u{feff}').lines().next();
    frontmatter.is_empty() && first.is_some_and(|line| line.trim_end() == "---")
}

/// A root file's frontmatter as a YAML mapping. No frontmatter is an empty one.
fn frontmatter_map(text: &str) -> Result<serde_yaml::Mapping, SettingsError> {
    let (frontmatter, body) = split_frontmatter(text);
    if unterminated(frontmatter, body) {
        return Err(SettingsError {
            key: ROOT_FILE.to_owned(),
            what: "frontmatter closed with `---`",
            detail: "it opens with `---` and never closes".to_owned(),
        });
    }
    let doc: serde_yaml::Value = if frontmatter.trim().is_empty() {
        serde_yaml::Value::Null
    } else {
        serde_yaml::from_str(frontmatter).map_err(|err| SettingsError {
            key: ROOT_FILE.to_owned(),
            what: "valid YAML frontmatter",
            detail: err.to_string(),
        })?
    };
    match doc {
        serde_yaml::Value::Null => Ok(serde_yaml::Mapping::new()),
        serde_yaml::Value::Mapping(map) => Ok(map),
        _ => Err(SettingsError {
            key: ROOT_FILE.to_owned(),
            what: "a frontmatter mapping",
            detail: "the frontmatter is not a mapping of keys".to_owned(),
        }),
    }
}

/// Only the part of a root file that decides which files are notes and where
/// they go: `workspace`, `build.exclude` and `build.patterns`. Every other
/// key is left at its default and is not checked, so a bad value elsewhere
/// in the file is for the verbs that read it to refuse.
///
/// # Errors
///
/// Frontmatter that is not a YAML mapping, and a `workspace`, `build`,
/// `build.exclude` or `build.patterns` of the wrong shape.
pub(crate) fn parse_root_layout(text: &str) -> Result<RootFile, SettingsError> {
    let mut root = RootFile::default();
    for (key, value) in frontmatter_map(text)? {
        match key.as_str() {
            Some("workspace") => root.workspace = field("workspace", &value, "true or false")?,
            Some("build") => {
                for (key, value) in build_map(&value)?.into_iter().flatten() {
                    match key.as_str() {
                        Some("exclude") => root.build.exclude = globs("build.exclude", value)?,
                        Some("patterns") => {
                            root.build.patterns = folder_patterns(value, &mut Vec::new())?;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(root)
}

/// Read a root file's text. A file with no frontmatter is a root file with
/// every default.
///
/// # Errors
///
/// Frontmatter that is not a YAML mapping, and a known key whose value has
/// the wrong shape.
pub fn parse_root_file(text: &str) -> Result<RootFile, SettingsError> {
    let map = frontmatter_map(text)?;

    let mut root = RootFile::default();
    let mut marketplace = serde_yaml::Mapping::new();
    for (key, value) in map {
        let Some(key) = key.as_str().map(str::to_owned) else {
            root.warnings
                .push("a frontmatter key that is not a string is ignored".to_owned());
            continue;
        };
        match key.as_str() {
            "workspace" => root.workspace = field(&key, &value, "true or false")?,
            "build" => root.build = build(&value, &mut root.warnings)?,
            known if marketplace_keys().any(|key| key == known) => {
                check_shape(known, &value)?;
                marketplace.insert(serde_yaml::Value::String(key), value);
            }
            harness if crate::keys::HARNESS_BLOCKS.contains(&harness) => {
                let block: Option<serde_json::Map<String, serde_json::Value>> =
                    field(&key, &value, "a mapping of that harness's own keys")?;
                if let Some(block) = block {
                    let known = serde_json::Map::from_iter([(
                        key.clone(),
                        serde_json::Value::Object(block.clone()),
                    )]);
                    let unknown = crate::discover::unknown_container_block_keys(
                        &known,
                        crate::frontmatter::EntityKind::Marketplace,
                    );
                    for unknown in &unknown {
                        root.warnings.push(format!(
                            "`{unknown}` reaches no file that harness reads; it is ignored"
                        ));
                    }
                    let block = crate::harness::field_types::without_unset(&key, block, &unknown);
                    root.marketplace.blocks.insert(key, block);
                }
            }
            _ => root.warnings.push(format!(
                "`{key}` is not a key skillwright reads in {ROOT_FILE}; it is ignored"
            )),
        }
    }
    let blocks = std::mem::take(&mut root.marketplace.blocks);
    root.marketplace = field(
        "marketplace fields",
        &serde_yaml::Value::Mapping(marketplace),
        "the marketplace's fields",
    )?;
    root.marketplace.blocks = blocks;
    if let Some(unknown) = root
        .marketplace
        .targets
        .iter()
        .flatten()
        .find(|target| !crate::publish::KNOWN_TARGETS.contains(&target.as_str()))
    {
        return Err(SettingsError {
            key: "targets".to_owned(),
            what: "harness names",
            detail: format!("`{unknown}`"),
        });
    }
    Ok(root)
}

/// Refuses a marketplace key whose value is not the shape the key table
/// gives it, as a marketplace note's read does, or that has no JSON form to
/// check, such as a mapping with a list for a key. A key set to nothing is
/// unset, as [`field`] reads it.
fn check_shape(key: &str, value: &serde_yaml::Value) -> Result<(), SettingsError> {
    if value.is_null() {
        return Ok(());
    }
    let json = serde_json::to_value(value).map_err(|err| SettingsError {
        key: key.to_owned(),
        what: "a value with a JSON form",
        detail: err.to_string(),
    })?;
    crate::keys::check_shape(crate::frontmatter::EntityKind::Marketplace, key, &json).map_err(
        |refusal| SettingsError {
            key: key.to_owned(),
            what: refusal.what,
            detail: settings_detail(&refusal.detail),
        },
    )
}

/// A [`crate::keys::ShapeRefusal`]'s detail as it reads after `is not <what>:`:
/// `, not a string` becomes `it is a string`.
fn settings_detail(detail: &str) -> String {
    let detail = detail.trim_start_matches([',', ';', ' ']);
    let detail = detail.strip_prefix("but ").unwrap_or(detail);
    match detail.strip_prefix("not ") {
        Some(found) => format!("it is {found}"),
        None => detail.to_owned(),
    }
}

/// One value read as `T`, its error naming `key`. Null reads as `T`'s default
/// so a bare `key:` means unset.
fn field<T: DeserializeOwned + Default>(
    key: &str,
    value: &serde_yaml::Value,
    what: &'static str,
) -> Result<T, SettingsError> {
    if value.is_null() {
        return Ok(T::default());
    }
    from_yaml(value).map_err(|err| SettingsError {
        key: key.to_owned(),
        what,
        detail: err.to_string(),
    })
}

/// The `build:` block, with unknown keys warned about and dropped.
fn build(
    value: &serde_yaml::Value,
    warnings: &mut Vec<String>,
) -> Result<BuildSettings, SettingsError> {
    let Some(map) = build_map(value)? else {
        return Ok(BuildSettings::default());
    };
    let mut kept = serde_yaml::Mapping::new();
    let mut exclude = Vec::new();
    let mut patterns = FolderPatterns::default();
    for (key, value) in map {
        match key.as_str() {
            Some("exclude") => exclude = globs("build.exclude", value)?,
            Some("patterns") => patterns = folder_patterns(value, warnings)?,
            Some(name) if BUILD_KEYS.contains(&name) => {
                warn_nested(name, value, warnings);
                kept.insert(key.clone(), value.clone());
            }
            Some(name) => warnings.push(format!(
                "`build.{name}` is not a key skillwright reads; it is ignored"
            )),
            None => warnings.push("a `build:` key that is not a string is ignored".to_owned()),
        }
    }
    // Key by key first, so a wrong value is named by its own key.
    for (key, value) in &kept {
        let one: serde_yaml::Mapping = [(key.clone(), value.clone())].into_iter().collect();
        let name = format!("build.{}", key.as_str().unwrap_or_default());
        field::<BuildSettings>(&name, &serde_yaml::Value::Mapping(one), "valid")?;
    }
    let build: BuildSettings = field("build", &serde_yaml::Value::Mapping(kept), "a build block")?;
    Ok(BuildSettings {
        exclude,
        patterns,
        ..build
    })
}

/// The `build:` block's keys, `None` when it is unset.
fn build_map(value: &serde_yaml::Value) -> Result<Option<&serde_yaml::Mapping>, SettingsError> {
    match value {
        serde_yaml::Value::Null => Ok(None),
        serde_yaml::Value::Mapping(map) => Ok(Some(map)),
        _ => Err(SettingsError {
            key: "build".to_owned(),
            what: "a mapping",
            detail: "`build:` holds keys such as `out` and `harnesses`".to_owned(),
        }),
    }
}

/// `build.patterns`, with unknown keys warned about and dropped.
fn folder_patterns(
    value: &serde_yaml::Value,
    warnings: &mut Vec<String>,
) -> Result<FolderPatterns, SettingsError> {
    let map = match value {
        serde_yaml::Value::Null => return Ok(FolderPatterns::default()),
        serde_yaml::Value::Mapping(map) => map,
        _ => {
            return Err(SettingsError {
                key: "build.patterns".to_owned(),
                what: "a mapping",
                detail: "it holds `plugins`, `skills`, `agents`, `commands`, `hooks` and `assets`"
                    .to_owned(),
            });
        }
    };
    let mut patterns = FolderPatterns::default();
    for (key, value) in map {
        let Some(name) = key.as_str() else {
            warnings.push("a `build.patterns` key that is not a string is ignored".to_owned());
            continue;
        };
        let slot = match name {
            "plugins" => &mut patterns.plugins,
            "skills" => &mut patterns.skills,
            "agents" => &mut patterns.agents,
            "commands" => &mut patterns.commands,
            "hooks" => &mut patterns.hooks,
            "assets" => &mut patterns.assets,
            _ => {
                warnings.push(format!(
                    "`build.patterns.{name}` is not a key skillwright reads; it is ignored"
                ));
                continue;
            }
        };
        let key = format!("build.patterns.{name}");
        *slot = globs(&key, value)?;
        for glob in slot.iter() {
            if crate::patterns::names_a_never_ship_folder(glob, name != "assets") {
                warnings.push(format!(
                    "`{key}` glob `{glob}` names a folder that never ships; it ships nothing"
                ));
            }
        }
    }
    Ok(patterns)
}

/// A glob or a list of globs, each one checked.
fn globs(key: &str, value: &serde_yaml::Value) -> Result<Vec<String>, SettingsError> {
    let wrong_shape = || SettingsError {
        key: key.to_owned(),
        what: "a glob or a list of globs",
        detail: format!("{value:?}"),
    };
    let globs: Vec<String> = match value {
        serde_yaml::Value::Null => Vec::new(),
        serde_yaml::Value::String(glob) => vec![glob.clone()],
        serde_yaml::Value::Sequence(items) => items
            .iter()
            .map(|item| item.as_str().map(str::to_owned).ok_or_else(wrong_shape))
            .collect::<Result<_, _>>()?,
        _ => return Err(wrong_shape()),
    };
    for glob in &globs {
        if let Some(problem) = crate::patterns::glob_problem(glob) {
            return Err(SettingsError {
                key: key.to_owned(),
                what: "a valid glob",
                detail: format!("`{glob}`: {problem}"),
            });
        }
    }
    Ok(globs)
}

/// Warn about each unknown key in an entry of a `build:` list. Each entry's
/// struct ignores them when it is read.
fn warn_nested(list: &str, value: &serde_yaml::Value, warnings: &mut Vec<String>) {
    if list == "bump" {
        let keys = value.as_mapping().into_iter().flat_map(|map| map.keys());
        for key in keys.filter_map(serde_yaml::Value::as_str) {
            if key != "patch" {
                warnings.push(format!(
                    "`build.bump.{key}` is not a key skillwright reads; it is ignored"
                ));
            }
        }
        return;
    }
    let known: &[&str] = match list {
        "destinations" => &["root", "target", "scope"],
        "assets" => &["from", "to", "plugin"],
        "hooks" => &["from", "plugin"],
        _ => return,
    };
    let entries = value.as_sequence().map_or(&[][..], Vec::as_slice);
    let keys = entries
        .iter()
        .filter_map(serde_yaml::Value::as_mapping)
        .flat_map(|entry| entry.keys())
        .filter_map(serde_yaml::Value::as_str);
    for key in keys.filter(|key| !known.contains(key)) {
        warnings.push(format!(
            "`build.{list}[].{key}` is not a key skillwright reads; it is ignored"
        ));
    }
}

/// The build config a root file's text describes: [`parse_root_file`], then
/// [`RootFile::to_config`], for a host that wants the config and not the file.
///
/// # Errors
///
/// As [`parse_root_file`].
pub fn root_file_config(text: &str, folder_name: &str) -> Result<MarketplaceConfig, SettingsError> {
    Ok(parse_root_file(text)?.to_config(folder_name))
}

impl RootFile {
    /// Fill what this file leaves unset from the workspace file above it.
    ///
    /// The marketplace fields come down where the key table says a
    /// marketplace inherits them ([`crate::keys::inherits`]): the identity
    /// fields it marks, `targets`, and the harness blocks key by key.
    ///
    /// So do `harnesses`, `namespace`, `launcher_path`, `bump`,
    /// `destinations`, `scan_locations`, `assets`, `hooks` and each key of
    /// `patterns`. Their paths and globs read from each marketplace's own
    /// folder. `out` is each marketplace's own. `exclude` is not inherited: a
    /// workspace's globs add to a member's, read from the workspace folder
    /// ([`crate::patterns::Patterns::load`]).
    ///
    /// Key by key: a value this file sets wins, and every other key still
    /// comes from the workspace. The `build:` keys are not in the key table,
    /// so their list is this function's.
    pub fn inherit(&mut self, workspace: &RootFile) {
        use crate::frontmatter::EntityKind::Marketplace;
        let fields = &mut self.marketplace;
        let shared = &workspace.marketplace;
        fields
            .identity
            .fill_unset_from(&shared.identity, Marketplace);
        // `targets` is not identity data: see `keys::identity_keys`.
        if fields.targets.is_none() && crate::keys::inherits(Marketplace, "targets") {
            fields.targets.clone_from(&shared.targets);
        }
        inherit_blocks_by(&mut fields.blocks, &shared.blocks, |harness| {
            crate::keys::inherits(Marketplace, harness)
        });
        let build = &mut self.build;
        let shared = &workspace.build;
        if build.harnesses.is_none() {
            build.harnesses.clone_from(&shared.harnesses);
        }
        if build.namespace.is_none() {
            build.namespace.clone_from(&shared.namespace);
        }
        if build.launcher_path.is_none() {
            build.launcher_path.clone_from(&shared.launcher_path);
        }
        if build.bump.is_none() {
            build.bump.clone_from(&shared.bump);
        }
        if build.destinations.is_empty() {
            build.destinations.clone_from(&shared.destinations);
        }
        if build.scan_locations.is_empty() {
            build.scan_locations.clone_from(&shared.scan_locations);
        }
        if build.assets.is_empty() {
            build.assets.clone_from(&shared.assets);
        }
        if build.hooks.is_empty() {
            build.hooks.clone_from(&shared.hooks);
        }
        build.patterns.inherit(&shared.patterns);
    }

    /// The build config this file describes. `folder_name` is the root
    /// folder's name, the marketplace's name when the file gives none.
    pub fn to_config(&self, folder_name: &str) -> MarketplaceConfig {
        let fields = &self.marketplace;
        MarketplaceConfig {
            name: Some(
                fields
                    .name
                    .clone()
                    .unwrap_or_else(|| crate::discover::slugify(folder_name)),
            ),
            description: fields.description.clone(),
            identity: fields.identity.clone(),
            targets: fields.targets.clone(),
            blocks: fields.blocks.clone(),
            plugins: BTreeMap::new(),
            launcher_path: self.build.launcher_path.clone(),
        }
    }

    /// The harnesses to build. Empty means every one the export supports.
    pub fn harness_set(&self) -> BTreeSet<String> {
        self.build.harnesses.iter().flatten().cloned().collect()
    }
}

/// Fills `own`'s harness blocks from `shared`, key by key, for each harness
/// `inherits` allows. A key `own` already sets keeps its value.
pub(crate) fn inherit_blocks_by(
    own: &mut BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
    shared: &BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
    inherits: impl Fn(&str) -> bool,
) {
    for (harness, shared) in shared.iter().filter(|(harness, _)| inherits(harness)) {
        let block = own.entry(harness.clone()).or_default();
        for (key, value) in shared {
            block.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_file_refusals_say_what_the_value_is() {
        for (line, said) in [
            (
                "owner: {email: x}",
                "`owner` is not a name or a `{name, email, url}` mapping: it has no string `name`",
            ),
            (
                "targets: \"claude\"",
                "`targets` is not a list: it is a string.",
            ),
            (
                "repository: ftp://x",
                "`repository` is not an http or https URL: it is `ftp://x`",
            ),
            (
                "targets: [1]",
                "`targets` is not a list of strings: one entry is a number",
            ),
        ] {
            let err = parse_root_file(&format!("---\n{line}\n---\n")).unwrap_err();
            assert!(err.to_string().starts_with(said), "{line}: {err}");
        }
    }

    /// A value with no JSON form, such as a mapping with a list for a key,
    /// cannot be checked, so it is refused rather than passed.
    #[test]
    fn a_value_with_no_json_form_is_refused() {
        let err = parse_root_file("---\nowner: {[a]: b}\n---\n").unwrap_err();
        assert_eq!(err.key, "owner", "{err}");
    }

    #[test]
    fn marketplace_keys_are_the_table_s_marketplace_rows() {
        let keys: Vec<&str> = marketplace_keys().collect();
        assert_eq!(
            keys,
            [
                "name",
                "description",
                "version",
                "author",
                "owner",
                "license",
                "repository",
                "homepage",
                "display_name",
                "targets",
            ]
        );
    }

    /// The root file and a workspace's marketplace note read one value of
    /// ours alike, and a refusal names the key. The spec keys, `name` and
    /// `description`, are left out: the root file reads them into strings and
    /// refuses `name: 5`, while a note does not check their shape. So is a key
    /// set to nothing, which the root file reads as unset and a note refuses.
    #[test]
    fn root_file_and_marketplace_note_refuse_the_same_values() {
        use crate::frontmatter::{EntityKind, VaultFrontmatter};
        let cases = [
            ("author", "5"),
            ("author", "Ada"),
            ("owner", "{email: x}"),
            ("owner", "{name: x}"),
            ("repository", "ftp://x"),
            ("repository", "https://x"),
            ("homepage", "5"),
            ("version", "1"),
            ("version", "'1'"),
            ("license", "[a]"),
            ("license", "MIT"),
            ("targets", "\"claude\""),
            ("targets", "[claude]"),
            ("display_name", "5"),
            ("display_name", "Acme"),
        ];
        for (key, value) in cases {
            let line = format!("{key}: {value}");
            let root = parse_root_file(&format!("---\n{line}\n---\n"));
            let yaml: serde_yaml::Value = serde_yaml::from_str(&line).unwrap();
            let mut fields = match serde_json::to_value(yaml).unwrap() {
                serde_json::Value::Object(map) => map,
                other => panic!("{other}"),
            };
            let note = VaultFrontmatter::take_from(
                &mut fields,
                std::path::Path::new("/vault/marketplaces/m/skillwright.md"),
                &crate::namespace::Namespace::bare(),
                EntityKind::Marketplace,
            );
            assert_eq!(root.is_ok(), note.is_ok(), "{line}");
            if let (Err(root), Err(note)) = (root, note) {
                let named = format!("`{key}`");
                assert!(root.to_string().contains(&named), "{line}: {root}");
                assert!(note.to_string().contains(&named), "{line}: {note}");
            }
        }
    }

    #[test]
    fn a_block_key_the_harness_does_not_read_warns() {
        let root =
            parse_root_file("---\nname: m\ncodex:\n  interface: {}\n  nonsense: 1\n---\n").unwrap();
        assert_eq!(root.warnings.len(), 1, "{:?}", root.warnings);
        assert!(root.warnings[0].contains("`codex.nonsense`"));
        assert!(root.marketplace.blocks["codex"].contains_key("interface"));
    }

    #[test]
    fn a_target_that_is_not_a_harness_is_refused() {
        let err = parse_root_file("---\nname: m\ntargets: [emacs]\n---\n").unwrap_err();
        assert!(err.to_string().contains("emacs"), "{err}");
    }

    #[test]
    fn a_repository_that_is_not_a_url_is_refused() {
        let err = parse_root_file("---\nname: m\nrepository: not a url\n---\n").unwrap_err();
        assert!(err.to_string().contains("repository"), "{err}");
    }

    #[test]
    fn an_unknown_patterns_key_warns_and_a_wrong_shape_is_refused_by_name() {
        let root = parse_root_file(
            "---\nbuild:\n  patterns:\n    skill: a/*\n    skills: [docs/*, notes/*]\n---\n",
        )
        .unwrap();
        assert_eq!(root.build.patterns.skills, ["docs/*", "notes/*"]);
        let said = root.warnings.join("\n");
        assert!(said.contains("`build.patterns.skill`"), "{said}");
        assert!(
            said.contains("`docs/*` names a folder that never ships"),
            "{said}"
        );

        for (text, key) in [
            ("patterns: [notes]", "build.patterns"),
            ("patterns: notes", "build.patterns"),
            ("patterns: {skills: 3}", "build.patterns.skills"),
            ("patterns: {skills: [1]}", "build.patterns.skills"),
            ("exclude: {a: b}", "build.exclude"),
        ] {
            let err = parse_root_file(&format!("---\nbuild:\n  {text}\n---\n")).unwrap_err();
            assert_eq!(err.key, key, "{text}");
        }
    }

    #[test]
    fn a_bad_glob_is_refused_naming_the_key_and_the_value() {
        let err = parse_root_file("---\nbuild:\n  exclude: ['ok/*', '../x']\n---\n").unwrap_err();
        assert_eq!(err.key, "build.exclude");
        assert!(err.detail.contains("`../x`"), "{err}");
        let err =
            parse_root_file("---\nbuild:\n  patterns:\n    agents: '/abs/*'\n---\n").unwrap_err();
        assert_eq!(err.key, "build.patterns.agents");
    }

    #[test]
    fn a_member_s_build_value_overrides_the_workspace_key_by_key() {
        let workspace = parse_root_file(
            "---\nworkspace: true\nlicense: MIT\nbuild:\n  harnesses: [claude]\n  \
             namespace: sw\n  launcher_path: bin/ws\n  out: dist\n---\n",
        )
        .unwrap();
        let mut member = parse_root_file("---\nbuild:\n  namespace: acme\n---\n").unwrap();
        member.inherit(&workspace);

        // What the member sets wins.
        assert_eq!(member.build.namespace.as_deref(), Some("acme"));
        // Every other key still comes down.
        assert_eq!(member.build.harnesses, Some(vec!["claude".to_owned()]));
        assert_eq!(member.build.launcher_path.as_deref(), Some("bin/ws"));
        assert_eq!(member.marketplace.identity.license.as_deref(), Some("MIT"));
        // The output is each member's own.
        assert_eq!(member.build.out, None);
    }

    #[test]
    fn the_config_a_text_describes_takes_the_folder_s_name_by_default() {
        let config = root_file_config("---\nlicense: MIT\n---\n", "My House").unwrap();
        assert_eq!(config.name.as_deref(), Some("my-house"));
        assert_eq!(config.identity.license.as_deref(), Some("MIT"));
    }

    #[test]
    fn a_root_file_maps_onto_a_build_config() {
        let root = parse_root_file(
            "---\n\
             name: osdlc\n\
             owner: { name: Ada }\n\
             license: MIT\n\
             build:\n  \
               out: dist\n  \
               harnesses: [claude]\n  \
               launcher_path: bin/tool\n\
             ---\n\
             # osdlc\n\nThe body is not read.\n",
        )
        .unwrap();
        assert!(root.warnings.is_empty(), "{:?}", root.warnings);
        assert_eq!(root.build.out.as_deref(), Some("dist"));
        let config = root.to_config("folder");
        assert_eq!(config.name.as_deref(), Some("osdlc"));
        assert_eq!(
            config.identity.owner,
            Some(serde_json::json!({"name": "Ada"}))
        );
        assert_eq!(config.identity.license.as_deref(), Some("MIT"));
        assert_eq!(config.launcher_path.as_deref(), Some("bin/tool"));
        assert_eq!(root.harness_set(), BTreeSet::from(["claude".to_owned()]));
    }

    #[test]
    fn a_file_with_no_frontmatter_takes_every_default() {
        let root = parse_root_file("# Just a README\n").unwrap();
        assert_eq!(root, RootFile::default());
        let config = root.to_config("My Tools");
        assert_eq!(config.name.as_deref(), Some("my-tools"));
        assert_eq!(config.launcher_path, None);
    }

    #[test]
    fn an_unknown_key_is_a_warning_not_a_refusal() {
        let root =
            parse_root_file("---\nname: m\nflavour: mint\nbuild:\n  outt: x\n---\n").unwrap();
        assert_eq!(root.warnings.len(), 2, "{:?}", root.warnings);
        assert!(
            root.warnings[0].contains("`flavour`"),
            "{:?}",
            root.warnings
        );
        assert!(
            root.warnings[1].contains("`build.outt`"),
            "{:?}",
            root.warnings
        );
        assert_eq!(root.build.out, None);
    }

    #[test]
    fn build_bump_picks_the_patch_scheme() {
        let root = parse_root_file("---\nbuild:\n  bump:\n    patch: timestamp\n---\n").unwrap();
        assert_eq!(root.build.bump.unwrap().patch, PatchScheme::Timestamp);
    }

    #[test]
    fn an_unknown_key_under_build_bump_warns() {
        let root = parse_root_file("---\nbuild:\n  bump:\n    patch: timestamp\n    tag: x\n---\n")
            .unwrap();
        assert_eq!(root.build.bump.unwrap().patch, PatchScheme::Timestamp);
        assert!(
            root.warnings.iter().any(|w| w.contains("`build.bump.tag`")),
            "{:?}",
            root.warnings
        );
    }

    #[test]
    fn a_wrong_build_bump_is_named() {
        let err = parse_root_file("---\nbuild:\n  bump:\n    patch: weekly\n---\n").unwrap_err();
        assert_eq!(err.key, "build.bump");
    }

    #[test]
    fn a_marketplace_inherits_build_bump_from_its_workspace() {
        let workspace =
            parse_root_file("---\nworkspace: true\nbuild:\n  bump:\n    patch: timestamp\n---\n")
                .unwrap();
        let mut member = parse_root_file("---\nname: one\n---\n").unwrap();
        member.inherit(&workspace);
        assert_eq!(member.build.bump.unwrap().patch, PatchScheme::Timestamp);
    }

    #[test]
    fn a_known_key_of_the_wrong_shape_is_named() {
        let err = parse_root_file("---\nbuild:\n  harnesses: claude\n---\n").unwrap_err();
        assert_eq!(err.key, "build.harnesses");
        assert!(err.detail.contains("sequence"), "{err}");
        let err = parse_root_file("---\nworkspace: maybe\n---\n").unwrap_err();
        assert_eq!(err.key, "workspace");
    }

    #[test]
    fn a_marketplace_inherits_shared_defaults_from_its_workspace() {
        let workspace = parse_root_file(
            "---\nworkspace: true\nlicense: MIT\nauthor: Ada\n\
             build:\n  harnesses: [claude]\n  out: shared\n---\n",
        )
        .unwrap();
        assert!(workspace.workspace);
        let mut member = parse_root_file("---\nname: one\nlicense: Apache-2.0\n---\n").unwrap();
        member.inherit(&workspace);
        assert_eq!(
            member.marketplace.identity.license.as_deref(),
            Some("Apache-2.0")
        );
        assert_eq!(
            member.marketplace.identity.author,
            Some(serde_json::json!("Ada"))
        );
        assert_eq!(member.build.harnesses, Some(vec!["claude".to_owned()]));
        // The output is the member's own.
        assert_eq!(member.build.out, None);
    }

    #[test]
    fn a_marketplace_inherits_patterns_key_by_key_and_keeps_its_own_exclusions() {
        let workspace = parse_root_file(
            "---\nworkspace: true\nbuild:\n  exclude: '**/*.draft.md'\n  patterns:\n    \
             skills: notes/*\n    plugins: packs/*\n---\n",
        )
        .unwrap();
        let mut member =
            parse_root_file("---\nbuild:\n  patterns:\n    skills: mine/*\n---\n").unwrap();
        member.inherit(&workspace);
        assert!(member.build.exclude.is_empty());
        assert_eq!(member.build.patterns.skills, ["mine/*"]);
        assert_eq!(member.build.patterns.plugins, ["packs/*"]);
    }

    #[test]
    fn a_marketplace_inherits_its_build_lists_key_by_key() {
        let workspace = parse_root_file(
            "---\nworkspace: true\nbuild:\n  destinations:\n    - {root: d, target: claude}\n  \
             scan_locations: [shared]\n  assets:\n    - {from: a, to: b}\n  hooks:\n    \
             - {from: h.json}\n---\n",
        )
        .unwrap();
        let mut member = parse_root_file("---\nbuild:\n  scan_locations: [mine]\n---\n").unwrap();
        member.inherit(&workspace);
        assert_eq!(member.build.destinations, workspace.build.destinations);
        assert_eq!(member.build.scan_locations, ["mine"]);
        assert_eq!(member.build.assets, workspace.build.assets);
        assert_eq!(member.build.hooks, workspace.build.hooks);
    }

    #[test]
    fn an_unterminated_root_file_is_refused_and_a_readme_is_not() {
        for text in [
            "---\nbuild:\n  exclude: a\n",
            "\u{feff}---\nbuild:\n  exclude: a\n",
            "---\r\nname: m\r\n",
        ] {
            let err = parse_root_file(text).unwrap_err();
            assert!(err.detail.contains("never closes"), "{err}");
        }
        for text in ["", "# Notes\n", "# Notes\n\n---\n\nmore\n"] {
            parse_root_file(text).unwrap();
        }
    }

    /// A root file setting every marketplace key and a block of each harness.
    const EVERY_KEY: &str = "name: ws\n\
         description: The workspace.\n\
         owner: Org\n\
         author: Ada\n\
         display_name: Workspace\n\
         license: MIT\n\
         version: '9.9.9'\n\
         targets: [claude, codex]\n\
         repository: https://ws.example/repo\n\
         homepage: https://ws.example\n\
         claude:\n  metadata: {from: ws}\n  strict: true\n\
         codex:\n  interface: {from: ws}\n\
         cursor:\n  logo: ws.png\n\
         pi:\n  keywords: [ws]\n";

    /// The keys `value` sets to something other than `null`.
    fn set_keys(value: &impl Serialize) -> serde_json::Map<String, serde_json::Value> {
        match serde_json::to_value(value).unwrap() {
            serde_json::Value::Object(map) => {
                map.into_iter().filter(|(_, v)| !v.is_null()).collect()
            }
            other => panic!("not a mapping: {other}"),
        }
    }

    #[test]
    fn a_member_takes_exactly_the_inheritable_fields_from_its_workspace() {
        let workspace =
            parse_root_file(&format!("---\nworkspace: true\n{EVERY_KEY}---\n")).unwrap();
        assert!(workspace.warnings.is_empty(), "{:?}", workspace.warnings);

        let mut member = parse_root_file("---\n---\n").unwrap();
        member.inherit(&workspace);
        let took = set_keys(&member.marketplace);
        let keys: BTreeSet<&str> = took.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            BTreeSet::from([
                "author",
                "license",
                "repository",
                "homepage",
                "targets",
                "blocks"
            ])
        );
        let shared = set_keys(&workspace.marketplace);
        for key in &keys {
            assert_eq!(took[*key], shared[*key], "{key}");
        }

        // A member that sets each key keeps its own, and a block key it sets
        // keeps its value while the workspace's other keys in that block come
        // down.
        let mut own = parse_root_file(
            "---\nname: own\ndescription: Mine.\nowner: Me\nauthor: Bo\n\
             display_name: Own\nlicense: ISC\nversion: '1.0.0'\ntargets: [pi]\n\
             repository: https://own.example/repo\nhomepage: https://own.example\n\
             claude:\n  strict: false\n---\n",
        )
        .unwrap();
        let before = set_keys(&own.marketplace);
        own.inherit(&workspace);
        let after = set_keys(&own.marketplace);
        for (key, value) in &before {
            if key != "blocks" {
                assert_eq!(&after[key], value, "{key}");
            }
        }
        let claude = &own.marketplace.blocks["claude"];
        assert_eq!(claude["strict"], serde_json::json!(false));
        assert_eq!(claude["metadata"], serde_json::json!({"from": "ws"}));
        assert_eq!(
            own.marketplace.blocks.keys().collect::<Vec<_>>(),
            ["claude", "codex", "cursor", "pi"]
        );
    }

    #[test]
    fn to_config_carries_every_marketplace_field() {
        let root = parse_root_file(&format!("---\n{EVERY_KEY}---\n")).unwrap();
        let config = set_keys(&root.to_config("Folder Name"));
        let fields = set_keys(&root.marketplace);
        for key in marketplace_keys().chain(["blocks"]) {
            assert_eq!(config.get(key), fields.get(key), "{key}");
        }
        assert_eq!(config["name"], "ws");

        let unnamed = parse_root_file("---\nlicense: MIT\n---\n").unwrap();
        let config = unnamed.to_config("Folder Name");
        assert_eq!(config.name.as_deref(), Some("folder-name"));
    }

    #[test]
    fn a_root_file_serializes_flat() {
        let root = parse_root_file("---\nlicense: MIT\nauthor: Ada\n---\n").unwrap();
        assert_eq!(root.marketplace.identity.license.as_deref(), Some("MIT"));
        let written = serde_json::to_value(&root).unwrap();
        let fields = &written["marketplace"];
        assert!(fields.get("identity").is_none(), "{written}");
        assert_eq!(fields["license"], "MIT");
        assert_eq!(fields["author"], "Ada");
        let read: RootFile = serde_json::from_value(written).unwrap();
        assert_eq!(read, root);

        // Unset identity fields are absent, not `null`.
        let empty = serde_json::to_value(MarketplaceFields::default()).unwrap();
        assert_eq!(empty, serde_json::json!({}));
    }

    #[test]
    fn inherit_blocks_by_copies_only_the_harnesses_allowed() {
        let block =
            |from: &str| serde_json::Map::from_iter([("k".to_owned(), serde_json::json!(from))]);
        let shared: BTreeMap<_, _> = crate::keys::HARNESS_BLOCKS
            .iter()
            .map(|harness| ((*harness).to_owned(), block(harness)))
            .collect();
        for harness in crate::keys::HARNESS_BLOCKS {
            let mut own = BTreeMap::new();
            inherit_blocks_by(&mut own, &shared, |key| key == *harness);
            assert_eq!(
                own,
                BTreeMap::from([((*harness).to_owned(), block(harness))]),
                "{harness}"
            );
        }
    }
}
