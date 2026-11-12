# @osdlc/tradecraft-wasm

## Unreleased

### Breaking

- The core behind this package is skillwright v2: one fixed vault layout, built into `<root>/dist/<m>/`. skillwright's changelog lists the vault and build rules that changed.
- `publishSkill`, `adoptSkills`, `statusOf`, `publishableKinds`, `validateNamespace` and `canonicalTarget` are removed, as are `InvalidNamespaceError` and their request and report types (`PublishRequest`, `PublishReport`, `AdoptRequest`, `AdoptReport`, `StatusRequest`, `PublishUnitStatus`, `PublishedFile`, `PublishReason`, `ReasonToken` and the types they use). Use `buildVault`.
- `Target` is closed to `'claude' | 'codex' | 'cursor' | 'pi'`, with no open `string` tail. `knownTargets()` returns `Target[]`, and `VaultRequest` and `LoadRequest` type `harnesses` as `Target[]`.
- `exportSkills` and `ExportRequest` are removed. `buildVault` with `marketplace` set builds that one marketplace, and its report is the one entry in `marketplaces`. A build writes `<vault_root>/dist/<m>/` through the given `Vfs`; there is no output-root field.
- `loadMarketplace` takes the marketplace's folder name as `marketplace`, which is required on a vault of several: without it the call throws `Refused`, naming them.
- `rootFileConfig` is removed, with the `MarketplaceConfig` and `PluginOverrides` types. A `VaultRequest` or `LoadRequest` carries no configuration: the core reads the root file's shared defaults and `build.harnesses` from `skillwright.md` through the `Vfs`. A request that names `config` or `defaults` throws. `harnesses` replaces the root file's list for that call; the root file's list is only the default. A marketplace's name, `owner` and every plugin field come only from the vault's notes.
- `buildVault(vfs, request)` is added, with `VaultRequest`, `VaultReport` and `MarketplaceReport`: every marketplace planned, then all written or none. A check returns every marketplace's report, and lists each marketplace that refused in `refused` (`MarketplaceRefusal`) instead of throwing. A check of one marketplace may set `compare_with` to compare with another built folder in place of `dist/<m>/`.
- `LoadRequest` and `DiscoverRequest` lose `skills_root`, `out_root` and `namespace`; `LoadRequest` also loses `assets` and `hooks`. A request with an unknown key throws.
- `RootFile` is `{ defaults, build }`: `marketplace` and `workspace` are gone, with `MarketplaceFields`. `BuildSettings` loses `out`, `namespace`, `launcher_path`, `destinations`, `scan_locations`, `assets`, `hooks`, `exclude` and `patterns`. `DestinationSetting`, `AssetSetting`, `HookSetting`, `FolderPatterns`, `AssetSpec` and `HookFileSpec` are removed.
- `EntityKind` loses `'hook'`. `DiscoveredComponent` loses `kind_source`, `folder_kind` and `hook`, and `KindSource` and `DiscoveredHook` are removed.
- `SkillId` gains `kind`: a call names a skill or a command. `SkillId.plugin` is `string`, not `string | null`.
- A Markdown body or embedded file that holds a noncharacter U+FDD0 to U+FDD2 throws `Refused` (`noncharacter`): the build uses them as internal marks. Private-use text, such as icon-font glyphs, now ships unchanged.
- `loadMarketplace` returns a narrow shape, not the core's model. `Marketplace` loses `folder`. A `Plugin`'s `references` are paths. A `Component` loses `content`, `resolved` and `extensions`, and has `description`, `blocks`, `calls` (the `SkillId` of each skill or command its body calls) and `resources` (paths) instead. `Segment`, `Resolved`, `Body`, `SkillContent` and the types they reach are no longer exported. `bare_on` is typed as `Target[]`; `Target` is now generated from the Rust source.
- `targets` is removed: a `Component`, a `DiscoveredComponent` and `SharedDefaults` have no `targets` field, and a note or root file that sets it throws `Refused`. Pick harnesses with `build.harnesses` or a request's `harnesses`.
- An unset identity field is absent instead of `null`: every component's `identity` in `discoverComponents`, so a skill's is `{}`. `parseRootFile` already left unset fields out. Test for `undefined`, not `null`. ([#75](https://github.com/osdlc/skillwright/issues/75))
- `ContainerIdentity`'s fields are optional (`display_name?: string`, not `display_name: string | null`). The JSON stays flat: no `identity` key appears. ([#75](https://github.com/osdlc/skillwright/issues/75))

- `Target` is renamed `Harness`. `ExportUnit`'s `root` is typed as `Harness`, not `string`, and `knownTargets()` is typed as `Harness[]` in `WasmResponses` too.

### Changed

- A `Refused` `CoreError` carries its reason token as `token`. `MarketplaceRefusal` has `token` and `detail` in place of `error`.
- `CoreErrorName` loses `RootNotFound`, which the core never threw.
- A note that is not UTF-8 or whose frontmatter does not parse throws `Refused` (`unreadable-note`), not `Io`.
- `UnitStatus` is `Same` or `Differs`, and `DiscoveredComponent` carries `Position`; the TS types are regenerated. A unit's `status` is `'same'` or `'differs'` in place of `'ok'`, `'drift'`, `'written'` and `'unchanged'`; the report's `mode` gives the verb. `ExportUnit`'s not-carried `kind` is typed as `EntityKind`. A `DiscoveredComponent`'s `position` (`at`, `marketplace`, and `plugin`, `kind` and `name` where the note has them) replaces `name`, `kind`, `plugin` and `marketplace`. `Position` is exported.

## 0.2.0

### Minor Changes

- d05e4ca: The core now writes the names of the `skillwright` CLI. The default frontmatter namespace is `sw` (`sw-kind`, `sw-targets`, …), the manifest is `.skillwright/manifest.json`, and generated files carry `Generated by skillwright`. Under the default namespace a note with a `tc-*` key is refused with its `sw-*` spelling, and a destination with only `.tradecraft/manifest.json` is refused with a rename hint.

## 0.1.0

### Minor Changes

- c9c911a: First release. The Tradecraft core, built to WebAssembly, for Node.js and Bun. It publishes a skill note to an agent's destination (`publishSkill`), reports what a publish would do (`statusOf`), scans a vault (`discoverComponents`), renders or checks a marketplace (`exportSkills`) and adopts published skills back into a vault (`adoptSkills`). `knownTargets()`, `canonicalTarget()`, `publishableKinds()` and `validateNamespace()` expose the core's vocabularies. The core decides everything: this package passes JSON across the boundary and turns a thrown core error into `CoreError`. `createNodeVfs()` gives the filesystem the core is handed, backed by `node:fs`, with the Windows path conversion. The request and report types are generated from the Rust source.
