/**
 * Barrel: re-exports only, no logic of its own. The programmatic surface of
 * `@osdlc/tradecraft-wasm` — the core plus the Node vfs adapter. No CLI entry
 * point; the CLI is the separate `skillwright` Rust binary.
 */

export {
  buildVault,
  CoreError,
  discoverComponents,
  knownTargets,
  loadMarketplace,
  parseRootFile,
} from './core.ts'
export type {
  Asset,
  CoreErrorName,
  DiscoveredComponent,
  DiscoverRequest,
  DiscoveryReport,
  DiscoveryWarning,
  EntityKind,
  Component,
  LoadRequest,
  Marketplace,
  MarketplaceRefusal,
  MarketplaceReport,
  Plugin,
  ExportMode,
  ExportReport,
  ExportUnit,
  ContainerIdentity,
  RootFile,
  SharedDefaults,
  BuildSettings,
  BumpSettings,
  PatchScheme,
  SkillId,
  SkippedNote,
  StaleKind,
  Harness,
  UnitStatus,
  VaultReport,
  VaultRequest,
  VersionBumpSuggestion,
} from './core.ts'

export { toCorePath, toHostPath } from './host-path.ts'
export { createNodeVfs } from './node-vfs.ts'
export type { BaseVfs, LinkMethods, NodeFs, NodeVfsOptions, Vfs, VfsStat } from './node-vfs.ts'
