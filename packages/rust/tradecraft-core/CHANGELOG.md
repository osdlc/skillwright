# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this
project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.45.0](https://github.com/osdlc/skillwright/compare/tradecraft-core/v0.44.0...tradecraft-core/v0.45.0) - 2026-11-07

### Breaking

Behaviour:

- A value of the wrong kind is refused as `refused: malformed: <path>: <what>`, naming the key and the kind it must be. This covers a spec key on a skill, agent or command, a key a harness reads in any harness block of a skill, command, plugin or marketplace, and a skillwright key of the wrong shape. 0.44.0 checked only a block's `metadata`, and a skill's block only for the target being built. It carried a bad spec key under `metadata:`. The vault reader, `load`, `load_plugin`, `export_skills` and `publish_skill` all refuse. Every block that can ship is checked, whichever harnesses are selected. An agent's blocks are not checked, because they never ship.
- The vault scan makes the same checks, so such a note is a `SkippedNote` with reason `malformed`. The root file's own blocks are checked too, and a bad one is a `SkippedNote` for `skillwright.md`. A note whose only opt-in key is the wrong kind, such as `description: 2048`, is refused, not passed by as an ordinary note.
- An agent's `tools` must be a string, a list of strings or a mapping of tool names, and `temperature` must be a number.
- A key set to nothing is dropped, bare or in any harness block, including a container's. 0.44.0 carried it as `null`. It does not clear a value the note inherits from a container. A key set to nothing that no harness reads still warns `unknown-key`.
- Errors that 0.44.0 gave other forms now use `malformed`. A wrong-shape skillwright key was `CoreError::Io` (`reading <path>: …`) and is now `CoreError::Refused`. A container block `metadata` that is not a mapping was `refused: malformed-block`, and a skill's block `metadata` that is not a mapping was `CoreError::Io`. A `match` on these must look for the new form.
- `load` and `load_plugin` fail with `refused: malformed`, not `unreadable-note`, when every skipped note holds a value of the wrong kind, and say so in the text.

API:

- `publish::overrides_for` takes the note's `EntityKind` in place of a target name. It folds and checks every block, and returns them all.
- `SkippedNote::detail` is the prose after the reason token, no longer the whole error. Use `SkippedNote::sentence` for the token and the prose together.

### Added

- `ReasonToken::Malformed` (`malformed`), and `CoreError::wrong_kind(path, message)`, which refuses with it.
- `SkippedNote::sentence`.

### Fixed

- The vault reader reads Claude Code's lenient forms, bare and in blocks, and turns each into the plain form every harness reads: an `allowed-tools` string separated by commas, a boolean written `yes`, `no`, `on`, `off`, `1` or `0` in any case and quoted or not, and an unquoted `argument-hint: [issue-number]`, which YAML reads as a list.
- `load`, `load_plugin` and `publish_skill` check the root config's and plugin notes' blocks before a member inherits them, so a refusal names the container. `publish_skill` did not check them before.

## [0.44.0](https://github.com/osdlc/skillwright/compare/tradecraft-core/v0.43.0...tradecraft-core/v0.44.0) - 2026-11-07

This release carries the core of skillwright's new vault structure. For the vault side, see the [migration guide](https://github.com/osdlc/skillwright/blob/tradecraft-core/v0.44.0/docs/guide/migrating.md). For the library side, see [Load a vault and build it](https://github.com/osdlc/skillwright/blob/tradecraft-core/v0.44.0/packages/rust/tradecraft-core/README.md#load-a-vault-and-build-it) and [Stability](https://github.com/osdlc/skillwright/blob/tradecraft-core/v0.44.0/packages/rust/tradecraft-core/README.md#stability).

### Breaking

Behaviour:

- `Namespace::default()` is bare, so frontmatter keys are `version`, not `sw-version`. Pass `Namespace::sw()` to read the prefixed forms. `Namespace::DEFAULT_STEM` is removed. A `sw-<key>` spelling of a key the core owns is refused as `removed-key` on a bare vault.
- `Namespace::new` refuses a stem that claims a key a renderer reads, such as `model`, `context` or `user` (from `user-invocable`), with `StemProblem::ClaimsPublishedKey`. `Namespace::PUBLISHED_KEYS` lists those keys.
- `project_agent` refuses an agent that sets both `tools` and `allowed-tools` (`conflicting-keys`), and a `tools` that names no tool (`malformed-tools`). For Claude Code it also refuses a `tools` that is not a list or a string of names (`malformed-tools`).
- `kind`, `plugin`, `sw-kind` and `sw-plugin` are refused: the folder decides both. `sw-overrides` is replaced by `claude:`, `codex:`, `cursor:` and `pi:` blocks, which `export_skills` now applies too. Only those four harnesses have blocks.
- A note's `author`, `license`, `repository`, `homepage`, `targets` and harness blocks inherit from the nearest container on every path, so a publish can stop writing to a harness its containers exclude. A plugin's `author` no longer falls back to `owner`.
- `marketplaces/<m>/skillwright.md` is the only marketplace note. A plugin note whose `name` differs from its folder is refused, and a `skillwright.md` directly in a kind folder is refused as a reserved name.
- A link or embed into a folder that never ships is refused as `excluded-link`.
- A plain `[[skill]]` renders as the skill's name, not a call. Write `/[[skill]]` for a call. The `implicit-skill-call` warning is gone.
- Codex, Cursor and Pi call a launcher by its path relative to the skill, and the per-skill shim is no longer written. The rewrite happens only when the plugin ships an asset at `launcher_path`. With no `launcher_path` the rewrite is off.
- Manifests drop `version` from Claude's marketplace entry and `author.url` and `owner.url` from Cursor's.

Types you build:

- These types are `#[non_exhaustive]`, so a struct literal no longer compiles outside the crate. Call `new` with the required inputs and set other fields on the value: `PublishRequest`, `StatusRequest`, `ExportRequest`, `LoadRequest`, `AdoptRequest`, `ComponentRequest`, `CompileSource`, `NewNote`, `AssetSpec`, `HookFileSpec`.
- Build these from `Default` and set their fields: `MarketplaceConfig`, `PluginOverrides`, `Placement`.
- Output types can no longer be built or destructured exhaustively: `ExportReport`, `PublishReport`, `PublishedFile`, `DiscoveryReport`, `DiscoveredComponent`, `DiscoveryWarning`, `SkippedNote`, `AdoptReport`, `AdoptedSkill`. Read them by field. `DiscoveryWarning::new` and `PublishedFile::new` build a line for a host's own report.
- A `match` on these enums needs a `_` arm: `CoreError`, `AuthoringOp`, `ExportUnit`, `UnitStatus`, `StaleKind`, `PublishStatus`, `PublishOutcome`, `PublishState`, `AdoptStatus`, `ReasonToken`, `StemProblem`.
- `Plugin`, `Call`, `CallSite`, `Pack`, `Closure`, `ContainerIdentity`, `VaultFrontmatter` and `PluginFormat` gain fields, so a struct literal of one needs them.

Removed or renamed:

- `MarketplaceConfig::shim_target` is now `launcher_path: Option<String>`, and `MarketplaceConfig::default_plugin` is removed. `Plugin.shim_target` is now `launcher`.
- `Segment::CliInvocation` is now `Segment::LauncherCall`, and `CliInvocation::SkillRelativeShim` is now `SkillRelativePath`. `DEFAULT_SHIM_TARGET` and `SHIM_RELATIVE_PATH` are removed.
- `status::UnitStatus` is now `PublishUnitStatus`, with no alias.
- `MarketplaceSettings`, `ProjectMarketplace`, `parse_project_marketplace` and `settings::PROJECT_FILE` are removed. Read a root file with `parse_root_file`, or load everything with `load_marketplace`.
- `export::root_trees` and `HarnessProfile::tree` are replaced by `owned_paths`.
- `authoring::DEFAULT_PLUGIN`, `AuthoringOp::RenamePlugin` and the `frontmatter` field of `AuthoringOp::Move` are removed. A plugin rename is an ordinary `Rename`.
- `KindSource::Frontmatter`, `StemProblem::Empty`, `VaultFrontmatter::kind`, `VaultFrontmatter`'s `declared_kind` and `plugin` fields, and `DiscoveredComponent::folder_plugin` are removed.

Changed signatures:

- `resolve_kind(hints)`, `load_component` takes a cascade and namespace, and `VaultFrontmatter::take_from` takes the kind.
- `HarnessProfile::split_pack(packs, relative)` is an associated function. `HarnessTranslator::published_name` takes the plugin, and `render_body` takes the skill's folder.
- `Closure::hash` no longer takes an owner, which is now the `owner` field. `override_states` takes the kind.

The wasm wrapper over this crate, `@osdlc/tradecraft-wasm`, is now `"private": true` and is no longer set up to publish, so 0.2.0 stays its last npm version. In the source, `UnitStatus` is `PublishUnitStatus`, and a bad namespace throws `InvalidNamespaceError`.

### Added

- `load_marketplace(&vfs, start, home)` finds the root file, folds in a workspace above it, and returns the merged settings and build inputs in one call. Its pieces are public: `find_root_file`, `workspace_above`, `marketplace_inputs`, `parse_root_file` and `root_file_config`.
- The `keys` module holds one key table per kind, and `keys::field_reference()` generates the [field reference](https://github.com/osdlc/skillwright/blob/tradecraft-core/v0.44.0/docs/reference/fields.md).
- `SourceView`, `source_view` and `VaultScope` read the vault as a run does, hiding generated output. `effective_config`, `effective_configs`, `vault_config`, `check_packs`, `packs_of` and `built_paths_for` are also new.
- The `patterns` module reads `build.patterns` and `build.exclude`, which are experimental.
- `RootFile` and `BuildSettings` are the types of a parsed `skillwright.md`. `MarketplaceConfig` and `PluginOverrides` gain `targets`, `repository`, `homepage` and `blocks`.
- `ReasonToken` gains `ExcludedNote`. The status enums gain `as_str`, and `CoreError` gains `variant_name` and `reason_token`.
- `StatusRequest`, `status_of`, `PublishState`, `PublishUnitStatus`, `LinkIndex` and `FieldValue` are re-exported at the crate root.
- Plain `[[skill]]` mentions are recorded with a `mention` flag on `Call` and `CallSite`.

### Fixed

- An agent's `tools` and a numeric `temperature` reach the published agent, and its other keys are carried. `tools` was dropped, so a Claude Code agent could use every tool.
- A command's `disable-model-invocation`, `user-invocable`, `context` and foreign keys are carried.
- `adopt` keeps a link to a folder as written, handles escaped brackets in link labels, keeps the bundle path in wikilinks, and refuses a bundle resource named `<name>.md` as `path-collision`.
- `[[v1.2 notes]]` resolves to `v1.2 notes.md`.
- A configured plugin or marketplace description is kept, and it satisfies `require_descriptions`.
- Plugin-prefixed skill names that meet across plugins are refused as `path-collision`.
- A vault or asset source inside a tree the build replaces is refused as `unsafe-root`, and a build replaces only the paths it owns.
- A rendered path that switches between file and directory rebuilds cleanly.
- Source hashes use the note a file was rendered from as owner, so a resource's own frontmatter edit is seen.
- An unresolved embed with a size alias shows the target name, and a stray `[[` no longer takes in the lines after it.
- Code mentions link relative to the linking file.
- Authoring plans read bundle shape the way discovery does and match `.MD` the same as `.md`.
- A hook timeout too large for milliseconds is refused, and version bumps no longer overflow.
- A `SKILL.md` at the scan root opens, a skill named `test`, `build` or `dist` is listed, and a blank skill name falls back to the folder name.
- The version-bump advice is given only for plugins.

## [0.43.0](https://github.com/osdlc/skillwright/compare/tradecraft-core/v0.42.0...tradecraft-core/v0.43.0) - 2026-11-05

### Other

- find what the walk cannot read first, then load the vault once
- edit no file in a dot-named folder; skip a folder it cannot list
- one ApplyError for the walk failure; one pass for a blank line
- a blank line with spaces or a CRLF ends a Markdown link search
- pass the walk's io error through; the refusing walk returns paths
- Discovery and compile use the walk and the link index, not a Corpus
- Review fixes: a Markdown link beside a [ is not a leftover; one link index per load
- one leftover merge, a body_start helper, skill_index takes the link index, no frontmatter index; fix doc links
- Load reads the skills source through markdown-corpus; refuse a [[ that is not a link
- move the prototype into a frozen alpha corpus
- fixes from review of the scanner move
- build wiki tokens in one place; code_token needs no opener argument
- Test link resolution at the edges resolve_strict changed
- Find links with markdown-link's scanner; calls stay a layer on top

### Changed

- The compile pass finds `[[links]]` and `![[embeds]]` with `markdown-link`'s scanner, the one the other osdlc tools use. `/[[calls]]`, `\/[[x]]` escapes and hand-written calls stay this crate's, read on top of it. What changes is at the edges: a wikilink holding a `[` or `]`, or code, is no longer a link and is left as written; `[[]]` and `[[ |alias]]` are no longer links to the note itself; a `\|` escaping the pipe in a table cell is no longer part of the target; and a block fragment `#^ id` is read as `#^id`.
- The skills source walk (discovery and `load`) leaves out `.obsidian` and `node_modules` wherever they sit, as it already left out `.trash`: a declared note under one is no longer refused as a stray, and a bundle no longer lists their files. `model::read_notes` also returns the vault's link index, built from the same walk when the skills root is the vault, else from one walk of the vault, and `CompileSource::links` lets compile use it instead of walking the vault once per component. `compile::vault_walk_options` is the walk compile indexes a vault with. `compile::vault_walk_errors` lists what that walk cannot read, for a caller that skips it.
- A `[[` that is not a link is refused as `unresolved-syntax`, like any other Obsidian syntax left in a compiled file: a `[` or `]` before the closing `]]`, a target that is only code or has code in it, an empty target (`[[]]`, `[[ |alias]]`), or no closing `]]`. Such notes used to compile with the brackets left as written. A Markdown link beside a `[`, such as `[[WIP] Fix](url)`, is not refused. Code, HTML comments and a resource's frontmatter are not checked.
- vfs-seam 0.2, vfs-walk 0.2, markdown-vault-fs 0.5, markdown-link 0.8, markdown-corpus 0.2 and markdown-relink 0.3. File times are nanoseconds.

## [0.42.0](https://github.com/osdlc/skillwright/compare/tradecraft-core/v0.41.0...tradecraft-core/v0.42.0) - 2026-11-05

### Other

- Rename markdown-vault to markdown-corpus; bump osdlc#618 pin
- Plain walks through vfs-walk; bump osdlc#618 pin
- resolve at load; the pass count is a detail
- link resolution at the edges follows markdown-link
- Resolve links, select fragments and write hrefs with the shared crates
- Honour the vault's excluded files
- Walk the vault with markdown-vault-fs
- Vfs from vfs-seam; bump the osdlc crates to osdlc/osdlc#618
- Expose the marketplace model: loadMarketplace in wasm, list --model in the CLI
- Publish renders through the model and translators; cursor and pi take packs
- Marketplace model and harness translators; build renders through them
- Harness profile: one row of facts per harness
- Fix the review findings on the comment sweep
- make doc comments match the code

### Added

- `harness`: one `HarnessProfile` row per harness, and a `HarnessTranslator` per harness that writes the marketplace model as a `FileTree`.
- `model`: the marketplace model and its two-pass `load` — every note read, then every link and call resolved — which `build` and `check` now render from.

### Changed

- `build` and `check` apply a note's `sw-overrides` block for each harness to that harness's tree, as `publish` does. A block naming an unknown target, or a key its harness does not read, now refuses the build (`unknown-target`, `unknown-override-key`).
- `publish` of a plugin note to `cursor` or `pi` writes that harness's pack — its manifest and its members' skills, in the shape `build` writes — instead of refusing `no-renderer`. Every publish renders through the same model and translator as `build`.
- Discovery leaves out the files the vault excludes in Obsidian ("Excluded files", `userIgnoreFilters` in `.obsidian/app.json`), so `build`, `check` and a whole-vault `publish` no longer publish them. Links may still point at them.
- Links resolve through `markdown-link`'s `LinkIndex::resolve_strict`, so they resolve as other osdlc tools resolve them. An exact path, a path from the linking note's folder and a unique name resolve as before, and an ambiguous name is still refused (`link-ambiguous`), its candidates now listed shortest path first. What changes is at the edges: a partial path (`[[sub/x]]`) names the one file whose path ends with it, where it used to fall back to the bare name and be refused when that name was shared; a `./` target is relative to the linking note (a `../` one is still refused, `unsafe-path`); a percent-encoded target is decoded.
- Asset, script, bundle and adoption-source walks go through the new `vfs-walk` crate, with no vault rules; the vault walks go through `markdown-vault-fs`. The files each walk finds are unchanged.
- `vfs` re-exports the `vfs-seam` crate, the file-access trait the osdlc vault crates share. The paths are unchanged. A `Vfs` method now returns `VfsError`, which `?` turns into `CoreError::Io` with the same message, and `Vfs` has a required `metadata` method (size, modification time, directory, symlink).

## [0.41.0](https://github.com/osdlc/skillwright/compare/tradecraft-core/v0.40.2...tradecraft-core/v0.41.0) - 2026-10-30

### Fixed

- machine-file path resolution, refusal and help wording; drop old project name from changelogs

### Removed

- handling of old names; fix config and doc errors

## [0.40.2](https://github.com/osdlc/skillwright/compare/tradecraft-core/v0.40.1...tradecraft-core/v0.40.2) - 2026-10-30

### Other

- tradecraft-core now lives in https://github.com/osdlc/skillwright. Repository and documentation links point there.

## [0.40.1](https://github.com/osdlc/osdlc/compare/tradecraft-core/v0.40.0...tradecraft-core/v0.40.1) - 2026-10-29

### Other

- updated the following local packages: agent-adapt, markdown-link

## [0.40.0] - 2026-10-28

### Changed

- The CLI is now `skillwright`, and the names this library writes follow it. The default frontmatter namespace is `sw` (`sw-kind`, `sw-targets`, …), the manifest is `.skillwright/manifest.json`, a build stages in `.skillwright-stage`, generated files carry `Generated by skillwright`, and the project file is `skillwright.yaml`.
- Under the default namespace, a note with a `tc-*` key or a `tc:` block is refused (`old-key`), naming the key and its `sw-*` spelling, or saying to set `namespace: tc`.

### Migration

- A host that relied on `Namespace::default()` reading `tc-*` keys keeps that behaviour with `Namespace::new("tc")`.
- A destination with `.tradecraft/manifest.json` and no current manifest is refused with a rename hint, as the earlier folder already was. `Unusable::OldFolder` now carries the old folder's name.
- A marketplace tree built with the `Generated by tradecraft` marker is still recognised as this library's own, so a build replaces it.

## [0.39.2](https://github.com/osdlc/osdlc/compare/tradecraft-core/v0.39.1...tradecraft-core/v0.39.2) - 2026-10-28

### Other

- updated the following local packages: yaml-splice, markdown-link

## [0.39.1] - Unreleased

First public release. A file-canonical model of a vault of markdown skill notes: scan it, validate
each note against the Agent Skills specification, and publish or build it into each coding-agent
tool's own on-disk format, through a filesystem seam that also runs under WebAssembly. The crate is
`tradecraft-core`, imported as `tradecraft_core`.

### Names

- `Namespace::default()` is `tc`, so the claimed and stripped frontmatter keys are `tc-*`.
- `MANIFEST_RELATIVE_PATH` is `.tradecraft/manifest.json`. A build stages in `.tradecraft-stage`,
  and generated files carry the `Generated by tradecraft` marker.
- `PROJECT_FILE` is `tradecraft.yaml`.

### Errors for files from before the rename

- `Unusable::OldFolder`: the destination has the earlier folder's `manifest.json` and no
  `.tradecraft/manifest.json`. The old manifest is not read. A publish refuses with one
  `bad-manifest` row that says to rename the folder to `.tradecraft/`, and `status` reports
  `unknown` with the same reason.
- A marketplace tree built under the earlier project's generated-file marker is refused with
  `old-tree`, which says to delete the tree and rebuild.

### Added

- `authoring::draft_refusal_for`. A hook note that cannot be rendered is refused when it is written.

### Dependencies

- The dependencies are the published crates: `agent-adapt` 0.19, `markdown-link` 0.5,
  `path-guard` 0.3, `yaml-splice` 0.3 and `frontmatter-lossless` 1.1. `yaml-splice`'s `Value`
  appears in `authoring::FieldEdit`.
- `assert_destination_allowed` and the other destination checks refuse an empty or relative path
  with `refused: unsafe-path`. `path-guard` 0.3 compares absolute paths only.
- The crate turns on `serde_json`'s `preserve_order` feature. A parsed hooks file or manifest keeps
  its source order, so reports list entries as the author wrote them.

### Known limitations

- Windows is not supported yet. The crate builds there, but its tests are not run on Windows.
