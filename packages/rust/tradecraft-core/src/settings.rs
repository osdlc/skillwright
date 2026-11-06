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
use crate::skill::split_frontmatter;

/// The root file's name.
pub const ROOT_FILE: &str = "skillwright.md";

/// The marketplace's own fields: the root file's top-level keys.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MarketplaceFields {
    /// The marketplace's name. Absent means the root folder's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// One line on what the marketplace holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Codex's `interface.displayName`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Claude's and Cursor's marketplace `owner`; a string or
    /// `{name, email, url}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<serde_json::Value>,
    /// Default plugin author; falls back to `owner`. Inherited by every
    /// plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<serde_json::Value>,
    /// Default plugin license, as an SPDX identifier. Inherited by every
    /// plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

/// Where `publish` writes when `--to` is not given.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DestinationSetting {
    /// The destination root directory, relative to the root file.
    pub root: String,
    /// The target rendered into the root.
    pub target: String,
}

/// One `build.assets` entry: a file or folder copied into a plugin's root.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
pub struct BuildSettings {
    /// Where the harness trees are written, relative to the root file.
    /// Absent means beside it.
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
    /// Refuse a note with a `name` and no `description` instead of
    /// publishing a filler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_descriptions: Option<bool>,
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
}

/// A parsed root file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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

/// The marketplace fields the root file reads.
pub const MARKETPLACE_KEYS: &[&str] = &[
    "name",
    "description",
    "display_name",
    "owner",
    "author",
    "license",
];

/// The keys of the `build:` block.
pub const BUILD_KEYS: &[&str] = &[
    "out",
    "harnesses",
    "namespace",
    "launcher_path",
    "require_descriptions",
    "destinations",
    "scan_locations",
    "assets",
    "hooks",
];

/// A YAML value read as `T` through its JSON equivalent, so free-shaped
/// fields come out as the JSON a manifest carries.
fn from_yaml<T: DeserializeOwned>(value: &serde_yaml::Value) -> Result<T, serde_json::Error> {
    serde_json::to_value(value).and_then(serde_json::from_value)
}

/// Read a root file's text. A file with no frontmatter is a root file with
/// every default.
///
/// # Errors
///
/// Frontmatter that is not a YAML mapping, and a known key whose value has
/// the wrong shape.
pub fn parse_root_file(text: &str) -> Result<RootFile, SettingsError> {
    let (frontmatter, _body) = split_frontmatter(text);
    let doc: serde_yaml::Value = if frontmatter.trim().is_empty() {
        serde_yaml::Value::Null
    } else {
        serde_yaml::from_str(frontmatter).map_err(|err| SettingsError {
            key: ROOT_FILE.to_owned(),
            what: "valid YAML frontmatter",
            detail: err.to_string(),
        })?
    };
    let map = match doc {
        serde_yaml::Value::Null => serde_yaml::Mapping::new(),
        serde_yaml::Value::Mapping(map) => map,
        _ => {
            return Err(SettingsError {
                key: ROOT_FILE.to_owned(),
                what: "a frontmatter mapping",
                detail: "the frontmatter is not a mapping of keys".to_owned(),
            });
        }
    };

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
            known if MARKETPLACE_KEYS.contains(&known) => {
                marketplace.insert(serde_yaml::Value::String(key), value);
            }
            _ => root.warnings.push(format!(
                "`{key}` is not a key skillwright reads in {ROOT_FILE}; it is ignored"
            )),
        }
    }
    root.marketplace = field(
        "marketplace fields",
        &serde_yaml::Value::Mapping(marketplace),
        "the marketplace's fields",
    )?;
    Ok(root)
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
    let map = match value {
        serde_yaml::Value::Null => return Ok(BuildSettings::default()),
        serde_yaml::Value::Mapping(map) => map,
        _ => {
            return Err(SettingsError {
                key: "build".to_owned(),
                what: "a mapping",
                detail: "`build:` holds keys such as `out` and `harnesses`".to_owned(),
            });
        }
    };
    let mut kept = serde_yaml::Mapping::new();
    for (key, value) in map {
        match key.as_str() {
            Some(name) if BUILD_KEYS.contains(&name) => {
                kept.insert(key.clone(), value.clone());
            }
            Some(name) => warnings.push(format!(
                "`build.{name}` is not a key skillwright reads; it is ignored"
            )),
            None => warnings.push("a `build:` key that is not a string is ignored".to_owned()),
        }
    }
    field("build", &serde_yaml::Value::Mapping(kept), "a build block")
}

impl RootFile {
    /// Fill what this file leaves unset from the workspace file above it.
    ///
    /// The marketplace fields every plugin inherits (`author`, `license`)
    /// inherit from the workspace too. So do the `build:` settings that are
    /// not paths (`harnesses`, `namespace`, `launcher_path`,
    /// `require_descriptions`): a path in the workspace file names a place
    /// relative to the workspace, not to each marketplace.
    pub fn inherit(&mut self, workspace: &RootFile) {
        let fields = &mut self.marketplace;
        if fields.author.is_none() {
            fields.author.clone_from(&workspace.marketplace.author);
        }
        if fields.license.is_none() {
            fields.license.clone_from(&workspace.marketplace.license);
        }
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
        if build.require_descriptions.is_none() {
            build.require_descriptions = shared.require_descriptions;
        }
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
            display_name: fields.display_name.clone(),
            owner: fields.owner.clone(),
            description: fields.description.clone(),
            author: fields.author.clone(),
            license: fields.license.clone(),
            plugins: BTreeMap::new(),
            require_descriptions: self.build.require_descriptions.unwrap_or(false),
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
    fn a_root_file_maps_onto_a_build_config() {
        let root = parse_root_file(
            "---\n\
             name: osdlc\n\
             owner: { name: Ada }\n\
             license: MIT\n\
             build:\n  \
               out: dist\n  \
               harnesses: [claude]\n  \
               require_descriptions: true\n  \
               launcher_path: bin/tool\n\
             ---\n\
             # osdlc\n\nThe body is not read.\n",
        )
        .unwrap();
        assert!(root.warnings.is_empty(), "{:?}", root.warnings);
        assert_eq!(root.build.out.as_deref(), Some("dist"));
        let config = root.to_config("folder");
        assert_eq!(config.name.as_deref(), Some("osdlc"));
        assert_eq!(config.owner, Some(serde_json::json!({"name": "Ada"})));
        assert_eq!(config.license.as_deref(), Some("MIT"));
        assert!(config.require_descriptions);
        assert_eq!(config.launcher_path.as_deref(), Some("bin/tool"));
        assert_eq!(root.harness_set(), BTreeSet::from(["claude".to_owned()]));
    }

    #[test]
    fn a_file_with_no_frontmatter_takes_every_default() {
        let root = parse_root_file("# Just a README\n").unwrap();
        assert_eq!(root, RootFile::default());
        let config = root.to_config("My Tools");
        assert_eq!(config.name.as_deref(), Some("my-tools"));
        assert!(!config.require_descriptions);
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
    fn a_known_key_of_the_wrong_shape_is_named() {
        let err = parse_root_file("---\nbuild:\n  harnesses: claude\n---\n").unwrap_err();
        assert_eq!(err.key, "build");
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
        assert_eq!(member.marketplace.license.as_deref(), Some("Apache-2.0"));
        assert_eq!(member.marketplace.author, Some(serde_json::json!("Ada")));
        assert_eq!(member.build.harnesses, Some(vec!["claude".to_owned()]));
        // A path is the member's own.
        assert_eq!(member.build.out, None);
    }
}
