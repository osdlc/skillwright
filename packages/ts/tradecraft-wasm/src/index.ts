/**
 * Barrel: re-exports only, no logic of its own. The programmatic surface of
 * `@osdlc/tradecraft-wasm` — the core plus the Node vfs adapter. No CLI entry
 * point; the CLI is the separate `skillwright` Rust binary.
 */

export {
  canonicalTarget,
  CoreError,
  discoverComponents,
  exportSkills,
  knownTargets,
  loadMarketplace,
  parseRootFile,
  rootFileConfig,
} from './core.ts'
export type {
  CoreErrorName,
  DiscoveredComponent,
  DiscoverRequest,
  DiscoveryReport,
  EntityKind,
  KindSource,
  Component,
  LoadRequest,
  Marketplace,
  Plugin,
  Segment,
  ExportMode,
  ExportReport,
  ExportRequest,
  ExportUnit,
  ContainerIdentity,
  MarketplaceConfig,
  PluginOverrides,
  RootFile,
  BuildSettings,
  SkippedNote,
  Target,
  UnitStatus,
  VersionBumpSuggestion,
} from './core.ts'

export { toCorePath, toHostPath } from './host-path.ts'
export { createNodeVfs } from './node-vfs.ts'
export type { BaseVfs, LinkMethods, NodeFs, NodeVfsOptions, Vfs, VfsStat } from './node-vfs.ts'
