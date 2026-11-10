//! The closed key table: every frontmatter key skillwright reads, per kind.
//!
//! One table drives these things:
//!
//! * **The publish strip.** A key skillwright owns ([`Owner::Skillwright`])
//!   is read into the model and never reaches a destination as written.
//! * **Validation.** A key outside the table for a note's kind is unknown,
//!   and gets a warning. A key of ours with the wrong [`Shape`] is refused
//!   when the note is read. An [`Owner::Spec`] key on a
//!   skill, agent or command is refused by [`crate::vault`] when its value is
//!   the wrong kind, by the check a harness block applies to the same key. On
//!   other kinds a spec key's shape is documentation.
//! * **The cascade.** [`KeySpec::inherits`] decides which keys come down from
//!   a container; [`crate::model::cascade`] reads it through [`inherits`].
//! * **The field reference** the docs publish, generated from [`KEYS`].
//!
//! Keys are bare by default. Under a namespace (`build.namespace: sw`), every
//! [`Owner::Skillwright`] key takes the prefix (`sw-version`), or sits in the
//! nested block (`sw: {version: …}`). [`Owner::Spec`] keys are never
//! prefixed.
//!
//! The kind comes from the folder a note sits in, never from the note.

use serde_json::Value;

use crate::frontmatter::EntityKind;
use crate::namespace::Namespace;

/// Who a key belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// The Agent Skills spec's, or a harness's own: never prefixed, and
    /// published as the harness reads it.
    Spec,
    /// skillwright's: prefixed under a namespace, read into the model, and
    /// stripped at publish.
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
    /// A boolean.
    Bool,
    /// A number.
    Number,
    /// A string, or a `{name, email, url}` mapping. Left blank, it is unset.
    Person,
    /// A mapping of string keys to string values.
    StringMap,
    /// A list of `{name, description, required, values, default}` mappings.
    Arguments,
    /// A mapping of one harness's own keys.
    HarnessBlock,
    /// The `{source, format, source_hash, hash, at, also_at}` block adoption writes.
    Adopted,
}

impl Shape {
    /// Whether `value` has this shape. `null` passes only where the reader
    /// takes it as absent: a harness block and the `adopted` block.
    pub fn accepts(self, value: &Value) -> bool {
        match self {
            Self::String => value.is_string(),
            Self::Url => value.as_str().is_some_and(is_url),
            Self::StringList => value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string)),
            Self::Bool => value.is_boolean(),
            Self::Number => value.is_number(),
            Self::Person => {
                value.is_null()
                    || value.is_string()
                    || value.get("name").is_some_and(Value::is_string)
            }
            Self::StringMap => value
                .as_object()
                .is_some_and(|map| map.values().all(Value::is_string)),
            Self::Arguments => value.is_array(),
            Self::HarnessBlock | Self::Adopted => value.is_object() || value.is_null(),
        }
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
    /// sets it: the plugin note, then `skillwright.md`,
    /// which holds what the workspace file gave it.
    pub inherits: bool,
    /// One line for the field reference.
    pub summary: &'static str,
}

use EntityKind::{Agent, Command, Hook, Marketplace, Plugin, Skill};

const CONTAINERS: &[EntityKind] = &[Marketplace, Plugin];
const COMPONENTS: &[EntityKind] = &[Skill, Agent, Command];
const ALL_BUT_HOOK: &[EntityKind] = &[Marketplace, Plugin, Skill, Agent, Command];
const PUBLISHED: &[EntityKind] = &[Marketplace, Plugin, Skill, Agent, Command, Hook];
const ADOPTABLE: &[EntityKind] = &[Skill, Agent, Command, Hook];

/// The harnesses that take a block of their own keys: `claude:`, `codex:`,
/// `cursor:` and `pi:`.
pub const HARNESS_BLOCKS: &[&str] = &["claude", "codex", "cursor", "pi"];

/// Harness block keys that say which thing a manifest describes. A plugin
/// never takes one from the marketplace's block: each plugin has its own.
pub const BLOCK_IDENTITY_KEYS: &[&str] =
    &["name", "description", "version", "displayName", "source"];

/// Every key skillwright reads, in the order the field reference lists them.
pub const KEYS: &[KeySpec] = &[
    KeySpec {
        key: "name",
        kinds: &[Marketplace, Plugin, Skill, Agent, Command, Hook],
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "The name. Defaults to the file or folder name. A plugin's must match its folder.",
    },
    KeySpec {
        key: "description",
        kinds: ALL_BUT_HOOK,
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "What it does. A skill, command or agent with a `name` and no description is an error: `list` and `check` report it, and `build` and `publish` refuse it. A plugin or marketplace need not have one. A note with neither is not published.",
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
        summary: "The skill's license, the Agent Skills field. Inherited from the plugin or marketplace.",
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
        summary: "The human-facing name, Codex's `interface.displayName` and a Cursor plugin's `displayName`. A `cursor.displayName` overrides it for Cursor.",
    },
    KeySpec {
        key: "targets",
        kinds: PUBLISHED,
        owner: Owner::Skillwright,
        shape: Shape::StringList,
        inherits: true,
        summary: "Which harnesses get it: `claude`, `codex`, `cursor`, `pi`, or `amp`, `antigravity`, `opencode` and `portable` for `publish`. `claude-code` and `codex-cli` also work. `copilot` and `github-copilot` are accepted, but no skill renders for them: `publish --target copilot` refuses each one `no-renderer`. Absent means all; `[]` means none. Another name is refused.",
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
        shape: Shape::StringList,
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
        shape: Shape::StringMap,
        inherits: false,
        summary: "Free-form fields, the Agent Skills field. Unknown keys are published here.",
    },
    KeySpec {
        key: "argument-hint",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::String,
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
        shape: Shape::Bool,
        inherits: false,
        summary: "False hides it from the slash menu. Defaults to true.",
    },
    KeySpec {
        key: "disable-model-invocation",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::Bool,
        inherits: false,
        summary: "True stops the model from calling it on its own.",
    },
    KeySpec {
        key: "tools",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::StringList,
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
        key: "event",
        kinds: &[Hook],
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "The event the hook runs on, in Claude Code's spelling.",
    },
    KeySpec {
        key: "matcher",
        kinds: &[Hook],
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "Which tool or source the hook matches.",
    },
    KeySpec {
        key: "type",
        kinds: &[Hook],
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "`command`, `http` or `prompt`. Defaults to `command`.",
    },
    KeySpec {
        key: "command",
        kinds: &[Hook],
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "The command a command hook runs. The body may carry it instead.",
    },
    KeySpec {
        key: "prompt",
        kinds: &[Hook],
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "The prompt a prompt hook sends. The body may carry it instead.",
    },
    KeySpec {
        key: "url",
        kinds: &[Hook],
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "The URL an http hook calls.",
    },
    KeySpec {
        key: "if",
        kinds: &[Hook],
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "A condition the hook runs under.",
    },
    KeySpec {
        key: "async",
        kinds: &[Hook],
        owner: Owner::Spec,
        shape: Shape::Bool,
        inherits: false,
        summary: "True runs the hook without waiting for it.",
    },
    KeySpec {
        key: "once",
        kinds: &[Hook],
        owner: Owner::Spec,
        shape: Shape::Bool,
        inherits: false,
        summary: "True runs the hook once per session.",
    },
    KeySpec {
        key: "timeout",
        kinds: &[Hook],
        owner: Owner::Spec,
        shape: Shape::Number,
        inherits: false,
        summary: "The timeout in seconds.",
    },
    KeySpec {
        key: "claude",
        kinds: ALL_BUT_HOOK,
        owner: Owner::Skillwright,
        shape: Shape::HarnessBlock,
        inherits: true,
        summary: "Fields only Claude Code reads, in its own key names, merged over what skillwright generates.",
    },
    KeySpec {
        key: "codex",
        kinds: ALL_BUT_HOOK,
        owner: Owner::Skillwright,
        shape: Shape::HarnessBlock,
        inherits: true,
        summary: "Fields only Codex reads, such as a plugin's `interface` or a marketplace's `policy`.",
    },
    KeySpec {
        key: "cursor",
        kinds: ALL_BUT_HOOK,
        owner: Owner::Skillwright,
        shape: Shape::HarnessBlock,
        inherits: true,
        summary: "Fields only Cursor reads, in its own key names.",
    },
    KeySpec {
        key: "pi",
        kinds: ALL_BUT_HOOK,
        owner: Owner::Skillwright,
        shape: Shape::HarnessBlock,
        inherits: true,
        summary: "Fields only Pi reads, in its own key names.",
    },
    KeySpec {
        key: "adopted",
        kinds: ADOPTABLE,
        owner: Owner::Skillwright,
        shape: Shape::Adopted,
        inherits: false,
        summary: "Where adoption read the note from: `{source, format, source_hash, hash, at, also_at}`. Written by `adopt`.",
    },
];

/// The heading and order of each kind in the field reference.
const REFERENCE_KINDS: &[(EntityKind, &str)] = &[
    (Marketplace, "Marketplace"),
    (Plugin, "Plugin"),
    (Skill, "Skill"),
    (Agent, "Agent"),
    (Command, "Command"),
    (Hook, "Hook"),
];

impl Shape {
    /// The shape as the field reference words it.
    const fn label(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Url => "URL",
            Self::StringList => "list of strings",
            Self::Bool => "boolean",
            Self::Number => "number",
            Self::Person => "string or `{name, email, url}`",
            Self::StringMap => "map of strings",
            Self::Arguments => "list of arguments",
            Self::HarnessBlock => "harness block",
            Self::Adopted => "adopted block",
        }
    }
}

/// The per-kind field reference, in Markdown. `docs/reference/fields.md` is
/// this text, and a test holds the two together.
#[must_use]
pub fn field_reference() -> String {
    let mut out = String::from(
        r#"# Field reference

Generated from the key table in `tradecraft-core`. Do not edit this file: run `TRADECRAFT_BLESS=1 cargo test -p tradecraft-core --test field_reference` from `packages/rust`.

Each table below lists the frontmatter keys that one kind of note reads. A note's kind comes from its folder.

## Reading the tables

- **Value** is the kind of value the key takes. Some keys take more forms; see [Other accepted forms](#other-accepted-forms).
- **Owner** says who the key belongs to:
  - `skillwright`: skillwright reads the key and removes it when it publishes. With `build.namespace: sw`, the key is written `sw-version`, or inside an `sw:` block. Keys in `skillwright.md` always stay bare.
  - `spec`: the key belongs to the Agent Skills spec or a harness. It is published as written and never prefixed.
- **Inherited**: `yes` means a note without the key takes it from the nearest container that sets it: the plugin note, then `skillwright.md`, then the workspace's `skillwright.md`.

## What is checked

- A key the table does not list for that kind gets a warning.
- A listed key with the wrong kind of value is refused, and the message names the key. This applies to every `skillwright` key, and to every `spec` key on a skill, agent or command.
- A `spec` key with no value, such as `model:`, is dropped. It does not clear a value the note inherits.

## Other accepted forms

Some keys take more forms than the Value column shows, because a harness's docs use them:

| Key | Also takes |
|---|---|
| `allowed-tools` | One string of names separated by spaces or commas: `Read Grep` or `Read, Grep`. |
| an agent's `tools` | One string of names separated by commas: `Read, Grep`. Or a mapping of tool names, as OpenCode writes it, which a Claude Code target refuses. |
| `argument-hint` | A list of strings. An unquoted `[issue-number]` reads as the text `[issue-number]`. |
| `arguments` | A string, or any list. |
| `metadata` | Any mapping. |
| `user-invocable`, `disable-model-invocation` | `yes`, `no`, `on`, `off`, `1` or `0`, in any case, quoted or not. A harness that gets the key receives `true` or `false`. |

Quote a hint with more than one bracket group, such as `"[file] [format]"`. Unquoted, it is not valid YAML.
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
"#,
    );
    for harness in HARNESS_BLOCKS {
        let Some(profile) = crate::harness::profile_named(harness) else {
            continue;
        };
        out.push_str(&format!("\n### `{harness}:`\n\n"));
        let list = |keys: &[&str]| {
            let mut keys = keys.to_vec();
            keys.sort_unstable();
            keys.dedup();
            let keys: Vec<String> = keys.iter().map(|key| format!("`{key}`")).collect();
            if keys.is_empty() {
                "none".to_owned()
            } else {
                keys.join(", ")
            }
        };
        let component = profile.accepted_override_keys();
        let component: Vec<&str> = component.iter().map(String::as_str).collect();
        out.push_str(&format!("- Component: {}\n", list(&component)));
        let Some(format) = profile.plugin.as_ref() else {
            continue;
        };
        let mut plugin = component.clone();
        plugin.extend(format.manifest_fields);
        plugin.extend(format.entry_fields);
        if *harness == "pi" {
            plugin.push(crate::harness::manifest::NAMING_KEY);
        }
        out.push_str(&format!("- Plugin: {}\n", list(&plugin)));
        let mut marketplace = plugin.clone();
        marketplace.extend(format.marketplace_fields);
        marketplace.push(crate::harness::manifest::PACKS_KEY);
        out.push_str(&format!("- Marketplace: {}\n", list(&marketplace)));
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

/// Whether a note of `kind` reads the bare spelling `key` under `ns`. Under a
/// namespace nothing reads a bare key of ours: only its prefixed form counts.
pub fn is_known(kind: EntityKind, key: &str, ns: &Namespace) -> bool {
    lookup(kind, key).is_some_and(|spec| ns.is_bare() || spec.owner == Owner::Spec)
}

/// Whether a note of `kind` takes `key` from its containers.
pub fn inherits(kind: EntityKind, key: &str) -> bool {
    lookup(kind, key).is_some_and(|spec| spec.inherits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn license_is_the_pack_s_on_a_container_and_the_spec_s_on_a_skill() {
        assert!(is_owned(Plugin, "license"));
        assert!(is_owned(Marketplace, "license"));
        assert!(!is_owned(Skill, "license"));
        let bare = Namespace::default();
        assert!(is_known(Skill, "license", &bare));
        assert!(!is_known(Agent, "license", &bare));
    }

    #[test]
    fn under_a_namespace_only_spec_keys_are_known_bare() {
        let sw = Namespace::sw();
        assert!(is_known(Skill, "license", &sw));
        assert!(!is_known(Skill, "targets", &sw));
        assert!(!is_known(Plugin, "license", &sw));
    }

    /// `targets` inherits on every kind, which the cascade relies on: it asks
    /// no kind whether `targets` inherits.
    #[test]
    fn targets_inherits_on_every_kind() {
        for kind in EntityKind::ALL {
            assert!(inherits(kind, "targets"), "{kind:?}");
        }
    }

    /// The Decided block's list, for the kinds that carry each key.
    #[test]
    fn the_inherited_keys_are_the_decided_list() {
        let listed: std::collections::BTreeSet<&str> = KEYS
            .iter()
            .filter(|spec| spec.inherits)
            .map(|spec| spec.key)
            .collect();
        let mut decided = vec!["author", "license", "repository", "homepage", "targets"];
        decided.extend(HARNESS_BLOCKS);
        assert_eq!(listed, decided.into_iter().collect());
        assert!(inherits(Skill, "license"));
        assert!(!inherits(Agent, "license"));
        assert!(!inherits(Hook, "claude"));
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
            for kind in ALL_BUT_HOOK {
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
}
