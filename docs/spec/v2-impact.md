# Structure v2: impact (draft)

What [structure v2](structure-v2.md) changes from today's tool (v1): what it
removes, what each decision changes for existing vaults, which code shrinks,
what it means for the open PRs and issues, and an order to build it in. Code
was read at `c9b3321b`. Paths are under `packages/rust/`. "Section" numbers
refer to structure-v2.md.

## Commands removed

v2 keeps `init`, `build`, `check` and `bump`. Every other command leaves v2
and may return in a later milestone:

| Command | Goes with it |
|---|---|
| `publish`, `unpublish` | `build.destinations` |
| `harness install`, `update`, `uninstall`, `link`, `unlink` | |
| `adopt` | `--from`, `--from-git`, `build.scan_locations` |
| `new`, `rename`, `move`, `delete`, `convert` | |
| `build --watch` | |
| `list`, `status`, `author` | |

## Features removed or changed

### The root and the root file

- A vault with no root file, read with every default.
- The walk-up for the nearest `skillwright.md`, and its reserved-name rule
  (`reserved-name`). The walk-up stays, but it now stops only at a folder
  that holds both `skillwright.md` and `marketplaces/` (section 1 rule 2).
- `--config`. `--vault` names the root.
- The root file as a marketplace. Its marketplace fields move to
  `marketplaces/<m>/<m>.md`.
- `workspace: true`, and `marketplaces/<m>/skillwright.md` as a member file.
  There is one root file, and every marketplace sits under it.
- The `name` key on any note. The folder name is the name, and a `name` key is
  refused.
- `build.namespace` and prefixed keys (`sw-version`, an `sw:` block). A
  prefixed key is now an unknown key.

### The layout

- A root `skills/`, `agents/`, `commands/` or `hooks/` as a plugin named after
  the marketplace, and the root file's `version` as that plugin's version.
- A root `plugins/` as the plugins of a single marketplace.
- Marketplaces below the top, and components promoted to a marketplace.
- Flat entry notes (`skills/x.md`) and `SKILL.md` entries.
- Optional plugin notes (decision D6).
- `build.patterns` and every custom folder glob.
- `build.exclude` (decision D5). A file that must not ship goes in `docs/`.
- Loose files in a component folder. Each folder now holds only the entries
  its row in section 3's "What a folder may hold" table lists. Anything else
  is an error.
- New: six reserved names (`marketplaces`, `plugins`, `skills`, `agents`,
  `commands`, `hooks`) for any marketplace, plugin or component.
- New: a `.mcp.json` in a plugin folder is an error (decision D7), not a
  shipped asset.
- New: the body of `<m>.md` and `<p>.md` is not interpreted and does not ship.

### Output

- `build.out`, `--out` and the "no default output" rule. Output is fixed at
  `dist/<m>/`.
- `<harness>: {packs: <path>}`, with its overlap checks and the
  `stale-output` warning.
- `build.assets` and `build.hooks` (decision D4).
- The plugin `assets/` folder copied flat to the pack root (`assets/bin/tool`
  shipping as `bin/tool`). A plugin's `assets/` now ships as `assets/`. The
  launcher moves to `scripts/`.
- Bare skill names as Cursor's default. Cursor now writes a plugin's skill as
  `<p>-<s>`, folder and `name`, as Pi already does. `cursor: {naming: bare}`
  keeps today's plain names (section 8 rule 6). A call to a Cursor skill
  renders `/<p>-<s>`, not `/<s>`.
- New: `check` with no `dist/<m>/` reports drift (exit 1).
- New: `build` with no `--marketplace` builds every marketplace. The working
  directory no longer picks one.

### Links, calls and embeds

| v1 | v2 |
|---|---|
| A plain `[[skill]]` rendered the skill's name (a mention). | An ordinary file link that warns `Did you mean to call /[[skill]]?` (D3). |
| A link resolved vault-wide: a basename match across the vault, the `own_folder_candidate` tiebreak, and `link-ambiguous` across folders. | It resolves within the plugin in two steps (section 4 rule 3). |
| A linked file from anywhere was copied to `references/vault/` or `assets/vault/`. | Every reachable file already ships, so nothing is copied for a link. |
| An embed or link to nothing was reported, and its words shipped. | An error (section 4 rule 11). |
| An embed of a file that is neither a note nor an image became a plain link. | A text file is inlined as a fenced code block (D8). |
| Calls inside code spans and fenced blocks were interpreted. | Code is literal (section 4 rule 12). |
| A fragment on a call or a mention was checked and then dropped. | On a call, `/[[setup#Usage]]`, an error. On a plain link it is kept. |
| A note with malformed frontmatter embedded its body. | An error that names both notes (section 5 rule 4). |
| A hand-written call string was refused in prose and code. | Refused in prose only. In code it ships as written. |
| Two skills sharing a called name: `skill-call-ambiguous`, with the path of the one meant as the fix. | An error that names both plugins. There is no path form of a call; rename one. |

### Plugin-root paths

- `build.launcher_path` (decision D9). Authors now write
  `${PLUGIN_ROOT}/<path>`. Every such path in a component's Markdown is
  checked and replaced, not one configured path. A path that names nothing
  shipped, left as written in v1, is now an error.
- `${CLAUDE_PLUGIN_ROOT}` as the source form, in notes and in hooks files. It
  is now an error that says to write `${PLUGIN_ROOT}` (decision D9).

### Decisions amended

| Decision | Amendment |
|---|---|
| [D-WNTQ](../decisions/D-WNTQ-harness-profile-values-from-config.md) | `pi: {naming}` stays settable, `cursor: {naming}` is added, and `packs` is not settable. |
| [D-KVMB](../decisions/D-KVMB-bare-keys-by-default.md) | `build.namespace` and prefixed keys are gone. An unknown key still warns and is never refused. A key the table lists is refused where it does not belong: a marketplace-only key in the root file, `name` on any note, `build:` outside the root file, and the removed `build:` keys. |
| [D-GPYR](../decisions/D-GPYR-convention-first.md) | Its patterns layer is gone. |

## What each decision changes

Each decision's rule is in section 12. This part says what v1 does and what
an existing vault must change.

### D1. `build.out` and `--out`

v1 requires it. The repo's own `plugin/` builds beside its root file:

```yaml
build:
  out: .        # writes plugin/.claude-plugin/, plugin/.agents/, ...
```

Existing vaults:

1. Remove `out:`.
2. The repo's `plugin/` built trees move from `plugin/.claude-plugin/` and its
   siblings to `plugin/dist/osdlc/`.
3. The `plugin` workflow diffs that folder, and `plugin-release` copies from
   it.

### D2. The scope of a call

v1 looks a called name up across the whole vault. The repo's `author` skill
calls `/[[setup]]` in its own plugin, `skillwright`:

| Harness | Renders as |
|---|---|
| Claude Code | `/skillwright:setup` |
| Codex | `$skillwright:setup` |
| Pi | `/skill:skillwright-setup` |

If two skills share the name, v1 stops with `skill-call-ambiguous` and asks for
the path of the one meant.

Existing vaults: none, unless a call names a skill in another marketplace of
the same vault. That call is now an error.

### D3. Calls and plain links to a skill

v1 has two forms:

| Source | v1 renders | Runs the skill? |
|---|---|---|
| `/[[setup]]` | `/skillwright:setup` on Claude Code | Yes: a call |
| `[[setup]]` | `skillwright:setup`, the skill's name | No: a mention |

A skill's link to its own note ships as a file link, `[setup](SKILL.md)`
([#53](https://github.com/osdlc/skillwright/issues/53) asked for the name).
v1 also interprets calls inside code spans and fenced blocks. v1 vaults
already write calls as `/[[name]]`, which is one reason v2 keeps the `/`.

Existing vaults:

1. A v1 mention, `[[name]]`, now ships as a file link and warns. Write
   `/[[name]]` if a call was meant, or keep the link.
2. A call inside a code span or fenced block now ships as written. Move it
   into prose.
3. The repo's `plugin/` has 10 calls, all `/[[...]]`:

   | Where | Count | Change |
   |---|---|---|
   | Heading self-calls, such as `# /[[author]]` | 3 | None: they are in prose. |
   | Code spans in skill notes, such as `` `/[[setup]]` `` | 6 | Move into prose, keeping the `/`. A call renders in backticks, so the output is the same. |
   | The plugin note's body | 1 | None: a container note's body is not interpreted and does not ship. |

4. The `author` skill's "Links" section
   (`plugins/skillwright/skills/author/author.md`, lines 57 to 75) is
   rewritten to describe v2. That includes its last bullet, v1's vault-wide
   `link-ambiguous` rule. Its syntax examples stay in code and ship as
   written.

### D4. `build.assets` and `build.hooks`

v1:

```yaml
build:
  assets:
    - {from: shared/LICENSE, to: LICENSE, plugin: tools}
  hooks:
    - {from: plugin/hooks.json, plugin: tools}
```

`LICENSE` lands at the root of the `tools` pack. `hooks.json` is translated
into each harness's hooks file.

Existing vaults:

1. Move each file into its plugin.
2. A built file is written there by the author's build step, with its source
   in `src/` or outside the vault.
3. The repo's `plugin/` uses neither list. The `sdlc` plugin does; see
   "Consumer migration" below.

### D5. `build.exclude`

v1:

```yaml
build:
  exclude: ["**/*.draft.md"]
```

`skills/review/notes.draft.md` stays in the vault and is not shipped.

Existing vaults: move excluded files into `docs/`. The repo's `plugin/` uses
no `exclude`.

### D6. Plugin notes are required

v1 builds a plugin folder without a note. Its name is the folder's name, and
its version is `0.1.0` with a warning.

Existing vaults: add a note to any plugin without one. The repo's `plugin/`
already has `plugins/skillwright/skillwright.md`.

### D7. `.mcp.json`

v1 has no MCP kind. An author ships the file through the flat assets copy:
`plugins/tools/assets/.mcp.json` lands at `<pack>/.mcp.json` for every
harness.

Existing vaults: remove the `.mcp.json`, or keep the plugin on v1 until the
MCP kind lands. The repo's `plugin/` has none.

### D8. Embedding a file that is neither a note nor an image

v1 ships `![[run.sh]]` as a plain link, `[run.sh](scripts/run.sh)`, and copies
the file. The `!` changes nothing.

Existing vaults:

1. An `![[file]]` of a text file that shipped as a link now ships the file's
   text. Write `[[file]]` to keep the link.
2. An embed of a binary file is now refused.
3. The repo's `plugin/` has none.

### D9. Plugin-root paths replace `build.launcher_path`

v1, in steps:

1. `build.launcher_path` names one launcher, such as `bin/tool`, shipped
   through the flat assets copy.
2. A skill calls it as `${CLAUDE_PLUGIN_ROOT}/bin/tool`. Claude Code keeps the
   call.
3. For Codex, Cursor and Pi, the build rewrites that one path to
   `../../bin/tool`, relative to the skill's folder.
4. Any other `${CLAUDE_PLUGIN_ROOT}/...` path is left as written. That is dead
   text on those three harnesses.
5. If the plugin does not ship the launcher, the call is also left as written,
   with no warning.
6. On Pi, a separate step renames `${CLAUDE_PLUGIN_ROOT}/skills/<s>` to
   `skills/<p>-<s>`, but keeps the variable, which Pi does not expand.
7. A Markdown file in a skill's `references/` is rewritten relative to the
   skill's folder, not its own, so its `../..` is one level short.
8. Hooks write `${CLAUDE_PLUGIN_ROOT}`, swapped for each harness's own
   variable. v2 keeps the swap, with `${PLUGIN_ROOT}` as the source.

Existing vaults:

1. Remove `launcher_path`.
2. Move the launcher from `assets/bin/tool` to `scripts/tool`, and call it as
   `${PLUGIN_ROOT}/scripts/tool`.
3. Replace every `${CLAUDE_PLUGIN_ROOT}` in notes and hooks files with
   `${PLUGIN_ROOT}`.
4. A plugin-root path that names nothing shipped now fails the build.
5. The repo's `plugin/` uses neither the key nor the variable.

## Code that shrinks or goes

### tradecraft-core

- `paths.rs`:
  - Go: `find_root_file`, `is_component_note`, `is_reserved_note`,
    `is_vault_root_file`, `is_marketplace_entry`, `is_flat_root_file_name`,
    `workspace_above`, `RootRead`, `read_workspace_above`, `workspace_file`,
    `read_root`, `read_root_both`.
  - Change: the walk-up now stops at a folder holding both `skillwright.md`
    and `marketplaces/`.
  - `marketplace_inputs` and `load_marketplace` become one loader, in steps:
    read the root file, list `marketplaces/*/`, read each `<m>.md`.
- `patterns.rs`: almost all of it goes.
  - Go: `Patterns::canonical`, `plugin_folder`, `assets_folder`,
    `sibling_files`, `source_folders`, `warnings`, `warnings_with`,
    `keep_conventional`, `workspace_exclude_warnings`, and with D5 the exclude
    list and the glob matcher (`glob_match`, `glob_problem`).
  - Stays: the never-ship rule, which can move into `discover.rs`. It now
    applies only directly in a plugin or component folder.
- `settings.rs`:
  - Go: `FolderPatterns`, `folder_patterns`, `globs`, `warn_nested`, the
    `patterns`, `out`, `namespace` and `launcher_path` keys (now refused),
    `AssetSetting` and `HookSetting` (D4), `DestinationSetting` and the
    `destinations` and `scan_locations` keys.
  - Change: `RootFile::inherit` becomes the root-defaults step of the cascade.
    `MarketplaceFields` is read from `<m>.md`, not the root file.
    `compose_root_file` writes a root with no marketplace fields.
    `root_file_config` and `RootFile::to_config` shrink.
  - Change: an unknown root-file key warns. A marketplace-only key is refused.
- `discover.rs`:
  - `classify` becomes a fixed-depth match.
  - Go: `conventional`, `folder_placement_with`, `RESERVED_NAME_REASON`,
    `SKILL_ENTRY`, `is_skill_entry`, the `SKILL.md` branch of `is_entry`, and
    `placement`'s `container_level` (promoted components).
  - `container_notes`, `container_notes_with` and `read_container` read fixed
    paths.
  - `under_a_kind_folder` and `refuse_stray` turn into the layout check of
    section 11.
  - New: the six reserved names, the per-folder entry table, and the
    `.mcp.json` error.
- `model/cascade.rs`:
  - Go: `only_marketplace`, `sole_marketplace`, `marketplaces_in`, and the
    "a note under no marketplace takes the one marketplace" rule.
  - `marketplace_note` reads `<m>.md`. `belongs_to` becomes a path prefix
    test.
  - New: a marketplace with no `version` writes no version and no warning.
- `model/load.rs`:
  - Go or shrink: `standalone` (publish of one note), `members_folder`,
    `has_folder`, the marketplace's-own-plugin branch of `missing_versions`,
    `load_assets`, `load_hook_files` (D4), `folder_assets` (the flat copy;
    replaced by copying the three shared folders under their own names),
    `asset_covers` (its last use was the launcher check), `plugin_folder`, and
    the `plugin.launcher` set from `build.launcher_path`.
  - `marketplace_name` and `resolve_marketplace` read the folder.
  - Container note bodies are no longer read.
- `source_view.rs`: `output_overlaps_vault`, `output_in`, `output_paths`,
  `check_pack_location`, and the workspace folding in `effective_config`,
  `effective_configs`, `marketplace_folder` and `marketplace_notes` go. Output
  cannot overlap source when only `marketplaces/` is read.
- `compile.rs`:
  - Resolution: `CompileSource::resolve` changes from vault-wide to the two
    steps of section 4 rule 3. A file with no component starts at step 2. A
    shared file lands at the pack root, and its link is written relative to
    where the linking file lands. `own_folder_candidate` and the
    `references/vault/` and `assets/vault/` placements go.
    `vault_link_index` indexes one plugin, not the vault.
  - Containment: a real-path check is added beside `refuse_never_shipped`.
  - Calls (D3): `TokenKind::Call`, `TokenKind::Escape` (`\/[[`), `sigil_at`,
    `replace_call`, `render_call` and `format_call` stay. `sigil_at` drops the
    backtick from its positions. `check_call_fragment` turns from "checked,
    then dropped" into an error. The call index (`skill_index` in
    `model/load.rs`) adds commands for harnesses whose packs contain them.
  - Plain links: `skill_mention` and the `mention` path of `render_or_mark`
    go. A plain link to a skill is a file link, plus the new "Did you mean to
    call" warning.
  - Code is literal: `scan_code_calls`, `code_token`, `TokenKind::Unconverted`
    with its `unconverted-call` warning, and the in-code branch of
    `format_call` go.
  - Leftovers: `validate_no_obsidian_syntax` skips code spans, fenced blocks,
    `\[[`-escaped text and text a `\/[[` escape produced.
  - Hand-written calls: `literal_skill_calls` and `literal-skill-call` stay,
    but skip code spans and fenced blocks. The error says to write
    `/[[name]]`.
  - Missing targets: in `replace_token`, an embed or link to nothing becomes
    an error instead of an entry in `unresolved`.
  - Embeds: a non-note, non-image embed is inlined as a fenced block (D8).
    New code: the extension table, the fence length (counting fences indented
    up to three spaces), list indentation and the UTF-8 check. Malformed
    frontmatter in an embedded note becomes an error. Embed expansion,
    `select_fragment` and the cycle check stay.
- `harness/profile.rs`:
  - `PluginFormat::packs` stays a fixed value. `owned_paths_at` and the config
    override path go.
  - `CliInvocation` stays as the per-harness switch, now for every
    plugin-root path (D9).
  - Cursor's naming changes from `SkillNaming::Bare` (taken from harness
    knowledge, `skills/plugin-namespace`) to
    `Source::Constant(SkillNaming::PluginPrefixed)`, as Pi's row is.
    `held("cursor")`'s expected value changes with it.
- `harness/manifest.rs` and `model/load.rs`:
  - `bare_on_pi` becomes one check for `cursor` and `pi` blocks, filling
    `bare_on` with each harness that sets `naming: bare`. A `naming` key on a
    component is refused.
  - `refuse_prefixed_collisions` and the 64-character check then cover
    Cursor too.
  - Inheritance needs no change: `plugin_block` already starts from the
    marketplace's block, which holds the root file's defaults, and overlays
    the plugin's.
  - New code: a collision check for `bare`. Today two bare skills of one name
    across plugins on Pi are not refused. v2 refuses them per harness, naming
    both plugins.
- `model/mod.rs` and `harness/translator.rs` (D9):
  - `Segment::LauncherCall` and `push_text`'s one-path split become a
    plugin-root path segment that holds any path after `${PLUGIN_ROOT}/`,
    checked against the files the pack ships. A trailing `.` is not part of
    the path.
  - A source `${CLAUDE_PLUGIN_ROOT}` and a lone `${PLUGIN_ROOT}` are refused
    there.
  - `write_body` writes `${CLAUDE_PLUGIN_ROOT}/<path>` for Claude Code. For
    the other harnesses it writes the path relative to the folder the text
    lands in, so a resource under `references/` gets its own depth, not the
    skill's.
  - `rewrite_sibling_paths` goes: Pi's `skills/<p>-<s>/` folder is where the
    rewrite lands. `rewrite_launcher_calls` goes with its test.
    `relative_path` stays.
  - `hooks_file::localize`, the variable swap for hooks, stays with
    `${PLUGIN_ROOT}` as its source (`CANONICAL_ROOT` changes) and gains the
    path check.
- `export.rs`: the `stale-output` warning and moved-packs handling go.
  `MarketplaceConfig::launcher_path` and the launcher tests go. Their cases
  move to commit 5's plugin-root path tests.
- `vault.rs`: `bundle_root` keeps only the `<name>/<name>.md` shape.

### The removed commands

- `tradecraft-core/src/publish.rs`:
  - Go: everything that writes to a destination: `publish_skill`'s
    single-note arms, the destination manifest, the rails, `publishes_to`,
    `overrides_for`, `override_states` and `accepted_override_keys`.
  - Stay, because `export.rs` and `model/cascade.rs` call them: the plugin
    arm, `plugin_format_carries_commands`, `plugin_format_carries_hooks`,
    `kind_has_renderer`, `canonical_target`, `KNOWN_TARGETS` and
    `accepted_keys`. Moving those into `export.rs` or `harness/` is a
    follow-up, not v2.
- `tradecraft-core/src/status.rs`: `status_of` and `units_depending_on` go.
  `version_bump_if_unmoved`, which `export.rs` uses for `bump`, stays.
- `tradecraft-core/src/authoring.rs`: all of it.
- `tradecraft-core/src/adopt.rs`: `adopt_skills`, `AdoptRequest`,
  `AdoptReport`, `AdoptedSkill`, `detect_format` and `rewrite_to_wikilinks`.
- `tradecraft-core/src/scan.rs`: `scan_roots`, `scan_root` and
  `ScanOptions`. Only `adopt` and one `publish.rs` test call them. The parts
  of `corpus.rs` that only the scan uses go with it.
- `skillwright/src/commands/`: `publish.rs`, `status.rs`, `list.rs`,
  `authoring.rs`, `adopt.rs`, `watch.rs`, `author/` and `harness/`.
- `skillwright/src/apply.rs` (the authoring writer), `git_source.rs` and
  `dirs.rs` (the `--from-git` cache), and the `author.editor` machine setting
  in `config.rs`.
- `skillwright/src/cli.rs`: every command but `init`, `build`, `check` and
  `bump`, with its flags, and `--watch`.
- `tradecraft-wasm/src/lib.rs`: `publishSkill`, `adoptSkills`, `statusOf`
  and `publishableKinds` go. `exportSkills`, `discoverComponents`,
  `loadMarketplace`, `parseRootFile`, `rootFileConfig`,
  `validateNamespace`, `knownTargets` and `canonicalTarget` stay.
- Their tests: `skillwright/tests/authoring.rs`, `author.rs`, `harness.rs`,
  and the publish, status, list and adopt cases in `cli.rs` and
  `consumer_flows.rs`.

### skillwright (CLI)

- `resolve.rs`:
  - `resolve_run` keeps a walk-up, but one that stops at a folder holding both
    `skillwright.md` and `marketplaces/`. It loses `--config` and the
    workspace fold.
  - `workspace_runs`, `refuse_workspace`, `workspace_warnings`,
    `member_workspace_warnings`, `is_workspace`, `configured_out` and
    `resolve_skills_root` go or shrink to "every marketplace, or the one
    `--marketplace` names". The working directory no longer picks one.
- `config.rs`: `workspace_above` and `workspace_members` go.
- `commands/init.rs`: takes `<m> <p>` and writes the v2 skeleton.
- `commands/marketplace.rs` (`build` and `check`): loses `--out` and the
  workspace runs. `check` treats a missing `dist/<m>/` as drift.
- `commands/bump.rs`: the marketplace's-own-plugin version rule goes.
  `bump <p>` names a plugin, and `bump --marketplace <m>` with no name bumps
  the marketplace. `BuiltRef::build` checks out `skillwright.md` and
  `marketplaces/` only.

### tradecraft-wasm

- `lib.rs`: the `out_root` argument and the pattern and workspace options of
  `discovery_view` and `load_view` go.

### Tests and docs

- Tests for every removed feature are deleted, not ported: the `patterns.rs`
  and `settings.rs` workspace tests, the reserved-name tests in `paths.rs`,
  and the workspace cases in `tests/cli.rs`, `tests/consumer_flows.rs` and
  `tests/release_api.rs`.
- `docs/reference/skillwright-md.md` is rewritten. Its Patterns, Several
  marketplaces and Publishing one note by path sections go.
- The CLI README's verb and option tables shrink to the four commands. Its
  Configuration section is rewritten.
- `docs/reference/fields.md` is regenerated after the key table changes.

### The repo's own `plugin/`

1. `plugin/skillwright.md` keeps only `author`, `license` and `build:`.
2. Its marketplace fields move to `plugin/marketplaces/osdlc/osdlc.md`.
3. `plugin/plugins/skillwright/` moves to
   `plugin/marketplaces/osdlc/plugins/skillwright/`, with each skill as
   `skills/<s>/<s>.md`.
4. The built trees move from `plugin/.claude-plugin/`, `plugin/.agents/`,
   `plugin/.cursor-plugin/` and `plugin/.pi/` to `plugin/dist/osdlc/`.
5. The `plugin` workflow keeps its drift check: it builds and fails on any
   difference under `plugin/dist/osdlc/`, and runs `check`. It drops
   `--config`.
6. The `plugin-release` workflow copies the pack and manifest entry from
   `plugin/dist/osdlc/` into `osdlc/marketplace`. It already copies; only the
   source path changes.
7. Install from `osdlc/marketplace` is unchanged. `plugin/` itself is no
   longer installable from its own root. Nothing documents that route today.

Its skills teach v1 commands and need rewriting for v2:

| Skill | Teaches today | v2 |
|---|---|---|
| `publish` | `publish`, `status --check`, `harness install`, `update`, `uninstall`, `link`, `unlink` | Rewrite around `build`, `check` and `bump`, or drop the skill and fold `build` and `check` into `author`. |
| `setup` | `init --out`, "the nearest `skillwright.md`", `build.destinations`, `skillwright list` | `init <m> <p>`, the two-file root test, no destinations, `check` to verify. |
| `author` | `skillwright list` in its steps, `build.namespace`, v1 links and mentions | `check` in its steps, no namespace, the v2 link, call and embed rules (D3). |

## Consumer migration: the sdlc plugin

The `sdlc` plugin in the owner's dev repo builds with
`solutions/ontological/agent-pants.yaml`. It ships about 25 `assets:` entries
from outside the vault to arbitrary pack paths, such as `cli/sdlc`,
`cli/sdlc.js`, `dashboard-spa/dist`, `conventions/`, `lib/model/entities` and
`lib/services/docs/*.eta`. v2 drops `build.assets` (D4), so it moves to the
`src/` and external-build pattern (section 3 rule 16):

1. Its own build step, such as a moon task, writes each file into the plugin's
   `scripts/`, `references/` or `assets/` before `skillwright build`.
2. Its pack paths move under those three folders:

   | Today | v2, for example |
   |---|---|
   | `cli/sdlc.js` | `scripts/sdlc.js` |
   | `conventions/` | `references/conventions/` |

3. Its runtime path lookups, and its skills' `${PLUGIN_ROOT}/...` paths,
   change to the new pack paths.

This is work in the dev repo, not in skillwright. It also depends on open
question 1 in the spec: how a skill's shell command reaches a plugin-shared
script.

## Open PRs

| PR | Verdict | Reason |
|---|---|---|
| [#136](https://github.com/osdlc/skillwright/pull/136) self-link as a skill link | Merge as is | Its leftover-`[[` checks stay in v2. Its `\[[` escape keeps the backslash, which the spec's open question 2 recommends removing. Its self-link change does not stay: it renders a skill's link to its own note as the skill's name through `skill_mention`, and under D3 that link is a file link to `SKILL.md`. Commit 5 removes `skill_mention` with it. [#138](https://github.com/osdlc/skillwright/pull/138) stacks on it. |
| [#138](https://github.com/osdlc/skillwright/pull/138) key table and harness profile own their facts | Merge, v2 builds on it | v2's root-file key check and its smaller key set come from the key table this PR builds. |
| [#143](https://github.com/osdlc/skillwright/pull/143) one identity struct | Merge, v2 builds on it | The struct and key-table inheritance hold v2's root, marketplace, plugin, component cascade. Merge after [#138](https://github.com/osdlc/skillwright/pull/138). |
| [#139](https://github.com/osdlc/skillwright/pull/139) bump checks out only what the build reads | Merge as is | `bump --changed-since` stays in v2, and this is a self-contained speed-up of it. v2 later shrinks its path set to `skillwright.md` and `marketplaces/`. |
| [#135](https://github.com/osdlc/skillwright/pull/135) rename reports what it could not read | Closed (moot for v2) | Most of it is `rename` work, and `rename` leaves v2. Its dependency bumps (markdown-vault-fs 0.6, markdown-corpus 0.3, markdown-relink 0.4) are taken in v2's first PR if `build` needs them. The rename milestone can reuse its approach. |
| [#134](https://github.com/osdlc/skillwright/pull/134) one loader and one path rule | Closed (moot) | It unifies the workspace loader and the `skillwright.md` path rule, which v2 deletes. v2 writes the one loader for the fixed layout. |
| [#137](https://github.com/osdlc/skillwright/pull/137) refuse a publish by path that would overwrite | Closed (moot) | `publish` leaves v2, and most of its diff is [#134](https://github.com/osdlc/skillwright/pull/134). |
| [#140](https://github.com/osdlc/skillwright/pull/140) core hides build output | Closed (moot) | v2 reads only `marketplaces/` and writes only `dist/`, so output can never be read as source. It is also stacked on [#134](https://github.com/osdlc/skillwright/pull/134). |

## Open issues

"Deferred" means the issue belongs to a later milestone for a command v2
leaves out. It stays open.

| Issue | Verdict | Reason |
|---|---|---|
| [#144](https://github.com/osdlc/skillwright/issues/144) published links: labels, shared-note paths, missing files | Changed, in v2 | The flattening part is moot: shared notes outside a plugin are gone. Missing-file links become build errors (section 4). The `.md` label part stays. |
| [#141](https://github.com/osdlc/skillwright/issues/141) bump: scan once, in-memory baseline | Keep | `bump` stays. The reads it lists get simpler. |
| [#82](https://github.com/osdlc/skillwright/issues/82) bump: sparse checkout, one scan | Keep | `bump` stays. [#139](https://github.com/osdlc/skillwright/pull/139) does part of it. |
| [#123](https://github.com/osdlc/skillwright/issues/123) Cursor entries have no displayName | Keep | Manifest output. [#138](https://github.com/osdlc/skillwright/pull/138) addresses it. |
| [#108](https://github.com/osdlc/skillwright/issues/108) Cursor plugin-prefixed names | Keep | Naming. [#138](https://github.com/osdlc/skillwright/pull/138) addresses it. |
| [#79](https://github.com/osdlc/skillwright/issues/79) per-harness facts in HarnessProfile | Keep | [#138](https://github.com/osdlc/skillwright/pull/138) does it. |
| [#80](https://github.com/osdlc/skillwright/issues/80) key types and root-file checks from the key table | Changed | The root file's key set shrinks. [#138](https://github.com/osdlc/skillwright/pull/138) does the rest. |
| [#75](https://github.com/osdlc/skillwright/issues/75) one identity struct | Changed | The workspace step goes. [#143](https://github.com/osdlc/skillwright/pull/143) does the rest. |
| [#77](https://github.com/osdlc/skillwright/issues/77) `resolve_run` should call `load_marketplace` | Changed | Still one loader, written fresh in v2. Close when it lands. |
| [#76](https://github.com/osdlc/skillwright/issues/76) one root-file and workspace loader | Changed | No workspace fold. Parse-once still applies. |
| [#83](https://github.com/osdlc/skillwright/issues/83) path-guard and globset | Changed | The output-overlap check goes. Link containment by real path is new work for path-guard. With D5 the glob half goes. |
| [#53](https://github.com/osdlc/skillwright/issues/53) a skill's link to itself | Settled by D3 | A plain self-link is a file link to the skill's own `SKILL.md` by rule, with no warning. To run the skill, write `/[[name]]`. Close when v2 lands. |
| [#50](https://github.com/osdlc/skillwright/issues/50) compile scans bodies twice | Keep | Compile stays. |
| [#47](https://github.com/osdlc/skillwright/issues/47) leftover `[[` check gaps | Keep | Compile stays. |
| [#46](https://github.com/osdlc/skillwright/issues/46) valid Markdown refused as leftover `[[` | Changed | v2's leftover check skips code spans, fenced blocks and escaped text. Indented code, math, `<pre>` and autolinks are still open. |
| [#78](https://github.com/osdlc/skillwright/issues/78) one rule for what a `skillwright.md` path is | Closed (moot) | There is one `skillwright.md`, at the root. Position decides every other note. |
| [#81](https://github.com/osdlc/skillwright/issues/81) core should hide build output | Closed (moot) | Output is fixed at `dist/` and only `marketplaces/` is read. |
| [#91](https://github.com/osdlc/skillwright/issues/91) symlinked `--vault` loses its workspace | Closed (moot) | Workspaces and member files are removed. |
| [#118](https://github.com/osdlc/skillwright/issues/118) publish by path can overwrite a same-named skill | Closed (moot) | Publishing a note by path is gone for good, not deferred. |
| [#142](https://github.com/osdlc/skillwright/issues/142) publish collision checks | Deferred: publish | All three asks are about `publish`. Inside a pack, v2's own rule refuses two sources on one path (section 9). |
| [#87](https://github.com/osdlc/skillwright/issues/87) rename of a bundle that half-lands | Deferred: rename | `rename` leaves v2. |
| [#49](https://github.com/osdlc/skillwright/issues/49) rename reads the vault three times | Deferred: rename | `rename` leaves v2. |
| [#45](https://github.com/osdlc/skillwright/issues/45) rename misses an unreadable note | Deferred: rename | `rename` leaves v2. |
| [#43](https://github.com/osdlc/skillwright/issues/43) rename can make a link ambiguous | Deferred: rename | `rename` leaves v2. When it returns, links resolve within one plugin, so the risk is smaller. |
| [#51](https://github.com/osdlc/skillwright/issues/51) adopt in reverse | Deferred: adopt | Reading harness files into notes is `adopt`'s job. Nothing in v2 needs it. |
| [#94](https://github.com/osdlc/skillwright/issues/94) install natively first | Deferred: install and adopt | Both halves need commands v2 leaves out: `harness install` for the native path, `adopt` for the fallback. |
| [#42](https://github.com/osdlc/skillwright/issues/42) markdown-db and markdown-contract | Keep | Not about skillwright. Already labelled to-close. |
| [#4](https://github.com/osdlc/skillwright/issues/4) gaps migrating a marketplace | Keep | Harness output gaps. Already labelled to-close. |

## Build order

v2 is one PR, built as six ordered commits. Each commit is written test-first:

1. Write its tests.
2. See them fail for the right reason.
3. Make the change.
4. Run the gates: `cargo fmt --all --check`,
   `cargo clippy --workspace --all-targets -- -D warnings` and
   `cargo test --workspace`.

Each commit passes the gates alone, so the branch can be bisected and reviewed
commit by commit.

Before the branch starts, merge
[#136](https://github.com/osdlc/skillwright/pull/136),
[#138](https://github.com/osdlc/skillwright/pull/138),
[#143](https://github.com/osdlc/skillwright/pull/143) and
[#139](https://github.com/osdlc/skillwright/pull/139).
[#134](https://github.com/osdlc/skillwright/pull/134),
[#135](https://github.com/osdlc/skillwright/pull/135),
[#137](https://github.com/osdlc/skillwright/pull/137) and
[#140](https://github.com/osdlc/skillwright/pull/140) are closed. The spec's
three open questions should be answered before commit 5.

### 1. Cut to four commands

Delete every command but `init`, `build`, `check` and `bump`, with their core
code and tests ("The removed commands" above). Do it first, so no later commit
ports a command v2 drops.

Tests first:

- Each removed command is an unknown command with exit code 2.
- The existing `build`, `check` and `bump` tests still pass unchanged.

### 2. Layout reader

Root discovery, the fixed-position `classify`, the one loader, the folder
rules of section 3, and the refusals of section 11. Delete the
nearest-`skillwright.md` walk-up, reserved notes, workspaces, patterns,
`SKILL.md` and flat entries.

Tests first:

- A v2 fixture vault lists every component with its marketplace and plugin.
- One test per case in section 11: the error names the path and prints the
  expected layout.
- A loose file in a skill folder is refused, naming the allowed entries. A
  `README.md` in `marketplaces/` is refused.
- A `skillwright` skill's note is not taken for a root. A component named
  `skills` is refused.
- A `.mcp.json` in a plugin is an error. `assets/.eslintrc.json` is skipped.
- `references/docs/x.md` ships; `skills/review/docs/x.md` does not.
- A name with digits, `v2-tools`, is accepted; `a--b` is refused.

### 3. Root file and cascade

The root file's key set, `<m>.md` as the marketplace note, the
root-marketplace-plugin-component cascade, required plugin notes, and
name-equals-folder.

Tests first:

- One table test per inherited key across the four levels.
- A plugin's `claude: {author: Bob}` beats its plain `author: Ada` for Claude
  Code only (section 7 rule 5).
- A marketplace-only key in the root file is refused, naming `<m>.md`. An
  unknown root key warns. `build:` outside the root is refused.
- A plugin with no `version` builds as `0.1.0` with a warning. A marketplace
  with no `version` writes none, with no warning.

### 4. Bundling and output

Fixed `dist/<m>/`, default packs folders, `check` against `dist/`, and the
pack contents of section 9. Delete `build.out`, `--out`, the `packs`
override, the overlap checks, `stale-output` and the flat `assets/` copy.

Tests first, one test per row of the "What a pack holds" table:

- A plugin's `assets/logo.png` lands at `<pack>/assets/logo.png`.
- A skill's `scripts/run.sh` keeps its exec bit.
- A Markdown file in `references/` is compiled. `CHANGELOG.md` is copied
  byte for byte.
- An agent ships nowhere.
- Two sources on one path are refused, naming both.

Output tests:

- A two-marketplace build writes two trees under `dist/`. `--marketplace`
  builds one.
- `check` reports drift after a hand edit there, and when `dist/` is missing.
- A `packs` key is refused.
- A skill with `targets: [claude]` is only in the Claude Code pack.

Naming tests, for the `tools` plugin's `review` skill:

| Setting | Cursor and Pi | Claude Code and Codex |
|---|---|---|
| none | `skills/tools-review/SKILL.md`, `name: tools-review` | `skills/review/` |
| `cursor: {naming: bare}`, `pi: {naming: bare}` | `skills/review/` | `skills/review/` |
| marketplace `pi: {naming: bare}` | `skills/review/` on Pi, for every plugin | unchanged |
| that, plus the plugin's `pi: {naming: prefixed}` | `skills/tools-review/` on Pi | unchanged |
| another `naming` value | refused | refused |
| `naming` on the skill | refused | refused |

Also: a prefixed name over 64 characters is refused. With `bare`, a `review`
skill in two plugins fails the build for that harness, naming both plugins.
Plugin `a-b`'s skill `c` and plugin `a`'s skill `b-c` fail for Cursor and Pi.

### 5. Links and transclusion

Scoped resolution, the link table, the real-path check, missing targets as
errors, calls and plain links (D2, D3), literal code, the leftover check, and
the embed rules of section 5.

Before this commit, run the `plugin-layout` probe on Codex. harness-reference
records neither where Codex resolves a skill's relative paths
(`Codex CLI Relative paths in SKILL.md open`) nor whether Cursor sets
`${CURSOR_PLUGIN_ROOT}` for hooks. If Codex does not resolve from the skill's
folder, links up to the pack root need another form there.

Tests first, through `build`, one test per rule in sections 4 and 5. All use
plugin `tools`.

Scope:

- A symlink in a skill folder that points out of the marketplace is refused.
- The plugin's `references/style.md` renders as `../../references/style.md`
  from a skill.
- `[[guide]]` from `author` prefers its own `references/guide.md` over the
  shared one.
- A link from the plugin's `references/style.md` starts at step 2.
- A link to a file in another plugin is refused.

Plain links:

- `[[setup]]` from `author` renders as `[setup](../setup/SKILL.md)` on Claude
  Code and `[setup](../tools-setup/SKILL.md)` on Cursor and Pi, and warns
  `Did you mean to call /[[setup]]?`.
- `[[setup#Usage]]` keeps its fragment.
- A skill's link to its own note is `[author](SKILL.md)`, with no warning.
- A link to a command is a file link on Claude Code and fails the build for
  Codex. A link to an agent fails the build.

Calls:

| Source | Claude Code | Codex | Cursor | Pi |
|---|---|---|---|---|
| `/[[setup]]` | `` `/tools:setup` `` | `` `$tools:setup` `` | `` `/tools-setup` `` | `` `/skill:tools-setup` `` |
| `/[[setup]]`, `naming: bare` | same | same | `` `/setup` `` | `` `/skill:setup` `` |
| `/[[setup\|words]]` | ``words (`/tools:setup`)`` | ``words (`$tools:setup`)`` | ``words (`/tools-setup`)`` | ``words (`/skill:tools-setup`)`` |
| `/[[deploy]]`, a command | `` `/tools:deploy` `` | error | error | error |

Also: `/[[setup#Usage]]` is an error. `/[[review]]` finds a skill in another
plugin; a name two plugins share is an error naming both plugins.
`dir/[[x]]` is a link, not a call. `` `/[[x]]` `` in a code span ships as
written. `\/[[x]]` ships as `/[[x]]` and passes the leftover check.
`/tools:setup` written in prose is an error; in a code span it ships as
written.

Code and leftovers:

- `[[x]]`, `/[[x]]` and `![[x]]` in a code span or fenced block ship as
  written.
- `\[[x]]` in prose ships as `[[x]]`, as open question 2 recommends.
- A leftover `[[` in prose is refused. One in code or after `\` is not.

Embeds:

- The worked example of section 5 ships as shown.
- A heading embed inlines the section with its subsections.
- A block embed inlines the block without its marker.
- A missing heading is `fragment-unresolved`.
- A nested embed's link is written relative to the outer file.
- A cycle prints its loop.
- An embedded note's frontmatter is dropped. Malformed frontmatter is an error
  naming both notes.
- An image embed is an image link.

D8:

- `![[run.sh]]` ships as a `bash` fenced block. An unknown extension gets no
  info string.
- A file holding its own ```` ``` ```` block and `[[` text is fenced with four
  backticks and ships byte for byte, with its `[[` not interpreted. So is a
  file whose fence is indented three spaces.
- An embed inside a list item is indented to the item.
- A binary file is refused. `![[run.sh#main]]` is refused. `[[run.sh]]` is a
  link.

Plugin-root paths (D9):

| Source | Claude Code | Codex, Cursor, Pi |
|---|---|---|
| `${PLUGIN_ROOT}/scripts/tool` in a skill's prose or code span | `${CLAUDE_PLUGIN_ROOT}/scripts/tool` | `../../scripts/tool` |
| the same, from the skill's `references/guide.md` | `${CLAUDE_PLUGIN_ROOT}/scripts/tool` | `../../../scripts/tool` |
| `Run ${PLUGIN_ROOT}/scripts/tool.` | path `scripts/tool`, full stop kept | the same |
| `${PLUGIN_ROOT}/commands/deploy.md` | `${CLAUDE_PLUGIN_ROOT}/commands/deploy.md` | error |
| `$PLUGIN_ROOT/x` | as written | as written |

Also:

- A path into another skill follows `skills/<p>-<s>/` on Cursor and Pi.
- `${PLUGIN_ROOT}/scripts/tool.js` is not `scripts/tool`.
- A path naming nothing shipped fails, naming the file and the reference.
- A lone `${PLUGIN_ROOT}` in Markdown is an error. `${CLAUDE_PLUGIN_ROOT}` in
  a note, a code span or a hooks file is an error that says to write
  `${PLUGIN_ROOT}`.
- A plain `scripts/x.py` in a `SKILL.md` ships as written and is not checked.
- An embedded reference is written from the embedding file.
- A script in `scripts/` holding the variable ships byte for byte.
- A hook command's `${PLUGIN_ROOT}` becomes `${CLAUDE_PLUGIN_ROOT}` on Claude
  Code, stays on Codex, and becomes `${CURSOR_PLUGIN_ROOT}` on Cursor. Its
  path is checked.

### 6. `init`, `bump`, docs and the repo's plugin

Changes:

1. `init <m> <p>` writes the skeleton.
2. `bump <p>`, `bump --marketplace <m>` and `--changed-since` on the new
   layout.
3. The reference docs and README.
4. `plugin/` moved, and its two workflows updated.
5. The Cursor pack's skills renamed from `skills/author/`, `skills/publish/`
   and `skills/setup/` to `skills/skillwright-author/` and so on, with
   matching `name` fields and call strings. The Pi pack already uses these
   names.
6. The six `/[[...]]` calls in code spans in skill notes moved into prose,
   keeping the `/` (`` `/[[setup]]` `` becomes `/[[setup]]`). The output is the
   same. The three heading self-calls, such as `# /[[author]]`, and the
   plugin note's call stay as they are.
7. The `publish`, `setup` and `author` skills rewritten for v2 (the table in
   "The repo's own `plugin/`"). The `author` skill's syntax examples stay in
   code, where they ship as written.

`plugin/` cannot build from commit 2 until this commit, which is fine inside
one PR.

Tests first:

- `init acme tools` in an empty folder writes the three notes, and a `build`
  of it succeeds.
- `init` refuses a folder with `skillwright.md`, and a name such as `Tools`.
- `bump tools` writes `tools.md`. `bump --marketplace acme` writes `acme.md`.
  With plugin `osdlc` in marketplace `osdlc`, `bump osdlc` bumps the plugin.
- `bump` with a plugin name two marketplaces share asks for
  `--marketplace`.
- `build.bump.patch: timestamp` sets the patch to the UTC time.
- `bump --changed-since` bumps only a changed plugin.
- The `plugin` workflow's drift check passes on the rebuilt
  `plugin/dist/osdlc/`.
