//! `skillwright.md`, the root file: the vault's configuration.
//!
//! Its frontmatter's top-level keys are the shared defaults every marketplace
//! and plugin inherits. Settings for how skillwright runs sit under `build:`.
//! The body is ignored. The folder holding the file is the vault root.
//!
//! A marketplace's own fields are in its note, `marketplaces/<m>/<m>.md`,
//! read by [`parse_marketplace_note`]. The root file's `name`, `owner`,
//! `description`, `version` and `display_name` are not read.
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

/// A marketplace's own fields, as its note `<m>.md` writes them, or the
/// top-level keys of the root file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MarketplaceFields {
    /// The marketplace's name. Absent means its folder's name.
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
    /// The top-level keys. Only the shared defaults among them are read
    /// ([`RootFile::to_config`]).
    #[serde(default)]
    pub marketplace: MarketplaceFields,
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
/// Frontmatter that is not a YAML mapping, and a known key whose value has
/// the wrong shape.
pub fn parse_root_file(text: &str) -> Result<RootFile, SettingsError> {
    read_fields(frontmatter_map(text)?, true)
}

/// Read a marketplace note's text, `marketplaces/<m>/<m>.md`, into its fields.
/// A key that is not a marketplace field is left for the scan to warn about.
///
/// # Errors
///
/// Frontmatter that is not a YAML mapping, and a marketplace key whose value
/// has the wrong shape.
pub fn parse_marketplace_note(text: &str) -> Result<MarketplaceFields, SettingsError> {
    read_fields(frontmatter_map(text)?, false).map(|root| root.marketplace)
}

/// The fields of a root file (`root_file`) or a marketplace note.
fn read_fields(map: serde_yaml::Mapping, root_file: bool) -> Result<RootFile, SettingsError> {
    let mut root = RootFile::default();
    let mut marketplace = serde_yaml::Mapping::new();
    for (key, value) in map {
        let Some(key) = key.as_str().map(str::to_owned) else {
            root.warnings
                .push("a frontmatter key that is not a string is ignored".to_owned());
            continue;
        };
        match key.as_str() {
            "build" if root_file => root.build = build(&value, &mut root.warnings)?,
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

/// The refused `build:` keys of a root file's text, as `build.<key>`, for the
/// layout report (spec section 11 case 7). Empty when the text cannot be read:
/// [`parse_root_file`] says what is wrong with it.
pub(crate) fn refused_build_keys(text: &str) -> Vec<String> {
    let Ok(map) = frontmatter_map(text) else {
        return Vec::new();
    };
    map.get("build")
        .and_then(serde_yaml::Value::as_mapping)
        .map(refused_in)
        .unwrap_or_default()
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
    /// Fill what this file leaves unset from `from`.
    ///
    /// The top-level fields come down where the key table says a marketplace
    /// inherits them ([`crate::keys::inherits`]): the identity fields it
    /// marks, `targets`, and the harness blocks key by key. So do
    /// `harnesses`, `launcher_path`, `bump`, `assets` and `hooks`.
    ///
    /// Key by key: a value this file sets wins. The `build:` keys are not in
    /// the key table, so their list is this function's.
    pub fn inherit(&mut self, from: &RootFile) {
        use crate::frontmatter::EntityKind::Marketplace;
        let fields = &mut self.marketplace;
        let shared = &from.marketplace;
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
        let shared = &from.build;
        if build.harnesses.is_none() {
            build.harnesses.clone_from(&shared.harnesses);
        }
        if build.launcher_path.is_none() {
            build.launcher_path.clone_from(&shared.launcher_path);
        }
        if build.bump.is_none() {
            build.bump.clone_from(&shared.bump);
        }
        if build.assets.is_empty() {
            build.assets.clone_from(&shared.assets);
        }
        if build.hooks.is_empty() {
            build.hooks.clone_from(&shared.hooks);
        }
    }

    /// The build config this file describes: its shared defaults, the keys
    /// the key table marks as inherited. Its `name`, `description` and other
    /// marketplace fields are not read: each marketplace's note gives them.
    pub fn to_config(&self) -> MarketplaceConfig {
        let fields = &self.marketplace;
        let mut identity = ContainerIdentity::default();
        identity.fill_unset_from(
            &fields.identity,
            crate::frontmatter::EntityKind::Marketplace,
        );
        MarketplaceConfig {
            name: None,
            description: None,
            identity,
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
    fn a_set_build_value_overrides_the_shared_key_by_key() {
        let shared = parse_root_file(
            "---\nlicense: MIT\nbuild:\n  harnesses: [claude]\n  \
             launcher_path: bin/ws\n---\n",
        )
        .unwrap();
        let mut member = parse_root_file("---\nbuild:\n  launcher_path: bin/m\n---\n").unwrap();
        member.inherit(&shared);

        // What the member sets wins.
        assert_eq!(member.build.launcher_path.as_deref(), Some("bin/m"));
        // Every other key still comes down.
        assert_eq!(member.build.harnesses, Some(vec!["claude".to_owned()]));
        assert_eq!(member.marketplace.identity.license.as_deref(), Some("MIT"));
    }

    #[test]
    fn the_config_a_root_file_describes_has_no_name() {
        let config = root_file_config("---\nname: ws\nlicense: MIT\n---\n").unwrap();
        assert_eq!(config.name, None);
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
               harnesses: [claude]\n  \
               launcher_path: bin/tool\n\
             ---\n\
             # osdlc\n\nThe body is not read.\n",
        )
        .unwrap();
        assert!(root.warnings.is_empty(), "{:?}", root.warnings);
        assert_eq!(root.marketplace.name.as_deref(), Some("osdlc"));
        let config = root.to_config();
        assert_eq!(config.name, None);
        // `owner` is each marketplace's own; `license` is a shared default.
        assert_eq!(config.identity.owner, None);
        assert_eq!(config.identity.license.as_deref(), Some("MIT"));
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
        assert_eq!(refused_build_keys(text), ["build.out", "build.patterns"]);
        assert!(refused_build_keys("---\nbuild:\n  harnesses: [claude]\n---\n").is_empty());
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
    fn build_bump_is_inherited() {
        let shared = parse_root_file("---\nbuild:\n  bump:\n    patch: timestamp\n---\n").unwrap();
        let mut member = parse_root_file("---\nname: one\n---\n").unwrap();
        member.inherit(&shared);
        assert_eq!(member.build.bump.unwrap().patch, PatchScheme::Timestamp);
    }

    #[test]
    fn a_known_key_of_the_wrong_shape_is_named() {
        let err = parse_root_file("---\nbuild:\n  harnesses: claude\n---\n").unwrap_err();
        assert_eq!(err.key, "build.harnesses");
        assert!(err.detail.contains("sequence"), "{err}");
    }

    #[test]
    fn shared_defaults_are_inherited() {
        let shared =
            parse_root_file("---\nlicense: MIT\nauthor: Ada\nbuild:\n  harnesses: [claude]\n---\n")
                .unwrap();
        let mut member = parse_root_file("---\nname: one\nlicense: Apache-2.0\n---\n").unwrap();
        member.inherit(&shared);
        assert_eq!(
            member.marketplace.identity.license.as_deref(),
            Some("Apache-2.0")
        );
        assert_eq!(
            member.marketplace.identity.author,
            Some(serde_json::json!("Ada"))
        );
        assert_eq!(member.build.harnesses, Some(vec!["claude".to_owned()]));
    }

    #[test]
    fn build_lists_are_inherited_key_by_key() {
        let shared = parse_root_file(
            "---\nbuild:\n  assets:\n    - {from: a, to: b}\n  hooks:\n    \
             - {from: h.json}\n---\n",
        )
        .unwrap();
        let mut member =
            parse_root_file("---\nbuild:\n  assets:\n    - {from: m, to: n}\n---\n").unwrap();
        member.inherit(&shared);
        assert_eq!(member.build.assets[0].from, "m");
        assert_eq!(member.build.hooks, shared.build.hooks);
    }

    #[test]
    fn an_unterminated_root_file_is_refused_and_a_readme_is_not() {
        for text in [
            "---\nbuild:\n  harnesses: [a]\n",
            "\u{feff}---\nbuild:\n  harnesses: [a]\n",
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
    fn a_file_takes_exactly_the_inheritable_fields() {
        let shared = parse_root_file(&format!("---\n{EVERY_KEY}---\n")).unwrap();
        assert!(shared.warnings.is_empty(), "{:?}", shared.warnings);

        let mut member = parse_root_file("---\n---\n").unwrap();
        member.inherit(&shared);
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
        let given = set_keys(&shared.marketplace);
        for key in &keys {
            assert_eq!(took[*key], given[*key], "{key}");
        }

        // A file that sets each key keeps its own, and a block key it sets
        // keeps its value while the other keys in that block come down.
        let mut own = parse_root_file(
            "---\nname: own\ndescription: Mine.\nowner: Me\nauthor: Bo\n\
             display_name: Own\nlicense: ISC\nversion: '1.0.0'\ntargets: [pi]\n\
             repository: https://own.example/repo\nhomepage: https://own.example\n\
             claude:\n  strict: false\n---\n",
        )
        .unwrap();
        let before = set_keys(&own.marketplace);
        own.inherit(&shared);
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
    fn to_config_carries_only_the_inheritable_fields() {
        let root = parse_root_file(&format!("---\n{EVERY_KEY}---\n")).unwrap();
        let config = set_keys(&root.to_config());
        let mut inherited = parse_root_file("---\n---\n").unwrap();
        inherited.inherit(&root);
        let fields = set_keys(&inherited.marketplace);
        for key in marketplace_keys().chain(["blocks"]) {
            assert_eq!(config.get(key), fields.get(key), "{key}");
        }
        assert!(config.get("name").is_none());
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
