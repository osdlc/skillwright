//! The kind of value each harness-block key, and each bare spec key on a
//! component, must have.
//!
//! A container's keys take the type harness-reference records for them in
//! each harness's `plugins/manifest-fields` and `plugins/marketplace-fields`
//! facts. Where a key reaches several files, or a fact holds several
//! observations, a value any of them accepts passes. A key the facts give as
//! dotted subfields only (`policy.installation`) is a mapping.
//!
//! A component's keys take the type of the [`Shape`] [`crate::keys::KEYS`]
//! lists for its kind. The same type checks a key written bare and in a
//! block. A key with neither has no type here and is not checked. A test
//! names each container key the facts leave untyped.

use serde_json::Value;

use crate::frontmatter::EntityKind;
use crate::harness::{HarnessProfile, PluginDialect};
use crate::keys::Shape;

/// One kind of JSON value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Json {
    String,
    Bool,
    /// A boolean, or one of the words Claude Code's skills docs say its
    /// boolean fields read: `yes`, `no`, `on`, `off`, `true`, `false`, `1`
    /// and `0`, in any case. `1` and `0` may be numbers or quoted.
    Flag,
    Number,
    Object,
    /// A list of anything.
    List,
    /// A list of strings.
    StringList,
}

/// The kinds of value one key accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldType(&'static [Json]);

use Json::{Bool, Flag, List, Number, Object, String as Str, StringList};

const STRING: FieldType = FieldType(&[Str]);
const BOOL: FieldType = FieldType(&[Bool]);
const NUMBER: FieldType = FieldType(&[Number]);
const FLAG: FieldType = FieldType(&[Flag]);
const OBJECT: FieldType = FieldType(&[Object]);
const LIST: FieldType = FieldType(&[List]);
const STRINGS: FieldType = FieldType(&[StringList]);
const STRING_OR_OBJECT: FieldType = FieldType(&[Str, Object]);
const STRING_OR_STRINGS: FieldType = FieldType(&[Str, StringList]);
const STRING_OR_LIST: FieldType = FieldType(&[Str, List]);
const STRING_OBJECT_OR_LIST: FieldType = FieldType(&[Str, Object, List]);
const STRINGS_OR_OBJECT: FieldType = FieldType(&[Str, StringList, Object]);

impl FieldType {
    /// Whether `value` is one of the kinds. `null` never is: no fact allows
    /// it. A key set to nothing in a note is dropped before this check, by
    /// [`without_unset`] in a block and by the vault reader when bare.
    pub(crate) fn accepts(self, value: &Value) -> bool {
        self.0.iter().any(|kind| match kind {
            Str => value.is_string(),
            Bool => value.is_boolean(),
            Flag => flag(value).is_some(),
            Number => value.is_number(),
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
                Flag => "a boolean",
                Number => "a number",
                Object => "a mapping",
                List => "a list",
                StringList => "a list of strings",
            })
            .collect();
        words.join(" or ")
    }

    /// What a refusal says after `` `key` ``: the kinds this takes and what
    /// `value` is, with a hint where one helps.
    pub(crate) fn refusal(self, value: &Value) -> String {
        let hint = if self.0.contains(&Flag) {
            "; write `true` or `false`"
        } else if self.0.contains(&Str) && (value.is_number() || value.is_boolean()) {
            crate::frontmatter::quoting_hint(value)
        } else {
            ""
        };
        format!(
            "must be {}, not {}{hint}",
            self.label(),
            crate::frontmatter::type_name(value)
        )
    }
}

/// The boolean `value` spells, in any form [`Json::Flag`] reads.
pub(crate) fn flag(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(yes) => Some(*yes),
        Value::Number(n) => match n.as_u64() {
            Some(1) => Some(true),
            Some(0) => Some(false),
            _ => None,
        },
        Value::String(word) => match word.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Some(true),
            "false" | "no" | "off" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// An `argument-hint` list as its hint text: an unquoted `[issue-number]`,
/// which YAML reads as a list, is the text `[issue-number]`.
pub(crate) fn hint_text(value: &Value) -> Option<String> {
    let words = value
        .as_array()?
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<&str>>>()?;
    Some(format!("[{}]", words.join(", ")))
}

/// `value` of a component's `key` in the form every harness reads: a flag as
/// a real boolean, a list `argument-hint` as its text. Anything else as is.
pub(crate) fn normalized(kind: EntityKind, key: &str, value: Value) -> Value {
    let normal = match crate::keys::lookup(kind, key).map(|spec| spec.shape) {
        Some(Shape::Flag) => flag(&value).map(Value::Bool),
        Some(Shape::Hint) => hint_text(&value).map(Value::String),
        _ => None,
    };
    normal.unwrap_or(value)
}

/// `block` with a string `author` read as `{name: <string>}`, as the plain
/// `author` key reads it.
pub(crate) fn with_author_shorthand(
    mut block: serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    if let Some(Value::String(name)) = block.get("author") {
        let person = serde_json::json!({ "name": name });
        block.insert("author".to_owned(), person);
    }
    block
}

/// `harness`'s `block` less the keys it reads that are set to nothing. Such a
/// key is unset, as it is written bare, so it is dropped rather than shipped
/// as `null`. A key in `unknown`, as `<harness>.<key>`, stays, so it warns.
pub(crate) fn without_unset(
    harness: &str,
    block: serde_json::Map<String, Value>,
    unknown: &[String],
) -> serde_json::Map<String, Value> {
    // Not `Map::retain`: the serde_json floor predates it.
    block
        .into_iter()
        .filter(|(key, value)| !value.is_null() || unknown.contains(&format!("{harness}.{key}")))
        .collect()
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
    ("commands", STRINGS_OR_OBJECT),
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
    ("homepage", STRING),
    ("repository", STRING),
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

/// The container key types of the harness whose manifests take `dialect`.
pub(crate) fn facts_for(dialect: PluginDialect) -> &'static [(&'static str, FieldType)] {
    match dialect {
        PluginDialect::Claude => CLAUDE,
        PluginDialect::Codex => CODEX,
        PluginDialect::Cursor => CURSOR,
        PluginDialect::Pi => PI,
    }
}

/// The type of `key` on a note of `kind`, bare or in a harness block: its
/// key table row's [`Shape::field_type`]. `None` for a key the table does not
/// list for `kind`.
pub(crate) fn component_type(kind: EntityKind, key: &str) -> Option<FieldType> {
    crate::keys::lookup(kind, key)?.shape.field_type()
}

impl Shape {
    /// The kinds of value this shape takes, as the bare spec-key check and a
    /// harness block read them. `None` for a shape with its own check.
    pub(crate) const fn field_type(self) -> Option<FieldType> {
        Some(match self {
            Self::String => STRING,
            Self::StringList => STRINGS,
            Self::Words | Self::Hint => STRING_OR_STRINGS,
            Self::AgentTools => STRINGS_OR_OBJECT,
            Self::Bool => BOOL,
            Self::Flag => FLAG,
            Self::Number => NUMBER,
            Self::Mapping => OBJECT,
            Self::Arguments => STRING_OR_LIST,
            Self::Url | Self::Person | Self::HarnessBlock | Self::Refused => return None,
        })
    }
}

/// The type of `key` in a marketplace's or plugin's block for `profile`: the
/// facts' type, or for a key that passes to members, [`component_type`].
/// `None` for a row with no plugin format.
pub(crate) fn container_type(profile: &HarnessProfile, key: &str) -> Option<FieldType> {
    facts_for(profile.plugin_format()?.dialect)
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, ty)| *ty)
        .or_else(|| {
            profile
                .member_block_keys(EntityKind::Skill)
                .contains(key)
                .then(|| component_type(EntityKind::Skill, key))
                .flatten()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Container keys a harness reads that no fact gives a type for, by harness.
    /// They are not checked. `packs` and `naming` have their own checks.
    const UNTYPED: &[(&str, &[&str])] = &[(
        "pi",
        &["description", "author", "license", "repository", "homepage"],
    )];

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
                    container_type(profile, key).is_some() != untyped.contains(key),
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
        assert_eq!(flag(&json!("0")), Some(false));
        assert_eq!(flag(&json!("ON")), Some(true));
        assert_eq!(flag(&json!(2)), None);
        assert_eq!(STRING_OR_OBJECT.label(), "a string or a mapping");
    }

    #[test]
    fn the_known_keys_take_their_table_or_fact_type() {
        assert_eq!(component_type(EntityKind::Skill, "model"), Some(STRING));
        assert_eq!(
            component_type(EntityKind::Skill, "allowed-tools"),
            Some(STRING_OR_STRINGS)
        );
        assert_eq!(component_type(EntityKind::Skill, "permissionMode"), None);
        assert_eq!(
            component_type(EntityKind::Agent, "tools"),
            Some(STRINGS_OR_OBJECT)
        );
        assert_eq!(
            component_type(EntityKind::Skill, "tools"),
            Some(STRING_OR_STRINGS)
        );
        let profile = |name| crate::harness::profile_named(name).unwrap();
        assert_eq!(container_type(profile("codex"), "policy"), Some(OBJECT));
        assert_eq!(container_type(profile("claude"), "model"), Some(STRING));
        assert_eq!(container_type(profile("pi"), "author"), None);
    }

    #[test]
    fn component_type_is_the_row_s_shape() {
        for spec in crate::keys::KEYS {
            for kind in EntityKind::ALL {
                assert_eq!(
                    component_type(kind, spec.key),
                    crate::keys::lookup(kind, spec.key).and_then(|s| s.shape.field_type()),
                    "{kind:?} {}",
                    spec.key
                );
            }
        }
    }
}
