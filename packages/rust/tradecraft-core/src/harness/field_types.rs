//! The value each harness-block key must have.
//!
//! A container's keys take the type harness-reference records for them in
//! each harness's `plugins/manifest-fields` and `plugins/marketplace-fields`
//! facts. Where a key reaches several files, or a fact holds several
//! observations, a value any of them accepts passes. A key the facts give as
//! dotted subfields only (`policy.installation`) is a mapping.
//!
//! A component's keys take the shape [`crate::keys::KEYS`] lists for a skill.
//! A key with neither has no type here and is not checked. A test names each
//! container key the facts leave untyped.

use serde_json::Value;

use crate::frontmatter::EntityKind;
use crate::keys::Shape;

/// One kind of JSON value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Json {
    String,
    Bool,
    Object,
    /// A list of anything.
    List,
    /// A list of strings.
    StringList,
}

/// The kinds of value one key accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldType(&'static [Json]);

use Json::{Bool, List, Object, String as Str, StringList};

const STRING: FieldType = FieldType(&[Str]);
const BOOL: FieldType = FieldType(&[Bool]);
const OBJECT: FieldType = FieldType(&[Object]);
const LIST: FieldType = FieldType(&[List]);
const STRINGS: FieldType = FieldType(&[StringList]);
const STRING_OR_OBJECT: FieldType = FieldType(&[Str, Object]);
const STRING_OR_STRINGS: FieldType = FieldType(&[Str, StringList]);
const STRING_OR_LIST: FieldType = FieldType(&[Str, List]);
const STRING_OBJECT_OR_LIST: FieldType = FieldType(&[Str, Object, List]);
const PATHS_OR_OBJECT: FieldType = FieldType(&[Str, StringList, Object]);

impl FieldType {
    /// Whether `value` is one of the kinds. `null` never is: a key set to
    /// nothing ships as `null`, which no fact allows.
    pub(crate) fn accepts(self, value: &Value) -> bool {
        self.0.iter().any(|kind| match kind {
            Str => value.is_string(),
            Bool => value.is_boolean(),
            Object => value.is_object(),
            List => value.is_array(),
            StringList => value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string)),
        })
    }

    /// The kinds, as a refusal words them: `a string or a mapping`.
    pub(crate) fn label(self) -> String {
        let words: Vec<&str> = self
            .0
            .iter()
            .map(|kind| match kind {
                Str => "a string",
                Bool => "a boolean",
                Object => "a mapping",
                List => "a list",
                StringList => "a list of strings",
            })
            .collect();
        words.join(" or ")
    }
}

/// What a value is, for a refusal: `a number`, `a list`.
pub(crate) fn found(value: &Value) -> &'static str {
    match value {
        Value::Null => "nothing",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "a mapping",
    }
}

/// Claude Code's `plugin.json`, marketplace entry and marketplace keys.
const CLAUDE: &[(&str, FieldType)] = &[
    ("$schema", STRING),
    ("name", STRING),
    ("displayName", STRING),
    ("version", STRING),
    ("description", STRING),
    ("author", OBJECT),
    ("homepage", STRING),
    ("repository", STRING),
    ("license", STRING),
    ("keywords", STRINGS),
    ("metadata", OBJECT),
    ("icon", STRING),
    ("documentationUrl", STRING),
    ("supportUrl", STRING),
    ("privacyPolicyUrl", STRING),
    ("termsOfServiceUrl", STRING),
    ("defaultEnabled", BOOL),
    // `(string | object)[]` in `plugin.json`, `array` in an entry.
    ("dependencies", LIST),
    ("settings", OBJECT),
    ("userConfig", OBJECT),
    ("types", STRING),
    ("channels", LIST),
    ("skills", STRING_OR_STRINGS),
    ("commands", PATHS_OR_OBJECT),
    ("agents", STRING_OR_STRINGS),
    ("hooks", STRING_OBJECT_OR_LIST),
    ("mcpServers", STRING_OBJECT_OR_LIST),
    ("lspServers", STRING_OBJECT_OR_LIST),
    ("outputStyles", STRING_OR_STRINGS),
    ("workflows", STRING_OR_STRINGS),
    ("experimental", OBJECT),
    ("themes", STRING_OR_STRINGS),
    ("monitors", STRING_OR_LIST),
    ("source", STRING_OR_OBJECT),
    ("category", STRING),
    ("tags", STRINGS),
    ("strict", BOOL),
    ("relevance", OBJECT),
    ("headers", OBJECT),
    ("headersHelper", STRING),
    ("owner", OBJECT),
    ("plugins", LIST),
    ("forceRemoveDeletedPlugins", BOOL),
    ("allowCrossMarketplaceDependenciesOn", STRINGS),
    ("renames", OBJECT),
];

/// Codex's `.codex-plugin/plugin.json`, marketplace entry and marketplace
/// keys. `author`, `interface` and `policy` are given as dotted subfields.
const CODEX: &[(&str, FieldType)] = &[
    ("name", STRING),
    ("version", STRING),
    ("description", STRING),
    ("author", OBJECT),
    ("license", STRING),
    ("keywords", STRINGS),
    ("skills", STRING_OR_STRINGS),
    ("commands", STRING_OR_STRINGS),
    ("mcpServers", STRING_OR_OBJECT),
    ("apps", STRING),
    // `string | string[] | object | object[]`.
    ("hooks", STRING_OBJECT_OR_LIST),
    ("interface", OBJECT),
    ("extensions", OBJECT),
    ("plugins", LIST),
    ("source", STRING_OR_OBJECT),
    ("policy", OBJECT),
    ("category", STRING),
];

/// Cursor's `plugin.json`, marketplace entry and marketplace keys, from its
/// docs and its schema.
const CURSOR: &[(&str, FieldType)] = &[
    ("name", STRING),
    ("description", STRING),
    ("version", STRING),
    ("author", OBJECT),
    ("homepage", STRING),
    ("repository", STRING),
    ("license", STRING),
    // `array` in the docs, `string[]` in the schema.
    ("keywords", LIST),
    ("logo", STRING),
    ("rules", STRING_OR_LIST),
    ("agents", STRING_OR_LIST),
    ("skills", STRING_OR_LIST),
    ("commands", STRING_OR_LIST),
    ("hooks", STRING_OR_OBJECT),
    ("mcpServers", STRING_OBJECT_OR_LIST),
    ("variables", OBJECT),
    ("displayName", STRING),
    ("minClientVersions", OBJECT),
    ("publisher", STRING),
    ("category", STRING),
    ("tags", LIST),
    ("owner", OBJECT),
    ("plugins", LIST),
    ("metadata", OBJECT),
    ("source", STRING_OR_OBJECT),
];

/// Pi's `package.json` keys. `pi` is given as dotted subfields.
const PI: &[(&str, FieldType)] = &[
    ("name", STRING),
    ("version", STRING),
    ("keywords", STRINGS),
    ("dependencies", OBJECT),
    ("peerDependencies", OBJECT),
    ("pi", OBJECT),
];

/// Container keys a harness reads that no fact gives a type for, by harness.
/// They are not checked. `packs` and `naming` have their own checks.
#[cfg(test)]
const UNTYPED: &[(&str, &[&str])] = &[(
    "pi",
    &["description", "author", "license", "repository", "homepage"],
)];

fn facts(harness: &str) -> &'static [(&'static str, FieldType)] {
    match harness {
        "claude" => CLAUDE,
        "codex" => CODEX,
        "cursor" => CURSOR,
        "pi" => PI,
        _ => &[],
    }
}

/// The type of `key` in a skill's or command's `harness` block, from the key
/// table. `None` for a key the table does not list.
pub(crate) fn component_type(key: &str) -> Option<FieldType> {
    let spec = crate::keys::lookup(EntityKind::Skill, key)?;
    Some(match (key, spec.shape) {
        // The table's summary: a list, or one space-separated string.
        ("allowed-tools", _) => STRING_OR_STRINGS,
        // A typed list, or Claude's own list of names or one string.
        ("arguments", _) => STRING_OR_LIST,
        // A nested key skillwright ships under `metadata` keeps its own shape.
        (_, Shape::StringMap) => OBJECT,
        (_, Shape::String) => STRING,
        (_, Shape::Bool) => BOOL,
        (_, Shape::StringList) => STRINGS,
        _ => return None,
    })
}

/// The type of `key` in a marketplace's or plugin's `harness` block: the
/// facts' type, or for a key that passes to members, [`component_type`].
pub(crate) fn container_type(harness: &str, key: &str) -> Option<FieldType> {
    facts(harness)
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, ty)| *ty)
        .or_else(|| {
            crate::model::member_block_keys(EntityKind::Skill, harness)
                .contains(key)
                .then(|| component_type(key))
                .flatten()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every key a container block accepts has a type, or is named untyped.
    #[test]
    fn every_container_key_is_typed_or_named() {
        for profile in crate::harness::with_plugin_format() {
            let format = profile.format();
            let untyped: &[&str] = UNTYPED
                .iter()
                .find(|(harness, _)| *harness == profile.name)
                .map_or(&[], |(_, keys)| keys);
            let keys = format
                .manifest_fields
                .iter()
                .chain(format.entry_fields)
                .chain(format.marketplace_fields);
            for key in keys {
                assert!(
                    container_type(profile.name, key).is_some() != untyped.contains(key),
                    "{}.{key}",
                    profile.name
                );
            }
        }
    }

    #[test]
    fn a_wrong_kind_of_value_is_not_accepted() {
        assert!(!STRING_OR_STRINGS.accepts(&json!(5)));
        assert!(STRING_OR_STRINGS.accepts(&json!("Read Grep")));
        assert!(!STRINGS.accepts(&json!([1])));
        assert!(!OBJECT.accepts(&json!(5)));
        assert!(!STRING.accepts(&Value::Null));
        assert_eq!(STRING_OR_OBJECT.label(), "a string or a mapping");
    }

    #[test]
    fn the_known_keys_take_their_table_or_fact_type() {
        assert_eq!(component_type("model"), Some(STRING));
        assert_eq!(component_type("allowed-tools"), Some(STRING_OR_STRINGS));
        assert_eq!(component_type("permissionMode"), None);
        assert_eq!(container_type("codex", "policy"), Some(OBJECT));
        assert_eq!(container_type("claude", "model"), Some(STRING));
        assert_eq!(container_type("pi", "author"), None);
    }
}
