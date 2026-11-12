//! The closed key table: every frontmatter key skillwright reads, per kind.
//!
//! One table drives these things:
//!
//! * **The strip.** A key skillwright owns ([`Owner::Skillwright`]) is read
//!   into the model and never reaches a pack as written.
//! * **Validation.** A key outside the table for a note's kind is unknown,
//!   and gets a warning. A key of ours with the wrong [`Shape`] is refused
//!   when the note is read. An [`Owner::Spec`] key on a
//!   skill, agent or command is refused by [`crate::vault`] when its value is
//!   the wrong kind, by the check a harness block applies to the same key. On
//!   other kinds a spec key's shape is documentation.
//! * **The cascade.** [`KeySpec::inherits`] decides which keys come down from
//!   a container, and every inheritance step asks it through [`inherits`]:
//!   [`crate::frontmatter::ContainerIdentity::fill_unset_from`] for a
//!   container's identity ([`identity_keys`]), and [`crate::model::cascade`]
//!   for the harness blocks.
//! * **The field reference** the docs publish, generated from [`KEYS`].
//!
//! The kind comes from the folder a note sits in, never from the note.

use serde_json::Value;

use crate::frontmatter::EntityKind;
use crate::harness::profile::{CLAUDE, CODEX, CURSOR, PI};

/// Who a key belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// The Agent Skills spec's, or a harness's own: built as the harness
    /// reads it.
    Spec,
    /// skillwright's: read into the model and stripped at build.
    Skillwright,
}

/// The shape a key's value must have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// A string.
    String,
    /// A URL string, `http://` or `https://`.
    Url,
    /// A list of strings.
    StringList,
    /// A list of strings, or one string of words separated by spaces or
    /// commas: `Read Grep` or `Read, Grep`.
    Words,
    /// An agent's tools: [`Self::Words`], or a mapping of tool names, as
    /// OpenCode writes it. Its values are not checked.
    AgentTools,
    /// A hint: a string, or a list of strings, which is how YAML reads an
    /// unquoted `[issue-number]`.
    Hint,
    /// A boolean.
    Bool,
    /// A boolean, or one of the words Claude Code's skills docs say its
    /// boolean fields read: `yes`, `no`, `on`, `off`, `true`, `false`, `1`
    /// and `0`, in any case.
    Flag,
    /// A number.
    Number,
    /// A string, or a `{name, email, url}` mapping. Left blank, it is unset.
    Person,
    /// Any mapping. Its values are not checked.
    Mapping,
    /// A list of `{name, description, required, values, default}` mappings.
    /// A string or a list of strings is Claude's own `arguments` key, which the field type takes and this shape does not.
    Arguments,
    /// A mapping of one harness's own keys.
    HarnessBlock,
    /// No value: the key is refused wherever it is set.
    Refused,
}

impl Shape {
    /// Whether `value` has this shape. `null` passes only where the reader
    /// takes it as absent: a person and a harness block.
    /// A shape that the bare spec-key check and a harness block also read
    /// accepts what they accept, except [`Self::Arguments`].
    pub fn accepts(self, value: &Value) -> bool {
        match self {
            Self::Url => value.as_str().is_some_and(is_url),
            Self::Person => {
                value.is_null()
                    || value.is_string()
                    || value.get("name").is_some_and(Value::is_string)
            }
            Self::Arguments => value.is_array(),
            Self::HarnessBlock => value.is_object() || value.is_null(),
            Self::Refused => false,
            _ => self.field_type().is_some_and(|ty| ty.accepts(value)),
        }
    }
}

/// Why a value is not the shape its key takes, in the shape's own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeRefusal {
    /// What the value must be: `a string`.
    pub what: &'static str,
    /// The rest of the sentence after `must be <what>`: what the value is
    /// instead, and a hint where one helps. `, not a number`.
    pub detail: String,
}

impl ShapeRefusal {
    /// The refusal as one sentence about `key`, spelled as the note wrote it.
    pub fn sentence(&self, key: &str) -> String {
        format!("`{key}` must be {}{}", self.what, self.detail)
    }
}

impl Shape {
    /// Why `value` is not this shape.
    fn refusal(self, value: &Value) -> ShapeRefusal {
        use crate::frontmatter::{quoting_hint, type_name};
        let found = type_name(value);
        let not = format!(", not {found}");
        let (what, detail) = match self {
            Self::String => ("a string", format!("{not}{}", quoting_hint(value))),
            Self::Url => (
                "an http or https URL",
                match value.as_str() {
                    Some(text) => format!(", not `{text}`"),
                    None => not,
                },
            ),
            Self::StringList => match value {
                Value::Array(items) => (
                    "a list of strings",
                    format!(
                        "; one entry is {}",
                        items
                            .iter()
                            .find(|item| !item.is_string())
                            .map_or("not a string", type_name)
                    ),
                ),
                _ => (
                    "a list",
                    format!(
                        "{not}. An empty list means nowhere; removing the key means everywhere"
                    ),
                ),
            },
            Self::Words | Self::Hint => ("a string or a list of strings", not),
            Self::AgentTools => ("a string, a list of strings or a mapping", not),
            Self::Bool => ("a boolean", not),
            Self::Flag => ("a boolean", format!("{not}; write `true` or `false`")),
            Self::Number => ("a number", not),
            Self::Person => (
                "a name or a `{name, email, url}` mapping",
                if value.is_object() {
                    ", but it has no string `name`".to_owned()
                } else {
                    not
                },
            ),
            Self::Mapping => ("a mapping", not),
            Self::Arguments => ("a list of argument objects", not),
            Self::HarnessBlock => ("a mapping of that harness's own keys", not),
            Self::Refused => ("unset", format!("; {NAME_REFUSAL}")),
        };
        ShapeRefusal { what, detail }
    }
}

/// Checks `value` against the shape the table gives `key` on a note of
/// `kind`. Only skillwright's own keys are checked: a key the table does not
/// list, and a spec key, pass. A `null` is checked like any value; a reader
/// that reads it as unset, as the root file does, skips it first.
///
/// # Errors
///
/// A [`ShapeRefusal`] when `value` is not the key's shape.
pub fn check_shape(kind: EntityKind, key: &str, value: &Value) -> Result<(), ShapeRefusal> {
    match lookup(kind, key) {
        Some(spec) if spec.owner == Owner::Skillwright && !spec.shape.accepts(value) => {
            Err(spec.shape.refusal(value))
        }
        _ => Ok(()),
    }
}

/// Whether `value` is an `http` or `https` URL with no whitespace.
pub fn is_url(value: &str) -> bool {
    (value.starts_with("https://") || value.starts_with("http://"))
        && !value.contains(char::is_whitespace)
}

/// One key the table lists.
#[derive(Debug, Clone, Copy)]
pub struct KeySpec {
    /// The bare spelling.
    pub key: &'static str,
    /// The kinds of note it is read on.
    pub kinds: &'static [EntityKind],
    /// Who owns it.
    pub owner: Owner,
    /// What its value must be.
    pub shape: Shape,
    /// Whether a note leaving it out takes it from the nearest container that
    /// sets it: the plugin note, the marketplace note, then `skillwright.md`.
    pub inherits: bool,
    /// One line for the field reference.
    pub summary: &'static str,
}

use EntityKind::{Agent, Command, Marketplace, Plugin, Skill};

const CONTAINERS: &[EntityKind] = &[Marketplace, Plugin];
const COMPONENTS: &[EntityKind] = &[Skill, Agent, Command];
const PUBLISHED: &[EntityKind] = &[Marketplace, Plugin, Skill, Agent, Command];

/// The harnesses that take a block of their own keys: every harness row. A
/// test holds this list to [`crate::harness::with_plugin_format`] and to the
/// [`Shape::HarnessBlock`] rows of [`KEYS`].
pub const HARNESS_BLOCKS: &[&str] = &[CLAUDE.name, CODEX.name, CURSOR.name, PI.name];

/// The key no note sets, at the top level or in a harness block: the folder
/// gives the name.
pub const NAME_KEY: &str = "name";

/// Why a [`Shape::Refused`] key is refused: what a refusal of `name` says.
pub const NAME_REFUSAL: &str = "the folder's name is the note's name; delete it";

/// Removed harness block keys. A block holding one is refused wherever it is:
/// each harness's packs folder is fixed.
pub const REFUSED_BLOCK_KEYS: &[&str] = &["packs"];

/// `<harness>.name` for each harness block in `fields` that sets `name`.
pub(crate) fn named_blocks(fields: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    HARNESS_BLOCKS
        .iter()
        .filter(|harness| {
            fields
                .get(**harness)
                .and_then(serde_json::Value::as_object)
                .is_some_and(|block| block.contains_key(NAME_KEY))
        })
        .map(|harness| format!("{harness}.{NAME_KEY}"))
        .collect()
}

/// The removed keys in a note's harness blocks, as `<harness>.<key>`.
/// `fields` is the note's frontmatter; keys other than the harness blocks
/// are passed over.
pub(crate) fn refused_block_keys(
    fields: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    fields
        .iter()
        .filter(|(harness, _)| HARNESS_BLOCKS.contains(&harness.as_str()))
        .flat_map(|(harness, block)| {
            let keys = block.as_object().into_iter().flat_map(|block| block.keys());
            keys.filter(|key| REFUSED_BLOCK_KEYS.contains(&key.as_str()))
                .map(move |key| format!("{harness}.{key}"))
        })
        .collect()
}

/// Harness block keys that say which thing a manifest describes. A plugin
/// never takes one from the marketplace's block: each plugin has its own.
pub const BLOCK_IDENTITY_KEYS: &[&str] =
    &["name", "description", "version", "displayName", "source"];

/// Every key skillwright reads, in the order the field reference lists them.
pub const KEYS: &[KeySpec] = &[
    KeySpec {
        key: "name",
        kinds: PUBLISHED,
        owner: Owner::Skillwright,
        shape: Shape::Refused,
        inherits: false,
        summary: "Refused on every note and in `skillwright.md`: the folder's name is the note's name. The build writes it as the `name` each harness reads.",
    },
    KeySpec {
        key: "description",
        kinds: PUBLISHED,
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "What it does. A skill, command or agent with no description is an error: `check` reports it and `build` refuses it. It is never inherited. A plugin or marketplace need not have one.",
    },
    KeySpec {
        key: "version",
        kinds: CONTAINERS,
        owner: Owner::Skillwright,
        shape: Shape::String,
        inherits: false,
        summary: "The pack or marketplace version. `bump` edits it. Defaults to 0.1.0, with a warning.",
    },
    KeySpec {
        key: "author",
        kinds: CONTAINERS,
        owner: Owner::Skillwright,
        shape: Shape::Person,
        inherits: true,
        summary: "The author, a name or `{name, email, url}`.",
    },
    KeySpec {
        key: "owner",
        kinds: &[Marketplace],
        owner: Owner::Skillwright,
        shape: Shape::Person,
        inherits: false,
        summary: "The marketplace owner, a name or `{name, email, url}`.",
    },
    KeySpec {
        key: "license",
        kinds: CONTAINERS,
        owner: Owner::Skillwright,
        shape: Shape::String,
        inherits: true,
        summary: "The pack license, an SPDX expression. Defaults to UNLICENSED.",
    },
    KeySpec {
        key: "license",
        kinds: &[Skill],
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: true,
        summary: "The skill's license, the Agent Skills field. Inherited from the plugin, the marketplace or `skillwright.md`.",
    },
    KeySpec {
        key: "repository",
        kinds: CONTAINERS,
        owner: Owner::Skillwright,
        shape: Shape::Url,
        inherits: true,
        summary: "The source repository URL, written into every manifest that has the field.",
    },
    KeySpec {
        key: "homepage",
        kinds: CONTAINERS,
        owner: Owner::Skillwright,
        shape: Shape::Url,
        inherits: true,
        summary: "The documentation URL, written into every manifest that has the field.",
    },
    KeySpec {
        key: "keywords",
        kinds: &[Plugin],
        owner: Owner::Skillwright,
        shape: Shape::StringList,
        inherits: false,
        summary: "Plugin keywords; also the Claude and Cursor marketplace entry's `tags`. Pi's `package.json` adds `pi-package`, the keyword its gallery lists a package by.",
    },
    KeySpec {
        key: "category",
        kinds: &[Plugin],
        owner: Owner::Skillwright,
        shape: Shape::String,
        inherits: false,
        summary: "The plugin's category in a marketplace entry. Codex defaults it to Productivity.",
    },
    KeySpec {
        key: "display_name",
        kinds: CONTAINERS,
        owner: Owner::Skillwright,
        shape: Shape::String,
        inherits: false,
        summary: "The human-facing name: Codex's `interface.displayName`, and the `displayName` of a Cursor plugin and of its Cursor catalog entry. A `cursor.displayName` overrides it for Cursor.",
    },
    KeySpec {
        key: "arguments",
        kinds: COMPONENTS,
        owner: Owner::Skillwright,
        shape: Shape::Arguments,
        inherits: false,
        summary: "A typed argument list. Derives `argument-hint`. A list of plain strings, or one space-separated string, is Claude's own `arguments` key.",
    },
    KeySpec {
        key: "allowed-tools",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::Words,
        inherits: false,
        summary: "Tools the skill may use without asking: a list, or one string separated by spaces or commas.",
    },
    KeySpec {
        key: "compatibility",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "What the skill needs to run, the Agent Skills field.",
    },
    KeySpec {
        key: "metadata",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::Mapping,
        inherits: false,
        summary: "Free-form fields, the Agent Skills field. Unknown keys are published here.",
    },
    KeySpec {
        key: "argument-hint",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::Hint,
        inherits: false,
        summary: "The hint shown after the command name.",
    },
    KeySpec {
        key: "model",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "The model to run it with.",
    },
    KeySpec {
        key: "context",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "Claude's context mode, such as `fork`.",
    },
    KeySpec {
        key: "user-invocable",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::Flag,
        inherits: false,
        summary: "False hides it from the slash menu. Defaults to true.",
    },
    KeySpec {
        key: "disable-model-invocation",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::Flag,
        inherits: false,
        summary: "True stops the model from calling it on its own.",
    },
    KeySpec {
        key: "tools",
        kinds: &[Skill, Command],
        owner: Owner::Spec,
        shape: Shape::Words,
        inherits: false,
        summary: "A tool list, in the form an agent's `tools` takes. `allowed-tools` is the Agent Skills field.",
    },
    KeySpec {
        key: "tools",
        kinds: &[Agent],
        owner: Owner::Spec,
        shape: Shape::AgentTools,
        inherits: false,
        summary: "An agent's tool list.",
    },
    KeySpec {
        key: "temperature",
        kinds: &[Agent],
        owner: Owner::Spec,
        shape: Shape::Number,
        inherits: false,
        summary: "An agent's sampling temperature.",
    },
    KeySpec {
        key: CLAUDE.name,
        kinds: PUBLISHED,
        owner: Owner::Skillwright,
        shape: Shape::HarnessBlock,
        inherits: true,
        summary: "Fields only Claude Code reads, in its own key names, merged over what skillwright generates.",
    },
    KeySpec {
        key: CODEX.name,
        kinds: PUBLISHED,
        owner: Owner::Skillwright,
        shape: Shape::HarnessBlock,
        inherits: true,
        summary: "Fields only Codex reads, such as a plugin's `interface` or a marketplace's `policy`.",
    },
    KeySpec {
        key: CURSOR.name,
        kinds: PUBLISHED,
        owner: Owner::Skillwright,
        shape: Shape::HarnessBlock,
        inherits: true,
        summary: "Fields only Cursor reads, in its own key names.",
    },
    KeySpec {
        key: PI.name,
        kinds: PUBLISHED,
        owner: Owner::Skillwright,
        shape: Shape::HarnessBlock,
        inherits: true,
        summary: "Fields only Pi reads, in its own key names.",
    },
];

/// The heading and order of each kind in the field reference.
#[cfg(test)]
const REFERENCE_KINDS: &[(EntityKind, &str)] = &[
    (Marketplace, "Marketplace"),
    (Plugin, "Plugin"),
    (Skill, "Skill"),
    (Agent, "Agent"),
    (Command, "Command"),
];

impl Shape {
    /// The shape as the field reference words it.
    #[cfg(test)]
    const fn label(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Url => "URL",
            Self::StringList => "list of strings",
            Self::Words => "list of strings, or one string separated by spaces or commas",
            Self::AgentTools => "list of strings, one string separated by commas, or a mapping",
            Self::Hint => "string, or a list of strings read as `[a, b]`",
            Self::Bool => "boolean",
            Self::Flag => "boolean, or `yes`, `no`, `on`, `off`, `1`, `0`",
            Self::Number => "number",
            Self::Person => "string or `{name, email, url}`",
            Self::Mapping => "mapping",
            Self::Arguments => {
                "list of arguments; a string or a list of strings is Claude's own key"
            }
            Self::HarnessBlock => "harness block",
            Self::Refused => "refused",
        }
    }
}

/// The per-kind field reference, in Markdown. `docs/reference/fields.md` is
/// this text, and the `the_field_reference_matches_the_key_table` test holds
/// the two together.
#[cfg(test)]
#[must_use]
pub(crate) fn field_reference() -> String {
    let mut out = String::from(
        r#"# Field reference

Generated from the key table in `tradecraft-core`. Do not edit this file: run `TRADECRAFT_BLESS=1 cargo test -p tradecraft-core --lib keys::tests` from `packages/rust`.

Each table below lists the frontmatter keys that one kind of note reads. A note's kind comes from its folder.

## Reading the tables

- **Value** is the kind of value the key takes. [Other accepted forms](#other-accepted-forms) notes what the column cannot say.
- **Owner** says who the key belongs to:
  - `skillwright`: skillwright reads the key and leaves it out of what it builds.
  - `spec`: the key belongs to the Agent Skills spec or a harness. It ships as written.
- **Inherited**: `yes` means a note without the key takes it from the nearest container that sets it: the plugin note, the marketplace note, then `skillwright.md`. Only these keys may be set in `skillwright.md`.

## What is checked

- A key the table does not list for that kind gets a warning.
- `name` is refused on every note. `build:` is refused on every note: only `skillwright.md` reads it.
- A listed key with the wrong kind of value is refused, and the message names the key. This applies to every `skillwright` key, and to every `spec` key on a skill, agent or command.
- A `spec` key with no value, such as `model:`, is dropped. It does not clear a value the note inherits.

## Other accepted forms

- Quote a hint with more than one bracket group, such as `"[file] [format]"`. Unquoted, it is not valid YAML.
- An agent's `tools` mapping is OpenCode's form. Claude Code refuses it.
- `user-invocable` and `disable-model-invocation` read their words in any case, quoted or not. A harness that gets the key receives `true` or `false`.
"#,
    );
    for (kind, heading) in REFERENCE_KINDS {
        out.push_str(&format!(
            "\n## {heading}\n\n\
             | Key | Value | Owner | Inherited | Meaning |\n\
             |---|---|---|---|---|\n"
        ));
        for spec in KEYS.iter().filter(|spec| spec.kinds.contains(kind)) {
            let owner = match spec.owner {
                Owner::Spec => "spec",
                Owner::Skillwright => "skillwright",
            };
            let inherited = if spec.inherits { "yes" } else { "no" };
            out.push_str(&format!(
                "| `{}` | {} | {owner} | {inherited} | {} |\n",
                spec.key,
                spec.shape.label(),
                spec.summary
            ));
        }
    }
    out.push_str(
        r#"
## Harness blocks

A block such as `claude:` holds fields that only that harness reads. The lists below give the keys each block accepts, by the kind of note it sits on.

- Any other key gets a warning and is left out of that harness's output.
- A listed key with the wrong kind of value is refused. Every block is checked, even for a harness the build leaves out.
- On a skill, agent or command, a key takes the same type it takes written bare.
- On a plugin or marketplace, a key takes the type that harness's manifest and marketplace docs give. A key with no documented type is not checked. A key the plugin passes down to its skills, such as `model`, takes the type it has on a skill.
- A key with no value is dropped. It does not clear a value the note inherits.
- The "kept" lines list the keys that stay with a plugin's or marketplace's block: the member below never takes them (spec section 7 rule 4).
"#,
    );
    let list = |keys: &std::collections::BTreeSet<String>| {
        if keys.is_empty() {
            return "none".to_owned();
        }
        let keys: Vec<String> = keys.iter().map(|key| format!("`{key}`")).collect();
        keys.join(", ")
    };
    for profile in crate::harness::with_plugin_format() {
        out.push_str(&format!("\n### `{}:`\n\n", profile.name));
        for (kind, label) in [
            (Skill, "Skill and command"),
            (Plugin, "Plugin"),
            (Marketplace, "Marketplace"),
        ] {
            let keys = profile
                .block_keys(kind)
                .expect("a plugin row checks these kinds' blocks");
            out.push_str(&format!("- {label}: {}\n", list(keys)));
            let (member, passed) = match kind {
                Skill => {
                    out.push_str("- Agent: not checked.\n");
                    continue;
                }
                Plugin => ("skills and commands", profile.member_block_keys(Skill)),
                _ => ("plugins", profile.passed_down_keys()),
            };
            let kept = keys.difference(passed).cloned().collect();
            out.push_str(&format!("- {label}, kept from {member}: {}\n", list(&kept)));
        }
    }
    out
}

/// The table's entry for `key` on a note of `kind`, if it lists one.
pub fn lookup(kind: EntityKind, key: &str) -> Option<&'static KeySpec> {
    KEYS.iter()
        .find(|spec| spec.key == key && spec.kinds.contains(&kind))
}

/// Whether `key` (bare) is skillwright's own on a note of `kind`.
pub fn is_owned(kind: EntityKind, key: &str) -> bool {
    lookup(kind, key).is_some_and(|spec| spec.owner == Owner::Skillwright)
}

/// Whether a note of `kind` reads `key`.
pub fn is_known(kind: EntityKind, key: &str) -> bool {
    lookup(kind, key).is_some()
}

/// Whether a note of `kind` takes `key` from its containers.
pub fn inherits(kind: EntityKind, key: &str) -> bool {
    lookup(kind, key).is_some_and(|spec| spec.inherits)
}

/// The keys a container's identity holds: the key table's rows for a
/// marketplace or a plugin that skillwright owns, less the harness blocks,
/// which other kinds read too, and the refused `name`. In table order,
/// deduplicated.
///
/// Any later skillwright-owned key a marketplace or plugin reads joins the
/// identity by this rule, and a test fails until
/// [`crate::frontmatter::ContainerIdentity`] has a field for it. A container
/// key that is not identity data must be excluded here on purpose, with a
/// reason.
pub fn identity_keys() -> impl Iterator<Item = &'static str> {
    let mut seen = Vec::new();
    KEYS.iter()
        .filter(|spec| spec.kinds.iter().any(|kind| CONTAINERS.contains(kind)))
        .filter(|spec| spec.owner == Owner::Skillwright)
        .filter(|spec| !matches!(spec.shape, Shape::HarnessBlock | Shape::Refused))
        .map(|spec| spec.key)
        .filter(move |key| {
            let first = !seen.contains(key);
            seen.push(*key);
            first
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn license_is_the_pack_s_on_a_container_and_the_spec_s_on_a_skill() {
        assert!(is_owned(Plugin, "license"));
        assert!(is_owned(Marketplace, "license"));
        assert!(!is_owned(Skill, "license"));
        assert!(is_known(Skill, "license"));
        assert!(!is_known(Agent, "license"));
    }

    /// The Decided block's list, for the kinds that carry each key.
    #[test]
    fn the_inherited_keys_are_the_decided_list() {
        let listed: std::collections::BTreeSet<&str> = KEYS
            .iter()
            .filter(|spec| spec.inherits)
            .map(|spec| spec.key)
            .collect();
        let mut decided = vec!["author", "license", "repository", "homepage"];
        decided.extend(HARNESS_BLOCKS);
        assert_eq!(listed, decided.into_iter().collect());
        assert!(inherits(Skill, "license"));
        assert!(!inherits(Agent, "license"));
    }

    #[test]
    fn a_person_is_a_name_or_a_mapping_holding_one() {
        assert!(Shape::Person.accepts(&serde_json::json!("Ada")));
        assert!(Shape::Person.accepts(&serde_json::json!({"name": "Ada", "url": "u"})));
        assert!(!Shape::Person.accepts(&serde_json::json!(5)));
        assert!(!Shape::Person.accepts(&serde_json::json!({"email": "a@b"})));
    }

    #[test]
    fn every_harness_block_is_in_the_table() {
        for block in HARNESS_BLOCKS {
            for kind in PUBLISHED {
                assert!(is_owned(*kind, block), "{block} on {kind:?}");
            }
        }
    }

    #[test]
    fn no_key_is_listed_twice_for_one_kind() {
        for (i, a) in KEYS.iter().enumerate() {
            for b in &KEYS[i + 1..] {
                if a.key == b.key {
                    assert!(
                        a.kinds.iter().all(|k| !b.kinds.contains(k)),
                        "{} is listed twice for one kind",
                        a.key
                    );
                }
            }
        }
    }

    /// Values every shape is asked about.
    fn samples() -> Vec<Value> {
        use serde_json::json;
        vec![
            Value::Null,
            json!(true),
            json!(false),
            json!(0),
            json!(1),
            json!(2),
            json!(1.5),
            json!(""),
            json!("a b"),
            json!("yes"),
            json!("Off"),
            json!("https://x"),
            json!([]),
            json!(["a", "b"]),
            json!([1]),
            json!([{"name": "a"}]),
            json!({}),
            json!({"a": "b"}),
            json!({"a": 1}),
            json!({"name": "Ada"}),
        ]
    }

    #[test]
    fn every_shape_s_field_type_agrees_with_accepts() {
        let shapes = [
            Shape::String,
            Shape::Url,
            Shape::StringList,
            Shape::Words,
            Shape::AgentTools,
            Shape::Hint,
            Shape::Bool,
            Shape::Flag,
            Shape::Number,
            Shape::Person,
            Shape::Mapping,
            Shape::Arguments,
            Shape::HarnessBlock,
            Shape::Refused,
        ];
        for shape in shapes {
            let Some(ty) = shape.field_type() else {
                continue;
            };
            if shape == Shape::Arguments {
                continue;
            }
            for value in samples() {
                assert_eq!(
                    shape.accepts(&value),
                    ty.accepts(&value),
                    "{shape:?} {value}"
                );
            }
        }
        // Ours is a list of argument objects; Claude's own `arguments` is a string or a list of strings.
        let ty = Shape::Arguments.field_type().unwrap();
        let words = serde_json::json!("a b");
        assert!(ty.accepts(&words));
        assert!(!Shape::Arguments.accepts(&words));
        for value in samples().iter().filter(|v| Shape::Arguments.accepts(v)) {
            assert!(ty.accepts(value), "{value}");
        }
    }

    #[test]
    fn tools_has_one_row_per_kind() {
        assert_eq!(lookup(Agent, "tools").unwrap().shape, Shape::AgentTools);
        assert_eq!(lookup(Skill, "tools").unwrap().shape, Shape::Words);
        assert_eq!(lookup(Command, "tools").unwrap().shape, Shape::Words);
    }

    /// `docs/reference/fields.md` is the key table's own text. When the table
    /// changes, refresh it from `packages/rust` with
    /// `TRADECRAFT_BLESS=1 cargo test -p tradecraft-core --lib keys::tests`.
    #[test]
    fn the_field_reference_matches_the_key_table() {
        const REFERENCE: &str = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../docs/reference/fields.md"
        );
        let generated = field_reference();
        if std::env::var_os("TRADECRAFT_BLESS").is_some() {
            std::fs::write(REFERENCE, &generated).unwrap();
        }
        let committed = std::fs::read_to_string(REFERENCE).unwrap();
        assert!(
            committed == generated,
            "docs/reference/fields.md drifted from the key table; see this test's doc"
        );
    }

    /// The keys `field_reference` lists for `harness` on the line that starts
    /// with `label`.
    fn listed(reference: &str, harness: &str, label: &str) -> std::collections::BTreeSet<String> {
        let section = reference
            .split(&format!("### `{harness}:`\n"))
            .nth(1)
            .unwrap_or_else(|| panic!("no section for {harness}"));
        let line = section
            .lines()
            .find_map(|line| line.strip_prefix(&format!("- {label}: ")))
            .unwrap_or_else(|| panic!("no {label} line for {harness}"));
        if line == "none" {
            return std::collections::BTreeSet::new();
        }
        line.split(", ")
            .map(|key| key.trim_matches('`').to_owned())
            .collect()
    }

    /// The docs list, for each harness and kind, exactly what `check` takes.
    #[test]
    fn the_field_reference_lists_exactly_the_keys_check_accepts() {
        use serde_json::{Map, Value, json};
        let reference = field_reference();
        for profile in crate::harness::with_plugin_format() {
            let harness = profile.name;
            assert!(
                reference.contains(&format!("### `{harness}:`\n\n- Skill and command: ")),
                "{harness}"
            );
            assert!(reference.contains("- Agent: not checked.\n"), "{harness}");
            for (kind, label) in [
                (Skill, "Skill and command"),
                (Plugin, "Plugin"),
                (Marketplace, "Marketplace"),
            ] {
                let keys = listed(&reference, harness, label);
                assert_eq!(
                    &keys,
                    profile.block_keys(kind).unwrap(),
                    "{harness} {label}"
                );
                let unknown = |key: &str| {
                    let block: Map<String, Value> = Map::from_iter([(key.to_owned(), json!(1))]);
                    let blocks = Map::from_iter([(harness.to_owned(), Value::Object(block))]);
                    if kind == Skill {
                        crate::discover::unknown_component_block_keys(&blocks)
                    } else {
                        crate::discover::unknown_container_block_keys(&blocks, kind)
                    }
                };
                for key in &keys {
                    assert_eq!(
                        unknown(key),
                        Vec::<String>::new(),
                        "{harness} {label} {key}"
                    );
                }
                // Every key another harness lists here, and this one does
                // not, warns.
                let others: std::collections::BTreeSet<&String> =
                    crate::harness::with_plugin_format()
                        .filter_map(|other| other.block_keys(kind))
                        .flatten()
                        .filter(|key| !keys.contains(*key))
                        .collect();
                assert!(!others.is_empty(), "{harness} {label}");
                for key in others {
                    assert_eq!(
                        unknown(key),
                        vec![format!("{harness}.{key}")],
                        "{harness} {label} {key}"
                    );
                }
                assert_eq!(
                    unknown("zz-unknown"),
                    vec![format!("{harness}.zz-unknown")],
                    "{harness} {label}"
                );
            }
        }
    }
}
