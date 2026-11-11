# Structure v2: implementation plan

## Goal

Build [structure v2](structure-v2.md) in skillwright as one PR. v2 reads one
fixed vault layout and has four commands: `init`, `build`, `check` and
`bump`. It writes each marketplace to `dist/<m>/` for Claude Code, Codex,
Cursor and Pi, with scoped links, calls, embeds and plugin-root paths. The
plan fixes what must hold and in what order: the commit order, the tests each
commit must have, the probes and the gates. How to write the code is the
implementor's choice.

## Read first

1. [structure-v2.md](structure-v2.md): the spec. Every rule in it needs a
   test.
2. [v2-changes.md](v2-changes.md): what v2 removes and changes for v1 users.
   It explains why many v1 tests are deleted, not ported.

"Section" numbers below refer to structure-v2.md.

## Settled and open

Sections 1 to 12 of the spec and its Appendix A are settled. These are not,
and each blocks a commit:

| Item | Where | Blocks |
|---|---|---|
| A shell command that runs a plugin-shared script | Spec, [Open questions](structure-v2.md#open-questions) item 1 | Commit 5 (plugin-root path tests) and the `sdlc` consumer migration |
| The `\[[` escape: drop the backslash or keep it | Spec, Open questions item 2 | Commit 5 (leftover tests) |
| Hook notes, or `hooks/hooks.json` only | Spec, Open questions item 3 | Commit 2 (which entries `hooks/` may hold) and commit 4 (the hooks file) |
| Codex relative paths | Probes, below | Commit 5 |
| Cursor's hook variable | Probes, below | Commit 5 (hook tests) |

## Probes

Run these before commit 5. harness-reference does not record the answers.

| Probe | Question | If the answer is no |
|---|---|---|
| `plugin-layout` on Codex | Does Codex resolve a skill's relative paths from the skill's folder (`Codex CLI Relative paths in SKILL.md open`, [#148](https://github.com/osdlc/skillwright/issues/148))? | Links up to the pack root need another form on Codex. |
| Cursor hooks | Does Cursor set `${CURSOR_PLUGIN_ROOT}` for hook commands? | Cursor hooks need another way to reach the pack. |

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

Each commit's blockers are listed under "Settled and open" above.

### 1. Cut to four commands

Delete every command but `init`, `build`, `check` and `bump`, with their core
code and tests ("The removed commands" in the code inventory). Do it first, so no later commit
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

Run the probes first (see "Probes").

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

## Definition of done

The PR is done when all of these hold:

- [ ] The gates pass on every commit: `cargo fmt --all --check`,
      `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo test --workspace`.
- [ ] Every numbered rule in the spec, and every case in its Appendix A, is
      covered by at least one test.
- [ ] The three open questions are answered, and the spec says the answer.
- [ ] Both probes have run, and the spec's "not verified" notes are updated.
- [ ] `plugin/` is moved to the v2 layout and rebuilt to
      `plugin/dist/osdlc/`, and the `plugin` workflow's drift check passes.
- [ ] `docs/reference/skillwright-md.md`, `docs/reference/fields.md` and the
      CLI README describe v2.

## Code inventory

This inventory is a starting map of where v2 touches the code, read at
`c9b3321b`, with paths under `packages/rust/`. It is not a set of
instructions: the implementor decides how, as long as the commit order, the
tests and the gates above hold.

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
    `replace_call`, `render_call` and `format_call` stay. The call positions
    no longer include a backtick. `check_call_fragment` turns from "checked,
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
  - Cursor's naming default becomes prefixed, as Pi's is. Today it comes
    from harness knowledge (`skills/plugin-namespace`) as bare.
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
  - `hooks_file::localize`, the variable swap for hooks, stays. Its source
    variable becomes `${PLUGIN_ROOT}`, and it gains the path check.
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

## Tracking

### Pull requests

The PRs this work depended on are done:
[#136](https://github.com/osdlc/skillwright/pull/136),
[#138](https://github.com/osdlc/skillwright/pull/138),
[#139](https://github.com/osdlc/skillwright/pull/139) and
[#143](https://github.com/osdlc/skillwright/pull/143) are merged;
[#134](https://github.com/osdlc/skillwright/pull/134),
[#135](https://github.com/osdlc/skillwright/pull/135),
[#137](https://github.com/osdlc/skillwright/pull/137) and
[#140](https://github.com/osdlc/skillwright/pull/140) are closed. v2 itself
is one PR, built as the six commits of the build order.

### Open issues

Issues still open that v2 touches. These are closed and need nothing:
[#53](https://github.com/osdlc/skillwright/issues/53),
[#75](https://github.com/osdlc/skillwright/issues/75),
[#78](https://github.com/osdlc/skillwright/issues/78),
[#79](https://github.com/osdlc/skillwright/issues/79),
[#80](https://github.com/osdlc/skillwright/issues/80),
[#81](https://github.com/osdlc/skillwright/issues/81),
[#82](https://github.com/osdlc/skillwright/issues/82),
[#91](https://github.com/osdlc/skillwright/issues/91),
[#108](https://github.com/osdlc/skillwright/issues/108),
[#118](https://github.com/osdlc/skillwright/issues/118) and
[#123](https://github.com/osdlc/skillwright/issues/123).

"Deferred" means the issue belongs to a later milestone for a command v2
leaves out. It stays open.

| Issue | Verdict | Reason |
|---|---|---|
| [#144](https://github.com/osdlc/skillwright/issues/144) published links: labels, shared-note paths, missing files | Changed, in v2 | The flattening part is moot: shared notes outside a plugin are gone. Missing-file links become build errors (section 4). The `.md` label part stays. |
| [#141](https://github.com/osdlc/skillwright/issues/141) bump: scan once, in-memory baseline | Keep | `bump` stays. The reads it lists get simpler. |
| [#77](https://github.com/osdlc/skillwright/issues/77) `resolve_run` should call `load_marketplace` | Changed | Still one loader, written fresh in v2. Close when it lands. |
| [#76](https://github.com/osdlc/skillwright/issues/76) one root-file and workspace loader | Changed | No workspace fold. Parse-once still applies. |
| [#83](https://github.com/osdlc/skillwright/issues/83) path-guard and globset | Changed | The output-overlap check goes. Link containment by real path is new work for path-guard. With D5 the glob half goes. |
| [#50](https://github.com/osdlc/skillwright/issues/50) compile scans bodies twice | Keep | Compile stays. |
| [#47](https://github.com/osdlc/skillwright/issues/47) leftover `[[` check gaps | Keep | Compile stays. |
| [#46](https://github.com/osdlc/skillwright/issues/46) valid Markdown refused as leftover `[[` | Changed | v2's leftover check skips code spans, fenced blocks and escaped text. Indented code, math, `<pre>` and autolinks are still open. |
| [#142](https://github.com/osdlc/skillwright/issues/142) publish collision checks | Deferred: publish | All three asks are about `publish`. Inside a pack, v2's own rule refuses two sources on one path (section 9). |
| [#87](https://github.com/osdlc/skillwright/issues/87) rename of a bundle that half-lands | Deferred: rename | `rename` leaves v2. |
| [#49](https://github.com/osdlc/skillwright/issues/49) rename reads the vault three times | Deferred: rename | `rename` leaves v2. |
| [#45](https://github.com/osdlc/skillwright/issues/45) rename misses an unreadable note | Deferred: rename | `rename` leaves v2. |
| [#43](https://github.com/osdlc/skillwright/issues/43) rename can make a link ambiguous | Deferred: rename | `rename` leaves v2. When it returns, links resolve within one plugin, so the risk is smaller. |
| [#51](https://github.com/osdlc/skillwright/issues/51) adopt in reverse | Deferred: adopt | Reading harness files into notes is `adopt`'s job. Nothing in v2 needs it. |
| [#94](https://github.com/osdlc/skillwright/issues/94) install natively first | Deferred: install and adopt | Both halves need commands v2 leaves out: `harness install` for the native path, `adopt` for the fallback. |
| [#42](https://github.com/osdlc/skillwright/issues/42) markdown-db and markdown-contract | Keep | Not about skillwright. Already labelled to-close. |
| [#4](https://github.com/osdlc/skillwright/issues/4) gaps migrating a marketplace | Keep | Harness output gaps. Already labelled to-close. |
