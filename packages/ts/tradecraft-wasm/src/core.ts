/**
 * The seam to the Rust core — `tradecraft-core`, reached through the
 * WASM binding that `pnpm run build:wasm` writes to `wasm/`
 * (`packages/rust/tradecraft-wasm`).
 *
 * This is the ONLY file in the package that imports the wasm glue. Nothing
 * above this module may decide anything an export
 * involves — parsing a skill note, resolving links, choosing a target's
 * layout, deciding whether a destination file may be overwritten all live
 * in the core. This file's job is the JSON boundary and turning a thrown
 * `CoreError` into a typed one, exactly mirroring what
 * `@osdlc/yaml-splice`'s `index.ts` does for `yaml-splice-wasm`.
 *
 * The binding exports six functions (`packages/rust/tradecraft-wasm/src/lib.rs`):
 *
 * | Export | Reaches | Serves |
 * |---|---|---|
 * | `discoverComponents` | the vault scan | `build`, `check`, and whatever needs a roster |
 * | `buildVault` | every marketplace, planned then written all or none | `build`, and `check` through `mode` |
 * | `exportSkills` | the marketplace renderer, one marketplace | `build`, and `check` through `mode` |
 * | `loadMarketplace` | the marketplace, loaded and resolved, in the binding's own narrow shape | a host that wants structure rather than files |
 * | `knownTargets` | the target vocabulary | every verb that takes a target name |
 * | `parseRootFile` | the `skillwright.md` reader | a host reading the root file |
 *
 * A mode is a key on a request, never an export of its own — `check` is
 * `exportSkills` with `mode: "check"`. This file never predicts what it would
 * do: the core runs the identical path over a capturing overlay and hands back
 * the report the write would have produced.
 *
 * No request carries configuration: the core reads `skillwright.md` through
 * the vfs, and a request's `harnesses` only narrows its `build.harnesses`.
 */

import {
  buildVault as buildVaultWasm,
  discoverComponents as discoverComponentsWasm,
  exportSkills as exportSkillsWasm,
  loadMarketplace as loadMarketplaceWasm,
  knownTargets as knownTargetsWasm,
  parseRootFile as parseRootFileWasm,
} from '#wasm'

import type { Vfs } from './node-vfs.ts'

import type {
  DiscoveredComponent as GeneratedDiscoveredComponent,
  DiscoverRequest,
  DiscoveryReport as GeneratedDiscoveryReport,
  ExportReport,
  ExportRequest as GeneratedExportRequest,
  LoadRequest as GeneratedLoadRequest,
  Marketplace,
  RootFile,
  Target,
  VaultReport,
  VaultRequest as GeneratedVaultRequest,
} from './core.generated.ts'

/**
 * Every request and report shape, generated from the Rust source by
 * `tradecraft-typegen` and re-exported here so this file stays the package's
 * one door to the core.
 *
 * Re-exported, never re-declared, except for the `harnesses` of the three
 * requests and discovery's `targets`, below, typed as {@link Target}s.
 * Everything else here is exactly what
 * `tradecraft-typegen` read off the Rust source.
 * Regenerate with `cargo run -p tradecraft-typegen`; `cargo test -p tradecraft-typegen`
 * fails when the committed file is behind the Rust source, and a test in this
 * package fails when src/core.generated.ts differs from the crate's copy.
 */
export type {
  Asset,
  ContainerIdentity,
  DiscoverRequest,
  DiscoveryWarning,
  EntityKind,
  ExportMode,
  ExportReport,
  ExportUnit,
  Marketplace,
  MarketplaceRefusal,
  MarketplaceReport,
  Plugin,
  Component,
  RootFile,
  SharedDefaults,
  BuildSettings,
  SkillId,
  SkippedNote,
  Target,
  UnitStatus,
  VaultReport,
  VersionBumpSuggestion,
  WasmExportsWithoutJson,
  WasmRequests,
  WasmResponses,
} from './core.generated.ts'

/** The generated `DiscoveredComponent`, with `targets` typed as {@link Target}s. */
export type DiscoveredComponent = Omit<GeneratedDiscoveredComponent, 'targets'> & {
  targets: Target[] | null
}

/** The generated `DiscoveryReport`, with its components' `targets` typed. */
export type DiscoveryReport = Omit<GeneratedDiscoveryReport, 'components'> & {
  components: DiscoveredComponent[]
}

/** The generated `ExportRequest`, with `harnesses` typed as {@link Target}s. */
export type ExportRequest = Omit<GeneratedExportRequest, 'harnesses'> & {
  harnesses?: Target[]
}

/** The generated `VaultRequest`, with `harnesses` typed as {@link Target}s. */
export type VaultRequest = Omit<GeneratedVaultRequest, 'harnesses'> & {
  harnesses?: Target[]
}

/** The generated `LoadRequest`, with `harnesses` typed as {@link Target}s. */
export type LoadRequest = Omit<GeneratedLoadRequest, 'harnesses'> & {
  harnesses?: Target[]
}

/**
 * The `CoreError` variant names, exhaustively — see
 * `packages/rust/tradecraft-core/src/error.rs`. A thrown wasm `Error`'s `.name` is
 * set to one of these by `packages/rust/tradecraft-wasm/src/lib.rs::variant_name`.
 */
export type CoreErrorName =
  | 'RootNotFound'
  | 'PathEscapesRoot'
  | 'Io'
  | 'Refused'
  | 'Projection'
  /**
   * The marketplace commit failed part-way through its root swaps and rolled
   * back. Its own name, not a `Refused`, because a caller has to be able to
   * tell "nothing happened" from "something happened and was undone".
   */
  | 'SwapFailed'

const CORE_ERROR_NAMES: ReadonlySet<string> = new Set<CoreErrorName>([
  'RootNotFound',
  'PathEscapesRoot',
  'Io',
  'Refused',
  'Projection',
  'SwapFailed',
])

/**
 * A typed refusal or failure from the Rust core, thrown by the exports
 * in place of the raw bindgen error. `coreName` is `"Unknown"` for a boundary
 * failure that never reached the core (bad JSON in, unserializable report
 * out) — see `packages/rust/tradecraft-wasm/src/lib.rs`'s `throwable`.
 */
export class CoreError extends Error {
  readonly coreName: CoreErrorName | 'Unknown'

  constructor(message: string, coreName: CoreErrorName | 'Unknown') {
    super(message)
    this.name = 'CoreError'
    this.coreName = coreName
  }
}

function toCoreError(raw: unknown): CoreError {
  if (raw instanceof Error) {
    const coreName: CoreErrorName | 'Unknown' = CORE_ERROR_NAMES.has(raw.name)
      ? (raw.name as CoreErrorName)
      : 'Unknown'
    return new CoreError(raw.message, coreName)
  }
  return new CoreError(String(raw), 'Unknown')
}

/**
 * Every target name this build knows — `tradecraft_core::harness::KNOWN_TARGETS`,
 * verbatim and in report order. A name typed on a command line and a name in
 * a note's `targets` are checked against the same values.
 */
export function knownTargets(): Target[] {
  return callCore<Target[]>(() => knownTargetsWasm())
}

// --- discovery ---------------------------------------------------------------

/**
 * Every note under the vault's `marketplaces/`, vetted and sorted
 * by name, and every note the scan could not read.
 *
 * A bad note stops itself, not the scan: one half-written draft in a shared
 * vault must not block everybody else's build. What still throws is what is
 * not a fact about one note — a vault outside the layout (every problem in
 * one message), a duplicate name, a symlink. A caller that ignores `skipped` is claiming the vault holds
 * nothing but what it listed, so every one of them reports it.
 */
export function discoverComponents(vfs: Vfs, request: DiscoverRequest): DiscoveryReport {
  return callCore<DiscoveryReport>(() => discoverComponentsWasm(vfs, JSON.stringify(request)))
}

// --- the marketplace ---------------------------------------------------------

/**
 * Render one marketplace of the vault into `<vault_root>/dist/<m>/`, one tree
 * per harness, written through `vfs` — and, per `mode`, either swap the
 * drifted roots into place or report the drift and write nothing. `marketplace`
 * names it, and is required when the vault holds more than one.
 *
 * `build` and `check` are this one call with one key different, because in the
 * core they are one render and one comparison. That is the whole reason their
 * verdicts cannot disagree, and it is why there is no second function here to
 * keep pointed at the first.
 *
 * Throws {@link CoreError} for everything an export refuses — no
 * `marketplace` on a vault of several, an empty marketplace, a failed swap. There is no per-unit refusal to look for
 * inside the report: an export's rails all fire before anything is staged.
 */
export function exportSkills(vfs: Vfs, request: ExportRequest): ExportReport {
  return callCore<ExportReport>(() => exportSkillsWasm(vfs, JSON.stringify(request)))
}

/**
 * Build or check every marketplace of the vault, or the one `marketplace`
 * names, each into its `<vault_root>/dist/<m>/`. Every marketplace is planned
 * before any is written, and a build writes all of them or none: a failed
 * swap rolls back every marketplace already swapped.
 *
 * Throws {@link CoreError} for the first marketplace's refusal, with nothing
 * written, and for a failed swap.
 */
export function buildVault(vfs: Vfs, request: VaultRequest): VaultReport {
  return callCore<VaultReport>(() => buildVaultWasm(vfs, JSON.stringify(request)))
}

/**
 * Load one marketplace of the vault and write nothing: every plugin and its
 * members, each with its description, the skills it calls and the files it
 * carries. `marketplace` names it, and is required when the vault holds more
 * than one.
 *
 * The structure of the model `exportSkills` translates into files, so a host
 * that wants a vault's structure reads it here rather than parsing a built
 * tree. The rendered bodies do not cross.
 *
 * Throws {@link CoreError} for everything the load refuses — a vault outside
 * the layout, no `marketplace` on a vault of several, an unreadable member
 * note, a call that names no skill, an empty marketplace.
 */
export function loadMarketplace(vfs: Vfs, request: LoadRequest): Marketplace {
  return callCore<Marketplace>(() => loadMarketplaceWasm(vfs, JSON.stringify(request)))
}

// --- the root file -----------------------------------------------------------

/**
 * Read a `skillwright.md`'s text, with the reader the CLI uses. Finding the
 * file is the caller's; an unknown key comes back in `warnings`. A build
 * reads the file itself; this is for a host that shows it.
 *
 * @throws {CoreError} on frontmatter that is not a mapping, or a known key of
 * the wrong shape.
 */
export function parseRootFile(text: string): RootFile {
  return callCore<RootFile>(() => parseRootFileWasm(text))
}

/**
 * One call into the binding: run it, parse the report it hands back, and turn
 * whatever it threw into a {@link CoreError}.
 *
 * The shape every export above shares, said once. Nothing decided here — the
 * generic is the report's type, and the only judgement in the function is that
 * a thrown value is an error rather than a result.
 */
function callCore<T>(call: () => string): T {
  let resultJson: string
  try {
    resultJson = call()
  } catch (raw) {
    throw toCoreError(raw)
  }
  return JSON.parse(resultJson) as T
}
