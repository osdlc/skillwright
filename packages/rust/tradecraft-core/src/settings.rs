//! `skillwright.md`, the root file: the vault's configuration.
//!
//! Its top-level keys are the shared defaults, the keys the key table marks
//! as inherited ([`SharedDefaults`]). Settings for how skillwright runs sit
//! under `build:`. The body is ignored. The folder holding the file is the
//! vault root.
//!
//! The CLI and other hosts read the file through [`parse_root_file`], so they
//! share one shape, and pass its [`SharedDefaults`] to a build.
//! Finding the file stays with the caller: this module parses text and never
//! touches a filesystem.
//!
//! An unknown key warns and is ignored. `workspace`, `targets`, `name`, a key
//! the table lists but does not mark as inherited, a removed `build:` key and
//! `packs` in a harness block are refused. A known key with a value of the wrong
//! shape is refused, naming the key.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::frontmatter::EntityKind;
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
    /// The harness blocks (`claude:`, `codex:`, `cursor:`, `pi:`), keyed by
    /// harness. Inherited key by key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub blocks: BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
}

/// The `build:` block: how skillwright runs, never what lands in a manifest.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BuildSettings {
    /// Which harnesses to build. Absent means all of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harnesses: Option<Vec<String>>,
    /// How `bump` writes a version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<BumpSettings>,
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
pub const BUILD_KEYS: &[&str] = &["harnesses", "bump"];

/// The removed `build:` keys: refused, naming the key, rather than warned
/// about.
pub const REFUSED_BUILD_KEYS: &[&str] = &[
    "destinations",
    "scan_locations",
    "out",
    "patterns",
    "exclude",
    "namespace",
    "assets",
    "launcher_path",
    "hooks",
];

/// The top-level keys the root file refuses besides the key table's rows
/// that are not inherited.
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
/// Frontmatter that is not a YAML mapping or never closes; a key the root
/// file refuses (`workspace`, `targets`, `name` at the top level or in a
/// harness block, a key the table lists but does not mark as inherited, a
/// removed `build:` key, `packs` in a harness block); a known key whose value
/// has the wrong shape; an unknown harness in `build.harnesses`; and an
/// empty `build.harnesses`.
pub fn parse_root_file(text: &str) -> Result<RootFile, SettingsError> {
    let map = frontmatter_map(text)?;
    if let Some((key, detail)) = refused_in_root(&map).into_iter().next() {
        return Err(SettingsError {
            key,
            what: "a key of the root file",
            detail,
        });
    }
    let named = named_blocks_in(&map);
    if !named.is_empty() {
        return Err(SettingsError {
            key: named.join("`, `"),
            what: "a key of the root file",
            detail: crate::keys::NAME_REFUSAL.to_owned(),
        });
    }
    let blocks = refused_blocks_in(&map);
    if !blocks.is_empty() {
        return Err(SettingsError {
            key: blocks.join("`, `"),
            what: "a key skillwright reads",
            detail: PACKS_REFUSAL.to_owned(),
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
    check_harness_names("build.harnesses", root.build.harnesses.as_deref())?;
    // The spec does not say what an empty list builds, so it is refused.
    if root.build.harnesses.as_ref().is_some_and(Vec::is_empty) {
        return Err(SettingsError {
            key: "build.harnesses".to_owned(),
            what: "a list of harnesses",
            detail: "it is empty, which would build nothing. List the harnesses to build, \
                     or remove the key to build all four"
                .to_owned(),
        });
    }
    Ok(root)
}

/// Refuses a name in `names` that is not in [`crate::harness::KNOWN_TARGETS`].
fn check_harness_names(key: &str, names: Option<&[String]>) -> Result<(), SettingsError> {
    let unknown: Vec<&str> = names
        .into_iter()
        .flatten()
        .map(String::as_str)
        .filter(|name| !crate::harness::KNOWN_TARGETS.contains(name))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(SettingsError {
        key: key.to_owned(),
        what: "a list of harnesses",
        detail: format!(
            "it {}",
            crate::harness::profile::unknown_targets_detail(&unknown)
        ),
    })
}

/// Why the root file refuses the top-level `key`, or `None` when it reads or
/// warns about it (spec section 2 rules 3 and 4).
fn root_key_refusal(key: &str) -> Option<String> {
    if key == TARGETS_KEY {
        return Some(format!("it is a removed key; {TARGETS_REFUSAL}"));
    }
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
/// key it refuses with why (spec section 11 case 2), then each removed key
/// (case 7): a `build:` key as `build.<key>`, then `packs` in a harness block
/// as `<harness>.packs`. Empty when the text cannot be read: [`parse_root_file`]
/// says what is wrong with it.
pub(crate) fn refused_root_file_keys(text: &str) -> (Vec<(String, String)>, Vec<String>) {
    let Ok(map) = frontmatter_map(text) else {
        return (Vec::new(), Vec::new());
    };
    let mut removed = map
        .get("build")
        .and_then(serde_yaml::Value::as_mapping)
        .map(refused_in)
        .unwrap_or_default();
    removed.extend(refused_blocks_in(&map));
    (refused_in_root(&map), removed)
}

/// The removed `targets` key, refused in the root file and on every note.
pub(crate) const TARGETS_KEY: &str = "targets";

/// What a refusal of `targets` says.
pub(crate) const TARGETS_REFUSAL: &str = "the harnesses built are picked per run, with \
    `build.harnesses` in skillwright.md or `--harness`; delete it";

/// What a refusal of `packs` in a harness block says.
pub(crate) const PACKS_REFUSAL: &str = "each harness's packs folder is fixed; delete it";

/// `<harness>.name` for each harness block in `map` that sets `name`.
fn named_blocks_in(map: &serde_yaml::Mapping) -> Vec<String> {
    crate::keys::HARNESS_BLOCKS
        .iter()
        .filter(|harness| {
            map.get(**harness)
                .and_then(serde_yaml::Value::as_mapping)
                .is_some_and(|block| block.contains_key(crate::keys::NAME_KEY))
        })
        .map(|harness| format!("{harness}.{}", crate::keys::NAME_KEY))
        .collect()
}

/// The removed keys in a root file's harness blocks, as `<harness>.<key>`.
fn refused_blocks_in(map: &serde_yaml::Mapping) -> Vec<String> {
    let blocks = crate::keys::HARNESS_BLOCKS
        .iter()
        .filter_map(|harness| Some((*harness, map.get(*harness)?.as_mapping()?)));
    blocks
        .flat_map(|(harness, block)| {
            block
                .keys()
                .filter_map(serde_yaml::Value::as_str)
                .filter(|key| crate::keys::REFUSED_BLOCK_KEYS.contains(key))
                .map(move |key| format!("{harness}.{key}"))
        })
        .collect()
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
                if name == "bump" {
                    warn_bump_keys(value, warnings);
                }
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

/// Warn about each unknown key under `build.bump`, which its struct ignores
/// when it is read.
fn warn_bump_keys(value: &serde_yaml::Value, warnings: &mut Vec<String>) {
    let keys = value.as_mapping().into_iter().flat_map(|map| map.keys());
    for key in keys.filter_map(serde_yaml::Value::as_str) {
        if key != "patch" {
            warnings.push(format!(
                "`build.bump.{key}` is not a key skillwright reads; it is ignored"
            ));
        }
    }
}

impl RootFile {
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
                "repository: ftp://x",
                "`repository` is not an http or https URL: it is `ftp://x`",
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
        assert_eq!(keys, ["author", "license", "repository", "homepage"]);
        let every = SharedDefaults {
            author: Some(serde_json::json!("Ada")),
            license: Some("MIT".to_owned()),
            repository: Some("https://x".to_owned()),
            homepage: Some("https://x".to_owned()),
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
        let err = parse_root_file("---\ntargets: [claude]\n---\n").unwrap_err();
        assert_eq!(err.key, "targets");
        assert!(err.detail.contains("removed key"), "{err}");
        assert!(err.detail.contains("`build.harnesses`"), "{err}");
        assert!(err.detail.contains("`--harness`"), "{err}");
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
    fn a_harness_that_is_not_one_of_the_four_is_refused() {
        for line in [
            "build: {harnesses: [emacs]}",
            "build: {harnesses: [claude-code]}",
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
             build:\n  \
               harnesses: [claude]\n\
             ---\n\
             # osdlc\n\nThe body is not read.\n",
        )
        .unwrap();
        assert!(root.warnings.is_empty(), "{:?}", root.warnings);
        let config = crate::model::cascade::root_config(&root.defaults);
        assert_eq!(config.name, None);
        assert_eq!(config.description, None);
        assert_eq!(config.identity.author, Some(serde_json::json!("Ada")));
        assert_eq!(config.identity.license.as_deref(), Some("MIT"));
        assert_eq!(root.harness_set(), BTreeSet::from(["claude".to_owned()]));
    }

    #[test]
    fn a_file_with_no_frontmatter_takes_every_default() {
        let root = parse_root_file("# Just a README\n").unwrap();
        assert_eq!(root, RootFile::default());
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
            ("assets", "\n    - {from: bin, to: bin, plugin: tools}"),
            ("launcher_path", " bin/tool"),
            ("hooks", "\n    - {from: hooks.json, plugin: tools}"),
        ] {
            let err = parse_root_file(&format!("---\nbuild:\n  {key}:{value}\n---\n")).unwrap_err();
            assert_eq!(err.key, format!("build.{key}"), "{err}");
        }
    }

    #[test]
    fn packs_in_a_harness_block_is_refused_by_name() {
        let text = "---\nclaude:\n  packs: out\npi:\n  packs: x\n---\n";
        let err = parse_root_file(text).unwrap_err();
        assert_eq!(err.key, "claude.packs`, `pi.packs", "{err}");
        assert!(err.detail.contains("fixed"), "{err}");
        assert_eq!(refused_root_file_keys(text).1, ["claude.packs", "pi.packs"]);
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

    /// An empty `build.harnesses` would build nothing, or read as absent and
    /// build everything. The spec says neither, so it is refused.
    #[test]
    fn an_empty_build_harnesses_is_refused() {
        let err = parse_root_file("---\nbuild:\n  harnesses: []\n---\n").unwrap_err();
        assert_eq!(err.key, "build.harnesses");
        assert!(err.to_string().contains("remove the key"), "{err}");
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
    fn every_shared_default_reaches_the_cascade() {
        let root = parse_root_file(&format!("---\n{EVERY_KEY}---\n")).unwrap();
        assert!(root.warnings.is_empty(), "{:?}", root.warnings);
        let folded = crate::model::cascade::root_config(&root.defaults);
        let mut config = set_keys(&folded.identity);
        config.insert("blocks".to_owned(), serde_json::json!(folded.blocks));
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
