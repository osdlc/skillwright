# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this
project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Breaking

- skillwright v2 reads one fixed vault layout and builds it into `dist/<m>/`. `docs/spec/v2-changes.md` says what each change below means for an existing vault.
- Four commands remain: `init`, `build`, `check` and `bump`. `publish`, `unpublish`, `adopt`, `list`, `status`, `author`, `new`, `rename`, `move`, `delete`, `convert`, `harness install`, `harness update`, `harness uninstall`, `link` and `unlink` are removed. A removed command is an unknown command: a usage error, exit 3.
- The flags `--config`, `--namespace`, `--out`, `--skills-root` and `--watch` are removed, and `init` loses `--out` and `--name`. `--vault` names the root. `check` no longer takes `--dry-run` or `--yes`.
- The vault has one layout: `skillwright.md` and `marketplaces/` at the root, `marketplaces/<m>/<m>.md` for each marketplace, `plugins/<p>/<p>.md` for each plugin, and `skills/<s>/<s>.md` for each skill. Plugin notes are required. The walk up from the working directory stops only at a folder holding both `skillwright.md` and `marketplaces/`. A layout problem is reported once, with its paths and the expected layout.
- Gone from the layout: workspaces, a root file that is also a marketplace, a root `skills/` or `plugins/`, flat entry notes, `SKILL.md` entries, hook notes, custom folder patterns, Obsidian's excluded files, and loose files in a component folder. A `.mcp.json` in a plugin folder is an error. `marketplaces`, `plugins`, `skills`, `agents`, `commands` and `hooks` are reserved names.
- The root file holds only the shared defaults (`author`, `license`, `repository`, `homepage`, `targets` and the harness blocks) and `build:`. `workspace` and the `build:` keys `out`, `namespace`, `exclude`, `patterns`, `assets`, `hooks`, `launcher_path`, `destinations` and `scan_locations` are refused. `build:` outside the root file is refused.
- `name` is refused on every note and in a harness block: the folder's name is the name. There is no key prefix: `sw-version` and an `sw:` block are unknown keys. `packs` in a harness block is refused.
- `build` writes each marketplace to `<root>/dist/<m>/`. There is no other output location. With no `--marketplace` it builds every marketplace, and writes all of them or none. `check` with no `dist/<m>/` reports drift.
- `targets` and `build.harnesses` accept only `claude`, `codex`, `cursor` and `pi`. The other targets and the aliases `claude-code`, `codex-cli` and `github-copilot` are refused.
- Cursor names a plugin's skill `<p>-<s>`, as Pi does, and a call to it renders `/<p>-<s>`. `cursor: {naming: bare}` keeps plain names. Two skills of different plugins that build to one name are refused, naming both plugins.
- A plugin's `references/`, `scripts/` and `assets/` ship under their own names at the pack root. `assets/bin/tool` ships as `assets/bin/tool`, not `bin/tool`.
- Hooks come only from a plugin's `hooks/hooks.json`, which writes `${PLUGIN_ROOT}`. A `${CLAUDE_PLUGIN_ROOT}` there is refused. The Cursor pack ships no hooks for now, and the build warns ([#152](https://github.com/osdlc/skillwright/issues/152)). Packs no longer ship `hooks/gap-report.json`.
- A link resolves inside its plugin. A link that leaves the plugin, reaches a never-ship folder or names nothing is an error. A plain `[[skill]]` is a file link that warns `Did you mean to call /[[skill]]?`; it is not a mention. Nothing is copied to `references/vault/` or `assets/vault/`.
- A call, `/[[name]]`, names a skill or command of the marketplace. A name two plugins share, a fragment on a call and a call to an agent are errors. Code spans and fenced blocks are literal: no call, link or embed is read in them. A call string written by hand in prose is an error. `\[[x]]` ships as `[[x]]`, and the `\/` escape is gone.
- A note or embedded file that holds a noncharacter U+FDD0 to U+FDD2 is refused (`noncharacter`): the build uses them as internal marks.
- An embed of a text file that is neither a note nor an image ships as a fenced code block. An embed of a binary file is refused. Malformed frontmatter in an embedded note is an error that names both notes.
- `${PLUGIN_ROOT}/<path>` in a component's Markdown is checked against what each pack ships, and written for each harness. A path that names nothing shipped is an error. `${CLAUDE_PLUGIN_ROOT}` in a note is an error.
- `init` takes `<m> <p>` and writes the root file, the marketplace note and the plugin note. `bump` names a plugin, or bumps `<m>.md` with `--marketplace`.
- The library modules `apply`, `dirs` and `git_source` are removed, with the modules of the removed commands. The crate still has no supported library API.

### Added

- `cursor: {naming: bare | prefixed}` on `skillwright.md`, a marketplace note or a plugin note. A plugin's value overrides the marketplace's.
- A plugin's Cursor display name reaches its entry in `.cursor-plugin/marketplace.json` as well as its `plugin.json`.

### Changed

- `docs/reference/fields.md` states the types `check` accepts, and lists only the harness block keys `check` accepts. It also lists, per harness, the block keys that stay with a plugin or marketplace and never reach its members.
- A root file value of the wrong shape is refused with the words a note gets, naming the key.

### Fixed

- Private-use characters, such as icon-font glyphs, ship unchanged. U+E002 shipped as `[[`.

## [0.11.1](https://github.com/osdlc/skillwright/compare/skillwright/v0.11.0...skillwright/v0.11.1) - 2026-11-10

### Added

- Write `\[[` to keep a `[[` as text, such as `\[[TODO`. The build ships it as written, backslash included, and Markdown viewers show `[[TODO`. The error for a `[[` that is not a link now says so. ([#47](https://github.com/osdlc/skillwright/issues/47))

### Changed

- `bump --changed-since <ref>` checks out only what the ref's build reads: the root file, the workspace file, the vault folder, the `build.assets` and `build.hooks` sources, and the `.gitattributes` above them. It wrote out the whole repository before, which in a monorepo cost as much as the repository. A symlink in or above those paths still checks out the whole tree. The checkout no longer registers a git worktree.
- `\[[note]]` is no longer a link; it ships as written. It used to build to broken Markdown. ([#47](https://github.com/osdlc/skillwright/issues/47))
- Under `build.namespace`, `check` warns about a prefixed or nested identity key the note's kind does not read, such as `sw-owner` on a plugin note or `sw-keywords` on a marketplace note. It was ignored without a word (a `null` or blank value was refused). ([#75](https://github.com/osdlc/skillwright/issues/75))

### Fixed

- A link from a skill to itself, such as `[[work]]` in the `work` skill or in its `references/bug-route.md`, is written as the skill's name, the way a link to any other skill is: `` `t:work` `` on Claude Code and Codex, `` `t-work` `` on Pi. It was a link to `SKILL.md`. A link with a `#heading` stays a link to that heading. Every skill with such a link builds different output, so `check` reports drift until you rebuild. ([#53](https://github.com/osdlc/skillwright/issues/53))
- A YAML list in a resource's frontmatter, such as `tags: [[a]]`, no longer fails the build as a link. A quoted link there, such as `related: "[[other]]"`, still fails, and the error now says to move it into the body, since frontmatter ships as written. ([#47](https://github.com/osdlc/skillwright/issues/47))

## [0.11.0](https://github.com/osdlc/skillwright/compare/skillwright/v0.10.0...skillwright/v0.11.0) - 2026-11-10

### Breaking

- `list` exits 2 when it skips a note that `build` and `check` refuse, such as a skill with a `description` outside a kind folder (`unreadable-note`) or a note with a value of the wrong kind (`malformed`). It exited 0. In a workspace, only a note skipped inside a marketplace, or the workspace's own `skillwright.md`, counts, since a workspace build reads only those.

### Added

- A plugin's `display_name` is also the `displayName` in its Cursor `plugin.json`. A `cursor.displayName` still wins for Cursor.

### Fixed

- A skill, command or agent that states no `name` takes one from its file name. When that name is empty, as for `日本.md`, or over 64 characters, `build`, `check` and `publish` refuse the note with `invalid-name`, saying why and to set `name:`, and nothing is written. `list` lists the note, flags its row `[invalid-name]`, prints an `error ·` line for it and exits 2; its `--json` row has an `invalid_name` key. The note was skipped as `unreadable-note`, with a message that did not say the name came from the file name. The name is never shortened or transliterated.
- A bare link such as `[[contract]]` that matches files in several folders resolves to the one match under the linking note's own folder, such as the copy in its own skill bundle's `references/`. With no match there, or several, it is still refused `link-ambiguous`; several names only those. Every match was refused before, even one in the note's own bundle. A rename reads links by the same rule, so it rewrites such a link when it renames the file.
- The Codex `plugin.json` carries `repository` and `homepage` from the plugin or `skillwright.md`, as the Claude and Cursor manifests do.
- The Pi `package.json` carries the plugin's `keywords`, with `pi-package` added when it is missing, so the pi.dev gallery lists a pack published to npm. A `pi.keywords` block key replaces the whole list.
- The author skill says how to show a call as literal text: `\/[[skill]]` ships as `/[[skill]]` in every harness, in prose and in code spans.

## [0.10.0](https://github.com/osdlc/skillwright/compare/skillwright/v0.9.0...skillwright/v0.10.0) - 2026-11-10

### Breaking

- A skill, command or agent with a `name` and no `description` is an error. `build` and `check` refuse it and exit 2. `publish` of the whole vault refuses before writing anything and exits 2. `publish <path>` of the note, or of a plugin that holds it, reports that note refused with `missing-description`. They published it with "No description provided." before. `list` still lists the note, flags its row `[missing-description]`, prints an `error ·` line for it, and exits 2. Its `--json` row has a `missing_description` key.
- `build.require_descriptions` is removed, since the refusal is always on. A root file that sets it gets an unknown-key warning.
- A plugin or marketplace note with a `name` and no `description` no longer warns or ships a filler. Its manifest takes the configured description or the default.
- `list` prints a `path-collision` line as `error ·`, not `warning ·`.

### Changed

- Documented that `publish <path>` publishes a note as a skill when no kind folder or `build.patterns` glob places it. The `publish` help, the structure spec and the `skillwright.md` reference say so, and say that `list`, `check`, `build` and `publish --all` do not include such a note. No behaviour change.

## [0.9.0](https://github.com/osdlc/skillwright/compare/skillwright/v0.8.0...skillwright/v0.9.0) - 2026-11-09

### Breaking

- In a workspace, `build.exclude` works like nested `.gitignore` files: each glob reads from the folder of the `skillwright.md` it is written in, and a member's exclusions add to the workspace's. A workspace glob `secrets/**` now means `<workspace>/secrets/**`; write `**/secrets/**` or `marketplaces/*/secrets/**` to reach each member. A member's `exclude` can no longer drop a workspace glob; narrow the workspace glob instead, such as `marketplaces/other/secrets/**` in place of `**/secrets/**`. `build`, `check` and `list`, at the workspace root or in a member, warn once, naming the workspace file, about a workspace exclude glob that matches nothing in any member. `publish <note>` names the file whose `build.exclude` excluded the note. A member's build warns when a workspace glob misses it but would have matched it under the old rule, and says how to rewrite it. Experimental.
- Library API: `ResolvedRun::config_dir` is removed; it always equalled `vault`. `ResolvedRun::namespace` is a `Namespace`, bare by default, not an `Option`. `config::resolve_configured_path` is removed; use `config::resolve_against_cwd`, or `ResolvedRun::configured_path` for a path the root file names. `ResolvedRun::core_namespace` returns `&Namespace`. `ResolvedRun::configured_out` is public.

### Fixed

- `delete <plugin>` trashes a plugin whose folder holds only `assets/` besides its note. It was refused, with the asset files named as members it still held.
- `publish <note>` for a note outside `marketplaces/` is no longer refused by a duplicate name inside a marketplace.
- With a pattern that matches the vault root, such as `build.patterns.skills: "*"`, `list`, `build` and `publish` no longer treat the vault's own `skillwright.md` as a skill.
- A move treats `marketplaces/<m>/skillwright.md` as a marketplace's entry for every `<m>`, `skillwright` included, and every other `<dir>/skillwright.md` as an ordinary bundle, such as `tools/skillwright/skillwright.md` or `skills/group/skillwright/skillwright.md`. It decided by the folder's name, so `marketplaces/skillwright/skillwright.md` moved as a bundle, and a bundle named `skillwright` outside a kind folder moved as a marketplace, with its links left as written.
- Moving or renaming the vault's own `skillwright.md`, or moving a note onto it, is refused, and nothing is written. The check ignores case, so `SKILLWRIGHT.md` is refused too: on a case-insensitive file system, as macOS's is by default, it is the same file. Moving it failed with a file error, and moving a plain note onto it replaced the vault's config file.
- A marketplace rename that half-lands names the entry `skillwright.md` in its report and error, not the temporary `<m>.md` name the move used. Putting the entry's name back no longer renames a folder of yours that holds `<m>.md`.
- When a marketplace rename lands but the entry's `skillwright.md` name cannot be put back, the error lists what was written and says where the entry is. It reported a bare file error. When the move failed too, both failures are reported.
- When a marketplace folder moves and its entry's rename then fails, the error says the folder moved and where the entry now is. It reported a bare file error, which means nothing was written.

## [0.8.0](https://github.com/osdlc/skillwright/compare/skillwright/v0.7.0...skillwright/v0.8.0) - 2026-11-08

### Breaking

- skillwright no longer checks for the pre-1.0 vault shape. Old keys and files are treated like any other key or file it does not know:
  - The `removed-key` refusal is gone. `kind` and `plugin` on a note, `sw-kind` and `sw-plugin` under `build.namespace: sw`, and, with no namespace, an `sw-` spelling or an `sw:` block of a skillwright key are ordinary unknown keys. Each warns `unknown-key` and is handled like any unknown key: a skill ships it under `metadata:`, a command carries it as written, and a container ignores it.
  - A top-level `bump:` and `build.shim_target` in `skillwright.md` are ordinary unknown keys: each warns that it is not a key skillwright reads, and is ignored. They were refused.
  - A `skillwright.yaml` or `tradecraft.yaml` beside the vault gets no note.
  - A `marketplaces/<m>/<m>.md` note is no longer refused with a pointer to `marketplaces/<m>/skillwright.md`. It is skipped as a stray note if it declares a `description`.
  - `adopt` keeps a source skill's `kind` or `plugin` key at the top level instead of moving it under `metadata:`. It now warns `unknown-key` on build.
- `check --fix-adopt-hashes` is removed. It only refreshed adopt hashes after the move from prefixed to bare keys.
- The plugin's `migrate` skill and the migration guide are removed.

## [0.7.0](https://github.com/osdlc/skillwright/compare/skillwright/v0.6.0...skillwright/v0.7.0) - 2026-11-07

This release refuses some vaults that 0.6.0 built. To find what needs fixing, run `skillwright list`: it shows each refused note as `skipped · malformed: …`, naming the key and the kind of value it must hold. The [field reference](https://github.com/osdlc/skillwright/blob/skillwright/v0.7.0/docs/reference/fields.md) gives each key's type.

### Breaking

- A value of the wrong kind is refused with the code `malformed`. `build` and `check` stop and write nothing, `list` shows the note as skipped, and `publish` refuses it. Fix the value. If YAML read a number, `true` or `false` that you meant as text, quote it. These are refused where 0.6.0 built or published them:
  - A key a harness reads in a `claude:`, `codex:`, `cursor:` or `pi:` block, such as `claude: {allowed-tools: 5}` on a skill or `codex: {policy: 5}` on a plugin. Every block is now checked, even a block for a harness that `build.harnesses` leaves out. A key the block does not read still only warns `unknown-key`.
  - A spec key on a skill, agent or command, such as `model: [1]`, `name: 2048`, `compatibility: [claude-code]` or `allowed-tools: 5`. 0.6.0 carried it under `metadata:`.
  - `description: 2048`, `description: [a, b]` or `name: 5`. With a `name`, 0.6.0 published the note with "No description provided." and a `missing-description` warning. Without one, it skipped the note with a `no-component-keys` warning.
  - An agent's `tools` that is not a string, a list of strings or a mapping of tool names, and a `temperature` that is not a number, such as `temperature: "0.2"`. 0.6.0 published these to OpenCode. Such an agent now also stops `build` and `check`, although a build does not carry agents.
- A spec key or a block key set to nothing, such as `model:` with no value, is dropped. 0.6.0 shipped it as `null`: under `metadata:` when written bare, and in the skill or manifest when written in a block. It no longer clears a value the note inherits: a skill's `claude: {model: }` under a plugin's `claude: {model: opus}` now ships `model: opus`. To change an inherited value, set the value you want, or remove it from the container. A bare unknown key set to nothing still ships as `null` under `metadata:`, with an `unknown-key` warning.
- Some refusals have a new code. A script that reads the code must look for `malformed`:
  - A skillwright key of the wrong shape, such as `targets: 5`, was `unreadable-note` from `build`, `check` and `list`, and `io` from `publish`.
  - A plugin's or marketplace's block `metadata` that is not a mapping was `malformed-block`, and `io` from `publish`.
  - A skill's block `metadata` that is not a mapping had no code from `build` and `check`, and was `io` from `publish`.

### Fixed

- `allowed-tools: Read, Grep` is read as two tools, as Claude Code's docs allow. 0.6.0 warned `unknown-key` and carried it under `metadata:`.
- `user-invocable` and `disable-model-invocation` also read `yes`, `no`, `on`, `off`, `1` and `0`, in any case and quoted or not, and ship as `true` or `false`. 0.6.0 carried these words under `metadata:` when bare, so `user-invocable: no` left the skill invocable.
- An unquoted `argument-hint: [issue-number]`, which YAML reads as a list, ships as the hint `"[issue-number]"`, bare or in a `claude:` block. 0.6.0 carried it as a list: under `metadata:` when bare, and as a top-level `argument-hint` list in a block.
- `list` skips every note a build refuses, including a bad block value on a plugin and a bad block in the root file. A bad block in a workspace file is one row, naming the workspace file.
- A bad block in the root file or a plugin note is blamed on that marketplace or plugin, by `build`, by `publish <note>` and by `status`. 0.6.0 published a member skill under a bad container block.
- A skipped note's code is printed once: `skipped · malformed: …`, not `skipped · removed-key: refused: removed-key: …`.
- `build` and `check` with an `--out` other than `build.out`, and `bump --changed-since` against a folder or a git ref, no longer read the output inside `build.out` as notes. A packs folder moved inside `build.out`, such as `pi: {packs: pi-packs}`, was refused as `unreadable-note`.

## [0.6.0](https://github.com/osdlc/skillwright/compare/skillwright/v0.5.1...skillwright/v0.6.0) - 2026-11-07

This release changes how a vault is configured and laid out, and most vaults stop building until they move. Start with the [migration guide](https://github.com/osdlc/skillwright/blob/skillwright/v0.6.0/docs/guide/migrating.md). Its steps are in [`migrate.md`](https://github.com/osdlc/skillwright/blob/skillwright/v0.6.0/plugin/plugins/skillwright/skills/migrate/migrate.md), which an agent can read and follow.

### Breaking

Configuration:

- `skillwright.yaml`, `tradecraft.yaml`, the machine config file and its `SKILLWRIGHT_MACHINE_CONFIG` variable are no longer read. A leftover `skillwright.yaml` or `tradecraft.yaml` gets a note saying so. Move the settings to one `skillwright.md` in the vault folder; `skillwright init` writes one. See the [`skillwright.md` reference](https://github.com/osdlc/skillwright/blob/skillwright/v0.6.0/docs/reference/skillwright-md.md).
- `build`, `check` and `build --watch` need `--out` or `build.out`. There is no default output folder. `out: .` builds beside `skillwright.md`, and `out` is relative to that file. The `harness` verbs find the build through `--path` or `build.out` the same way.
- `default_plugin` and `--default-plugin` are removed. A note outside every plugin folder belongs to the marketplace's own plugin, named after the marketplace.
- A top-level `bump:` is refused. Move it to `build.bump`.
- `marketplace.shim_target` is now `build.launcher_path`. It has no default: the old `cli/sdlc` default is gone, and with no value launcher calls are left as written. A leftover `build.shim_target` is refused.

Notes and folders:

- Frontmatter keys are bare by default: `version`, not `sw-version`. A `sw-<key>` spelling of a key skillwright owns is refused as `removed-key`. To keep the prefix, set `build.namespace: sw`; see [Keeping the prefix](https://github.com/osdlc/skillwright/blob/skillwright/v0.6.0/docs/guide/migrating.md#keeping-the-prefix). Keys in `skillwright.md` are always bare.
- `kind`, `plugin`, `sw-kind` and `sw-plugin` are refused. The folder decides both: move the note under `plugins/<p>/` and `skills/`, `agents/`, `commands/` or `hooks/`. See the [structure spec](https://github.com/osdlc/skillwright/blob/skillwright/v0.6.0/docs/spec/structure-v1.md).
- `sw-overrides` is replaced by top-level `claude:`, `codex:`, `cursor:` and `pi:` blocks. They now apply to `build` and `check` as well as `publish`, so built output can change. The other targets (`portable`, `copilot`, `opencode`) have no block, so overrides for them have no replacement. An unknown key in a block warns and is left out. `policy.allow_implicit_invocation` is not accepted under `codex:`.
- `marketplaces/<m>/<m>.md` is refused. Move it to `marketplaces/<m>/skillwright.md` with bare keys. Such a marketplace takes its `version` from its own note only, never from a root file above it.
- A plugin note whose `name` differs from its folder is refused. Rename one of them.
- A `skillwright.md` directly in a kind folder, such as `skills/skillwright.md`, is refused as a reserved name. Move it to `skills/skillwright/skillwright.md`.
- A plugin or marketplace note with neither `name` nor `description` is now read instead of being dropped.
- An `author` or `owner` that is neither a name nor a mapping with a `name` is refused.
- A plugin's `author` no longer falls back to `owner`. Set `author` in `skillwright.md` or on the plugin note to keep it in the manifests.
- A link or embed into a folder that never ships, such as a bundle's `docs/`, `tests/` or `fixtures/`, is refused as `excluded-link`. A note with a `description` in such a folder warns `never-ships`, and `publish` refuses it.
- Unknown keys warn as `unknown-key`. On a skill they ship under `metadata:` as YAML, and on a container or in `skillwright.md` they are ignored. This includes `allowed-tools: Read, Grep`: the comma form is not read as the key, so use a list or the space-separated form.
- An agent that sets both `tools` and `allowed-tools` is refused as `conflicting-keys`. A `tools` that names no tool is refused as `malformed-tools`. So is a `tools` that is not a list or a string of names, when the agent publishes to Claude Code.
- `check --fix-adopt-hashes` is needed after moving an adopted note to the new keys. Adopt provenance now sits in one `adopted:` block, and `adopt` no longer writes `kind` or `plugin`.

Rendered output:

- A plain `[[skill]]` in a skill or command body now renders as the skill's name, not a call. Write `/[[skill]]` where you want a call. The `implicit-skill-call` warning is gone.
- Codex, Cursor and Pi skills call a plugin's launcher by its path relative to the skill, such as `../../bin/tool`. This happens only when the plugin ships the launcher as an asset at `build.launcher_path`, for example under `plugins/<p>/assets/`; otherwise the call is left as written, with no warning. Builds no longer write a `scripts/sdlc` shim into each skill, and there is no `sdlc` `PATH` fallback. Set `build.launcher_path`, and expect many built files to change.
- Claude's marketplace entry no longer holds `version`, and Cursor's manifests no longer hold `author.url` or `owner.url`.
- `author`, `license`, `repository`, `homepage`, `targets` and harness blocks inherit from the nearest container. A skill can gain the inherited license, and a single-note or pack `publish` can stop writing to a harness its containers exclude.

Publishing:

- With no `--to` and no `build.destinations`, `publish` and `status` use each harness's user skill folder, such as `~/.claude/skills`. A bare `adopt` with no `build.scan_locations` reads the same folders. Set `build.destinations` or pass `--to` to keep writing somewhere else.
- A bare `publish` writes skills only, with no plugin packs. `publish <plugin note>` still writes a pack.

Library:

- `apply::apply` and `apply::preview` take an `apply::Vault`, built with `Vault::new(dir, source)`, in place of the vault path.
- `Applied`, `ApplyError`, `Outcome` and `AppliedEntry` in `apply` are `#[non_exhaustive]`. A `match` needs a `_` arm, and the structs are read by field.
- `apply` is the supported library API. The other modules (`cli`, `commands`, `config`, `ctx`, `dirs`, `exit`, `git_source`, `prompt`, `report`, `resolve`) are `#[doc(hidden)]` and may change in any release.
- Removed from `config`: `load_config`, `SkillwrightConfig`, `ConfigLoad`, `DestinationConfig`, `AssetEntry`, `DeclaredFolders`, `HookFileEntry`, `MarketplaceSettings`, `machine_config_path`, `MACHINE_CONFIG_ENV`, `PROJECT_FILE`, `TOOL` and `NEEDS`. Also removed: `report::render_kind` and the `test-support` feature. The path helpers `normalize_join` and `relative_path` moved to `tradecraft_core::paths`.

The bundled `skillwright` plugin moved too: its note carries `version`, and its `migrate`, `setup`, `author` and `publish` skills describe the new structure.

### Added

- `skillwright init [--out <path>] [--name <name>]` writes a `skillwright.md`. It refuses if the file exists.
- `check --fix-adopt-hashes` refreshes the adopt hash of a note whose only change is the key rename. A note that really changed is reported and left alone.
- Folder convention: `plugins/<p>/` is a plugin and a root `skills/` is the marketplace's own plugin. `plugins/<p>/assets/` is copied to the plugin's root.
- `plugins/<p>/CHANGELOG.md` ships in every pack, and `check` warns when it has no heading for the plugin's `version`.
- Workspaces: a root file with `workspace: true` sits above `marketplaces/<m>/skillwright.md` files. Each member inherits every `build:` key but `out`, and `author`, `license`, `targets`, `repository`, `homepage` and the harness blocks. It does not inherit `owner`. `build` and `check` run over each member, and `--marketplace` picks one.
- `repository` and `homepage` reach every manifest with the field, and `version` is read on `skillwright.md`.
- `bump --changed-since <ref|dir>` bumps every plugin whose output differs from a git ref or a built folder. `bump --marketplace <name>` picks one marketplace. `build.bump.patch: timestamp` sets the patch to the UTC time. See [Bump](https://github.com/osdlc/skillwright/blob/skillwright/v0.6.0/packages/rust/skillwright/README.md#bump).
- `harness install`, `update` and `uninstall` support Pi. `harness link` and `unlink` symlink built skills into Claude Code's or Codex's user folder for a dev loop.
- `<harness>: {packs: <path>}` moves a harness's packs folder. A folder left behind by a move gets a `stale-output` warning.
- `pi: {naming: bare}` drops the plugin prefix from Pi skill names.
- Experimental: `build.patterns` and `build.exclude` describe a layout that differs from the convention. See [Patterns](https://github.com/osdlc/skillwright/blob/skillwright/v0.6.0/docs/reference/skillwright-md.md#patterns).
- Docs: a [getting-started guide](https://github.com/osdlc/skillwright/blob/skillwright/v0.6.0/docs/guide/getting-started.md), the structure spec, and a [field reference](https://github.com/osdlc/skillwright/blob/skillwright/v0.6.0/docs/reference/fields.md).

### Fixed

- An agent's `tools` reaches Claude Code. It was dropped, so the published agent could use every tool.
- A command's `disable-model-invocation`, `user-invocable` and `context` are carried to the built command.
- `list` shows the roster with colliding rows flagged and exits 2, instead of failing.
- A marketplace rename rewrites links into it from outside, and a refused rename touches no file.
- A bundle or marketplace folder named `skillwright` moves and relinks.
- `adopt` keeps a link to a folder as written, handles escaped brackets in link labels, and keeps the bundle path in wikilinks, so two skills that share a resource name compile.
- `[[v1.2 notes]]` resolves to `v1.2 notes.md`.
- A build keeps a configured plugin or marketplace description, instead of replacing it with "No description provided.".
- Plugin-prefixed skill names that meet across plugins are refused as `path-collision`, not silently overwritten.
- A vault or asset source inside a tree the build replaces is refused as `unsafe-root`. A build replaces only the paths it owns in each harness folder.
- A rendered path that switches between file and directory rebuilds cleanly.
- `create`, `delete` and `move` report partial writes honestly when a later step fails.
- `publish` reports a plugin's own manifest that the rendered one replaces on `replaced_manifest`, in its output and in `--json`.
- `delete`, `rename`, `move`, `convert` and `bump` read bundle shape the way discovery does, and match `.MD` the same as `.md`.
- A multi-source `adopt --dry-run` predicts the collision the real run would refuse.
- `harness update --git-url` refreshes the marketplace before updating a plugin.
- `build --watch` ignores file-access events.

## [0.5.1](https://github.com/osdlc/skillwright/compare/skillwright/v0.5.0...skillwright/v0.5.1) - 2026-11-05

### Other

- stop on an unlistable folder inside a moved bundle; quieter skip rows
- find what the walk cannot read first, then load the vault once
- edit no file in a dot-named folder; skip a folder it cannot list
- one ApplyError for the walk failure; one pass for a blank line
- name the folder the walk cannot list, not only the vault
- pass the walk's io error through; the refusing walk returns paths
- Discovery and compile use the walk and the link index, not a Corpus
- read the vault with the lenient frontmatter split, as a build does
- watch test: stage each save beside the note so the rename stays on one filesystem
- Find links with markdown-link's scanner; calls stay a layer on top

### Changed

- `build` and `publish` find links in a note the way the other osdlc tools do. A wikilink holding a `[` or `]`, or code, is no longer a link and is left as written; `[[]]` is no longer a link to the note itself; and `[[Note\|alias]]` in a table cell now links to `Note`.
- `build`, `check` and `publish` refuse a note with a `[[` that is not a link (`unresolved-syntax`): a `[` or `]` before the closing `]]`, a target that is only code or has code in it, an empty target, or no closing `]]`. Before, it shipped as literal brackets.
- The skills source walk leaves out `.obsidian` and `node_modules` wherever they sit, as it already left out `.trash`.
- A rename or move reads the vault as a build does: dot-named files are seen, `.obsidian`, `.trash` and `node_modules` are not, and frontmatter is split leniently. A file in a dot-named folder is still never edited. A link the build reads as ambiguous because one candidate sits under a dot-named folder is no longer rewritten as if it named the other. A block with spaces after a fence is frontmatter, so a link in it is left alone like any frontmatter link, instead of being rewritten as a body link.
- A rename or move skips a folder in the vault it cannot list and reports it as a `skipped` row (`unreadable`), so links in it are not rewritten, as before. A folder under a dot-named folder gets no row, since nothing there is edited. It stops, with an error naming the folder, only when that folder holds the note or its destination, or sits inside a bundle the move carries.

## [0.5.0](https://github.com/osdlc/skillwright/compare/skillwright/v0.4.0...skillwright/v0.5.0) - 2026-11-05

### Other

- watch test: save atomically so a rebuild never reads a half-written note
- Rename markdown-vault to markdown-corpus; bump osdlc#618 pin
- Honour the vault's excluded files
- plan moves over the shared vault model
- Vfs from vfs-seam; bump the osdlc crates to osdlc/osdlc#618
- Expose the marketplace model: loadMarketplace in wasm, list --model in the CLI
- Publish renders through the model and translators; cursor and pi take packs
- Marketplace model and harness translators; build renders through them
- Harness profile: one row of facts per harness
- Fix the review findings on the comment sweep
- make CLI comments and test notes match the code

### Added

- `list --model` prints the loaded marketplace model as JSON. `list --json` is unchanged.

### Changed

- `build` and `check` apply a note's `sw-overrides` block for each harness to that harness's tree, as `publish` does. A block naming an unknown target, or a key its harness does not read, now refuses the build.
- `publish` of a plugin note to `cursor` or `pi` writes that harness's pack instead of refusing.
- A note the vault excludes in Obsidian ("Excluded files", `userIgnoreFilters` in `.obsidian/app.json`) is no longer built or published by `build`, `check` or `publish --all`.

## [0.4.0](https://github.com/osdlc/skillwright/compare/skillwright/v0.3.9...skillwright/v0.4.0) - 2026-10-30

### Fixed

- machine-file path resolution, refusal and help wording; drop old project name from changelogs

### Removed

- handling of old names; fix config and doc errors

## [0.3.9](https://github.com/osdlc/skillwright/compare/skillwright/v0.3.8...skillwright/v0.3.9) - 2026-10-30

### Other

- skillwright now lives in https://github.com/osdlc/skillwright. Repository, homepage and install links point there, and releases are built from it.

## [0.3.8](https://github.com/osdlc/osdlc/compare/skillwright/v0.3.7...skillwright/v0.3.8) - 2026-10-29

### Other

- updated the following local packages: markdown-relink, tradecraft-core

## [0.3.7] - Unreleased

The first release under the name `skillwright`. It is the `tradecraft` 0.3.7
CLI renamed, built on `tradecraft-core` 0.40.0. The `tradecraft` entries below
are its history.

### Changed

- The crate and the binary are `skillwright`. Install with
  `cargo install skillwright`, `brew install osdlc/tap/skillwright` or
  `npm install --global @osdlc/skillwright`.
- The project file is `skillwright.yaml`.
- The frontmatter keys are `sw-*` and the nested block is `sw:`
  (default namespace `sw`).
- A destination's manifest is `.skillwright/manifest.json`, and a build
  stages in `.skillwright-stage`.
- Generated files carry `Generated by skillwright`. A tree built with
  `Generated by tradecraft` is still recognised, and the next build replaces
  it.
- The environment variables are `SKILLWRIGHT_*`, including
  `SKILLWRIGHT_MACHINE_CONFIG`. The machine file is
  `<config-home>/skillwright/config.yaml`, and the checkout cache is under
  `dev.osdlc.skillwright`.

### Rename errors

Each of these stops with one line that says what to rename:

- `tradecraft.yaml` (or the earlier project file) and no `skillwright.yaml`.
- A destination with `.tradecraft/manifest.json` (or the earlier folder) and
  no `.skillwright/manifest.json`.
- A note with a `tc-*` key or a `tc:` block, under the default namespace.
  The line names the `sw-*` spelling, or says to set `namespace: tc`.

## tradecraft [0.3.7](https://github.com/osdlc/osdlc/compare/tradecraft/v0.3.6...tradecraft/v0.3.7) - 2026-10-28

### Other

- read an empty config file as an empty mapping ([#478](https://github.com/osdlc/osdlc/pull/478))
- Release tradecraft binaries to GitHub, npm and Homebrew ([#532](https://github.com/osdlc/osdlc/pull/532))

## tradecraft [0.3.6](https://github.com/osdlc/osdlc/compare/tradecraft/v0.3.5...tradecraft/v0.3.6) - 2026-10-28

### Other

- updated the following local packages: yaml-splice, markdown-relink, tradecraft-core

## tradecraft [0.3.5] - Unreleased

First public release. A command-line tool that reads a vault of markdown skill notes and publishes
or builds them into the formats of several coding-agent tools (Claude Code, Codex, Cursor and Pi),
with drift checks and commands to author and reorganise the vault. Install it with
`cargo install tradecraft`.

### Names

- The crate and the binary are `tradecraft`. Help text, `--version` and error messages say
  `tradecraft`.
- The project file is `tradecraft.yaml`. The machine file is `<config-home>/tradecraft/config.yaml`,
  and `TRADECRAFT_MACHINE_CONFIG` relocates it.
- Frontmatter keys start with `tc-`, the default namespace `tc`.
- Each destination holds `.tradecraft/manifest.json`. A build stages in `.tradecraft-stage`, and
  generated files carry the `Generated by tradecraft` marker.
- The machine file and the `adopt --from-git` checkout cache are found with `tiered-dirs` 0.3,
  under the vendor folder `dev.osdlc` (the cache folder is `dev.osdlc.tradecraft`). The
  `TRADECRAFT_ROOT`, `TRADECRAFT_<HOME>_DIR`, `TRADECRAFT_SCRATCH_DIR`, `TRADECRAFT_CONVENTION` and
  `TRADECRAFT_REQUIRE_OVERRIDE` variables move the base directories.

### Errors for files from before the rename

- A run that searches for `tradecraft.yaml`, finds none, and finds the earlier project file fails with
  exit 2. The message says to rename the file to `tradecraft.yaml`, set `namespace: tc` or drop the
  line, and change the vault notes' earlier keys to `tc-`. A run with `--config` or `--vault` does not
  check.
- A destination that has the earlier folder's `manifest.json` and no `.tradecraft/manifest.json` is refused
  with one `bad-manifest` line that says to rename the folder to `.tradecraft/`.
- A marketplace tree built under the earlier project's generated-file marker is refused with one line that
  says to delete the tree and rebuild.

### Dependencies

- Configuration loading uses `config-layers` 0.7. A path in a config file that starts `~user/` is
  not expanded, and resolves as a relative path.
- Git runs through `typed-git` 0.4, renames through `markdown-relink` 0.1, and path checks through
  `path-guard` 0.3.

### Known limitations

- Windows is not supported yet. The tool builds there, but its tests are not run on Windows.
