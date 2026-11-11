//! `skillwright.md`, the root file: the vault's configuration.
//!
//! Its top-level keys are the shared defaults, the keys the key table marks
//! as inherited ([`SharedDefaults`]). Settings for how skillwright runs sit
//! under `build:`. The body is ignored. The folder holding the file is the
//! vault root.
//!
//! The CLI and other hosts read the file through [`parse_root_file`], so they
//! share one shape and one mapping onto a build's [`MarketplaceConfig`].
//! Finding the file stays with the caller: this module parses text and never
//! touches a filesystem.
//!
//! An unknown key warns and is ignored. `workspace`, `name`, a key the table
//! lists but does not mark as inherited, and a removed `build:` key are
//! refused. A known key with a value of the wrong shape is refused, naming
//! the key.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::export::MarketplaceConfig;
use crate::frontmatter::{ContainerIdentity, EntityKind};
use crate::keys::{KEYS, Shape};
use crate::skill::split_frontmatter;

/// The root file's name.
pub const ROOT_FILE: &str = "skillwright.md";

/// The root file's shared defaults: the keys the key table marks as
/// inherited. Every marketplace and plugin takes each one it does not set.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SharedDefaults {
    /// `author`: a name or `{name, email, url}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<serde_json::Value>,
    /// `license`, an SPDX expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// `repository`, the source repository URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// `homepage`, the documentation URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// `targets`: which harnesses get each component. Absent means all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<String>>,
    /// The harness blocks (`claude:`, `codex:`, `cursor:`, `pi:`), keyed by
    /// harness. Inherited key by key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub blocks: BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
}

/// One `build.assets` entry: a file or folder copied into a plugin's root.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AssetSetting {
    /// The source file or folder, relative to the root file.
    pub from: String,
    /// Where it lands, relative to the built plugin's own root.
    pub to: String,
    /// Which plugin it lands in. A build refuses an entry without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

/// One `build.hooks` entry: a Claude Code `hooks.json` rendered per harness.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HookSetting {
    /// The file, relative to the root file.
    pub from: String,
    /// Which plugin's packs carry it. A build refuses an entry without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

/// The `build:` block: how skillwright runs, never what lands in a manifest.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BuildSettings {
    /// Which harnesses to build. Absent means all of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harnesses: Option<Vec<String>>,
    /// The plugin's launcher, relative to the plugin's root. See
    /// [`MarketplaceConfig::launcher_path`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher_path: Option<String>,
    /// How `bump` writes a version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<BumpSettings>,
    /// Files copied into a plugin's root besides each plugin's `assets/`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<AssetSetting>,
    /// Canonical hooks files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<HookSetting>,
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
    /// The shared defaults.
    #[serde(default)]
    pub defaults: SharedDefaults,
    /// The `build:` block.
    #[serde(default)]
    pub build: BuildSettings,
    /// One line per key the file carries that skillwright does not read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// A key whose value has the wrong shape, or a key the root file refuses,
/// named.
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

/// The shared defaults' keys, less the harness blocks: the key table's
/// inherited marketplace keys, in table order.
pub fn shared_default_keys() -> impl Iterator<Item = &'static str> {
    KEYS.iter()
        .filter(|spec| {
            spec.inherits
                && spec.kinds.contains(&EntityKind::Marketplace)
                && spec.shape != Shape::HarnessBlock
        })
        .map(|spec| spec.key)
}

/// The keys of the `build:` block.
pub const BUILD_KEYS: &[&str] = &["harnesses", "launcher_path", "bump", "assets", "hooks"];

/// The `build:` keys v2 refuses, naming the key, rather than warns about.
pub const REFUSED_BUILD_KEYS: &[&str] = &[
    "destinations",
    "scan_locations",
    "out",
    "patterns",
    "exclude",
    "namespace",
];

/// The top-level keys v2 refuses in the root file besides the key table's
/// rows that are not inherited.
pub const REFUSED_ROOT_KEYS: &[&str] = &["workspace"];

/// A YAML value read as `T` through its JSON equivalent, so free-shaped
/// fields come out as the JSON a manifest carries.
fn from_yaml<T: DeserializeOwned>(value: &serde_yaml::Value) -> Result<T, serde_json::Error> {
    serde_json::to_value(value).and_then(serde_json::from_value)
}

/// A new root file, the one `skillwright init` writes: no keys, so every
/// default holds.
#[must_use]
pub fn compose_root_file() -> String {
    "---\n---\n".to_owned()
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

/// Read a root file's text. A file with no frontmatter is a root file with
/// every default.
///
/// # Errors
///
/// Frontmatter that is not a YAML mapping, a key the root file refuses
/// (`workspace`, `name`, and a key the table lists but does not mark as
/// inherited), and a known key whose value has the wrong shape.
pub fn parse_root_file(text: &str) -> Result<RootFile, SettingsError> {
    let map = frontmatter_map(text)?;
    if let Some((key, detail)) = refused_in_root(&map).into_iter().next() {
        return Err(SettingsError {
            key,
            what: "a key of the root file",
            detail,
        });
    }
    let mut root = RootFile::default();
    let mut defaults = serde_yaml::Mapping::new();
    for (key, value) in map {
        let Some(key) = key.as_str().map(str::to_owned) else {
            root.warnings
                .push("a frontmatter key that is not a string is ignored".to_owned());
            continue;
        };
        match key.as_str() {
            "build" => root.build = build(&value, &mut root.warnings)?,
            shared if shared_default_keys().any(|key| key == shared) => {
                check_shape(shared, &value)?;
                defaults.insert(serde_yaml::Value::String(key), value);
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
                        EntityKind::Marketplace,
                    );
                    for unknown in &unknown {
                        root.warnings.push(format!(
                            "`{unknown}` reaches no file that harness reads; it is ignored"
                        ));
                    }
                    let block = crate::harness::field_types::with_author_shorthand(
                        crate::harness::field_types::without_unset(&key, block, &unknown),
                    );
                    root.defaults.blocks.insert(key, block);
                }
            }
            _ => root.warnings.push(format!(
                "`{key}` is not a key skillwright reads in {ROOT_FILE}; it is ignored"
            )),
        }
    }
    let blocks = std::mem::take(&mut root.defaults.blocks);
    root.defaults = field(
        "shared defaults",
        &serde_yaml::Value::Mapping(defaults),
        "the shared defaults",
    )?;
    root.defaults.blocks = blocks;
    check_harness_names("targets", root.defaults.targets.as_deref())?;
    check_harness_names("build.harnesses", root.build.harnesses.as_deref())?;
    Ok(root)
}

/// Refuses a name in `names` that is not one of the four harnesses.
fn check_harness_names(key: &str, names: Option<&[String]>) -> Result<(), SettingsError> {
    let unknown: Vec<&str> = names
        .into_iter()
        .flatten()
        .map(String::as_str)
        .filter(|name| crate::publish::canonical_target(name).is_none())
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(SettingsError {
        key: key.to_owned(),
        what: "a list of harnesses",
        detail: format!("it {}", crate::publish::unknown_targets_detail(&unknown)),
    })
}

/// Why the root file refuses the top-level `key`, or `None` when it reads or
/// warns about it (spec section 2 rules 3 and 4).
fn root_key_refusal(key: &str) -> Option<String> {
    if REFUSED_ROOT_KEYS.contains(&key) {
        return Some("it is a removed key; delete it".to_owned());
    }
    let rows: Vec<_> = KEYS.iter().filter(|spec| spec.key == key).collect();
    if rows.is_empty() || rows.iter().any(|spec| spec.inherits) {
        return None;
    }
    if rows.iter().any(|spec| spec.shape == Shape::Refused) {
        return Some(crate::keys::NAME_REFUSAL.to_owned());
    }
    let kinds: Vec<EntityKind> = [
        EntityKind::Marketplace,
        EntityKind::Plugin,
        EntityKind::Skill,
        EntityKind::Agent,
        EntityKind::Command,
    ]
    .into_iter()
    .filter(|kind| rows.iter().any(|spec| spec.kinds.contains(kind)))
    .collect();
    let nouns: Vec<String> = kinds.iter().map(|kind| kind.as_str().to_owned()).collect();
    let files: Vec<String> = kinds
        .iter()
        .map(|kind| format!("`<{}>.md`", &kind.as_str()[..1]))
        .collect();
    Some(format!(
        "it is not a shared default; it belongs in a {} note, {}",
        or_list(&nouns),
        or_list(&files)
    ))
}

/// `a`, `a or b`, `a, b or c`.
fn or_list(items: &[String]) -> String {
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

/// The top-level keys of a root file's mapping it refuses, each with why, in
/// file order.
fn refused_in_root(map: &serde_yaml::Mapping) -> Vec<(String, String)> {
    map.keys()
        .filter_map(serde_yaml::Value::as_str)
        .filter_map(|key| root_key_refusal(key).map(|why| (key.to_owned(), why)))
        .collect()
}

/// The keys a root file's text refuses, for the layout report: each top-level
/// key it refuses with why (spec section 11 case 2), then each removed
/// `build:` key as `build.<key>` (case 7). Empty when the text cannot be read:
/// [`parse_root_file`] says what is wrong with it.
pub(crate) fn refused_root_file_keys(text: &str) -> (Vec<(String, String)>, Vec<String>) {
    let Ok(map) = frontmatter_map(text) else {
        return (Vec::new(), Vec::new());
    };
    let build = map
        .get("build")
        .and_then(serde_yaml::Value::as_mapping)
        .map(refused_in)
        .unwrap_or_default();
    (refused_in_root(&map), build)
}

/// Refuses a shared default whose value is not the shape the key table gives
/// it, or that has no JSON form to check, such as a mapping with a list for a
/// key. A key set to nothing is unset, as [`field`] reads it.
fn check_shape(key: &str, value: &serde_yaml::Value) -> Result<(), SettingsError> {
    if value.is_null() {
        return Ok(());
    }
    let json = serde_json::to_value(value).map_err(|err| SettingsError {
        key: key.to_owned(),
        what: "a value with a JSON form",
        detail: err.to_string(),
    })?;
    crate::keys::check_shape(EntityKind::Marketplace, key, &json).map_err(|refusal| SettingsError {
        key: key.to_owned(),
        what: refusal.what,
        detail: settings_detail(&refusal.detail),
    })
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
    let refused = refused_in(map);
    if !refused.is_empty() {
        return Err(SettingsError {
            key: refused.join("`, `"),
            what: "a key skillwright reads",
            detail: "delete it".to_owned(),
        });
    }
    let mut kept = serde_yaml::Mapping::new();
    for (key, value) in map {
        match key.as_str() {
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
    field("build", &serde_yaml::Value::Mapping(kept), "a build block")
}

/// The refused keys of a `build:` mapping, as `build.<key>`, in file order.
fn refused_in(map: &serde_yaml::Mapping) -> Vec<String> {
    map.keys()
        .filter_map(serde_yaml::Value::as_str)
        .filter(|name| REFUSED_BUILD_KEYS.contains(name))
        .map(|name| format!("build.{name}"))
        .collect()
}

/// The `build:` block's keys, `None` when it is unset.
fn build_map(value: &serde_yaml::Value) -> Result<Option<&serde_yaml::Mapping>, SettingsError> {
    match value {
        serde_yaml::Value::Null => Ok(None),
        serde_yaml::Value::Mapping(map) => Ok(Some(map)),
        _ => Err(SettingsError {
            key: "build".to_owned(),
            what: "a mapping",
            detail: "`build:` holds keys such as `harnesses` and `bump`".to_owned(),
        }),
    }
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
pub fn root_file_config(text: &str) -> Result<MarketplaceConfig, SettingsError> {
    Ok(parse_root_file(text)?.to_config())
}

impl RootFile {
    /// The build config this file describes: its shared defaults, with no
    /// marketplace's own fields. Each marketplace's note gives those.
    pub fn to_config(&self) -> MarketplaceConfig {
        let defaults = &self.defaults;
        let identity = ContainerIdentity {
            author: defaults.author.clone(),
            license: defaults.license.clone(),
            repository: defaults.repository.clone(),
            homepage: defaults.homepage.clone(),
            ..ContainerIdentity::default()
        };
        MarketplaceConfig {
            name: None,
            description: None,
            identity,
            targets: defaults.targets.clone(),
            blocks: defaults.blocks.clone(),
            plugins: BTreeMap::new(),
            launcher_path: self.build.launcher_path.clone(),
        }
    }

    /// The harnesses to build. Empty means every one the export supports.
    pub fn harness_set(&self) -> BTreeSet<String> {
        self.build.harnesses.iter().flatten().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_file_refusals_say_what_the_value_is() {
        for (line, said) in [
            (
                "author: {email: x}",
                "`author` is not a name or a `{name, email, url}` mapping: it has no string `name`",
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
        let err = parse_root_file("---\nauthor: {[a]: b}\n---\n").unwrap_err();
        assert_eq!(err.key, "author", "{err}");
    }

    #[test]
    fn the_shared_defaults_are_the_table_s_inherited_rows() {
        let keys: Vec<&str> = shared_default_keys().collect();
        assert_eq!(
            keys,
            ["author", "license", "repository", "homepage", "targets"]
        );
        let every = SharedDefaults {
            author: Some(serde_json::json!("Ada")),
            license: Some("MIT".to_owned()),
            repository: Some("https://x".to_owned()),
            homepage: Some("https://x".to_owned()),
            targets: Some(Vec::new()),
            blocks: BTreeMap::from([("claude".to_owned(), serde_json::Map::new())]),
        };
        let fields = set_keys(&every);
        let mut expected: BTreeSet<&str> = keys.into_iter().collect();
        expected.insert("blocks");
        assert_eq!(
            fields.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            expected
        );
    }

    /// The root file and a marketplace note read one shared default alike,
    /// and a refusal names the key. A key set to nothing is left out: the
    /// root file reads it as unset and a note refuses it.
    #[test]
    fn root_file_and_marketplace_note_refuse_the_same_values() {
        use crate::frontmatter::VaultFrontmatter;
        let cases = [
            ("author", "5"),
            ("author", "Ada"),
            ("author", "{email: x}"),
            ("repository", "ftp://x"),
            ("repository", "https://x"),
            ("homepage", "5"),
            ("license", "[a]"),
            ("license", "MIT"),
            ("targets", "\"claude\""),
            ("targets", "[claude]"),
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
                std::path::Path::new("/vault/marketplaces/m/m.md"),
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
    fn a_key_that_is_not_inherited_is_refused_saying_where_it_belongs() {
        for (key, belongs) in [
            ("owner", "a marketplace note, `<m>.md`"),
            ("keywords", "a plugin note, `<p>.md`"),
            ("category", "a plugin note, `<p>.md`"),
            (
                "version",
                "a marketplace or plugin note, `<m>.md` or `<p>.md`",
            ),
            (
                "description",
                "a marketplace, plugin, skill, agent or command note",
            ),
            (
                "model",
                "a skill, agent or command note, `<s>.md`, `<a>.md` or `<c>.md`",
            ),
        ] {
            let err = parse_root_file(&format!("---\n{key}: x\n---\n")).unwrap_err();
            assert_eq!(err.key, key);
            assert!(err.detail.contains(belongs), "{key}: {err}");
        }
        let err = parse_root_file("---\nname: m\n---\n").unwrap_err();
        assert!(err.detail.contains("delete it"), "{err}");
        let err = parse_root_file("---\nworkspace: true\n---\n").unwrap_err();
        assert_eq!(err.key, "workspace");
        assert_eq!(
            refused_root_file_keys("---\nowner: x\nlicense: MIT\nworkspace: true\n---\n")
                .0
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>(),
            ["owner", "workspace"]
        );
    }

    #[test]
    fn a_block_key_the_harness_does_not_read_warns() {
        let root = parse_root_file("---\ncodex:\n  interface: {}\n  nonsense: 1\n---\n").unwrap();
        assert_eq!(root.warnings.len(), 1, "{:?}", root.warnings);
        assert!(root.warnings[0].contains("`codex.nonsense`"));
        assert!(root.defaults.blocks["codex"].contains_key("interface"));
    }

    #[test]
    fn a_target_that_is_not_one_of_the_four_is_refused() {
        for line in [
            "targets: [emacs]",
            "targets: [claude-code]",
            "build: {harnesses: [amp]}",
        ] {
            let err = parse_root_file(&format!("---\n{line}\n---\n")).unwrap_err();
            assert!(
                err.to_string()
                    .contains("The harnesses are `claude`, `codex`, `cursor` and `pi`"),
                "{err}"
            );
        }
    }

    #[test]
    fn a_root_file_maps_onto_a_build_config() {
        let root = parse_root_file(
            "---\n\
             author: Ada\n\
             license: MIT\n\
             targets: [claude]\n\
             build:\n  \
               harnesses: [claude]\n  \
               launcher_path: bin/tool\n\
             ---\n\
             # osdlc\n\nThe body is not read.\n",
        )
        .unwrap();
        assert!(root.warnings.is_empty(), "{:?}", root.warnings);
        let config = root.to_config();
        assert_eq!(config.name, None);
        assert_eq!(config.description, None);
        assert_eq!(config.identity.author, Some(serde_json::json!("Ada")));
        assert_eq!(config.identity.license.as_deref(), Some("MIT"));
        assert_eq!(config.targets, Some(vec!["claude".to_owned()]));
        assert_eq!(config.launcher_path.as_deref(), Some("bin/tool"));
        assert_eq!(root.harness_set(), BTreeSet::from(["claude".to_owned()]));
    }

    #[test]
    fn a_file_with_no_frontmatter_takes_every_default() {
        let root = parse_root_file("# Just a README\n").unwrap();
        assert_eq!(root, RootFile::default());
        let config = root.to_config();
        assert_eq!(config.name, None);
        assert_eq!(config.launcher_path, None);
    }

    #[test]
    fn an_unknown_key_is_a_warning_not_a_refusal() {
        let root = parse_root_file("---\nflavour: mint\nbuild:\n  outt: x\n---\n").unwrap();
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
    }

    #[test]
    fn each_removed_build_key_is_refused_by_name() {
        for (key, value) in [
            ("destinations", "\n    - {root: d, target: claude}"),
            ("scan_locations", " [shared]"),
            ("out", " dist"),
            ("patterns", "\n    skills: notes/*"),
            ("exclude", " '**/*.draft.md'"),
            ("namespace", " sw"),
        ] {
            let err = parse_root_file(&format!("---\nbuild:\n  {key}:{value}\n---\n")).unwrap_err();
            assert_eq!(err.key, format!("build.{key}"), "{err}");
        }
    }

    #[test]
    fn every_refused_build_key_is_named_in_one_error() {
        let text =
            "---\nbuild:\n  out: dist\n  patterns: {skills: a/*}\n  harnesses: [claude]\n---\n";
        let err = parse_root_file(text).unwrap_err();
        assert_eq!(err.key, "build.out`, `build.patterns", "{err}");
        assert_eq!(
            refused_root_file_keys(text).1,
            ["build.out", "build.patterns"]
        );
        assert!(
            refused_root_file_keys("---\nbuild:\n  harnesses: [claude]\n---\n")
                .1
                .is_empty()
        );
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
    fn a_known_key_of_the_wrong_shape_is_named() {
        let err = parse_root_file("---\nbuild:\n  harnesses: claude\n---\n").unwrap_err();
        assert_eq!(err.key, "build.harnesses");
        assert!(err.detail.contains("sequence"), "{err}");
    }

    #[test]
    fn an_unterminated_root_file_is_refused_and_a_readme_is_not() {
        for text in [
            "---\nbuild:\n  harnesses: [a]\n",
            "\u{feff}---\nbuild:\n  harnesses: [a]\n",
            "---\r\nlicense: MIT\r\n",
        ] {
            let err = parse_root_file(text).unwrap_err();
            assert!(err.detail.contains("never closes"), "{err}");
        }
        for text in ["", "# Notes\n", "# Notes\n\n---\n\nmore\n"] {
            parse_root_file(text).unwrap();
        }
    }

    /// A root file setting every shared default and a block of each harness.
    const EVERY_KEY: &str = "author: Ada\n\
         license: MIT\n\
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
    fn to_config_carries_every_shared_default() {
        let root = parse_root_file(&format!("---\n{EVERY_KEY}---\n")).unwrap();
        assert!(root.warnings.is_empty(), "{:?}", root.warnings);
        let config = set_keys(&root.to_config());
        let defaults = set_keys(&root.defaults);
        for (key, value) in &defaults {
            assert_eq!(config.get(key), Some(value), "{key}");
        }
        assert!(config.get("name").is_none());
        assert_eq!(
            root.defaults.blocks.keys().collect::<Vec<_>>(),
            ["claude", "codex", "cursor", "pi"]
        );
    }

    #[test]
    fn a_root_file_serializes_its_defaults_flat() {
        let root = parse_root_file("---\nlicense: MIT\nauthor: Ada\n---\n").unwrap();
        let written = serde_json::to_value(&root).unwrap();
        assert_eq!(written["defaults"]["license"], "MIT");
        assert_eq!(written["defaults"]["author"], "Ada");
        let read: RootFile = serde_json::from_value(written).unwrap();
        assert_eq!(read, root);
        let empty = serde_json::to_value(SharedDefaults::default()).unwrap();
        assert_eq!(empty, serde_json::json!({}));
    }
}
