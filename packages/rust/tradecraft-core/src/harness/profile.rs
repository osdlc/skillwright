//! Every harness this crate knows, as one row of data each.
//!
//! Harness facts used to live in about eight places — the build's roots
//! table, publish's plugin layouts and its `plugin_format_carries_*`
//! predicates, the destination-name folds, the runtime maps, the CLI's
//! install adapters — and each covered a different set of harnesses. A fact
//! two of them disagreed on was a bug nobody could see from either side.
//! This table is the one place now (D-FDLQ): a harness differs from another
//! in the values of its row, and what cannot be said as a value is code in
//! its [`crate::harness::HarnessTranslator`].
//!
//! # Two kinds of row
//!
//! Claude, Codex, Cursor and Pi carry a [`PluginPolicy`]: they are the
//! harnesses a marketplace is built for and a pack is published to. The other
//! five — Amp, Antigravity, Copilot, OpenCode and the portable Agent Skills
//! layout — are destinations a single note publishes to. Their rows hold only
//! the names they answer to and the runtime they render through.
//!
//! # Facts from the knowledge file, policy in code
//!
//! A plugin row is a [`PluginPolicy`]: what this crate chooses (the packs
//! folder, the dialect, a naming the knowledge does not give, whether a vault
//! may choose the naming, the override lists) and, for each
//! fact (user skills folder, manifest and marketplace paths, plugin-root
//! variable, naming rule), either [`Source::Knowledge`] or a
//! [`Source::Constant`] with the reason. [`HarnessProfile::format`] resolves a
//! row against the compiled harness knowledge file on first use (D-QXRN). This
//! module's tests hold every knowledge-sourced fact to a moderate or stronger
//! statement and to the value the row held before, so a corpus change that
//! moves a fact is a visible test failure, not a silent change.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

use agent_adapt::RuntimeId;

use crate::frontmatter::EntityKind;

use crate::hooks_file::PluginRootVar;

/// The day the plugin-root hook variables below were last checked against
/// each harness's docs.
const HOOKS_OBSERVED: &str = "2026-10-05";

/// One harness: the names it answers to, the runtime it renders through and,
/// for the four that have one, its plugin policy.
#[derive(Debug)]
pub struct HarnessProfile {
    /// The canonical destination name. Every spelling folds onto it.
    pub name: &'static str,
    /// The harness's product name, for messages.
    pub label: &'static str,
    /// Every name `targets`, harness blocks and a publish's target accept
    /// for this harness, the canonical one first.
    pub spellings: &'static [&'static str],
    /// The `agent-adapt` runtime a note renders through for this destination.
    pub runtime: &'static str,
    /// The runtime whose capability declaration answers for this
    /// destination — which keys an override block may carry. `portable`
    /// renders through `codex-cli` and is declared by the standards-only
    /// adapter, so the two can differ.
    pub declaration: RuntimeId,
    /// The plugin policy, for a harness a marketplace is built for. The
    /// facts it leaves to the knowledge file are read by [`Self::format`].
    pub plugin: Option<PluginPolicy>,
    /// The harness's own `plugin` CLI, for one `skillwright harness` drives.
    pub install: Option<InstallCli>,
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
/// whether the knowledge file or a constant gives it. [`HarnessProfile::format`]
/// resolves it into a [`PluginFormat`] (D-QXRN).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginPolicy {
    /// The project directory the projection puts a skill under. Policy: a
    /// choice among the several a harness reads. A test holds it to one the
    /// knowledge file's `skills/project-dirs` lists.
    pub skills_dir: &'static str,
    /// The folder under the home directory; `skills/user-dirs`.
    pub user_skills_dir: Source<&'static str>,
    /// The pack manifest; `plugins/manifest-fields`.
    pub plugin_manifest: Source<&'static str>,
    /// The marketplace manifest, `None` with no marketplace;
    /// `plugins/marketplace-fields`.
    pub marketplace_manifest: Source<Option<&'static str>>,
    /// Where this harness's packs land. Policy.
    pub packs: &'static str,
    /// Which shape the manifests take.
    pub dialect: PluginDialect,
    /// How a rendered skill's body calls the launcher.
    pub cli_invocation: CliInvocation,
    /// The plugin-root variable; `None` when the pack carries no hooks file.
    pub plugin_root: Option<PluginRootPolicy>,
    /// How a published skill is named; `skills/plugin-namespace`.
    pub naming: Source<SkillNaming>,
    /// See [`PluginFormat::hooks_manifest_entry`].
    pub hooks_manifest_entry: Option<&'static str>,
    /// Whether the pack has a place for a command.
    pub carries_commands: bool,
    /// Whether the pack carries a hooks file.
    pub carries_hooks: bool,
    /// See [`PluginFormat::manifest_fields`].
    pub manifest_fields: &'static [&'static str],
    /// See [`PluginFormat::marketplace_fields`].
    pub marketplace_fields: &'static [&'static str],
    /// See [`PluginFormat::entry_fields`].
    pub entry_fields: &'static [&'static str],
    /// See [`PluginFormat::naming_choice`].
    pub naming_choice: bool,
    /// See [`PluginFormat::agent_only_overrides`].
    pub agent_only_overrides: &'static [&'static str],
}

/// Where a harness reads a plugin from, and how its pack is shaped.
#[derive(Debug)]
pub struct PluginFormat {
    /// The project directory the runtime's projection puts a skill under,
    /// and so where a pack published to a destination lands.
    pub skills_dir: &'static str,
    /// The folder under the home directory the harness loads a user's own
    /// skills from. `publish` writes there and `adopt` reads from there when
    /// no destination or source is named. Each row's `PluginPolicy::user_skills_dir`
    /// says where its value comes from and which folder it keeps when the
    /// harness reads several.
    pub user_skills_dir: &'static str,
    /// The pack manifest, relative to a pack's own directory, in this
    /// harness's dialect.
    pub plugin_manifest: &'static str,
    /// The marketplace manifest, relative to the marketplace directory. Its
    /// path is the harness's, fixed: it is where that harness looks, and the
    /// reason every harness can share one directory without colliding. `None`
    /// for a harness with no marketplace (Pi), whose packs install by path.
    pub marketplace_manifest: Option<&'static str>,
    /// Where this harness's packs land, relative to the marketplace
    /// directory. Distinct per harness, so two harnesses publishing the same
    /// pack never render to one path.
    pub packs: &'static str,
    /// Which shape the manifests take.
    pub dialect: PluginDialect,
    /// How a rendered skill's body should call the plugin's launcher.
    pub cli_invocation: CliInvocation,
    /// The variable this harness's hook commands name the plugin root by.
    /// `None` when its pack carries no hooks file (Pi).
    pub plugin_root_var: Option<PluginRootVar>,
    /// How a published skill is named, and so how a call to it is spelled.
    pub naming: SkillNaming,
    /// The pack manifest's `hooks` value naming the hooks file, for a harness
    /// that reads hooks only from there.
    pub hooks_manifest_entry: Option<&'static str>,
    /// Whether the pack has a place for a command.
    pub carries_commands: bool,
    /// Whether the pack carries a hooks file at
    /// [`crate::publish::PLUGIN_HOOKS_PATH`].
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
    /// skills, with `<harness>: {naming: bare | prefixed}`. Policy.
    pub naming_choice: bool,
    /// The override keys this harness reads on a subagent and nowhere else.
    /// `agent_adapt` declares them with every other key.
    pub agent_only_overrides: &'static [&'static str],
    /// The keys each kind of note's block for this harness accepts.
    block_keys: BlockKeys,
}

/// The block keys of a row with no plugin format.
static NO_KEYS: BTreeSet<String> = BTreeSet::new();

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
    fn of(profile: &HarnessProfile, policy: &PluginPolicy) -> Self {
        let component = profile.accepted_override_keys();
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
        marketplace.insert(crate::harness::manifest::PACKS_KEY.to_owned());
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

/// What `skillwright harness` needs to know about a harness's own CLI that
/// is a fact rather than an argv builder. The builders stay in the CLI crate,
/// which is the only one that runs them.
#[derive(Debug)]
pub struct InstallCli {
    /// The scopes a registration can take, by the name the CLI prints.
    pub scopes: &'static [&'static str],
    /// The harness registers a marketplace only from a git URL.
    pub needs_git_url: bool,
    /// The harness's add command takes a git ref.
    pub takes_git_ref: bool,
}

/// Which dialect a pack manifest is rendered in.
///
/// Claude, Codex and Cursor differ in the display name: Codex carries it in
/// an `interface` block, Cursor as `displayName`, and Claude not at all.
/// Pi's is a different file altogether. The marketplace manifests differ
/// more, and each dialect names its own builder. Field order in the rendered
/// manifest is fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginDialect {
    /// The Claude pack manifest.
    Claude,
    /// The Codex pack manifest, with an `interface` block.
    Codex,
    /// Cursor's pack manifest is Claude's shape — `name`, `version`,
    /// `description`, `author`, `license`, `keywords`, `skills`, every one of
    /// them a documented Cursor Plugin field
    /// (<https://cursor.com/docs/reference/plugins> retrieved 2026-09-20) —
    /// plus `displayName`, which the cursor/plugins schema allows. It lands
    /// in `.cursor-plugin/`, and its marketplace index is its own shape.
    Cursor,
    /// Pi's pack manifest is a `package.json` at the pack root whose
    /// `pi.skills` points at the pack's `skills/` directory. Pi has no
    /// marketplace index.
    Pi,
}

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

/// How a rendered skill body calls the plugin's launcher.
///
/// Every skill in the vault writes the call site the same way, as
/// `${CLAUDE_PLUGIN_ROOT}/<launcher_path>` — the vault's own text is written
/// for Claude, since that is the harness every skill author develops against.
/// This decides what each *other* harness does with that same text at render
/// time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliInvocation {
    /// Leave the vault's `${CLAUDE_PLUGIN_ROOT}/<launcher_path>` text exactly
    /// as written. Claude Code substitutes the token itself, inline, when it
    /// loads skill markdown — see the plugin manifest reference's
    /// "Environment variables" section
    /// (<https://code.claude.com/docs/en/plugins-reference>, retrieved
    /// 2026-09-26): `${CLAUDE_PLUGIN_ROOT}` resolves in "Skill, command, and
    /// agent content" bodies specifically, a Claude Code plugin mechanism
    /// with no counterpart in another harness.
    PluginRootToken,
    /// Rewrite every `${CLAUDE_PLUGIN_ROOT}/<launcher_path>` call site in a
    /// skill's rendered body to the launcher's path relative to the skill's
    /// own folder (`../../bin/tool`), the way a skill names its own files.
    /// Neither Codex CLI nor Cursor substitutes
    /// `${CLAUDE_PLUGIN_ROOT}` — Codex's own skills documentation
    /// (<https://developers.openai.com/codex/skills>) never mentions the
    /// token, and Cursor's skills documentation
    /// (<https://cursor.com/docs/skills>, retrieved 2026-09-26) instructs
    /// authors to "reference scripts in your SKILL.md using relative paths
    /// from the skill root" instead — so a literal, un-rewritten
    /// `${CLAUDE_PLUGIN_ROOT}/...` call site is dead text for either
    /// harness, expanding to `/<launcher_path>` with an empty prefix. Pi does
    /// not substitute it either, and tells the model a skill's base
    /// directory so its relative paths resolve.
    SkillRelativePath,
}

/// The Claude Code row.
pub const CLAUDE: HarnessProfile = HarnessProfile {
    name: "claude",
    label: "Claude Code",
    spellings: &["claude", "claude-code"],
    runtime: "claude-code",
    declaration: RuntimeId::ClaudeCode,
    plugin: Some(PluginPolicy {
        skills_dir: ".claude/skills",
        user_skills_dir: Source::Knowledge,
        plugin_manifest: Source::Knowledge,
        marketplace_manifest: Source::Knowledge,
        packs: ".claude-plugin/packs",
        dialect: PluginDialect::Claude,
        cli_invocation: CliInvocation::PluginRootToken,
        plugin_root: Some(PluginRootPolicy {
            name: Source::Knowledge,
            url: "https://code.claude.com/docs/en/plugins-reference",
            observed_on: "2026-09-26",
            note: None,
        }),
        naming: Source::Knowledge,
        hooks_manifest_entry: None,
        carries_commands: true,
        carries_hooks: true,
        manifest_fields: CLAUDE_MANIFEST_FIELDS,
        marketplace_fields: CLAUDE_MARKETPLACE_FIELDS,
        entry_fields: CLAUDE_ENTRY_FIELDS,
        naming_choice: false,
        agent_only_overrides: CLAUDE_AGENT_ONLY_OVERRIDES,
    }),
    install: Some(InstallCli {
        scopes: &["user", "project", "local"],
        needs_git_url: false,
        takes_git_ref: false,
    }),
};

/// The Codex row.
pub const CODEX: HarnessProfile = HarnessProfile {
    name: "codex",
    label: "Codex",
    spellings: &["codex", "codex-cli"],
    runtime: "codex-cli",
    declaration: RuntimeId::CodexCli,
    plugin: Some(PluginPolicy {
        skills_dir: ".agents/skills",
        user_skills_dir: Source::Knowledge,
        // knowledge disagrees: plugins/manifest-fields names the Agent Plugins
        // root `plugin.json`; `.codex-plugin/plugin.json` is the legacy path
        plugin_manifest: Source::Constant(".codex-plugin/plugin.json"),
        marketplace_manifest: Source::Knowledge,
        packs: ".agents/packs",
        dialect: PluginDialect::Codex,
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
        hooks_manifest_entry: None,
        carries_commands: false,
        carries_hooks: true,
        manifest_fields: CODEX_MANIFEST_FIELDS,
        marketplace_fields: CODEX_MARKETPLACE_FIELDS,
        entry_fields: CODEX_ENTRY_FIELDS,
        naming_choice: false,
        agent_only_overrides: &[],
    }),
    install: Some(InstallCli {
        scopes: &["user"],
        needs_git_url: false,
        takes_git_ref: true,
    }),
};

/// The Cursor row.
pub const CURSOR: HarnessProfile = HarnessProfile {
    name: "cursor",
    label: "Cursor",
    spellings: &["cursor"],
    runtime: "cursor",
    declaration: RuntimeId::Cursor,
    plugin: Some(PluginPolicy {
        skills_dir: ".cursor/skills",
        // knowledge disagrees: skills/user-dirs lists `~/.agents/skills` first
        // and `~/.cursor/skills` second; the row keeps the harness's own folder
        user_skills_dir: Source::Constant(".cursor/skills"),
        plugin_manifest: Source::Knowledge,
        marketplace_manifest: Source::Knowledge,
        packs: ".cursor-plugin/packs",
        dialect: PluginDialect::Cursor,
        cli_invocation: CliInvocation::SkillRelativePath,
        plugin_root: Some(PluginRootPolicy {
            name: Source::Knowledge,
            url: "https://cursor.com/docs/reference/plugins",
            observed_on: HOOKS_OBSERVED,
            note: Some(
                "documented as expanded, beside `CLAUDE_PLUGIN_ROOT`, in a plugin's MCP \
                 server fields; not documented for hook commands",
            ),
        }),
        naming: Source::Knowledge,
        hooks_manifest_entry: Some("./hooks/hooks.json"),
        carries_commands: false,
        carries_hooks: true,
        manifest_fields: CURSOR_MANIFEST_FIELDS,
        marketplace_fields: CURSOR_MARKETPLACE_FIELDS,
        entry_fields: CURSOR_ENTRY_FIELDS,
        naming_choice: true,
        agent_only_overrides: &[],
    }),
    install: Some(InstallCli {
        scopes: &["user"],
        needs_git_url: true,
        takes_git_ref: true,
    }),
};

/// The Pi row.
pub const PI: HarnessProfile = HarnessProfile {
    name: "pi",
    label: "Pi",
    spellings: &["pi"],
    runtime: "pi",
    declaration: RuntimeId::Pi,
    plugin: Some(PluginPolicy {
        skills_dir: ".pi/skills",
        user_skills_dir: Source::Knowledge,
        plugin_manifest: Source::Knowledge,
        marketplace_manifest: Source::Knowledge,
        packs: ".pi/packs",
        dialect: PluginDialect::Pi,
        cli_invocation: CliInvocation::SkillRelativePath,
        plugin_root: None,
        // Policy: Pi has no plugin namespace (knowledge says bare), and the
        // row prefixes the plugin so two plugins' skills cannot collide.
        // D-WNTQ lets a marketplace or a plugin choose `bare`.
        naming: Source::Constant(SkillNaming::PluginPrefixed),
        hooks_manifest_entry: None,
        carries_commands: false,
        // Pi takes lifecycle events as TypeScript extensions and has no
        // hooks file.
        carries_hooks: false,
        manifest_fields: PI_MANIFEST_FIELDS,
        marketplace_fields: &[],
        entry_fields: &[],
        naming_choice: true,
        agent_only_overrides: &[],
    }),
    // `pi install <path>` records a package in the user's settings, or with
    // `-l` the project's. Pi has no marketplace, so packs install by path.
    install: Some(InstallCli {
        scopes: &["user", "project"],
        needs_git_url: false,
        takes_git_ref: false,
    }),
};

/// The Amp row.
pub const AMP: HarnessProfile = HarnessProfile {
    name: "amp",
    label: "Amp",
    spellings: &["amp"],
    runtime: "amp",
    declaration: RuntimeId::Amp,
    plugin: None,
    install: None,
};

/// The Antigravity row.
pub const ANTIGRAVITY: HarnessProfile = HarnessProfile {
    name: "antigravity",
    label: "Antigravity",
    spellings: &["antigravity"],
    runtime: "antigravity",
    declaration: RuntimeId::Antigravity,
    plugin: None,
    install: None,
};

/// The GitHub Copilot row.
pub const COPILOT: HarnessProfile = HarnessProfile {
    name: "copilot",
    label: "GitHub Copilot",
    spellings: &["copilot", "github-copilot"],
    runtime: "github-copilot",
    declaration: RuntimeId::GithubCopilot,
    plugin: None,
    install: None,
};

/// The OpenCode row.
pub const OPENCODE: HarnessProfile = HarnessProfile {
    name: "opencode",
    label: "OpenCode",
    spellings: &["opencode"],
    runtime: "opencode",
    declaration: RuntimeId::OpenCode,
    plugin: None,
    install: None,
};

// `codex` and `portable` are the same layout, and that is the finding,
// not a shortcut — Codex reads the neutral `.agents/skills/` convention,
// which *is* the portable target. The portable target's `AGENTS.md` half
// is not emitted yet; it is an instructions file, not a skill.
/// The Agent Skills (portable) row.
pub const PORTABLE: HarnessProfile = HarnessProfile {
    name: "portable",
    label: "Agent Skills (portable)",
    spellings: &["portable"],
    runtime: "codex-cli",
    declaration: RuntimeId::Portable,
    plugin: None,
    install: None,
};

/// The harness's slug in the knowledge file.
pub(crate) fn knowledge_slug(name: &str) -> &str {
    match name {
        "claude" => "claude-code",
        "copilot" => "copilot-cli",
        "portable" => "agent-skills",
        other => other,
    }
}

/// The facts a profile takes from the knowledge file, one function each. Each
/// returns the value in the shape the profile holds it and the strength of the
/// statement behind it, or `None` when the file has no value in that shape.
pub(crate) mod facts {
    use super::{SkillNaming, knowledge_slug};
    use crate::harness::knowledge::{Found, Query, Strength, lookup};

    fn found(name: &str, subject: &str) -> Option<Found<'static>> {
        lookup(knowledge_slug(name), subject, Query::default())
            .unwrap_or_else(|e| panic!("{name} {subject}: {e}"))
    }

    /// `skills/user-dirs`: the first directory, less its `~/`.
    pub fn user_skills_dir(name: &str) -> Option<(&'static str, Strength)> {
        let f = found(name, "skills/user-dirs")?;
        let dir = f.value.get("dirs")?.get(0)?.as_str()?;
        Some((dir.strip_prefix("~/")?, f.strength))
    }

    /// `plugins/manifest-fields`: the manifest file.
    pub fn plugin_manifest(name: &str) -> Option<(&'static str, Strength)> {
        let f = found(name, "plugins/manifest-fields")?;
        Some((f.value.get("file")?.as_str()?, f.strength))
    }

    /// `plugins/marketplace-fields`: the catalog file, `Some(None)` where the
    /// harness has none.
    pub fn marketplace_manifest(name: &str) -> Option<(Option<&'static str>, Strength)> {
        let f = found(name, "plugins/marketplace-fields")?;
        if f.value.get("applicable").and_then(|a| a.as_bool()) == Some(false) {
            return Some((None, f.strength));
        }
        Some((Some(f.value.get("file")?.as_str()?), f.strength))
    }

    /// `skills/plugin-root-variable`: the variable's name, less `${` and `}` where written.
    pub fn plugin_root_var(name: &str) -> Option<(&'static str, Strength)> {
        let f = found(name, "skills/plugin-root-variable")?;
        let var = match f.value.get("variable") {
            Some(v) => v.as_str()?,
            None => f.value.get("variables")?.get(0)?.as_str()?,
        };
        let bare = var.strip_prefix("${").and_then(|v| v.strip_suffix('}'));
        Some((bare.unwrap_or(var), f.strength))
    }

    /// `skills/plugin-namespace`: a runtime name with the plugin in front
    /// (`acme-tools:review`) is host-namespaced, one without is bare.
    pub fn naming(name: &str) -> Option<(SkillNaming, Strength)> {
        let f = found(name, "skills/plugin-namespace")?;
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
    name: &str,
    subject: &str,
    from: impl FnOnce(&str) -> Option<(T, crate::harness::knowledge::Strength)>,
) -> T {
    match source {
        Source::Constant(value) => value,
        Source::Knowledge => {
            from(name)
                .unwrap_or_else(|| {
                    panic!("the harness knowledge has no usable {subject} for {name}")
                })
                .0
        }
    }
}

fn resolve_format(profile: &HarnessProfile) -> PluginFormat {
    let policy = profile
        .plugin
        .as_ref()
        .expect("a format is resolved from a plugin policy");
    let name = profile.name;
    PluginFormat {
        skills_dir: policy.skills_dir,
        user_skills_dir: sourced(
            policy.user_skills_dir,
            name,
            "skills/user-dirs",
            facts::user_skills_dir,
        ),
        plugin_manifest: sourced(
            policy.plugin_manifest,
            name,
            "plugins/manifest-fields",
            facts::plugin_manifest,
        ),
        marketplace_manifest: sourced(
            policy.marketplace_manifest,
            name,
            "plugins/marketplace-fields",
            facts::marketplace_manifest,
        ),
        packs: policy.packs,
        dialect: policy.dialect,
        cli_invocation: policy.cli_invocation,
        plugin_root_var: policy.plugin_root.as_ref().map(|root| PluginRootVar {
            name: sourced(
                root.name,
                name,
                "skills/plugin-root-variable",
                facts::plugin_root_var,
            ),
            url: root.url,
            observed_on: root.observed_on,
            note: root.note,
        }),
        naming: sourced(
            policy.naming,
            name,
            "skills/plugin-namespace",
            facts::naming,
        ),
        hooks_manifest_entry: policy.hooks_manifest_entry,
        carries_commands: policy.carries_commands,
        carries_hooks: policy.carries_hooks,
        manifest_fields: policy.manifest_fields,
        marketplace_fields: policy.marketplace_fields,
        entry_fields: policy.entry_fields,
        naming_choice: policy.naming_choice,
        agent_only_overrides: policy.agent_only_overrides,
        block_keys: BlockKeys::of(profile, policy),
    }
}

/// Every harness, the four with a plugin format first and in report order
/// (`claude`, `codex`, `cursor`, `pi`), then the spelling-only rows
/// alphabetically.
pub const PROFILES: [HarnessProfile; 9] = [
    CLAUDE,
    CODEX,
    CURSOR,
    PI,
    AMP,
    ANTIGRAVITY,
    COPILOT,
    OPENCODE,
    PORTABLE,
];

impl HarnessProfile {
    /// The install facts of a harness `skillwright harness` drives, readable
    /// in a `const` so the CLI's adapter table can be built from them.
    ///
    /// # Panics
    ///
    /// At compile time, on a row with none.
    pub const fn install_cli(&self) -> &InstallCli {
        match &self.install {
            Some(install) => install,
            None => panic!("this harness has no install CLI"),
        }
    }

    /// The plugin format of a harness that has one: its row's policy with the
    /// facts the knowledge file gives, resolved once on first use.
    ///
    /// # Panics
    ///
    /// On a spelling-only row. Every caller holds a row from
    /// [`with_plugin_format`], which yields no other kind. Also if the
    /// embedded knowledge lacks a fact the row takes from it. The canonical
    /// rows cannot, and that guarantee is the
    /// `the_resolved_formats_equal_the_constants_the_rows_held` test; a custom
    /// row that takes a fact the file lacks panics here.
    pub fn format(&self) -> &'static PluginFormat {
        self.plugin_format()
            .expect("only a harness with a plugin format is asked for one")
    }

    /// [`Self::format`], or `None` for a spelling-only row.
    ///
    /// Resolved from this row's own policy and declaration. A row equal to a
    /// canonical one shares its cached format; a modified or custom row is
    /// resolved once per distinct name, declaration and policy and cached,
    /// which is bounded by how many such rows a program makes.
    pub fn plugin_format(&self) -> Option<&'static PluginFormat> {
        static FORMATS: OnceLock<Vec<(&'static HarnessProfile, PluginFormat)>> = OnceLock::new();
        let policy = self.plugin.as_ref()?;
        let canonical = FORMATS.get_or_init(|| {
            with_plugin_format()
                .map(|profile| (profile, resolve_format(profile)))
                .collect()
        });
        if let Some((_, format)) = canonical.iter().find(|(row, _)| {
            row.name == self.name
                && row.declaration == self.declaration
                && row.plugin.as_ref() == Some(policy)
        }) {
            return Some(format);
        }
        static CUSTOM: Mutex<Vec<(&'static str, RuntimeId, PluginPolicy, &'static PluginFormat)>> =
            Mutex::new(Vec::new());
        let mut custom = CUSTOM
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((_, _, _, format)) = custom.iter().find(|(name, declaration, row, _)| {
            *name == self.name && *declaration == self.declaration && row == policy
        }) {
            return Some(format);
        }
        let format: &'static PluginFormat = Box::leak(Box::new(resolve_format(self)));
        custom.push((self.name, self.declaration, policy.clone(), format));
        Some(format)
    }

    /// The paths this harness owns in a marketplace directory: its packs
    /// folder and its marketplace manifest.
    ///
    /// Everything it renders lands at one of these, and nothing else in the
    /// directory is read, compared or replaced. That is what lets every
    /// harness share one directory, and what lets that directory be a
    /// repository root: the folders around them, such as `.agents/skills/`,
    /// hold what people author.
    pub fn owned_paths(&self) -> Vec<&'static str> {
        self.owned_paths_at(self.format().packs)
    }

    /// [`Self::owned_paths`] for a build whose packs folder is `packs`.
    pub fn owned_paths_at<'p>(&self, packs: &'p str) -> Vec<&'p str> {
        std::iter::once(packs)
            .chain(self.format().marketplace_manifest)
            .collect()
    }

    /// `<packs>/<name>/<tail>` for this harness's pack directory: the plugin
    /// and the rest, `tail` empty for the pack's own directory, `None` for a
    /// path naming no pack of this harness's.
    pub fn split_pack<'p>(packs: &str, relative: &'p str) -> Option<(&'p str, &'p str)> {
        let rest = relative
            .strip_prefix(packs)?
            .strip_prefix('/')
            .unwrap_or("");
        if rest.is_empty() {
            return None;
        }
        Some(match rest.split_once('/') {
            Some((name, tail)) => (name, tail),
            None => (rest, ""),
        })
    }

    /// The keys this harness reads in a harness block, from its
    /// runtime's own capability declaration, less those no harness fact backs.
    pub fn accepted_override_keys(&self) -> std::collections::BTreeSet<String> {
        agent_adapt::runtimes::declaration_for(self.declaration)
            .overrides
            .into_keys()
            .filter(|key| !UNREAD_OVERRIDES.contains(&key.as_str()))
            .collect()
    }

    /// The override keys this harness reads on a subagent and nowhere else:
    /// its row's [`PluginPolicy::agent_only_overrides`].
    pub fn agent_override_keys(&self) -> std::collections::BTreeSet<String> {
        self.plugin
            .iter()
            .flat_map(|policy| policy.agent_only_overrides)
            .map(|key| (*key).to_owned())
            .collect()
    }

    /// The keys this harness's block accepts on a note of `kind`.
    ///
    /// `None` for an agent or a hook, whose blocks are not checked, and for a
    /// row with no plugin format, which no note can carry a block for
    /// ([`crate::keys::HARNESS_BLOCKS`] lists only plugin rows).
    pub fn block_keys(&self, kind: EntityKind) -> Option<&'static BTreeSet<String>> {
        let keys = &self.plugin_format()?.block_keys;
        match kind {
            EntityKind::Skill | EntityKind::Command => Some(&keys.component),
            EntityKind::Plugin => Some(&keys.plugin),
            EntityKind::Marketplace => Some(&keys.marketplace),
            EntityKind::Agent | EntityKind::Hook => None,
        }
    }

    /// The keys of this harness's block a member of `kind` takes from its
    /// plugin: the keys `kind` accepts, less those a plugin's own manifests
    /// read and those the harness reads on a subagent alone. No keys for a
    /// row with no plugin format.
    pub(crate) fn member_block_keys(&self, kind: EntityKind) -> &'static BTreeSet<String> {
        self.plugin_format()
            .and_then(|format| format.block_keys.member.get(&kind))
            .unwrap_or(&NO_KEYS)
    }

    /// The keys of a marketplace's block that pass down to each plugin's
    /// block. No keys for a row with no plugin format.
    pub(crate) fn passed_down_keys(&self) -> &'static BTreeSet<String> {
        self.plugin_format()
            .map_or(&NO_KEYS, |format| &format.block_keys.passed_down)
    }
}

/// Claude's override keys that only a subagent reads. `agent_adapt` declares
/// them with every other key, and a test holds this list to that declaration.
const CLAUDE_AGENT_ONLY_OVERRIDES: &[&str] = &["permissionMode"];

/// Declared override keys that no harness fact shows being read in a
/// `SKILL.md`.
const UNREAD_OVERRIDES: &[&str] = &["policy.allow_implicit_invocation"];

/// The row a name answers to, by any of its spellings.
pub fn profile_for(name: &str) -> Option<&'static HarnessProfile> {
    PROFILES.iter().find(|p| p.spellings.contains(&name))
}

/// The row for a canonical name, and only a canonical one.
pub fn profile_named(canonical: &str) -> Option<&'static HarnessProfile> {
    PROFILES.iter().find(|p| p.name == canonical)
}

/// The harnesses a marketplace is built for, in report order.
pub fn with_plugin_format() -> impl Iterator<Item = &'static HarnessProfile> {
    PROFILES.iter().filter(|p| p.plugin.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_only_keys_are_keys_the_harness_declares() {
        let claude = profile_for("claude").unwrap();
        let declared: std::collections::BTreeSet<String> =
            agent_adapt::runtimes::declaration_for(claude.declaration)
                .overrides
                .into_keys()
                .collect();
        let agent_only = claude.agent_override_keys();
        assert_eq!(agent_only.len(), CLAUDE_AGENT_ONLY_OVERRIDES.len());
        assert!(agent_only.is_subset(&declared), "{agent_only:?}");
        assert!(
            profile_for("codex")
                .unwrap()
                .agent_override_keys()
                .is_empty()
        );
    }

    #[test]
    fn a_key_no_fact_backs_is_not_accepted() {
        let codex = profile_for("codex").unwrap();
        assert!(!codex.accepted_override_keys().contains(UNREAD_OVERRIDES[0]));
    }

    #[test]
    fn every_known_target_spelling_has_exactly_one_row() {
        let mut spellings: Vec<&str> = PROFILES
            .iter()
            .flat_map(|p| p.spellings.iter().copied())
            .collect();
        spellings.sort_unstable();
        let before = spellings.len();
        spellings.dedup();
        assert_eq!(before, spellings.len(), "a spelling names two rows");
        assert_eq!(spellings, crate::publish::KNOWN_TARGETS);
    }

    #[test]
    fn the_canonical_name_is_each_rows_first_spelling() {
        for profile in &PROFILES {
            assert_eq!(profile.spellings.first(), Some(&profile.name));
        }
    }

    #[test]
    fn every_runtime_parses() {
        for profile in &PROFILES {
            crate::projection::parse_runtime_target(profile.runtime)
                .unwrap_or_else(|e| panic!("{}: {e}", profile.name));
        }
    }

    #[test]
    fn the_plugin_rows_come_first_in_report_order() {
        let names: Vec<&str> = with_plugin_format().map(|p| p.name).collect();
        assert_eq!(names, ["claude", "codex", "cursor", "pi"]);
        assert!(
            PROFILES[..4].iter().all(|p| p.plugin.is_some()),
            "a spelling row sits among the plugin rows"
        );
    }

    #[test]
    fn a_hooks_variable_is_set_exactly_where_hooks_are_carried() {
        for profile in with_plugin_format() {
            let format = profile.format();
            assert_eq!(
                format.plugin_root_var.is_some(),
                format.carries_hooks,
                "{}",
                profile.name
            );
        }
    }

    // --- Checked against the harness knowledge file -------------------------

    use crate::harness::knowledge::{Query, Strength, lookup};

    /// The value `profile` has for `subject` in the knowledge file, weak and
    /// disputed ones included.
    fn known(profile: &HarnessProfile, subject: &str) -> Option<serde_json::Value> {
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
        lookup(knowledge_slug(profile.name), subject, query)
            .unwrap()
            .map(|found| found.value.clone())
    }

    /// What each plugin row held as constants before its facts came from the
    /// knowledge file: user skills dir, pack manifest, marketplace manifest,
    /// plugin-root variable and naming.
    type Held = (
        &'static str,
        &'static str,
        Option<&'static str>,
        Option<&'static str>,
        SkillNaming,
    );
    fn held(name: &str) -> Held {
        match name {
            "claude" => (
                ".claude/skills",
                ".claude-plugin/plugin.json",
                Some(".claude-plugin/marketplace.json"),
                Some("CLAUDE_PLUGIN_ROOT"),
                SkillNaming::HostNamespaced,
            ),
            "codex" => (
                ".agents/skills",
                ".codex-plugin/plugin.json",
                Some(".agents/plugins/marketplace.json"),
                Some("PLUGIN_ROOT"),
                SkillNaming::HostNamespaced,
            ),
            "cursor" => (
                ".cursor/skills",
                ".cursor-plugin/plugin.json",
                Some(".cursor-plugin/marketplace.json"),
                Some("CURSOR_PLUGIN_ROOT"),
                SkillNaming::Bare,
            ),
            "pi" => (
                ".pi/agent/skills",
                "package.json",
                None,
                None,
                SkillNaming::PluginPrefixed,
            ),
            other => panic!("{other} has no plugin row"),
        }
    }

    #[test]
    fn the_resolved_formats_equal_the_constants_the_rows_held() {
        for profile in with_plugin_format() {
            let format = profile.format();
            let (user, manifest, market, root, naming) = held(profile.name);
            assert_eq!(format.user_skills_dir, user, "{}", profile.name);
            assert_eq!(format.plugin_manifest, manifest, "{}", profile.name);
            assert_eq!(format.marketplace_manifest, market, "{}", profile.name);
            assert_eq!(
                format.plugin_root_var.map(|var| var.name),
                root,
                "{}",
                profile.name
            );
            assert_eq!(format.naming, naming, "{}", profile.name);
        }
    }

    #[test]
    fn every_lookup_a_profile_makes_resolves_at_moderate_or_better_to_the_held_value() {
        // A guard: `Query::default()` already drops weak values, so this fails
        // only if that default or the strength order changes.
        let enough = |strength: Strength, what: &str| {
            assert!(strength >= Strength::Moderate, "{what} is {strength:?}");
        };
        for profile in with_plugin_format() {
            let name = profile.name;
            let policy = profile.plugin.as_ref().unwrap();
            let (user, manifest, market, root, naming) = held(name);
            if matches!(policy.user_skills_dir, Source::Knowledge) {
                let (value, strength) = facts::user_skills_dir(name)
                    .unwrap_or_else(|| panic!("{name}: no skills/user-dirs"));
                enough(strength, &format!("{name} skills/user-dirs"));
                assert_eq!(value, user, "{name}");
            }
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
        for profile in with_plugin_format() {
            let dirs = known(profile, "skills/project-dirs")
                .unwrap_or_else(|| panic!("{}: no skills/project-dirs", profile.name));
            let dir = profile.format().skills_dir;
            let listed = dirs["dirs"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|d| d.as_str())
                .any(|d| d.strip_suffix('/').unwrap_or(d).ends_with(dir));
            assert!(
                listed,
                "{}: `{dir}` is not among the project skill dirs:\n{dirs}",
                profile.name
            );
        }
    }

    #[test]
    fn a_marketplace_manifest_is_written_only_where_the_harness_has_one() {
        for profile in with_plugin_format() {
            let Some(value) = known(profile, "plugins/marketplace") else {
                continue;
            };
            assert_eq!(
                profile.format().marketplace_manifest.is_some(),
                value["supported"] == true,
                "{}: the row and the knowledge file disagree on whether it has a marketplace",
                profile.name
            );
        }
    }

    #[test]
    fn block_keys_follow_their_definitions() {
        use crate::frontmatter::EntityKind::{Agent, Hook, Marketplace, Plugin};
        for profile in with_plugin_format() {
            let format = profile.format();
            let policy = profile.plugin.as_ref().unwrap();
            let market = profile.block_keys(Marketplace).unwrap();
            for key in format
                .marketplace_fields
                .iter()
                .chain([&crate::harness::manifest::PACKS_KEY])
            {
                assert!(market.contains(*key), "{} {key}", profile.name);
            }
            for key in crate::keys::BLOCK_IDENTITY_KEYS {
                assert!(
                    !market.contains(*key) || format.marketplace_fields.contains(key),
                    "{} {key}",
                    profile.name
                );
            }
            let plugin = profile.block_keys(Plugin).unwrap();
            for key in policy.agent_only_overrides {
                assert!(!plugin.contains(*key), "{} {key}", profile.name);
            }
            assert_eq!(
                plugin.contains(crate::harness::manifest::NAMING_KEY),
                policy.naming_choice,
                "{}",
                profile.name
            );
            assert_eq!(profile.block_keys(Agent), None, "{}", profile.name);
            assert_eq!(profile.block_keys(Hook), None, "{}", profile.name);
        }
    }

    #[test]
    fn only_claude_has_agent_only_overrides() {
        for profile in with_plugin_format() {
            let keys = profile.plugin.as_ref().unwrap().agent_only_overrides;
            assert_eq!(
                !keys.is_empty(),
                profile.name == "claude",
                "{}",
                profile.name
            );
        }
        assert_eq!(
            CLAUDE.plugin.as_ref().unwrap().agent_only_overrides,
            ["permissionMode"]
        );
    }

    #[test]
    fn harness_blocks_are_the_plugin_profiles() {
        let rows: Vec<&str> = crate::keys::KEYS
            .iter()
            .filter(|spec| spec.shape == crate::keys::Shape::HarnessBlock)
            .map(|spec| spec.key)
            .collect();
        let profiles: Vec<&str> = with_plugin_format().map(|p| p.name).collect();
        assert_eq!(crate::keys::HARNESS_BLOCKS, rows.as_slice());
        assert_eq!(crate::keys::HARNESS_BLOCKS, profiles.as_slice());
    }

    #[test]
    fn the_facts_the_test_reads_are_there() {
        // A moved or emptied file would turn every check above into a silent skip.
        let claude = profile_named("claude").unwrap();
        let pi = profile_named("pi").unwrap();
        assert!(known(claude, "plugins/manifest-fields").is_some());
        assert!(known(pi, "skills/project-dirs").is_some());
    }
}

#[cfg(test)]
mod custom_rows {
    use super::*;

    #[test]
    fn a_modified_row_resolves_from_its_own_policy() {
        let mut row = CLAUDE;
        row.plugin.as_mut().unwrap().packs = ".other/packs";
        row.plugin.as_mut().unwrap().plugin_manifest = Source::Constant("custom/plugin.json");
        let format = row.plugin_format().unwrap();
        assert_eq!(format.plugin_manifest, "custom/plugin.json");
        assert_eq!(format.packs, ".other/packs");
        assert_eq!(
            CLAUDE.format().plugin_manifest,
            ".claude-plugin/plugin.json"
        );
    }

    #[test]
    fn a_custom_row_of_constants_needs_no_knowledge() {
        let mut row = PI;
        row.name = "mine";
        let policy = row.plugin.as_mut().unwrap();
        policy.user_skills_dir = Source::Constant(".mine/skills");
        policy.plugin_manifest = Source::Constant("mine.json");
        policy.marketplace_manifest = Source::Constant(None);
        assert_eq!(row.format().user_skills_dir, ".mine/skills");
    }

    #[test]
    fn a_row_with_another_declaration_gets_that_declaration_s_keys() {
        use crate::frontmatter::EntityKind::Skill;
        let mut row = CLAUDE;
        row.declaration = CODEX.declaration;
        assert_ne!(
            CLAUDE.accepted_override_keys(),
            CODEX.accepted_override_keys()
        );
        assert_eq!(row.block_keys(Skill), Some(&CODEX.accepted_override_keys()));
    }

    #[test]
    fn a_custom_row_gets_its_own_block_keys() {
        use crate::frontmatter::EntityKind::Plugin;
        let mut row = CURSOR;
        let policy = row.plugin.as_mut().unwrap();
        policy.manifest_fields = &["name", "zz-extra"];
        policy.naming_choice = false;
        let keys = row.block_keys(Plugin).unwrap();
        assert!(keys.contains("zz-extra"));
        assert!(!keys.contains(crate::harness::manifest::NAMING_KEY));
        assert!(!CURSOR.block_keys(Plugin).unwrap().contains("zz-extra"));
    }

    #[test]
    fn a_custom_row_is_resolved_once() {
        let mut row = CLAUDE;
        row.plugin.as_mut().unwrap().packs = ".once/packs";
        let first = row.plugin_format().unwrap();
        for _ in 0..100 {
            assert!(std::ptr::eq(first, row.plugin_format().unwrap()));
        }
    }
}
