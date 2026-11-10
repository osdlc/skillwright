# Structure v2: impact (draft)

What [structure v2](structure-v2.md) removes, which code shrinks, what it means
for the open PRs and issues, and an order to build it in. Code was read at
`c9b3321b`. Paths are under `packages/rust/`.

## Commands removed

v2 keeps `init`, `build`, `check` and `bump`. Every other command leaves v2
and may return in a later milestone:

- `publish` and `unpublish`, with `build.destinations`.
- `harness install`, `update`, `uninstall`, `link` and `unlink`.
- `adopt`, with `--from`, `--from-git` and `build.scan_locations`.
- `new`, `rename`, `move`, `delete` and `convert`.
- `build --watch`.
- `list`, `status` and `author`.

## Features removed

- A plain `[[skill]]` rendering the skill's name (v1's mention). It is now an
  ordinary file link that warns `Did you mean to call /[[skill]]?` (D3).
- A vault with no root file, read with every default.
- The walk-up search for the nearest `skillwright.md`, and its reserved-name
  rule (`reserved-name`). The root is now the first folder holding both
  `skillwright.md` and `marketplaces/`.
- `--config`. `--vault` names the root.
- The root file as a marketplace. Its marketplace fields move to
  `marketplaces/<m>/<m>.md`.
- `workspace: true`, and `marketplaces/<m>/skillwright.md` as a member file.
  The root is always the workspace.
- A root `skills/`, `agents/`, `commands/` or `hooks/` as a plugin named after
  the marketplace, and the root file's `version` as that plugin's version.
- A root `plugins/` as the plugins of a single marketplace.
- Marketplaces below the top, and components promoted to a marketplace.
- Flat entry notes (`skills/x.md`) and `SKILL.md` entries.
- A note `name` that differs from its folder.
- Optional plugin notes (decision D6).
- `build.patterns` and every custom folder glob.
- `build.exclude` (decision D5). A file that must not ship goes in `docs/`.
- `build.out`, `--out` and the "no default output" rule. Output is fixed at
  `dist/<m>/`.
- `<harness>: {packs: <path>}`, with its overlap checks and the
  `stale-output` warning. This amends
  [D-WNTQ](../decisions/D-WNTQ-harness-profile-values-from-config.md):
  `pi: {naming}` stays settable, `packs` does not.
- Vault-wide link resolution: the basename match across the vault, the
  `own_folder_candidate` tiebreak, `link-ambiguous` across folders, and
  shared notes copied to `references/vault/<vault-relative>` or
  `assets/vault/<vault-relative>`.
- `build.assets` and `build.hooks` (decision D4).
- The plugin `assets/` folder copied flat to the pack root (`assets/bin/tool`
  shipping as `bin/tool`). A plugin's `assets/` now ships as `assets/`. The
  launcher moves to `scripts/`. `.mcp.json` has no home (decision D7).
- Loose files in a component folder. A skill holds `<s>.md`, `references/`,
  `scripts/` and `assets/`. An agent, command or hook folder holds its note.
  Anything else is an error.
- An embed or link to nothing shipped as its words. It is now an error.
- An embed of a file that is neither a note nor an image rendered as a plain
  link. It is now inlined as a fenced code block (decision D8).
- Calls read inside code spans and fenced blocks. Code is now fully literal.
- A fragment on a call or a mention, checked and then dropped. On a call,
  `/[[setup#Usage]]`, it is now an error. On a plain link, `[[setup#Usage]]`,
  it is kept in the file link.

If v2 is accepted, [D-GPYR](../decisions/D-GPYR-convention-first.md) needs an
amendment too: its patterns layer is gone.

## Code that shrinks or goes

### tradecraft-core

- `paths.rs`: `find_root_file`, `is_component_note`, `is_reserved_note`,
  `is_vault_root_file`, `is_marketplace_entry`, `is_flat_root_file_name`,
  `workspace_above`, `RootRead`, `read_workspace_above`, `workspace_file`,
  `read_root` and `read_root_both` go. `marketplace_inputs` and
  `load_marketplace` become one loader: read the root file, list
  `marketplaces/*/`, read each `<m>.md`.
- `patterns.rs`: almost all of it goes. `Patterns::canonical`,
  `plugin_folder`, `assets_folder`, `sibling_files`, `source_folders`,
  `warnings`, `warnings_with`, `keep_conventional` and
  `workspace_exclude_warnings` go, and with D5 the exclude list and the glob
  matcher (`glob_match`, `glob_problem`) too. What stays is the never-ship
  rule, which can move into `discover.rs`.
- `settings.rs`: `FolderPatterns`, `folder_patterns`, `globs`, `warn_nested`
  and the `patterns` and `out` keys go. `RootFile::inherit` (workspace into
  member) becomes the root-defaults step of the cascade. `MarketplaceFields`
  is read from `<m>.md`, not the root file. `compose_root_file` writes a root
  with no marketplace fields. `root_file_config` and `RootFile::to_config`
  shrink. `AssetSetting` and `HookSetting` go under D4. `DestinationSetting`
  and the `destinations` and `scan_locations` keys go with their commands.
- `discover.rs`: `classify` becomes a fixed-depth match. `conventional`,
  `folder_placement_with`, `RESERVED_NAME_REASON`, `SKILL_ENTRY`,
  `is_skill_entry` and the `SKILL.md` branch of `is_entry` go. `placement`
  loses `container_level` (promoted components). `container_notes`,
  `container_notes_with` and `read_container` read fixed paths.
  `under_a_kind_folder` and `refuse_stray` turn into the layout check of
  section 11.
- `model/cascade.rs`: `only_marketplace`, `sole_marketplace`,
  `marketplaces_in` and the "a note under no marketplace takes the one
  marketplace" rule go. `marketplace_note` reads `<m>.md`. `belongs_to`
  becomes a path prefix test.
- `model/load.rs`: `standalone` (publish of one note), `members_folder`,
  `has_folder`, the marketplace's-own-plugin branch of `missing_versions`,
  `load_assets`, `load_hook_files` (under D4), `folder_assets` (the flat
  copy; replaced by copying the three shared folders under their own names),
  `asset_covers` (only the launcher check is left) and `plugin_folder` go or
  shrink. `marketplace_name` and `resolve_marketplace` read the folder.
- `source_view.rs`: `output_overlaps_vault`, `output_in`, `output_paths`,
  `check_pack_location` and the workspace folding in `effective_config`,
  `effective_configs`, `marketplace_folder` and `marketplace_notes` go.
  Output cannot overlap source when only `marketplaces/` is read.
- `compile.rs`: `CompileSource::resolve` changes from vault-wide to scoped
  resolution: own folder, then the plugin's shared folders and components.
  A shared file lands at the pack root, and its link is written relative to
  where the linking file lands. `own_folder_candidate`
  and the `references/vault/` and `assets/vault/` placements go.
  `vault_link_index` indexes one plugin, not the vault. A real-path
  containment check is added beside `refuse_never_shipped`. Calls and
  plain links (D3): the call syntax stays, with `TokenKind::Call`,
  `TokenKind::Escape` (`\/[[`), `sigil_at` for the call positions,
  `replace_call`, `render_call` and `format_call`. `check_call_fragment`
  turns from "checked, then dropped" into an error. The call index
  (`skill_index` in `model/load.rs`) adds commands for harnesses that carry
  them. `skill_mention` and the `mention` path of `render_or_mark` go: a plain
  link to a skill is a file link, plus the new "Did you mean to call"
  warning. Nothing in code is read any more, so `scan_code_calls`,
  `code_token`, `TokenKind::Unconverted` with its `unconverted-call` warning,
  and the in-code branch of `format_call` go. The leftover-`[[` check
  (`validate_no_obsidian_syntax`) skips code spans, fenced blocks and
  `\[[`-escaped text. `literal_skill_calls` and `literal-skill-call`, which refuse a
  hand-written `/plugin:skill`, stay. In `replace_token`, an embed or link to
  nothing becomes an error instead of an entry in `unresolved`. A non-note,
  non-image embed is inlined as a fenced block under D8: new code for the
  extension table, the fence length, list indentation and the UTF-8 check. Embed expansion,
  `select_fragment` and the cycle check stay as they are.
- `harness/profile.rs`: `PluginFormat::packs` stays a fixed value.
  `owned_paths_at` and the config override path go.
- `export.rs`: the `stale-output` warning and moved-packs handling go.
- `vault.rs`: `bundle_root` keeps only the `<name>/<name>.md` shape.

### The removed commands

- `tradecraft-core/src/publish.rs`: everything that writes to a destination
  goes: `publish_skill`'s single-note arms, the destination manifest, the
  rails, `publishes_to`, `overrides_for`, `override_states` and
  `accepted_override_keys`. What `export.rs` and `model/cascade.rs` call
  stays: the plugin arm, `plugin_format_carries_commands`,
  `plugin_format_carries_hooks`, `kind_has_renderer`, `canonical_target`,
  `KNOWN_TARGETS` and `accepted_keys`. Moving those into `export.rs` or
  `harness/` is a follow-up, not v2.
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

- `resolve.rs`: `resolve_run` loses the walk-up, `--config` and the workspace
  fold. `workspace_runs`, `refuse_workspace`, `workspace_warnings`,
  `member_workspace_warnings`, `is_workspace`, `configured_out` and
  `resolve_skills_root` go or shrink to "every marketplace, or the one named".
- `config.rs`: `workspace_above` and `workspace_members` go.
- `commands/init.rs`: writes the v2 skeleton.
- `commands/marketplace.rs` (`build` and `check`): loses `--out` and the
  workspace runs.
- `commands/bump.rs`: the marketplace's-own-plugin version rule goes.
  `BuiltRef::build` checks out `skillwright.md` and `marketplaces/` only.

### tradecraft-wasm

- `lib.rs`: the `out_root` argument and the pattern and workspace options of
  `discovery_view` and `load_view` go.

### Tests and docs

- Tests for every removed feature are deleted, not ported: the
  `patterns.rs` and `settings.rs` workspace tests, the reserved-name tests in
  `paths.rs`, and the workspace cases in `tests/cli.rs`,
  `tests/consumer_flows.rs` and `tests/release_api.rs`.
- `docs/reference/skillwright-md.md` is rewritten. Its Patterns, Several
  marketplaces and Publishing one note by path sections go.
- The CLI README's verb and option tables shrink to the four commands.
- `docs/reference/fields.md` is regenerated after the key table changes.
- The CLI README's Configuration section is rewritten.

### The repo's own `plugin/`

- `plugin/skillwright.md` keeps only `author`, `license` and `build:`.
- Its marketplace fields move to `plugin/marketplaces/osdlc/osdlc.md`.
- `plugin/plugins/skillwright/` moves to
  `plugin/marketplaces/osdlc/plugins/skillwright/`, with each skill as
  `skills/<s>/<s>.md`.
- The built trees move from `plugin/.claude-plugin/`, `plugin/.agents/`,
  `plugin/.cursor-plugin/` and `plugin/.pi/` to `plugin/dist/osdlc/`.
- The `plugin` workflow keeps its drift check: it builds and fails on any
  difference under `plugin/dist/osdlc/`, and runs `check`. It drops
  `--config`.
- The `plugin-release` workflow copies the pack and manifest entry from
  `plugin/dist/osdlc/` into `osdlc/marketplace`. It already copies; only the
  source path changes.
- Install from `osdlc/marketplace` is unchanged. `plugin/` itself is no longer
  installable from its own root. Nothing documents that route today.

## Open PRs

| PR | Verdict | Reason |
|---|---|---|
| [#136](https://github.com/osdlc/skillwright/pull/136) self-link as a skill link | Merge as is | Its `\[[` escape and leftover-`[[` checks stay in v2. Its self-link change does not: it renders a skill's link to its own note as the skill's name through `skill_mention`, and under D3 that link is a file link to `SKILL.md`. Commit 5 removes `skill_mention` with it. [#138](https://github.com/osdlc/skillwright/pull/138) stacks on it. |
| [#138](https://github.com/osdlc/skillwright/pull/138) key table and harness profile own their facts | Merge, v2 builds on it | v2's root-file key check and its smaller key set come from the key table this PR builds. |
| [#143](https://github.com/osdlc/skillwright/pull/143) one identity struct | Merge, v2 builds on it | The struct and key-table inheritance carry v2's root, marketplace, plugin, component cascade. Merge after [#138](https://github.com/osdlc/skillwright/pull/138). |
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
| [#123](https://github.com/osdlc/skillwright/issues/123) Cursor entries carry no displayName | Keep | Manifest output. [#138](https://github.com/osdlc/skillwright/pull/138) addresses it. |
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
| [#46](https://github.com/osdlc/skillwright/issues/46) valid Markdown refused as leftover `[[` | Changed | v2's leftover check skips code spans, fenced blocks and `\[[` text. Indented code, math, `<pre>` and autolinks are still open. |
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
write its tests, see them fail for the right reason, then make the change.
Each commit
passes the gates alone (`cargo fmt --all --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo test --workspace`), so the branch can be
bisected and reviewed commit by commit.

Before the branch starts, merge
[#136](https://github.com/osdlc/skillwright/pull/136),
[#138](https://github.com/osdlc/skillwright/pull/138),
[#143](https://github.com/osdlc/skillwright/pull/143) and
[#139](https://github.com/osdlc/skillwright/pull/139).
[#134](https://github.com/osdlc/skillwright/pull/134),
[#135](https://github.com/osdlc/skillwright/pull/135),
[#137](https://github.com/osdlc/skillwright/pull/137) and
[#140](https://github.com/osdlc/skillwright/pull/140) are closed.

The commits, in order:

1. **Cut to four commands.** Delete every command but `init`, `build`, `check`
   and `bump`, with their core code and tests ("The removed commands" above).
   Do it first, so no later commit ports a command v2 drops. Test first: each
   removed command is an unknown command with exit code 2; the existing
   `build`, `check` and `bump` tests still pass unchanged.
2. **Layout reader.** Root discovery, the fixed-position `classify`, the
   one loader, the folder rules of section 3, and the refusals of section 11.
   Delete the walk-up, reserved names, workspaces, patterns, `SKILL.md` and
   flat entries. Test first: a v2 fixture vault lists every component with its
   marketplace and plugin; one test per case in section 11 asserts the error
   names the path and prints the expected layout; a loose file in a skill
   folder is refused naming the allowed entries; a `skillwright` plugin's note
   is not taken for a root.
3. **Root file and cascade.** The root file's key set, `<m>.md` as the
   marketplace note, the root-marketplace-plugin-component cascade, required
   plugin notes, and name-equals-folder. Test first: one table test per
   inherited key across the four levels; a marketplace field in the root file
   is refused naming `<m>.md`; `build:` outside the root is refused.
4. **Bundling and output.** Fixed `dist/<m>/`, default packs folders,
   `check` against `dist/`, and the pack contents of section 9. Delete
   `build.out`, `--out`, the `packs` override, the overlap checks,
   `stale-output` and the flat `assets/` copy. Test first, one test per row of
   the "What a pack holds" table: a plugin's `assets/logo.png` lands at
   `<pack>/assets/logo.png`; a skill's `scripts/run.sh` keeps its exec bit; a
   Markdown file in `references/` is compiled; an agent ships nowhere; two
   sources on one path are refused naming both. Also: a two-marketplace build
   writes two trees under `dist/`; `check` reports drift after a hand edit
   there; a `packs` key is refused.
5. **Links and transclusion.** Scoped resolution, the link table, the
   real-path check, missing targets as errors, calls and plain links (D2,
   D3), literal code, the leftover check, and the embed rules of section 5.
   Test first, through `build`, one test per rule in sections 4 and 5:
   - Scope: a symlink in a skill folder pointing out of the marketplace is
     refused; the plugin's `references/x.md` renders as
     `../../references/x.md` from a skill; a link to a file in another plugin
     is refused.
   - Plain links: `[[setup]]` from `author` renders as
     `[setup](../setup/SKILL.md)` and warns `Did you mean to call /[[setup]]?`;
     `[[setup#Usage]]` keeps its fragment; a skill's link to its own note is
     `[author](SKILL.md)` with no warning; a link to a command is a file link
     on Claude Code and refused for Codex; a link to an agent is refused.
   - Calls: `/[[setup]]` renders as `` `/skillwright:setup` `` on Claude Code
     and `` `$skillwright:setup` `` on Codex; `/[[setup|words]]` keeps the
     words; `/[[setup#Usage]]` is an error; `/[[review]]` finds a skill in
     another plugin, and a name two plugins share is refused naming both;
     `/[[deploy]]` to a command renders on Claude Code and is refused for
     Codex; `dir/[[x]]` is a link, not a call; `\/[[x]]` ships as `/[[x]]`.
   - Code and leftovers: `[[x]]`, `/[[x]]` and `![[x]]` in a code span or
     fenced block ship as written; `\[[x]]` in prose ships as `[[x]]`; a
     leftover `[[` in prose is refused, and one in code or after `\` is not.
   - Embeds: a heading embed inlines the section with its subsections; a block
     embed inlines the block without its marker; a missing heading is
     `fragment-unresolved`; a nested embed's link is written relative to the
     outer file; a cycle prints its loop; an embedded note's frontmatter is
     dropped; an image embed is an image link.
   - D8: `![[run.sh]]` ships as a `bash` fenced block; an unknown extension
     gets no info string; a file holding its own ```` ``` ```` block and `[[`
     text is fenced with four backticks and ships byte for byte, with its `[[`
     unread; an embed inside a list item is indented to the item; a binary
     file is refused; `![[run.sh#main]]` is refused; `[[run.sh]]` is a link.
   Before this commit, run the `plugin-layout` probe on Codex. harness-reference
   does not record where Codex resolves a skill's relative paths
   (`Codex CLI Relative paths in SKILL.md open`). If it is not the skill's
   folder, links up to the pack root need another form for Codex.
6. **`init`, `bump`, docs and the repo's plugin.** `init` writes the skeleton;
   `bump` by name and `--changed-since` on the new layout; the reference docs
   and README; `plugin/` moved and its two workflows updated; the seven
   `/[[...]]` calls that sit in code spans in `plugin/` moved into prose,
   keeping the `/` (`` `/[[setup]]` `` becomes `/[[setup]]`), which gives the
   same output; the three heading self-calls, such as `# /[[author]]`, stay as
   they are; the `author` skill's text on links and calls rewritten for D3,
   with its syntax examples left in code, where they ship as written. `plugin/` cannot build
   from commit 2 until this commit, which is fine inside one PR. Test first:
   `init` in an empty folder writes the three notes and a `build` of it
   succeeds; `init` refuses a folder with `skillwright.md`; `bump <plugin>`
   and `bump <m>` write the right note; `bump --changed-since` bumps only a
   changed plugin; the `plugin` workflow's drift check passes on the rebuilt
   `plugin/dist/osdlc/`.
