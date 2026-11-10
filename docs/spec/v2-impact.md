# Structure v2: impact (draft)

What [structure v2](structure-v2.md) removes, which code shrinks, what it means
for the open PRs and issues, and an order to build it in. Code was read at
`c9b3321b`. Paths are under `packages/rust/`.

## Features removed

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
- Optional plugin notes (decision D7).
- `build.patterns` and every custom folder glob.
- `build.exclude` (decision D6). A file that must not ship goes in `docs/`.
- `publish <path>` of a note that no kind folder places.
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
- A plain link to another skill read as a call (decision D4).
- `build.assets` and `build.hooks` (decision D5).
- The plugin `assets/` folder copied flat to the pack root (`assets/bin/tool`
  shipping as `bin/tool`). A plugin's `assets/` now ships as `assets/`. The
  launcher moves to `scripts/`. `.mcp.json` has no home (decision D9).
- Loose files in a component folder. A skill holds `<s>.md`, `references/`,
  `scripts/` and `assets/`. An agent, command or hook folder holds its note.
  Anything else is an error.

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
  `workspace_exclude_warnings` go, and with D6 the exclude list and the glob
  matcher (`glob_match`, `glob_problem`) too. What stays is the never-ship
  rule, which can move into `discover.rs`.
- `settings.rs`: `FolderPatterns`, `folder_patterns`, `globs`, `warn_nested`
  and the `patterns` and `out` keys go. `RootFile::inherit` (workspace into
  member) becomes the root-defaults step of the cascade. `MarketplaceFields`
  is read from `<m>.md`, not the root file. `compose_root_file` writes a root
  with no marketplace fields. `root_file_config` and `RootFile::to_config`
  shrink. `AssetSetting` and `HookSetting` go under D5.
- `discover.rs`: `classify` becomes a fixed-depth match. `conventional`,
  `folder_placement_with`, `RESERVED_NAME_REASON`, `SKILL_ENTRY`,
  `is_skill_entry` and the `SKILL.md` branch of `is_entry` go. `placement`
  loses `container_level` (promoted components). `container_notes`,
  `container_notes_with` and `read_container` read fixed paths.
  `under_a_kind_folder` and `refuse_stray` turn into the layout check of
  section 10.
- `model/cascade.rs`: `only_marketplace`, `sole_marketplace`,
  `marketplaces_in` and the "a note under no marketplace takes the one
  marketplace" rule go. `marketplace_note` reads `<m>.md`. `belongs_to`
  becomes a path prefix test.
- `model/load.rs`: `standalone` (publish by path), `members_folder`,
  `has_folder`, the marketplace's-own-plugin branch of `missing_versions`,
  `load_assets`, `load_hook_files` (under D5), `folder_assets` (the flat
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
  containment check is added beside `refuse_never_shipped`. The plain
  link-as-call reading goes under D4.
- `harness/profile.rs`: `PluginFormat::packs` stays a fixed value.
  `owned_paths_at` and the config override path go.
- `export.rs` and `publish.rs`: the `stale-output` warning, moved-packs
  handling and the by-path publish branch go.
- `vault.rs`: `bundle_root` keeps only the `<name>/<name>.md` shape.

### skillwright (CLI)

- `resolve.rs`: `resolve_run` loses the walk-up, `--config` and the workspace
  fold. `workspace_runs`, `refuse_workspace`, `workspace_warnings`,
  `member_workspace_warnings`, `is_workspace`, `configured_out` and
  `resolve_skills_root` go or shrink to "every marketplace, or the one named".
- `config.rs`: `workspace_above` and `workspace_members` go.
- `commands/init.rs`, `commands/marketplace.rs`, `commands/authoring.rs` and
  `apply.rs`: `init`, `new`, `move`, `rename` and `convert` write fixed paths.
  The workspace-root refusals and the "rename a marketplace in a vault but not
  in a workspace" split go.
- `commands/bump.rs`: the marketplace's-own-plugin version rule goes.
  `BuiltRef::build` checks out `skillwright.md` and `marketplaces/` only.
- `commands/publish.rs`: the by-path branch goes.
- `commands/harness/*`: read built marketplaces from `dist/<m>/`.

### tradecraft-wasm

- `lib.rs`: the `out_root` argument and the pattern and workspace options of
  `discovery_view` and `load_view` go. `publishSkill` refuses a path outside
  `marketplaces/`.

### Tests and docs

- Tests for every removed feature are deleted, not ported: the
  `patterns.rs` and `settings.rs` workspace tests, the reserved-name tests in
  `paths.rs`, and the by-path and workspace cases in `tests/cli.rs`,
  `tests/consumer_flows.rs` and `tests/release_api.rs`.
- `docs/reference/skillwright-md.md` is rewritten. Its Patterns, Several
  marketplaces and Publishing one note by path sections go.
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
| [#135](https://github.com/osdlc/skillwright/pull/135) rename reports what it could not read | Merge as is | Dependency bumps, one walk and unreadable-file reporting. Layout-independent, and v2's `rename` still needs it. |
| [#136](https://github.com/osdlc/skillwright/pull/136) self-link as a skill link | Merge as is | Rendering and the leftover-`[[` check. Layout-independent. [#138](https://github.com/osdlc/skillwright/pull/138) stacks on it. |
| [#138](https://github.com/osdlc/skillwright/pull/138) key table and harness profile own their facts | Merge, v2 builds on it | v2's root-file key check and its smaller key set come from the key table this PR builds. |
| [#143](https://github.com/osdlc/skillwright/pull/143) one identity struct | Merge, v2 builds on it | The struct and key-table inheritance carry v2's root, marketplace, plugin, component cascade. Its workspace-to-member step becomes the root-defaults step. Merge after [#138](https://github.com/osdlc/skillwright/pull/138). |
| [#139](https://github.com/osdlc/skillwright/pull/139) bump checks out only what the build reads | Merge as is | A self-contained speed-up of `bump --changed-since`. v2 later shrinks its path set to `skillwright.md` and `marketplaces/`. |
| [#134](https://github.com/osdlc/skillwright/pull/134) one loader and one path rule | Close as moot | It unifies the workspace loader and the `skillwright.md` path rule, which v2 deletes. v2's first PR writes the one loader for the fixed layout. |
| [#137](https://github.com/osdlc/skillwright/pull/137) refuse a publish by path that would overwrite | Close as moot | Most of its diff is [#134](https://github.com/osdlc/skillwright/pull/134) and the by-path publish v2 removes. Names unique per plugin cover the rest. |
| [#140](https://github.com/osdlc/skillwright/pull/140) core hides build output | Close as moot | v2 reads only `marketplaces/` and writes only `dist/`, so output can never be read as source. It is also stacked on [#134](https://github.com/osdlc/skillwright/pull/134). |

## Open issues

| Issue | Verdict | Reason |
|---|---|---|
| [#144](https://github.com/osdlc/skillwright/issues/144) published links: labels, shared-note paths, missing files | Changed | Shared notes outside a component are gone, so the flattening part is moot. Missing-file links become errors under section 4. The `.md` label part stays. |
| [#142](https://github.com/osdlc/skillwright/issues/142) publish collision checks | Changed | The stray-note case goes. Cross-run ownership and agent and command collisions stay. |
| [#141](https://github.com/osdlc/skillwright/issues/141) bump: scan once, in-memory baseline | Keep | Performance. The reads it lists get simpler. |
| [#123](https://github.com/osdlc/skillwright/issues/123) Cursor entries carry no displayName | Keep | Manifest output. [#138](https://github.com/osdlc/skillwright/pull/138) addresses it. |
| [#118](https://github.com/osdlc/skillwright/issues/118) publish by path can overwrite a same-named skill | Moot | Publish by path is removed. |
| [#108](https://github.com/osdlc/skillwright/issues/108) Cursor plugin-prefixed names | Keep | Naming. [#138](https://github.com/osdlc/skillwright/pull/138) addresses it. |
| [#94](https://github.com/osdlc/skillwright/issues/94) install natively first | Keep | Install behaviour. `adopt` writes the v2 layout. |
| [#91](https://github.com/osdlc/skillwright/issues/91) symlinked `--vault` loses its workspace | Moot | Workspaces and member files are removed. |
| [#87](https://github.com/osdlc/skillwright/issues/87) rename of a bundle that half-lands | Keep | The failure mode stays. Fewer links can point at a bundle. |
| [#83](https://github.com/osdlc/skillwright/issues/83) path-guard and globset | Changed | The output-overlap check goes. Link containment by real path is new work for path-guard. With D6 the glob half goes. |
| [#82](https://github.com/osdlc/skillwright/issues/82) bump: sparse checkout, one scan | Keep | Performance. [#139](https://github.com/osdlc/skillwright/pull/139) does part of it. |
| [#81](https://github.com/osdlc/skillwright/issues/81) core should hide build output | Moot | Output is fixed at `dist/` and only `marketplaces/` is read. |
| [#80](https://github.com/osdlc/skillwright/issues/80) key types and root-file checks from the key table | Changed | The root file's key set shrinks. [#138](https://github.com/osdlc/skillwright/pull/138) does the rest. |
| [#79](https://github.com/osdlc/skillwright/issues/79) per-harness facts in HarnessProfile | Keep | Layout-independent. [#138](https://github.com/osdlc/skillwright/pull/138) does it. |
| [#78](https://github.com/osdlc/skillwright/issues/78) one rule for what a `skillwright.md` path is | Moot | There is one `skillwright.md`, at the root. Position decides every other note. |
| [#77](https://github.com/osdlc/skillwright/issues/77) `resolve_run` should call `load_marketplace` | Changed | Still one loader, but v2's first PR writes it fresh. Close when it lands. |
| [#76](https://github.com/osdlc/skillwright/issues/76) one root-file and workspace loader | Changed | No workspace fold. Parse-once still applies, in v2's first PR. |
| [#75](https://github.com/osdlc/skillwright/issues/75) one identity struct | Changed | The workspace step goes. [#143](https://github.com/osdlc/skillwright/pull/143) does the rest. |
| [#53](https://github.com/osdlc/skillwright/issues/53) a skill's link to itself | Keep | Rendering. [#136](https://github.com/osdlc/skillwright/pull/136) does it. |
| [#51](https://github.com/osdlc/skillwright/issues/51) adopt in reverse | Keep | It writes notes in the v2 layout. |
| [#50](https://github.com/osdlc/skillwright/issues/50) compile scans bodies twice | Keep | Internal. |
| [#49](https://github.com/osdlc/skillwright/issues/49) rename reads the vault three times | Keep | Still applies. [#135](https://github.com/osdlc/skillwright/pull/135) does part. |
| [#47](https://github.com/osdlc/skillwright/issues/47) leftover `[[` check gaps | Keep | Syntax, not scope. |
| [#46](https://github.com/osdlc/skillwright/issues/46) valid Markdown refused as leftover `[[` | Keep | Syntax, not scope. |
| [#45](https://github.com/osdlc/skillwright/issues/45) rename misses an unreadable note | Keep | Still applies. [#135](https://github.com/osdlc/skillwright/pull/135) does it. |
| [#43](https://github.com/osdlc/skillwright/issues/43) rename can make a link ambiguous | Changed | Bare links resolve only within a plugin with unique names, so the risk narrows to one plugin. |
| [#42](https://github.com/osdlc/skillwright/issues/42) markdown-db and markdown-contract | Keep | Not about skillwright's layout. Already labelled to-close. |
| [#4](https://github.com/osdlc/skillwright/issues/4) gaps migrating a marketplace | Keep | Harness output gaps. Already labelled to-close. |

## Build order

Merge [#135](https://github.com/osdlc/skillwright/pull/135),
[#136](https://github.com/osdlc/skillwright/pull/136),
[#138](https://github.com/osdlc/skillwright/pull/138),
[#143](https://github.com/osdlc/skillwright/pull/143) and
[#139](https://github.com/osdlc/skillwright/pull/139) first, and close
[#134](https://github.com/osdlc/skillwright/pull/134),
[#137](https://github.com/osdlc/skillwright/pull/137) and
[#140](https://github.com/osdlc/skillwright/pull/140). Then:

1. **Layout reader.** Root discovery, the fixed-position `classify`, the
   one loader, and the layout refusal of section 10. Delete the walk-up,
   reserved names, workspaces, patterns, `SKILL.md` and flat entries.
   Test first: a v2 fixture vault lists every component with its marketplace
   and plugin; one test per case in section 10 asserts the error names the
   path and prints the expected layout; a `skillwright` plugin's note is not
   taken for a root.
2. **Root file and cascade.** The root file's key set, `<m>.md` as the
   marketplace note, the root-marketplace-plugin-component cascade, required
   plugin notes, and name-equals-folder. Test first: one table test per
   inherited key across the four levels; a marketplace field in the root file
   is refused naming `<m>.md`; `build:` outside the root is refused.
3. **Output.** Fixed `dist/<m>/`, default packs folders, `check` against
   `dist/`, and the plugin's `references/`, `scripts/` and `assets/` copied
   under their own names. Delete `build.out`, `--out`, the `packs` override,
   the overlap checks, `stale-output` and the flat `assets/` copy. Test first:
   a two-marketplace build writes two trees under `dist/`; `check` reports
   drift after a hand edit there; a `packs` key is refused; a plugin's
   `assets/logo.png` lands at `<pack>/assets/logo.png`; a script in
   `scripts/` keeps its exec bit; a loose file in a skill folder is refused
   naming the allowed entries.
4. **Link scope and containment.** Scoped resolution, the link table, the
   real-path check, the one-skill rule, calls (D3) and plain links (D4).
   Test first, one test per table row and per rule in section 4, through the
   build: a symlink in a skill folder pointing out of the marketplace is
   refused; a link to a sibling skill renders as its installed path; a link
   from a skill to the plugin's `references/x.md` renders as
   `../../references/x.md`; the sibling link fails when publishing that skill
   alone, naming the link; `publish` and `harness link` of a skill that
   links into the plugin's `references/` are refused, naming the link.
   Before this PR, run the `plugin-layout` probe on Codex. harness-reference
   does not record where Codex resolves a skill's relative paths
   (`Codex CLI Relative paths in SKILL.md open`). If it is not the skill's
   folder, links up to the pack root need another form for Codex.
5. **Verbs, docs and the repo's plugin.** `init`, `new`, `move`, `rename`,
   `bump`, `publish` (D1), `adopt` and `harness` on the new layout; the
   reference docs; `plugin/` moved and its two workflows updated. Test first:
   a CLI test per verb writes the expected path; `adopt` of a skill with a
   loose `forms.md` reports it with `references/` as its folder and moves
   nothing; the `plugin` workflow's
   drift check passes on the rebuilt `plugin/dist/osdlc/`.

PRs 1 and 2 can be one PR if the diff stays reviewable. PR 4 depends only on 1.
