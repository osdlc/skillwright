/**
 * Barrel: re-exports only, no logic of its own. The programmatic surface of
 * `@osdlc/tradecraft-wasm` — the core plus the Node vfs adapter. No CLI entry
 * point; the CLI is the separate `skillwright` Rust binary.
 */

export {
  buildVault,
  CoreError,
  discoverComponents,
  exportSkills,
  knownTargets,
  loadMarketplace,
  parseRootFile,
} from './core.ts'
export type {
  CoreErrorName,
  DiscoveredComponent,
  DiscoverRequest,
  DiscoveryReport,
  EntityKind,
  Component,
  LoadRequest,
  Marketplace,
  MarketplaceReport,
  Plugin,
  Segment,
  ExportMode,
  ExportReport,
  ExportRequest,
  ExportUnit,
  ContainerIdentity,
  RootFile,
  SharedDefaults,
  BuildSettings,
  SkippedNote,
  Target,
  UnitStatus,
  VaultReport,
  VaultRequest,
  VersionBumpSuggestion,
} from './core.ts'

export { toCorePath, toHostPath } from './host-path.ts'
export { createNodeVfs } from './node-vfs.ts'
export type { BaseVfs, LinkMethods, NodeFs, NodeVfsOptions, Vfs, VfsStat } from './node-vfs.ts'
