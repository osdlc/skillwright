//! The closed key table: every frontmatter key skillwright reads, per kind.
//!
//! One table drives three things:
//!
//! * **The publish strip.** A key skillwright owns ([`Owner::Skillwright`])
//!   is read into the model and never reaches a destination as written.
//! * **Validation.** A key outside the table for a note's kind is unknown,
//!   and gets a warning. It is never refused.
//! * **The field reference** the docs publish, generated from [`KEYS`].
//!
//! Keys are bare by default. Under a namespace (`build.namespace: sw`), every
//! [`Owner::Skillwright`] key takes the prefix (`version`), or sits in the
//! nested block (`sw: {version: …}`). [`Owner::Spec`] keys are never
//! prefixed.
//!
//! The kind comes from the folder a note sits in, never from the note. The
//! `kind` and `plugin` keys are removed ([`REMOVED`]).

use crate::frontmatter::EntityKind;

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
    /// A string, or a `{name, email, url}` mapping.
    Person,
    /// A mapping of string keys to string values.
    StringMap,
    /// A list of `{name, description, required, values, default}` mappings.
    Arguments,
    /// A mapping of one harness's own keys.
    HarnessBlock,
    /// The `{source, format, hash, at}` block adoption writes.
    Adopted,
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
    /// Whether a component inherits it from the nearest container that sets
    /// it: the plugin note, then `skillwright.md`, then the workspace.
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

/// Keys that structured a vault before 1.0 and are refused now: the folder a
/// note sits in decides its kind and its plugin.
pub const REMOVED: &[&str] = &["kind", "plugin"];

/// Every key skillwright reads, in the order the field reference lists them.
pub const KEYS: &[KeySpec] = &[
    KeySpec {
        key: "name",
        kinds: &[Marketplace, Plugin, Skill, Agent, Command, Hook],
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "The name. Defaults to the file or folder name.",
    },
    KeySpec {
        key: "description",
        kinds: ALL_BUT_HOOK,
        owner: Owner::Spec,
        shape: Shape::String,
        inherits: false,
        summary: "What it does. A component with no description is not published.",
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
        summary: "Plugin keywords.",
    },
    KeySpec {
        key: "category",
        kinds: &[Plugin],
        owner: Owner::Skillwright,
        shape: Shape::String,
        inherits: false,
        summary: "The plugin's category in a marketplace entry. Defaults to Productivity.",
    },
    KeySpec {
        key: "display_name",
        kinds: CONTAINERS,
        owner: Owner::Skillwright,
        shape: Shape::String,
        inherits: false,
        summary: "The human-facing name, Codex's `interface.displayName`.",
    },
    KeySpec {
        key: "targets",
        kinds: PUBLISHED,
        owner: Owner::Skillwright,
        shape: Shape::StringList,
        inherits: true,
        summary: "Which harnesses get it. Absent means all; `[]` means none.",
    },
    KeySpec {
        key: "arguments",
        kinds: COMPONENTS,
        owner: Owner::Skillwright,
        shape: Shape::Arguments,
        inherits: false,
        summary: "A typed argument list. Derives `argument-hint`. A list of plain strings is Claude's own `arguments` key.",
    },
    KeySpec {
        key: "allowed-tools",
        kinds: COMPONENTS,
        owner: Owner::Spec,
        shape: Shape::StringList,
        inherits: false,
        summary: "Tools the skill may use without asking.",
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
        summary: "Where adoption read the note from: `{source, format, hash, at}`. Written by `adopt`.",
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
        "# Field reference\n\n\
         Generated from the key table in `tradecraft-core`. Do not edit this file: \
         run `TRADECRAFT_BLESS=1 cargo test -p tradecraft-core --test field_reference` \
         from `packages/rust`.\n\n\
         The kind of a note comes from its folder. A key not listed for a kind gets a \
         warning and is never refused. Owner `skillwright` means the key is read into \
         the model and stripped at publish, and takes the prefix under \
         `build.namespace`. Owner `spec` means the key belongs to the Agent Skills \
         spec or a harness, and is never prefixed. An inherited key comes from the \
         nearest container that sets it: the plugin note, then `skillwright.md`, \
         then the workspace.\n",
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
        "\n## Harness blocks\n\n\
         A block holds fields only that harness reads. These are the keys each block \
         accepts, by the kind of note it sits on. Any other key gets a warning on a \
         container and is refused on a component.\n",
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

/// skillwright's own keys on a note of `kind`: the ones a namespace
/// prefixes and the publish strip removes.
pub fn owned(kind: EntityKind) -> impl Iterator<Item = &'static KeySpec> {
    KEYS.iter()
        .filter(move |spec| spec.owner == Owner::Skillwright && spec.kinds.contains(&kind))
}

/// Whether `key` (bare) is skillwright's own on a note of `kind`.
pub fn is_owned(kind: EntityKind, key: &str) -> bool {
    lookup(kind, key).is_some_and(|spec| spec.owner == Owner::Skillwright)
}

/// Whether `key` (bare) is in the table for `kind` at all.
pub fn is_known(kind: EntityKind, key: &str) -> bool {
    lookup(kind, key).is_some()
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

    #[test]
    fn the_removed_keys_are_not_in_the_table() {
        for key in REMOVED {
            assert!(KEYS.iter().all(|spec| spec.key != *key), "{key}");
        }
    }
}
