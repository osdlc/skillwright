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
 * The binding exports seven functions (`packages/rust/tradecraft-wasm/src/lib.rs`):
 *
 * | Export | Reaches | Serves |
 * |---|---|---|
 * | `discoverComponents` | the vault scan | `build`, `check`, and whatever needs a roster |
 * | `exportSkills` | the marketplace renderer | `build`, and `check` through `mode` |
 * | `loadMarketplace` | the marketplace model, loaded and resolved | a host that wants structure rather than files |
 * | `knownTargets` | the target vocabulary | every verb that takes a target name |
 * | `parseRootFile` | the `skillwright.md` reader | a host reading the root file |
 * | `rootFileConfig` | the root file's build config | a host building from the root file |
 *
 * A mode is a key on a request, never an export of its own — `check` is
 * `exportSkills` with `mode: "check"`. This file never predicts what it would
 * do: the core runs the identical path over a capturing overlay and hands back
 * the report the write would have produced.
 */

import {
  discoverComponents as discoverComponentsWasm,
  exportSkills as exportSkillsWasm,
  loadMarketplace as loadMarketplaceWasm,
  knownTargets as knownTargetsWasm,
  parseRootFile as parseRootFileWasm,
  rootFileConfig as rootFileConfigWasm,
} from '#wasm'

import type { Vfs } from './node-vfs.ts'

import type {
  DiscoverRequest,
  DiscoveryReport,
  ExportReport,
  ExportRequest,
  LoadRequest,
  Marketplace,
  MarketplaceConfig,
  RootFile,
} from './core.generated.ts'

/**
 * Every request and report shape, generated from the Rust source by
 * `tradecraft-typegen` and re-exported here so this file stays the package's
 * one door to the core.
 *
 * Re-exported, never re-declared — except for `Target`, widened below.
 * Everything else here is exactly what `tradecraft-typegen` read off
 * the Rust source.
 * Regenerate with `cargo run -p tradecraft-typegen`; `cargo test -p tradecraft-typegen`
 * fails when the committed file is behind the Rust source, and a test in this
 * package fails when src/core.generated.ts differs from the crate's copy.
 */
export type {
  ContainerIdentity,
  DiscoveredComponent,
  DiscoverRequest,
  DiscoveryReport,
  EntityKind,
  ExportMode,
  ExportReport,
  ExportRequest,
  ExportUnit,
  KindSource,
  LoadRequest,
  Marketplace,
  MarketplaceConfig,
  Plugin,
  Component,
  Segment,
  PluginOverrides,
  RootFile,
  SharedDefaults,
  BuildSettings,
  SkippedNote,
  UnitStatus,
  VersionBumpSuggestion,
  WasmExportsWithoutJson,
  WasmRequests,
  WasmResponses,
} from './core.generated.ts'

/**
 * The four harness names the core accepts, for completion. Mirrors
 * `KNOWN_TARGETS` in `tradecraft_core::publish`; hand-maintained, so update
 * both together.
 *
 * The core types a target as a plain `String`, so a generated declaration
 * is `string`. The `(string & {})` tail keeps the union
 * open so an editor still completes the known names; the core rejects a name it does not know.
 */
export type Target = 'claude' | 'codex' | 'cursor' | 'pi' | (string & {})

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
 * verbatim and in report order.
 *
 * The one list. A `--target` typed on the command line and a name in a note's
 * targets key are checked against the same values, so a target added to the
 * core is accepted by both without a second edit here.
 */
export function knownTargets(): string[] {
  return callCore<string[]>(() => knownTargetsWasm())
}

// --- discovery ---------------------------------------------------------------

/**
 * Every publishable note under the vault's `marketplaces/`, vetted and sorted
 * by name, and every note the scan could not read.
 *
 * A bad note stops itself, not the scan: one half-written draft in a shared
 * vault must not block everybody else's publish. What still throws is what is
 * not a fact about one note — a vault outside the layout (every problem in
 * one message), a duplicate name, a symlink. A caller that ignores `skipped` is claiming the vault holds
 * nothing but what it listed, so every one of them reports it.
 */
export function discoverComponents(vfs: Vfs, request: DiscoverRequest): DiscoveryReport {
  return callCore<DiscoveryReport>(() => discoverComponentsWasm(vfs, JSON.stringify(request)))
}

// --- the marketplace ---------------------------------------------------------

/**
 * Render the vault into one marketplace tree per harness — and, per `mode`, either
 * swap the drifted roots into place or report the drift and write nothing.
 *
 * `build` and `check` are this one call with one key different, because in the
 * core they are one render and one comparison. That is the whole reason their
 * verdicts cannot disagree, and it is why there is no second function here to
 * keep pointed at the first.
 *
 * Throws {@link CoreError} for everything an export refuses — the rail, an
 * empty marketplace, a failed swap. There is no per-unit refusal to look for
 * inside the report: an export's rails all fire before anything is staged.
 */
export function exportSkills(vfs: Vfs, request: ExportRequest): ExportReport {
  return callCore<ExportReport>(() => exportSkillsWasm(vfs, JSON.stringify(request)))
}

/**
 * Load the vault into the marketplace model and write nothing: every plugin
 * and its members, each body as segments with its links and calls resolved.
 *
 * The same model `exportSkills` translates into files, so a host that wants
 * a vault's structure reads it here rather than parsing a built tree.
 *
 * Throws {@link CoreError} for everything the load refuses — a vault outside
 * the layout, an unreadable member note, a call that names no skill, an override key a requested
 * harness does not read, an empty marketplace.
 */
export function loadMarketplace(vfs: Vfs, request: LoadRequest): Marketplace {
  return callCore<Marketplace>(() => loadMarketplaceWasm(vfs, JSON.stringify(request)))
}

// --- the root file -----------------------------------------------------------

/**
 * Read a `skillwright.md`'s text, with the reader the CLI uses. Finding the
 * file is the caller's; an unknown key comes back in `warnings`.
 *
 * @throws {CoreError} on frontmatter that is not a mapping, or a known key of
 * the wrong shape.
 */
export function parseRootFile(text: string): RootFile {
  return callCore<RootFile>(() => parseRootFileWasm(text))
}

/**
 * The {@link MarketplaceConfig} a `skillwright.md`'s text describes, ready for
 * an {@link ExportRequest}: the shared defaults only. It names no
 * marketplace; each marketplace's note `<m>.md` does.
 *
 * @throws {CoreError} as {@link parseRootFile}.
 */
export function rootFileConfig(text: string): MarketplaceConfig {
  try {
    return rootFileConfigWasm(text) as MarketplaceConfig
  } catch (raw) {
    throw toCoreError(raw)
  }
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
