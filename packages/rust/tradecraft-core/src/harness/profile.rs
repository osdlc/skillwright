//! Every harness this crate knows: the [`Harness`] names, and one row of
//! data each.
//!
//! A harness differs from another in the values of its row (D-FDLQ), and
//! what cannot be said as a value is a `match` on [`Harness`] in its
//! translator or its manifests.
//!
//! # Facts from the knowledge file, policy in code
//!
//! A plugin row is a [`PluginPolicy`]: what this crate chooses (the packs
//! folder, a naming the knowledge does not give, whether a vault may choose
//! the naming, the override lists) and, for each fact (manifest and
//! marketplace paths, plugin-root variable, naming rule), either
//! [`Source::Knowledge`] or a [`Source::Constant`] with the reason.
//! [`Harness::format`] resolves a row against the compiled harness knowledge
//! file on first use (D-QXRN). This module's tests hold every
//! knowledge-sourced fact to a moderate or stronger statement and to the
//! value the row held before, so a corpus change that moves a fact is a
//! visible test failure, not a silent change.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::OnceLock;

use agent_adapt::RuntimeId;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::frontmatter::EntityKind;
use crate::hooks_file::PluginRootVar;
use crate::refusal;

/// The day the plugin-root hook variables below were last checked against
/// each harness's docs.
const HOOKS_OBSERVED: &str = "2026-10-05";

/// A harness a marketplace is built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    /// Claude Code.
    Claude,
    /// Codex.
    Codex,
    /// Cursor.
    Cursor,
    /// Pi.
    Pi,
}

/// Every name `build.harnesses` and `--harness` accept, in report order.
/// There are no aliases.
pub const KNOWN_TARGETS: &[Harness] = &Harness::ALL;

/// One harness's row: its product name, the runtime it renders through and
/// its plugin policy.
#[derive(Debug)]
pub struct HarnessProfile {
    /// The harness's product name, for messages.
    pub label: &'static str,
    /// The `agent-adapt` runtime a note renders through for this harness. Its
    /// capability declaration says which keys an override block may carry.
    pub runtime: RuntimeId,
    /// The plugin policy. The facts it leaves to the knowledge file are read
    /// by [`Harness::format`].
    pub plugin: PluginPolicy,
}

/// Where a profile value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source<T> {
    /// The compiled harness knowledge file, looked up at first use.
    Knowledge,
    /// A constant of the row: a policy, a fact the file does not yet carry in
    /// a usable shape, or a value the file disagrees with. Each one says which.
    Constant(T),
}

/// The plugin-root variable's row: which variable is a fact, the evidence note
/// beside it is the row's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRootPolicy {
    /// The variable's name.
    pub name: Source<&'static str>,
    /// The page that documents it.
    pub url: &'static str,
    /// The day it was last checked.
    pub observed_on: &'static str,
    /// What the page adds.
    pub note: Option<&'static str>,
}

/// A harness's plugin row as written in code: the policy, and for each fact
/// whether the knowledge file or a constant gives it. [`Harness::format`]
/// resolves it into a [`PluginFormat`] (D-QXRN).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginPolicy {
    /// The pack manifest; `plugins/manifest-fields`.
    pub plugin_manifest: Source<&'static str>,
    /// The marketplace manifest, `None` with no marketplace;
    /// `plugins/marketplace-fields`.
    pub marketplace_manifest: Source<Option<&'static str>>,
    /// Where this harness's packs land, relative to the marketplace
    /// directory. Distinct per harness, so two harnesses publishing the same
    /// pack never render to one path.
    pub packs: &'static str,
    /// How a rendered skill's body names a file in its plugin's root.
    pub cli_invocation: CliInvocation,
    /// The plugin-root variable; `None` when the pack carries no hooks file.
    pub plugin_root: Option<PluginRootPolicy>,
    /// How a published skill is named; `skills/plugin-namespace`.
    pub naming: Source<SkillNaming>,
    /// Whether the pack has a place for a command.
    pub carries_commands: bool,
    /// Whether the pack carries a hooks file at [`PLUGIN_HOOKS_PATH`].
    pub carries_hooks: bool,
    /// The keys this harness reads in a pack manifest. A container note's
    /// harness block may set these, and the build merges them over what it
    /// generates. Recorded in harness-reference as each harness's
    /// `plugins/manifest-fields` fact.
    pub manifest_fields: &'static [&'static str],
    /// The keys this harness reads at the top of its marketplace manifest,
    /// from its `plugins/marketplace-fields` fact. Empty with no marketplace.
    pub marketplace_fields: &'static [&'static str],
    /// The keys this harness reads in one marketplace entry, from the same
    /// fact.
    pub entry_fields: &'static [&'static str],
    /// Whether a marketplace or a plugin may choose how this harness names its
    /// skills, with `<harness>: {naming: bare | prefixed}`.
    pub naming_choice: bool,
    /// The override keys this harness reads on a subagent and nowhere else.
    /// `agent_adapt` declares them with every other key.
    pub agent_only_overrides: &'static [&'static str],
}

/// Where a harness reads a plugin from, and how its pack is shaped: its
/// policy, and the facts resolved from it.
#[derive(Debug)]
pub struct PluginFormat {
    /// The row's policy.
    pub policy: &'static PluginPolicy,
    /// The project directory the runtime's projection puts a skill under,
    /// agent-adapt's project `skills_dir`. The translator strips it and
    /// writes the skill under the pack's `skills/`. A test holds it to one the
    /// knowledge file's `skills/project-dirs` lists.
    pub skills_dir: String,
    /// The pack manifest, relative to a pack's own directory.
    pub plugin_manifest: &'static str,
    /// The marketplace manifest, relative to the marketplace directory. Its
    /// path is the harness's, fixed: it is where that harness looks, and the
    /// reason every harness can share one directory without colliding. `None`
    /// for a harness with no marketplace (Pi), whose packs install by path.
    pub marketplace_manifest: Option<&'static str>,
    /// The variable this harness's hook commands name the plugin root by.
    /// `None` when its pack carries no hooks file.
    pub plugin_root_var: Option<PluginRootVar>,
    /// How a published skill is named, and so how a call to it is spelled.
    pub naming: SkillNaming,
    /// The keys each kind of note's block for this harness accepts.
    block_keys: BlockKeys,
}

impl PluginFormat {
    /// Whether the pack has a place for a member of `kind`.
    pub fn carries(&self, kind: EntityKind) -> bool {
        kind != EntityKind::Command || self.policy.carries_commands
    }
}

/// The keys a harness's blocks accept, by the kind of note they sit on,
/// worked out once from the row.
#[derive(Debug)]
struct BlockKeys {
    /// A skill's or a command's block.
    component: BTreeSet<String>,
    /// What a member of each kind takes from its plugin's block.
    member: BTreeMap<EntityKind, BTreeSet<String>>,
    /// A plugin's block.
    plugin: BTreeSet<String>,
    /// What a marketplace's block passes down to each plugin's.
    passed_down: BTreeSet<String>,
    /// A marketplace's block.
    marketplace: BTreeSet<String>,
}

impl BlockKeys {
    /// `name` is in no set: a block refuses it (spec section 3 rule 4).
    fn of(harness: Harness, policy: &PluginPolicy) -> Self {
        let mut component = harness.accepted_override_keys();
        component.remove(crate::keys::NAME_KEY);
        let member: BTreeMap<EntityKind, BTreeSet<String>> = EntityKind::ALL
            .into_iter()
            .map(|kind| {
                let mut keys = if kind == EntityKind::Agent {
                    BTreeSet::new()
                } else {
                    component.clone()
                };
                keys.retain(|key| {
                    let key = key.as_str();
                    !policy.agent_only_overrides.contains(&key)
                        && !policy.manifest_fields.contains(&key)
                        && !policy.entry_fields.contains(&key)
                });
                (kind, keys)
            })
            .collect();
        let owned = |keys: &[&str]| keys.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>();
        let mut plugin: BTreeSet<String> = member[&EntityKind::Skill].clone();
        plugin.extend(owned(policy.manifest_fields));
        plugin.extend(owned(policy.entry_fields));
        if policy.naming_choice {
            plugin.insert(crate::harness::manifest::NAMING_KEY.to_owned());
        }
        plugin.remove(crate::keys::NAME_KEY);
        let passed_down: BTreeSet<String> = plugin
            .iter()
            .filter(|key| {
                !policy.marketplace_fields.contains(&key.as_str())
                    && !crate::keys::BLOCK_IDENTITY_KEYS.contains(&key.as_str())
            })
            .cloned()
            .collect();
        let mut marketplace = passed_down.clone();
        marketplace.extend(owned(policy.marketplace_fields));
        marketplace.remove(crate::keys::NAME_KEY);
        Self {
            component,
            member,
            plugin,
            passed_down,
            marketplace,
        }
    }
}

/// Claude Code's `plugin.json` keys, from its `plugins/manifest-fields` fact.
const CLAUDE_MANIFEST_FIELDS: &[&str] = &[
    "$schema",
    "name",
    "displayName",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "metadata",
    "icon",
    "documentationUrl",
    "supportUrl",
    "privacyPolicyUrl",
    "termsOfServiceUrl",
    "defaultEnabled",
    "dependencies",
    "settings",
    "userConfig",
    "types",
    "channels",
    "skills",
    "commands",
    "agents",
    "hooks",
    "mcpServers",
    "lspServers",
    "outputStyles",
    "workflows",
    "experimental",
    "themes",
    "monitors",
];
/// Claude Code's `marketplace.json` top-level keys.
const CLAUDE_MARKETPLACE_FIELDS: &[&str] = &[
    "$schema",
    "name",
    "owner",
    "plugins",
    "description",
    "version",
    "metadata",
    "forceRemoveDeletedPlugins",
    "allowCrossMarketplaceDependenciesOn",
    "renames",
];
/// Claude Code's marketplace entry keys: its own, plus every `plugin.json`
/// key but the directory listing fields.
const CLAUDE_ENTRY_FIELDS: &[&str] = &[
    "name",
    "source",
    "description",
    "version",
    "category",
    "tags",
    "strict",
    "relevance",
    "dependencies",
    "defaultEnabled",
    "displayName",
    "metadata",
    "headers",
    "headersHelper",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "settings",
    "userConfig",
    "types",
    "channels",
    "skills",
    "commands",
    "agents",
    "hooks",
    "mcpServers",
    "lspServers",
    "outputStyles",
    "workflows",
    "experimental",
    "themes",
    "monitors",
];
/// The keys Codex reads in `.codex-plugin/plugin.json`, plus the `author`,
/// `homepage`, `repository` and `license` every pack manifest carries, which
/// Codex reads from a root `plugin.json`.
const CODEX_MANIFEST_FIELDS: &[&str] = &[
    "name",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "skills",
    "commands",
    "mcpServers",
    "apps",
    "hooks",
    "interface",
    "extensions",
];
/// Codex's marketplace top-level keys.
const CODEX_MARKETPLACE_FIELDS: &[&str] = &["name", "interface", "plugins"];
/// Codex's marketplace entry keys.
const CODEX_ENTRY_FIELDS: &[&str] = &["name", "source", "policy", "category"];
/// Cursor's `plugin.json` keys, the docs' and the schema's.
const CURSOR_MANIFEST_FIELDS: &[&str] = &[
    "name",
    "description",
    "version",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "logo",
    "rules",
    "agents",
    "skills",
    "commands",
    "hooks",
    "mcpServers",
    "variables",
    "displayName",
    "minClientVersions",
    "publisher",
    "category",
    "tags",
];
/// Cursor's marketplace top-level keys.
const CURSOR_MARKETPLACE_FIELDS: &[&str] = &["name", "owner", "plugins", "metadata"];
/// Cursor's marketplace entry keys.
const CURSOR_ENTRY_FIELDS: &[&str] = &[
    "name",
    "source",
    "description",
    "version",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "logo",
    "category",
    "tags",
    "skills",
    "rules",
    "agents",
    "commands",
    "hooks",
    "mcpServers",
    "variables",
];
/// The `package.json` keys a Pi package uses: npm's standard fields and the
/// `pi` block.
const PI_MANIFEST_FIELDS: &[&str] = &[
    "name",
    "version",
    "description",
    "author",
    "license",
    "keywords",
    "repository",
    "homepage",
    "dependencies",
    "peerDependencies",
    "pi",
];

/// How a harness names a published skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillNaming {
    /// `<skill>`; the harness puts the plugin in front (`/plugin:skill`).
    HostNamespaced,
    /// `<skill>`, with no plugin anywhere.
    Bare,
    /// `<plugin>-<skill>`, for a harness with no plugin namespace that would
    /// otherwise collide across plugins.
    PluginPrefixed,
}

/// How a rendered skill body names a file in its plugin's root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliInvocation {
    /// As `${CLAUDE_PLUGIN_ROOT}/<path>`. Claude Code expands the token in
    /// skill, command and agent text
    /// (<https://code.claude.com/docs/en/plugins-reference>, retrieved
    /// 2026-09-26).
    PluginRootToken,
    /// As the file's path from the folder the text lands in
    /// (`../../scripts/tool`). Codex, Cursor and Pi expand no variable in
    /// skill text: Cursor's skills documentation
    /// (<https://cursor.com/docs/skills>, retrieved 2026-09-26) says to use
    /// paths relative to the skill root.
    SkillRelativePath,
}

/// The Claude Code row.
const CLAUDE: HarnessProfile = HarnessProfile {
    label: "Claude Code",
    runtime: RuntimeId::ClaudeCode,
    plugin: PluginPolicy {
        plugin_manifest: Source::Knowledge,
        marketplace_manifest: Source::Knowledge,
        packs: ".claude-plugin/packs",
        cli_invocation: CliInvocation::PluginRootToken,
        plugin_root: Some(PluginRootPolicy {
            name: Source::Knowledge,
            url: "https://code.claude.com/docs/en/plugins-reference",
            observed_on: "2026-09-26",
            note: None,
        }),
        naming: Source::Knowledge,
        carries_commands: true,
        carries_hooks: true,
        manifest_fields: CLAUDE_MANIFEST_FIELDS,
        marketplace_fields: CLAUDE_MARKETPLACE_FIELDS,
        entry_fields: CLAUDE_ENTRY_FIELDS,
        naming_choice: false,
        agent_only_overrides: CLAUDE_AGENT_ONLY_OVERRIDES,
    },
};

/// The Codex row.
const CODEX: HarnessProfile = HarnessProfile {
    label: "Codex",
    runtime: RuntimeId::CodexCli,
    plugin: PluginPolicy {
        // knowledge disagrees: plugins/manifest-fields names the Agent Plugins
        // root `plugin.json`; `.codex-plugin/plugin.json` is the legacy path
        plugin_manifest: Source::Constant(".codex-plugin/plugin.json"),
        marketplace_manifest: Source::Knowledge,
        packs: ".agents/packs",
        cli_invocation: CliInvocation::SkillRelativePath,
        plugin_root: Some(PluginRootPolicy {
            name: Source::Knowledge,
            url: "https://learn.chatgpt.com/docs/hooks",
            observed_on: HOOKS_OBSERVED,
            note: Some(
                "a plugin hook also gets `CLAUDE_PLUGIN_ROOT` for compatibility, and runs in \
                 the session's working directory",
            ),
        }),
        naming: Source::Knowledge,
        carries_commands: false,
        carries_hooks: true,
        manifest_fields: CODEX_MANIFEST_FIELDS,
        marketplace_fields: CODEX_MARKETPLACE_FIELDS,
        entry_fields: CODEX_ENTRY_FIELDS,
        naming_choice: false,
        agent_only_overrides: &[],
    },
};

/// The Cursor row.
const CURSOR: HarnessProfile = HarnessProfile {
    label: "Cursor",
    runtime: RuntimeId::Cursor,
    plugin: PluginPolicy {
        plugin_manifest: Source::Knowledge,
        marketplace_manifest: Source::Knowledge,
        packs: ".cursor-plugin/packs",
        cli_invocation: CliInvocation::SkillRelativePath,
        // Cursor documents `CURSOR_PLUGIN_ROOT` only in `mcp.json`, not for
        // hook commands, so the pack ships no hooks until a probe confirms it
        // (spec section 4 rule 23; issue #152).
        plugin_root: None,
        // Policy, as Pi's: knowledge records what Cursor allows (plain
        // names); prefixing the plugin is skillwright's default, so two
        // plugins' skills cannot collide (spec section 8 rule 6).
        naming: Source::Constant(SkillNaming::PluginPrefixed),
        carries_commands: false,
        carries_hooks: false,
        manifest_fields: CURSOR_MANIFEST_FIELDS,
        marketplace_fields: CURSOR_MARKETPLACE_FIELDS,
        entry_fields: CURSOR_ENTRY_FIELDS,
        naming_choice: true,
        agent_only_overrides: &[],
    },
};

/// The Pi row.
const PI: HarnessProfile = HarnessProfile {
    label: "Pi",
    runtime: RuntimeId::Pi,
    plugin: PluginPolicy {
        plugin_manifest: Source::Knowledge,
        marketplace_manifest: Source::Knowledge,
        packs: ".pi/packs",
        cli_invocation: CliInvocation::SkillRelativePath,
        plugin_root: None,
        // Policy: Pi has no plugin namespace (knowledge says bare), and the
        // row prefixes the plugin so two plugins' skills cannot collide.
        // D-WNTQ lets a marketplace or a plugin choose `bare`.
        naming: Source::Constant(SkillNaming::PluginPrefixed),
        carries_commands: false,
        // Pi takes lifecycle events as TypeScript extensions and has no
        // hooks file.
        carries_hooks: false,
        manifest_fields: PI_MANIFEST_FIELDS,
        marketplace_fields: &[],
        entry_fields: &[],
        naming_choice: true,
        agent_only_overrides: &[],
    },
};

/// The facts a profile takes from the knowledge file, one function each. Each
/// returns the value in the shape the profile holds it and the strength of the
/// statement behind it, or `None` when the file has no value in that shape.
pub(crate) mod facts {
    use super::{Harness, SkillNaming};
    use crate::harness::knowledge::{Found, Query, Strength, lookup};

    fn found(harness: Harness, subject: &str) -> Option<Found<'static>> {
        lookup(harness.knowledge_slug(), subject, Query::default())
            .unwrap_or_else(|e| panic!("{harness} {subject}: {e}"))
    }

    /// `plugins/manifest-fields`: the manifest file.
    pub fn plugin_manifest(harness: Harness) -> Option<(&'static str, Strength)> {
        let f = found(harness, "plugins/manifest-fields")?;
        Some((f.value.get("file")?.as_str()?, f.strength))
    }

    /// `plugins/marketplace-fields`: the catalog file, `Some(None)` where the
    /// harness has none.
    pub fn marketplace_manifest(harness: Harness) -> Option<(Option<&'static str>, Strength)> {
        let f = found(harness, "plugins/marketplace-fields")?;
        if f.value.get("applicable").and_then(|a| a.as_bool()) == Some(false) {
            return Some((None, f.strength));
        }
        Some((Some(f.value.get("file")?.as_str()?), f.strength))
    }

    /// `skills/plugin-root-variable`: the variable's name, less `${` and `}` where written.
    pub fn plugin_root_var(harness: Harness) -> Option<(&'static str, Strength)> {
        let f = found(harness, "skills/plugin-root-variable")?;
        let var = match f.value.get("variable") {
            Some(v) => v.as_str()?,
            None => f.value.get("variables")?.get(0)?.as_str()?,
        };
        let bare = var.strip_prefix("${").and_then(|v| v.strip_suffix('}'));
        Some((bare.unwrap_or(var), f.strength))
    }

    /// `skills/plugin-namespace`: a runtime name with the plugin in front
    /// (`acme-tools:review`) is host-namespaced, one without is bare.
    pub fn naming(harness: Harness) -> Option<(SkillNaming, Strength)> {
        let f = found(harness, "skills/plugin-namespace")?;
        let runtime = f.value.get("runtime_name")?.as_str()?;
        let naming = if runtime.contains(':') {
            SkillNaming::HostNamespaced
        } else {
            SkillNaming::Bare
        };
        Some((naming, f.strength))
    }
}

/// A policy's value: the constant, or the knowledge file's.
fn sourced<T>(
    source: Source<T>,
    harness: Harness,
    subject: &str,
    from: impl FnOnce(Harness) -> Option<(T, crate::harness::knowledge::Strength)>,
) -> T {
    match source {
        Source::Constant(value) => value,
        Source::Knowledge => {
            from(harness)
                .unwrap_or_else(|| {
                    panic!("the harness knowledge has no usable {subject} for {harness}")
                })
                .0
        }
    }
}

fn resolve_format(harness: Harness) -> PluginFormat {
    let profile = harness.profile();
    let policy = &profile.plugin;
    PluginFormat {
        policy,
        skills_dir: agent_adapt::runtimes::for_id(profile.runtime)
            .expect("every harness renders through an agent-adapt runtime")
            .paths()
            .skills_dir
            .for_scope(agent_adapt::Scope::Project)
            .to_string_lossy()
            .into_owned(),
        plugin_manifest: sourced(
            policy.plugin_manifest,
            harness,
            "plugins/manifest-fields",
            facts::plugin_manifest,
        ),
        marketplace_manifest: sourced(
            policy.marketplace_manifest,
            harness,
            "plugins/marketplace-fields",
            facts::marketplace_manifest,
        ),
        plugin_root_var: policy.plugin_root.as_ref().map(|root| PluginRootVar {
            name: sourced(
                root.name,
                harness,
                "skills/plugin-root-variable",
                facts::plugin_root_var,
            ),
            url: root.url,
            observed_on: root.observed_on,
            note: root.note,
        }),
        naming: sourced(
            policy.naming,
            harness,
            "skills/plugin-namespace",
            facts::naming,
        ),
        block_keys: BlockKeys::of(harness, policy),
    }
}

/// Every harness's row, in [`Harness::ALL`] order.
static PROFILES: [HarnessProfile; 4] = [CLAUDE, CODEX, CURSOR, PI];

/// Where a plugin's hooks land inside its pack, for the formats that carry
/// them.
pub const PLUGIN_HOOKS_PATH: &str = "hooks/hooks.json";

impl Harness {
    /// Every harness, in report order.
    pub const ALL: [Self; 4] = [Self::Claude, Self::Codex, Self::Cursor, Self::Pi];

    /// The name a note, `build.harnesses` and `--harness` spell it with.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::Pi => "pi",
        }
    }

    /// The harness named `name`.
    ///
    /// # Errors
    ///
    /// `refused: unknown-harness` — a name no harness answers to. Refused
    /// rather than ignored: silently building less than was asked for is the
    /// failure this crate is built around.
    pub fn parse(name: &str) -> Result<Self, CoreError> {
        Self::ALL
            .into_iter()
            .find(|harness| harness.as_str() == name)
            .ok_or_else(|| {
                let known: Vec<String> = Self::ALL.iter().map(|h| format!("`{h}`")).collect();
                let (last, rest) = known.split_last().expect("four harnesses");
                CoreError::refused(
                    refusal::UNKNOWN_HARNESS,
                    format!(
                        "`{name}` is not a harness. The harnesses are {} and {last}",
                        rest.join(", ")
                    ),
                )
            })
    }

    /// This harness's row.
    pub fn profile(self) -> &'static HarnessProfile {
        &PROFILES[self as usize]
    }

    /// The plugin format: this row's policy with the facts the knowledge
    /// file gives, resolved once on first use.
    ///
    /// # Panics
    ///
    /// If the embedded knowledge lacks a fact the row takes from it. The
    /// canonical rows cannot, and that guarantee is the
    /// `the_resolved_formats_equal_the_constants_the_rows_held` test.
    pub fn format(self) -> &'static PluginFormat {
        static FORMATS: OnceLock<[PluginFormat; 4]> = OnceLock::new();
        &FORMATS.get_or_init(|| Self::ALL.map(resolve_format))[self as usize]
    }

    /// The harness's slug in the knowledge file.
    pub(crate) const fn knowledge_slug(self) -> &'static str {
        match self {
            Self::Claude => "claude-code",
            other => other.as_str(),
        }
    }

    /// The paths this harness owns in a marketplace directory: its packs
    /// folder and its marketplace manifest.
    ///
    /// Everything it renders lands at one of these, and nothing else in the
    /// directory is read, compared or replaced. That is what lets every
    /// harness share one directory, and what lets that directory be a
    /// repository root: the folders around them, such as `.agents/skills/`,
    /// hold what people author.
    pub(crate) fn owned_paths(self) -> Vec<&'static str> {
        std::iter::once(self.format().policy.packs)
            .chain(self.format().marketplace_manifest)
            .collect()
    }

    /// The keys this harness reads in a harness block, from its
    /// runtime's own capability declaration, less those no harness fact backs.
    pub(crate) fn accepted_override_keys(self) -> BTreeSet<String> {
        agent_adapt::runtimes::declaration_for(self.profile().runtime)
            .overrides
            .into_keys()
            .filter(|key| !UNREAD_OVERRIDES.contains(&key.as_str()))
            .collect()
    }

    /// The keys this harness's block accepts on a note of `kind`.
    ///
    /// `None` for an agent, whose blocks are not checked.
    pub(crate) fn block_keys(self, kind: EntityKind) -> Option<&'static BTreeSet<String>> {
        let keys = &self.format().block_keys;
        match kind {
            EntityKind::Skill | EntityKind::Command => Some(&keys.component),
            EntityKind::Plugin => Some(&keys.plugin),
            EntityKind::Marketplace => Some(&keys.marketplace),
            EntityKind::Agent => None,
        }
    }

    /// The keys of this harness's block a member of `kind` takes from its
    /// plugin: the keys `kind` accepts, less those a plugin's own manifests
    /// read and those the harness reads on a subagent alone.
    pub(crate) fn member_block_keys(self, kind: EntityKind) -> &'static BTreeSet<String> {
        &self.format().block_keys.member[&kind]
    }

    /// The keys of a marketplace's block that pass down to each plugin's
    /// block.
    pub(crate) fn passed_down_keys(self) -> &'static BTreeSet<String> {
        &self.format().block_keys.passed_down
    }
}

impl fmt::Display for Harness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Claude's override keys that only a subagent reads. `agent_adapt` declares
/// them with every other key, and a test holds this list to that declaration.
const CLAUDE_AGENT_ONLY_OVERRIDES: &[&str] = &["permissionMode"];

/// Declared override keys that no harness fact shows being read in a
/// `SKILL.md`.
const UNREAD_OVERRIDES: &[&str] = &["policy.allow_implicit_invocation"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_only_keys_are_keys_the_harness_declares() {
        let declared: BTreeSet<String> =
            agent_adapt::runtimes::declaration_for(Harness::Claude.profile().runtime)
                .overrides
                .into_keys()
                .collect();
        let agent_only = Harness::Claude.profile().plugin.agent_only_overrides;
        assert_eq!(agent_only.len(), CLAUDE_AGENT_ONLY_OVERRIDES.len());
        assert!(
            agent_only.iter().all(|key| declared.contains(*key)),
            "{agent_only:?}"
        );
    }

    #[test]
    fn a_key_no_fact_backs_is_not_accepted() {
        let keys = Harness::Codex.accepted_override_keys();
        assert!(!keys.contains(UNREAD_OVERRIDES[0]));
    }

    #[test]
    fn a_name_is_a_harness_or_refused_by_one_token() {
        for harness in Harness::ALL {
            assert_eq!(Harness::parse(harness.as_str()).unwrap(), harness);
            let json = serde_json::to_string(&harness).unwrap();
            assert_eq!(json, format!("\"{harness}\""));
        }
        for alias in ["claude-code", "codex-cli", "Claude", "amp"] {
            let err = Harness::parse(alias).unwrap_err();
            assert_eq!(err.token(), Some(refusal::UNKNOWN_HARNESS), "{alias}");
            assert!(
                err.detail()
                    .contains("The harnesses are `claude`, `codex`, `cursor` and `pi`"),
                "{err}"
            );
        }
    }

    #[test]
    fn a_hooks_variable_is_set_exactly_where_hooks_are_carried() {
        for harness in Harness::ALL {
            let format = harness.format();
            assert_eq!(
                format.plugin_root_var.is_some(),
                format.policy.carries_hooks,
                "{harness}"
            );
        }
    }

    // --- Checked against the harness knowledge file -------------------------

    use crate::harness::knowledge::{Query, Strength, lookup};

    /// The value `harness` has for `subject` in the knowledge file, weak and
    /// disputed ones included.
    fn known(harness: Harness, subject: &str) -> Option<serde_json::Value> {
        assert!(
            crate::harness::knowledge::embedded()
                .subjects
                .contains_key(subject),
            "{subject} is not in data/embedded-slice.json"
        );
        let query = Query {
            accept_weak: true,
            accept_disputed: true,
            ..Query::default()
        };
        lookup(harness.knowledge_slug(), subject, query)
            .unwrap()
            .map(|found| found.value.clone())
    }

    /// What each plugin row held as constants before its facts came from the
    /// knowledge file: pack manifest, marketplace manifest, plugin-root
    /// variable and naming.
    type Held = (
        &'static str,
        Option<&'static str>,
        Option<&'static str>,
        SkillNaming,
    );
    fn held(harness: Harness) -> Held {
        match harness {
            Harness::Claude => (
                ".claude-plugin/plugin.json",
                Some(".claude-plugin/marketplace.json"),
                Some("CLAUDE_PLUGIN_ROOT"),
                SkillNaming::HostNamespaced,
            ),
            Harness::Codex => (
                ".codex-plugin/plugin.json",
                Some(".agents/plugins/marketplace.json"),
                Some("PLUGIN_ROOT"),
                SkillNaming::HostNamespaced,
            ),
            Harness::Cursor => (
                ".cursor-plugin/plugin.json",
                Some(".cursor-plugin/marketplace.json"),
                None,
                SkillNaming::PluginPrefixed,
            ),
            Harness::Pi => ("package.json", None, None, SkillNaming::PluginPrefixed),
        }
    }

    #[test]
    fn the_resolved_formats_equal_the_constants_the_rows_held() {
        for harness in Harness::ALL {
            let format = harness.format();
            let (manifest, market, root, naming) = held(harness);
            assert_eq!(format.plugin_manifest, manifest, "{harness}");
            assert_eq!(format.marketplace_manifest, market, "{harness}");
            let var = format.plugin_root_var.map(|var| var.name);
            assert_eq!(var, root, "{harness}");
            assert_eq!(format.naming, naming, "{harness}");
        }
    }

    #[test]
    fn every_lookup_a_profile_makes_resolves_at_moderate_or_better_to_the_held_value() {
        // A guard: `Query::default()` already drops weak values, so this fails
        // only if that default or the strength order changes.
        let enough = |strength: Strength, what: &str| {
            assert!(strength >= Strength::Moderate, "{what} is {strength:?}");
        };
        for name in Harness::ALL {
            let policy = &name.profile().plugin;
            let (manifest, market, root, naming) = held(name);
            if matches!(policy.plugin_manifest, Source::Knowledge) {
                let (value, strength) = facts::plugin_manifest(name)
                    .unwrap_or_else(|| panic!("{name}: no plugins/manifest-fields"));
                enough(strength, &format!("{name} plugins/manifest-fields"));
                assert_eq!(value, manifest, "{name}");
            }
            if matches!(policy.marketplace_manifest, Source::Knowledge) {
                let (value, strength) = facts::marketplace_manifest(name)
                    .unwrap_or_else(|| panic!("{name}: no plugins/marketplace-fields"));
                enough(strength, &format!("{name} plugins/marketplace-fields"));
                assert_eq!(value, market, "{name}");
            }
            if let Some(PluginRootPolicy {
                name: Source::Knowledge,
                ..
            }) = &policy.plugin_root
            {
                let (value, strength) = facts::plugin_root_var(name)
                    .unwrap_or_else(|| panic!("{name}: no skills/plugin-root-variable"));
                enough(strength, &format!("{name} skills/plugin-root-variable"));
                assert_eq!(Some(value), root, "{name}");
            }
            if matches!(policy.naming, Source::Knowledge) {
                let (value, strength) = facts::naming(name)
                    .unwrap_or_else(|| panic!("{name}: no skills/plugin-namespace"));
                enough(strength, &format!("{name} skills/plugin-namespace"));
                assert_eq!(value, naming, "{name}");
            }
        }
    }

    #[test]
    fn every_skills_dir_is_one_the_harness_loads_skills_from() {
        for harness in Harness::ALL {
            let dirs = known(harness, "skills/project-dirs")
                .unwrap_or_else(|| panic!("{harness}: no skills/project-dirs"));
            let dir = harness.format().skills_dir.as_str();
            let listed = dirs["dirs"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|d| d.as_str())
                .any(|d| d.strip_suffix('/').unwrap_or(d).ends_with(dir));
            assert!(
                listed,
                "{harness}: `{dir}` is not among the project skill dirs:\n{dirs}"
            );
        }
    }

    #[test]
    fn a_marketplace_manifest_is_written_only_where_the_harness_has_one() {
        for harness in Harness::ALL {
            let Some(value) = known(harness, "plugins/marketplace") else {
                continue;
            };
            assert_eq!(
                harness.format().marketplace_manifest.is_some(),
                value["supported"] == true,
                "{harness}: the row and the knowledge file disagree on whether it has a marketplace"
            );
        }
    }

    #[test]
    fn block_keys_follow_their_definitions() {
        use crate::frontmatter::EntityKind::{Agent, Marketplace, Plugin};
        for harness in Harness::ALL {
            let policy = harness.format().policy;
            let market = harness.block_keys(Marketplace).unwrap();
            for key in policy.marketplace_fields {
                let held = *key != crate::keys::NAME_KEY;
                assert_eq!(market.contains(*key), held, "{harness} {key}");
            }
            for key in crate::keys::BLOCK_IDENTITY_KEYS {
                assert!(
                    !market.contains(*key) || policy.marketplace_fields.contains(key),
                    "{harness} {key}"
                );
            }
            let plugin = harness.block_keys(Plugin).unwrap();
            for key in policy.agent_only_overrides {
                assert!(!plugin.contains(*key), "{harness} {key}");
            }
            assert_eq!(
                plugin.contains(crate::harness::manifest::NAMING_KEY),
                policy.naming_choice,
                "{harness}"
            );
            assert_eq!(harness.block_keys(Agent), None, "{harness}");
        }
    }

    #[test]
    fn only_claude_has_agent_only_overrides() {
        for harness in Harness::ALL {
            let keys = harness.profile().plugin.agent_only_overrides;
            assert_eq!(!keys.is_empty(), harness == Harness::Claude, "{harness}");
        }
        assert_eq!(CLAUDE_AGENT_ONLY_OVERRIDES, ["permissionMode"]);
    }

    #[test]
    fn harness_blocks_are_the_harnesses() {
        let rows: Vec<&str> = crate::keys::KEYS
            .iter()
            .filter(|spec| spec.shape == crate::keys::Shape::HarnessBlock)
            .map(|spec| spec.key)
            .collect();
        assert_eq!(Harness::ALL.map(Harness::as_str), rows.as_slice());
    }

    #[test]
    fn the_facts_the_test_reads_are_there() {
        // A moved or emptied file would turn every check above into a silent skip.
        assert!(known(Harness::Claude, "plugins/manifest-fields").is_some());
        assert!(known(Harness::Pi, "skills/project-dirs").is_some());
    }
}
