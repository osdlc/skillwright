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
//! Claude, Codex, Cursor and Pi carry a [`PluginFormat`]: they are the
//! harnesses a marketplace is built for and a pack is published to. The other
//! five — Amp, Antigravity, Copilot, OpenCode and the portable Agent Skills
//! layout — are destinations a single note publishes to. Their rows hold only
//! the names they answer to and the runtime they render through.
//!
//! # Checked, not generated
//!
//! The rows are written by hand and checked against `harness-reference/`'s
//! facts by this module's tests, where a matching fact exists. Generating
//! them would make a research note's wording a build input; checking them
//! makes a drift between the two a failing test.

use agent_adapt::RuntimeId;

use crate::hooks_file::PluginRootVar;

/// The day the plugin-root hook variables below were last checked against
/// each harness's docs.
const HOOKS_OBSERVED: &str = "2026-10-05";

/// One harness: the names it answers to, the runtime it renders through and,
/// for the four that have one, its plugin format.
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
    /// The plugin format, for a harness a marketplace is built for.
    pub plugin: Option<PluginFormat>,
    /// The harness's own `plugin` CLI, for one `skillwright harness` drives.
    pub install: Option<InstallCli>,
}

/// Where a harness reads a plugin from, and how its pack is shaped.
#[derive(Debug)]
pub struct PluginFormat {
    /// The project directory the runtime's projection puts a skill under,
    /// and so where a pack published to a destination lands.
    pub skills_dir: &'static str,
    /// The folder under the home directory the harness loads a user's own
    /// skills from. `publish` writes there and `adopt` reads from there when
    /// no destination or source is named. Recorded in harness-reference as
    /// each harness's `skills/user-dirs` fact (Claude Code's in
    /// `skills/live-reload`); where a harness reads several, the first.
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
/// The keys Codex reads in `.codex-plugin/plugin.json`, plus the `author` and
/// `license` every pack manifest carries.
const CODEX_MANIFEST_FIELDS: &[&str] = &[
    "name",
    "version",
    "description",
    "author",
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
/// Claude, Codex and Cursor differ in one field — Codex carries an
/// `interface` block and the other two do not. Pi's is a different file
/// altogether. The marketplace manifests differ more, and each dialect names
/// its own builder. Field order in the rendered manifest is fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginDialect {
    /// The Claude pack manifest.
    Claude,
    /// The Codex pack manifest, with an `interface` block.
    Codex,
    /// Cursor's pack manifest is Claude's shape — `name`, `version`,
    /// `description`, `author`, `license`, `keywords`, `skills`, every one of
    /// them a documented Cursor Plugin field
    /// (<https://cursor.com/docs/reference/plugins> retrieved 2026-09-20). It
    /// is a separate variant because it lands in `.cursor-plugin/` and
    /// because its marketplace index is its own shape, not because the pack
    /// manifest differs.
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
    plugin: Some(PluginFormat {
        skills_dir: ".claude/skills",
        user_skills_dir: ".claude/skills",
        plugin_manifest: ".claude-plugin/plugin.json",
        marketplace_manifest: Some(".claude-plugin/marketplace.json"),
        packs: ".claude-plugin/packs",
        dialect: PluginDialect::Claude,
        cli_invocation: CliInvocation::PluginRootToken,
        plugin_root_var: Some(PluginRootVar {
            name: "CLAUDE_PLUGIN_ROOT",
            url: "https://code.claude.com/docs/en/plugins-reference",
            observed_on: "2026-09-26",
            note: None,
        }),
        naming: SkillNaming::HostNamespaced,
        hooks_manifest_entry: None,
        carries_commands: true,
        carries_hooks: true,
        manifest_fields: CLAUDE_MANIFEST_FIELDS,
        marketplace_fields: CLAUDE_MARKETPLACE_FIELDS,
        entry_fields: CLAUDE_ENTRY_FIELDS,
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
    plugin: Some(PluginFormat {
        skills_dir: ".agents/skills",
        user_skills_dir: ".agents/skills",
        plugin_manifest: ".codex-plugin/plugin.json",
        marketplace_manifest: Some(".agents/plugins/marketplace.json"),
        packs: ".agents/packs",
        dialect: PluginDialect::Codex,
        cli_invocation: CliInvocation::SkillRelativePath,
        plugin_root_var: Some(PluginRootVar {
            name: "PLUGIN_ROOT",
            url: "https://learn.chatgpt.com/docs/hooks",
            observed_on: HOOKS_OBSERVED,
            note: Some(
                "a plugin hook also gets `CLAUDE_PLUGIN_ROOT` for compatibility, and runs in \
                 the session's working directory",
            ),
        }),
        naming: SkillNaming::HostNamespaced,
        hooks_manifest_entry: None,
        carries_commands: false,
        carries_hooks: true,
        manifest_fields: CODEX_MANIFEST_FIELDS,
        marketplace_fields: CODEX_MARKETPLACE_FIELDS,
        entry_fields: CODEX_ENTRY_FIELDS,
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
    plugin: Some(PluginFormat {
        skills_dir: ".cursor/skills",
        user_skills_dir: ".cursor/skills",
        plugin_manifest: ".cursor-plugin/plugin.json",
        marketplace_manifest: Some(".cursor-plugin/marketplace.json"),
        packs: ".cursor-plugin/packs",
        dialect: PluginDialect::Cursor,
        cli_invocation: CliInvocation::SkillRelativePath,
        plugin_root_var: Some(PluginRootVar {
            name: "CURSOR_PLUGIN_ROOT",
            url: "https://cursor.com/docs/reference/plugins",
            observed_on: HOOKS_OBSERVED,
            note: Some(
                "documented as expanded, beside `CLAUDE_PLUGIN_ROOT`, in a plugin's MCP \
                 server fields; not documented for hook commands",
            ),
        }),
        naming: SkillNaming::Bare,
        hooks_manifest_entry: Some("./hooks/hooks.json"),
        carries_commands: false,
        carries_hooks: true,
        manifest_fields: CURSOR_MANIFEST_FIELDS,
        marketplace_fields: CURSOR_MARKETPLACE_FIELDS,
        entry_fields: CURSOR_ENTRY_FIELDS,
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
    plugin: Some(PluginFormat {
        skills_dir: ".pi/skills",
        user_skills_dir: ".pi/agent/skills",
        plugin_manifest: "package.json",
        marketplace_manifest: None,
        packs: ".pi/packs",
        dialect: PluginDialect::Pi,
        cli_invocation: CliInvocation::SkillRelativePath,
        plugin_root_var: None,
        naming: SkillNaming::PluginPrefixed,
        hooks_manifest_entry: None,
        carries_commands: false,
        // Pi takes lifecycle events as TypeScript extensions and has no
        // hooks file.
        carries_hooks: false,
        manifest_fields: PI_MANIFEST_FIELDS,
        marketplace_fields: &[],
        entry_fields: &[],
    }),
    install: None,
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

    /// The plugin format of a harness that has one.
    ///
    /// # Panics
    ///
    /// On a spelling-only row. Every caller holds a row from
    /// [`with_plugin_format`], which yields no other kind.
    pub fn format(&self) -> &PluginFormat {
        self.plugin
            .as_ref()
            .expect("only a harness with a plugin format is asked for one")
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
    /// runtime's own capability declaration.
    pub fn accepted_override_keys(&self) -> std::collections::BTreeSet<String> {
        agent_adapt::runtimes::declaration_for(self.declaration)
            .overrides
            .into_keys()
            .collect()
    }
}

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
    use std::path::PathBuf;

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

    // --- Checked against harness-reference ----------------------------------

    /// A fact file's `observations:` values, as written. Only the YAML
    /// frontmatter is read; the prose below it restates the same values.
    fn observed(harness: &str, subject: &str) -> Option<String> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../harness-reference/corpora/alpha/facts")
            .join(harness)
            .join(format!("{subject}.md"));
        let text = std::fs::read_to_string(path).ok()?;
        let frontmatter = text.split("\n---").next()?;
        Some(
            frontmatter
                .lines()
                .filter(|line| line.trim_start().starts_with("- value:"))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    /// The fact directory harness-reference keeps each row under.
    fn reference_name(profile: &HarnessProfile) -> &'static str {
        match profile.name {
            "claude" => "claude-code",
            "copilot" => "copilot-cli",
            "portable" => "agent-skills",
            other => other,
        }
    }

    #[test]
    fn every_skills_dir_is_one_the_harness_loads_skills_from() {
        for profile in with_plugin_format() {
            let Some(values) = observed(reference_name(profile), "skills/project-dirs") else {
                continue;
            };
            let dir = profile.format().skills_dir;
            assert!(
                values.contains(dir),
                "{}: `{dir}` is not among the observed project skill dirs:\n{values}",
                profile.name
            );
        }
    }

    #[test]
    fn every_plugin_manifest_is_the_file_the_harness_reads() {
        for profile in with_plugin_format() {
            let Some(values) = observed(reference_name(profile), "plugins/manifest") else {
                continue;
            };
            let manifest = profile.format().plugin_manifest;
            assert!(
                values.contains(manifest),
                "{}: `{manifest}` is not an observed plugin manifest:\n{values}",
                profile.name
            );
        }
    }

    #[test]
    fn a_marketplace_manifest_is_written_only_where_the_harness_has_one() {
        for profile in with_plugin_format() {
            let Some(values) = observed(reference_name(profile), "plugins/marketplace") else {
                continue;
            };
            let supported = !values.contains("supported: false");
            assert_eq!(
                profile.format().marketplace_manifest.is_some(),
                supported,
                "{}: the row and harness-reference disagree on whether it has a marketplace",
                profile.name
            );
        }
    }

    #[test]
    fn the_facts_the_test_reads_are_there() {
        // A moved bundle would turn every check above into a silent skip.
        assert!(observed("claude-code", "plugins/manifest").is_some());
        assert!(observed("pi", "skills/project-dirs").is_some());
    }
}
