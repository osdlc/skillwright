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
    /// The marketplace's own version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Which harnesses get what the marketplace holds. Inherited by every
    /// plugin and member with none of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<String>>,
    /// The source repository URL. Inherited by every plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// The documentation URL. Inherited by every plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// The harness blocks (`claude:`, `codex:`, `cursor:`, `pi:`), keyed by
    /// harness. Inherited by every plugin, key by key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub blocks: BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
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
    /// Globs of what never ships, relative to the root file. Experimental.
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
    "version",
    "targets",
    "repository",
    "homepage",
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
    "exclude",
    "patterns",
];

/// A YAML value read as `T` through its JSON equivalent, so free-shaped
/// fields come out as the JSON a manifest carries.
fn from_yaml<T: DeserializeOwned>(value: &serde_yaml::Value) -> Result<T, serde_json::Error> {
    serde_json::to_value(value).and_then(serde_json::from_value)
}

/// Refuses an `owner` or `author` that the key table's
/// [`crate::keys::Shape::Person`] does not accept. Their fields are untyped,
/// so nothing else would catch `author: 5` before it reached a manifest.
fn check_person(key: &str, value: &serde_yaml::Value) -> Result<(), SettingsError> {
    let person = crate::keys::lookup(crate::frontmatter::EntityKind::Marketplace, key)
        .is_some_and(|spec| spec.shape == crate::keys::Shape::Person);
    if !person {
        return Ok(());
    }
    match serde_json::to_value(value) {
        Ok(json) if crate::keys::Shape::Person.accepts(&json) => Ok(()),
        _ => Err(SettingsError {
            key: key.to_owned(),
            what: "a name or a `{name, email, url}` mapping",
            detail: format!("{value:?}"),
        }),
    }
}

/// Whether a file split into `frontmatter` and `body` opened frontmatter it
/// never closed: read as a body, its settings would silently be lost.
fn unterminated(frontmatter: &str, body: &str) -> bool {
    let first = body.trim_start_matches('\u{feff}').lines().next();
    frontmatter.is_empty() && first.is_some_and(|line| line.trim_end() == "---")
}

/// Read a root file's text. A file with no frontmatter is a root file with
/// every default.
///
/// # Errors
///
/// Frontmatter that is not a YAML mapping, and a known key whose value has
/// the wrong shape.
pub fn parse_root_file(text: &str) -> Result<RootFile, SettingsError> {
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
                check_person(known, &value)?;
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
                    for unknown in crate::discover::unknown_container_block_keys(
                        &known,
                        crate::frontmatter::EntityKind::Marketplace,
                    ) {
                        root.warnings.push(format!(
                            "`{unknown}` reaches no file that harness reads; it is ignored"
                        ));
                    }
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
    for (key, value) in [
        ("repository", &root.marketplace.repository),
        ("homepage", &root.marketplace.homepage),
    ] {
        if let Some(url) = value
            && !crate::keys::is_url(url)
        {
            return Err(SettingsError {
                key: key.to_owned(),
                what: "an http or https URL",
                detail: format!("`{url}`"),
            });
        }
    }
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
    let mut exclude = Vec::new();
    let mut patterns = FolderPatterns::default();
    for (key, value) in map {
        match key.as_str() {
            Some("exclude") => exclude = globs("build.exclude", value)?,
            Some("patterns") => patterns = folder_patterns(value, warnings)?,
            Some(name) if BUILD_KEYS.contains(&name) => {
                kept.insert(key.clone(), value.clone());
            }
            Some(name) => warnings.push(format!(
                "`build.{name}` is not a key skillwright reads; it is ignored"
            )),
            None => warnings.push("a `build:` key that is not a string is ignored".to_owned()),
        }
    }
    let build: BuildSettings = field("build", &serde_yaml::Value::Mapping(kept), "a build block")?;
    Ok(BuildSettings {
        exclude,
        patterns,
        ..build
    })
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
    /// The marketplace fields every plugin inherits (`author`, `license`,
    /// `targets`, `repository`, `homepage` and the harness blocks, key by key)
    /// inherit from the workspace too. So do `harnesses`, `namespace`,
    /// `launcher_path`, `require_descriptions`, `exclude` and each key of
    /// `patterns`, whose globs read from each marketplace's own folder. An
    /// exclusion must not fail open. `out` is each marketplace's own.
    ///
    /// Key by key: a value this file sets wins, and every other key still
    /// comes from the workspace. The workspace root holds shared defaults and
    /// the `build:` settings for its members. The decided list of inherited
    /// fields is about frontmatter moving down notes, not this block.
    pub fn inherit(&mut self, workspace: &RootFile) {
        let fields = &mut self.marketplace;
        if fields.author.is_none() {
            fields.author.clone_from(&workspace.marketplace.author);
        }
        if fields.license.is_none() {
            fields.license.clone_from(&workspace.marketplace.license);
        }
        if fields.targets.is_none() {
            fields.targets.clone_from(&workspace.marketplace.targets);
        }
        if fields.repository.is_none() {
            fields
                .repository
                .clone_from(&workspace.marketplace.repository);
        }
        if fields.homepage.is_none() {
            fields.homepage.clone_from(&workspace.marketplace.homepage);
        }
        for (harness, shared) in &workspace.marketplace.blocks {
            let block = fields.blocks.entry(harness.clone()).or_default();
            for (key, value) in shared {
                block.entry(key.clone()).or_insert_with(|| value.clone());
            }
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
        if build.exclude.is_empty() {
            build.exclude.clone_from(&shared.exclude);
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
            display_name: fields.display_name.clone(),
            owner: fields.owner.clone(),
            description: fields.description.clone(),
            author: fields.author.clone(),
            license: fields.license.clone(),
            version: fields.version.clone(),
            targets: fields.targets.clone(),
            repository: fields.repository.clone(),
            homepage: fields.homepage.clone(),
            blocks: fields.blocks.clone(),
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
             namespace: sw\n  launcher_path: bin/ws\n  require_descriptions: true\n  \
             out: dist\n---\n",
        )
        .unwrap();
        let mut member =
            parse_root_file("---\nbuild:\n  namespace: acme\n  require_descriptions: false\n---\n")
                .unwrap();
        member.inherit(&workspace);

        // What the member sets wins, a `false` included.
        assert_eq!(member.build.namespace.as_deref(), Some("acme"));
        assert_eq!(member.build.require_descriptions, Some(false));
        // Every other key still comes down.
        assert_eq!(member.build.harnesses, Some(vec!["claude".to_owned()]));
        assert_eq!(member.build.launcher_path.as_deref(), Some("bin/ws"));
        assert_eq!(member.marketplace.license.as_deref(), Some("MIT"));
        // A path names a place relative to the workspace, so it does not.
        assert_eq!(member.build.out, None);
    }

    #[test]
    fn the_config_a_text_describes_takes_the_folder_s_name_by_default() {
        let config = root_file_config("---\nlicense: MIT\n---\n", "My House").unwrap();
        assert_eq!(config.name.as_deref(), Some("my-house"));
        assert_eq!(config.license.as_deref(), Some("MIT"));
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
        // The output is the member's own.
        assert_eq!(member.build.out, None);
    }

    #[test]
    fn a_marketplace_inherits_exclusions_and_patterns_key_by_key() {
        let workspace = parse_root_file(
            "---\nworkspace: true\nbuild:\n  exclude: '**/*.draft.md'\n  patterns:\n    \
             skills: notes/*\n    plugins: packs/*\n---\n",
        )
        .unwrap();
        let mut member =
            parse_root_file("---\nbuild:\n  patterns:\n    skills: mine/*\n---\n").unwrap();
        member.inherit(&workspace);
        assert_eq!(member.build.exclude, ["**/*.draft.md"]);
        assert_eq!(member.build.patterns.skills, ["mine/*"]);
        assert_eq!(member.build.patterns.plugins, ["packs/*"]);
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
}
