/**
 * GENERATED FILE — do not edit. Run `cargo run -p tradecraft-typegen`.
 *
 * The request and response types of every `#[wasm_bindgen]` export in
 * `tradecraft-wasm/src/lib.rs`, read out of the Rust source that defines
 * them. The wire is a JSON string in and a JSON string out, so nothing in
 * TypeScript checks its shape; this file is what makes the compiler check it.
 * `cargo test -p tradecraft-typegen` fails when this file is stale, so a Rust
 * type that moves without its TypeScript fails the pull request.
 *
 * Optionality follows the direction. A request field serde can default is
 * optional here; a response field is `| null` unless the Rust side skips it
 * when empty.
 *
 * Doc comments are the Rust ones, verbatim, so there is one description of
 * each field rather than two that can disagree.
 */

/** A parsed root file. */
export interface RootFile {
  /** The shared defaults. */
  defaults: SharedDefaults;

  /** The `build:` block. */
  build: BuildSettings;

  /** One line per key the file carries that skillwright does not read. */
  warnings?: string[];
}

/**
 * The root file's shared defaults: the keys the key table marks as
 * inherited. Every marketplace and plugin takes each one it does not set.
 */
export interface SharedDefaults {
  /** `author`: a name or `{name, email, url}`. */
  author?: unknown;

  /** `license`, an SPDX expression. */
  license?: string;

  /** `repository`, the source repository URL. */
  repository?: string;

  /** `homepage`, the documentation URL. */
  homepage?: string;

  /**
   * The harness blocks (`claude:`, `codex:`, `cursor:`, `pi:`), keyed by
   * harness. Inherited key by key.
   */
  blocks?: Record<string, Record<string, unknown>>;
}

/** The `build:` block: how skillwright runs, never what lands in a manifest. */
export interface BuildSettings {
  /** Which harnesses to build. Absent means all of them. */
  harnesses?: string[];

  /** How `bump` writes a version. */
  bump?: BumpSettings;
}

/** `build.bump`: how `bump` writes a version. */
export interface BumpSettings {
  /** How a bump writes the patch number. */
  patch: PatchScheme;
}

/** How `bump` writes the patch number of the version it sets. */
export type PatchScheme = "increment" | "timestamp";

/**
 * What crosses as `request_json` for [`discover_components`].
 *
 * The one wire shape this crate owns: `discover_components` takes its vault
 * root as a loose parameter, so there is no core struct to read.
 */
export interface DiscoverRequest {
  /**
   * Absolute path to the vault root, the folder holding `skillwright.md`
   * and `marketplaces/`.
   */
  vault_root: string;
}

/**
 * What one scan found, and what it could not read.
 *
 * Two lists rather than a `Result`, because a vault is a shared tree and a
 * note nobody else owns must not decide whether everybody else's build
 * runs. Every caller reads both: the components are the roster, and
 * [`Self::skipped`] is the part of the vault this answer does not cover.
 */
export interface DiscoveryReport {
  /** Every note the scan read, sorted by name. */
  components: DiscoveredComponent[];

  /** Every note the scan could not read, in path order. */
  skipped: SkippedNote[];

  /**
   * What the scan has to say about notes it admitted, in path order. An
   * entry [`DiscoveryWarning::is_error`] names is one a build refuses.
   */
  warnings: DiscoveryWarning[];
}

/** One component, as discovery hands it to the verbs. */
export interface DiscoveredComponent {
  /** The folder's name, which the layout has checked against the name rule. */
  name: string;

  /**
   * The frontmatter `description`, ≤1024 characters. Empty when the note
   * wrote none. A build refuses a skill, command or agent with none
   * ([`Self::lacks_description`]); a container need not have one.
   */
  description: string;

  /** What this component is, from its position. */
  kind: EntityKind;

  /** The plugin folder holding this note. `None` for a marketplace's note. */
  plugin: string | null;

  /**
   * The marketplace folder this sits under. `None` for a marketplace's
   * own note.
   */
  marketplace: string | null;

  /** Absolute path of the entry note. */
  entry_path: string;

  /** The folder this note owns, the one named for it. */
  bundle_root: string | null;

  /**
   * Files this note owns, relative to [`Self::bundle_root`] and
   * forward-slashed, sorted. A plugin does not own its components' files.
   */
  resources: string[];

  /**
   * The entry's path relative to the vault root, forward-slashed — the
   * spelling reports and provenance markers print.
   */
  vault_relative_path: string;

  /**
   * The manifest fields this note publishes under, read from its own
   * frontmatter.
   *
   * Carried for every kind and consulted for two: a marketplace note names
   * the distribution, a plugin note names the pack. A skill's is silent in
   * every vault worth having, because a skill's manifest entry is derived
   * from the skill and a second spelling of it could only disagree.
   *
   * Surfaced here rather than left in the note because the scan is the only
   * pass that reads a container note at all — [`crate::export`] renders
   * from what discovery hands it and never opens a file of its own.
   */
  identity: ContainerIdentity;

  /**
   * The note's harness blocks (`claude:`, `codex:`, `cursor:`, `pi:`),
   * keyed by harness. A container's are merged over the manifests it
   * generates and inherited by its members.
   */
  blocks?: Record<string, unknown>;

  /**
   * Keys the key table does not list for this kind, sorted. The scan
   * warns about each.
   */
  unknown_keys?: string[];
}

/**
 * Which kind of unit a note is: what the folder it sits in names.
 *
 * `mcp-server` is named by the decision as a later kind and is deliberately
 * not a variant yet.
 *
 * # A kind in the vocabulary need not ship
 *
 * Entering the vocabulary makes a note *discoverable*: a scan can list it and
 * say what it is. A build ships only the kinds a pack has a place for.
 */
export type EntityKind = "skill" | "plugin" | "command" | "agent" | "marketplace";

/**
 * What a container note says about the thing it names: the manifest fields
 * a marketplace or a plugin publishes under. Its fields are the key table's
 * identity rows ([`crate::keys::identity_keys`]).
 *
 * Read only on a container: the same words on a skill are the skill's own
 * (`license`) or unknown. A field a kind does not read stays unset.
 *
 * **Every field is optional and every absence has a default**, resolved by
 * [`crate::export`] rather than here, because the defaults are a property of
 * the manifest dialect being written and not of the note. An unset field is
 * left out of the JSON form, so absent and unset are the same thing.
 *
 * `owner` and `author` are [`Value`] rather than `String` because an author
 * may spell either as `{name, email, url}`, and narrowing them here would
 * refuse a shape the manifest accepts.
 *
 * The root file, the build config and a plugin's overrides embed this struct,
 * flattened, so its fields sit at their top level on the wire.
 */
export interface ContainerIdentity {
  /**
   * `owner`, a marketplace's only: a name or `{name, email, url}`. A string
   * is normalised to `{name: …}` in a manifest, because Claude and Cursor
   * require an object. Absent, the marketplace's name stands in.
   */
  owner?: unknown;

  /**
   * `author`: a name or `{name, email, url}`. A string is normalised to
   * `{name: …}`; Cursor drops `url`. A marketplace's is the default for
   * each plugin.
   */
  author?: unknown;

  /**
   * `display_name`: Codex's `interface.displayName`, the name when absent.
   * On a plugin, also Cursor's `displayName` on its manifest and its
   * catalog entry, absent by default; `cursor.displayName` overrides it.
   * Never inherited.
   */
  display_name?: string;

  /**
   * `license`, an SPDX expression, passed through. A plugin's defaults to
   * the marketplace's, then `UNLICENSED`.
   */
  license?: string;

  /**
   * `keywords`, a plugin's only. Default empty. Also the Claude and Cursor
   * marketplace entry's `tags`; Pi's `package.json` adds `pi-package`.
   */
  keywords?: string[];

  /**
   * `category`, a plugin's only. Codex always writes it, default
   * `Productivity`; Claude and Cursor write it on the marketplace entry
   * only when it is set.
   */
  category?: string;

  /**
   * `version`, the container's own. A plugin's defaults to `0.1.0`, with a
   * warning.
   */
  version?: string;

  /**
   * `repository`, the source repository URL, written into every manifest
   * that has the field. A marketplace's is the default for each plugin.
   */
  repository?: string;

  /**
   * `homepage`, the documentation URL, written into every manifest that
   * has the field. A marketplace's is the default for each plugin.
   */
  homepage?: string;
}

/** One note the scan could not read, and why. */
export interface SkippedNote {
  /**
   * The note's path relative to the vault root, forward-slashed — the same
   * spelling every other report prints.
   */
  path: string;

  /**
   * A closed-set reason token. The refusals that already carry one keep it
   * (`malformed`); everything else — frontmatter that will not parse,
   * a name the Agent Skills spec does not admit, an unreadable file — is
   * `unreadable-note`, the vocabulary's entry for "this note does not
   * parse".
   */
  reason: string;

  /** The sentence naming what is wrong, after the reason token. */
  detail: string;

  /**
   * The plugin folder holding this note.
   *
   * A pack build checks its own members against this: a pack never ships
   * a subset silently. `None` where no plugin folder
   * holds it: the note joins no pack, and no pack is blocked.
   */
  plugin: string | null;
}

/** One note the scan admitted, and what is wrong with it. */
export interface DiscoveryWarning {
  /** Vault-relative and forward-slashed, as [`SkippedNote::path`]. */
  path: string;

  /**
   * A closed-set token: `missing-description` from the scan;
   * [`crate::export`] adds `missing-version` for a plugin.
   */
  reason: string;

  /** Extra detail for a person to read. */
  detail: string;
}

/**
 * One marketplace export: this vault, built into its `dist/<m>/` or checked
 * against it.
 */
export interface ExportRequest {
  /**
   * Absolute path to the vault root. Discovery, compilation, the output
   * folder and the provenance markers all resolve against it.
   */
  vault_root: string;

  /**
   * Which marketplace this build renders, by folder. `None` names the
   * vault's only one; a vault holding more than one is refused.
   */
  marketplace?: string;

  /**
   * Which harnesses to build, by name. It replaces the root file's
   * `build.harnesses` for this run; empty means that list, and with no
   * list, every harness.
   *
   * A name no harness answers to is refused rather than ignored: silently
   * publishing other than was asked for is the failure this crate is
   * built around.
   */
  harnesses?: string[];

  /** Build, or check. Absent on the wire means [`ExportMode::Build`]. */
  mode?: ExportMode;

  /**
   * Check only: the built marketplace to compare the render with, in place
   * of `<root>/dist/<m>/`. `bump --changed-since` compares with a baseline
   * this way. A build refuses it: a build always writes `dist/<m>/`.
   */
  compare_with?: string;
}

/** Whether an export writes the marketplace, or only reports its drift. */
export type ExportMode = "build" | "check";

/** What one export did, or would do — per plugin, per root. */
export interface ExportReport {
  /**
   * The mode the report describes, so `ok`/`drift` and
   * `written`/`unchanged` read against the right verb.
   */
  mode: ExportMode;

  /** How many skills discovery found. */
  checked: number;

  /** Check: anything drifted or stale. Build: anything was swapped. */
  changed: boolean;

  /**
   * One unit per plugin per root, one per marketplace manifest, plus whatever the
   * destination holds that the vault no longer produces, plus the members a
   * layout cannot carry. Ordered: the members no root carries first; then roots in
   * `claude`, `codex`, `cursor`, `pi` order; within a root, plugins by name, then the
   * manifest, then stale paths, then the members that root cannot carry.
   */
  units: ExportUnit[];

  /**
   * What the scan and the build have to say about admitted notes, in path
   * order.
   */
  warnings: DiscoveryWarning[];
}

/** One comparable unit of the marketplace. */
export type ExportUnit =
  /** One plugin directory in one root — its manifest and its skills. */
  | {
      unit: "plugin";
      /** The harness root the plugin sits in. */
      root: string;
      /** The plugin's name. */
      name: string;
      /** Member skills admitted to this root. */
      skills: number;
      /** Whether the plugin matches what is installed. */
      status: UnitStatus;
      /**
       * This plugin's members drifted from what is installed here, with
       * its own `version` unmoved since — the one-click bump a build
       * report can offer alongside `drift`/`written`. `None` for a plugin
       * that does not meet the rule, or that carries no `version` to
       * compare. See [`VersionBumpSuggestion`].
       */
      version_bump_suggestion: VersionBumpSuggestion | null;
    }
  /** One root's marketplace manifest. */
  | {
      unit: "manifest";
      /** The harness root the manifest sits in. */
      root: string;
      /** Whether the manifest matches what is installed. */
      status: UnitStatus;
    }
  /**
   * Present at the destination, absent from the render. `check` reports
   * it as drift; `build`'s swap removes it.
   *
   * `kind` is the difference between losing a whole pack and losing a
   * leftover file, which is not a difference a reader should have to
   * infer from the path.
   */
  | {
      unit: "stale";
      /** The harness root the path sits in. */
      root: string;
      /** The stale path, relative to the destination. */
      path: string;
      /** What the stale path is. */
      kind: StaleKind;
    }
  /**
   * A component the marketplace layout has no place for, named rather than
   * dropped.
   *
   * A pack has a place for skills, and for commands on a root whose
   * [`crate::harness::PluginFormat::carries_commands`] is `true`.
   * Everything else is named here rather than dropped; what a build must
   * not do is leave silently.
   *
   * `root` is the root that cannot carry it — a command on Codex, say.
   * `None` when no root can, as for an agent: the reason is the layout's,
   * so it is listed once.
   */
  | {
      unit: "not-carried";
      /** The root that cannot carry the component, or `None` when no root can. */
      root: string | null;
      /** The component's path in the vault. */
      path: string;
      /** The component's kind. */
      kind: string;
    };

/**
 * One unit's outcome. `check` speaks `ok`/`drift`; `build` speaks
 * `written`/`unchanged` — the same comparison, two verbs.
 */
export type UnitStatus = "ok" | "drift" | "written" | "unchanged";

/**
 * A plugin whose members changed since it was last built, with its own
 * `version` unmoved since: a suggestion to run `bump`.
 *
 * The version at the last build is the built `plugin.json`'s own
 * `"version"` field, read back.
 */
export interface VersionBumpSuggestion {
  /** The note's current `version`. */
  current_version: string;

  /**
   * The `version` the installed pack manifest carries. Equal to
   * `current_version` by construction — a suggestion exists exactly
   * because nothing moved to answer the members having changed.
   */
  published_version: string;
}

/**
 * What a stale path is, which decides how much a reader should care.
 *
 * The swap removes all three the same way. They are not the same news: a
 * plugin is a whole pack the marketplace stops carrying, a file is one
 * leftover, and a directory is a shape the destination kept after its
 * contents went. Reported separately rather than left to be read off the
 * path, which only says `plugins/<name>` for the first of them by
 * coincidence of layout.
 */
export type StaleKind = "plugin" | "file" | "empty-dir";

/** A build or check of every marketplace in a vault, or of the one named. */
export interface VaultRequest {
  /** Absolute path to the vault root. */
  vault_root: string;

  /** Which marketplace to build, by folder. `None` builds every one. */
  marketplace?: string;

  /**
   * Which harnesses to build. It replaces the root file's
   * `build.harnesses`, as [`ExportRequest::harnesses`] does.
   */
  harnesses?: string[];

  /** Build, or check. Absent on the wire means [`ExportMode::Build`]. */
  mode?: ExportMode;
}

/** What [`build_vault`] did, or would do, per marketplace. */
export interface VaultReport {
  /** One report per marketplace, in folder order. */
  marketplaces: MarketplaceReport[];

  /**
   * A check's refused marketplaces, in folder order. A build that meets a
   * refusal fails whole, so its report holds none.
   */
  refused?: MarketplaceRefusal[];
}

/** One marketplace's report in a [`VaultReport`]. */
export interface MarketplaceReport {
  /** The marketplace's folder name. */
  marketplace: string;

  /** Its export report. */
  report: ExportReport;
}

/** A marketplace a check could not plan, in a [`VaultReport`]. */
export interface MarketplaceRefusal {
  /** The marketplace's folder name. */
  marketplace: string;

  /** What refused it, as the error reads. */
  error: string;
}

/**
 * What a marketplace load reads: this vault, this marketplace of it, for
 * these harnesses. The root file's shared defaults and `build.harnesses` are
 * read from the vault, never passed.
 */
export interface LoadRequest {
  /** Absolute path to the vault. */
  vault_root: string;

  /**
   * Which marketplace to load, by folder. `None` names the vault's only
   * one.
   */
  marketplace?: string;

  /**
   * Which harnesses to load for, by canonical name. It replaces the root
   * file's `build.harnesses`; empty means that list. A body no harness
   * here admits is read but not resolved.
   */
  harnesses?: string[];
}

/** A vault's marketplace, as `loadMarketplace` returns it. */
export interface Marketplace {
  /** The name the marketplace publishes under. */
  name: string;

  /** Every plugin, by name. */
  plugins: Record<string, Plugin>;

  /** Components no plugin has a place for, such as agents. In path order. */
  uncarried: Component[];

  /** What the scan and the load noticed, in path order. */
  warnings: DiscoveryWarning[];

  /** How many skills the marketplace holds. */
  checked: number;
}

/** One plugin and what it carries. */
export interface Plugin {
  /** The plugin's name. */
  name: string;

  /** Its skills, in name order. */
  skills: Component[];

  /** Its commands, in name order. */
  commands: Component[];

  /** Files copied verbatim into the built plugin. */
  assets: Asset[];

  /**
   * The paths of the Markdown files in its shared `references/`, from the
   * plugin root.
   */
  references: string[];

  /**
   * The harnesses that name this plugin's skills without the plugin
   * prefix their default adds.
   */
  bare_on?: Target[];
}

/** A skill, a command or an agent. */
export interface Component {
  /** What it is. */
  kind: EntityKind;

  /** Its source name, never prefixed. */
  name: string;

  /** The note, vault-relative. */
  path: string;

  /** Its harness blocks, by harness. */
  blocks: Record<string, Record<string, unknown>>;

  /**
   * Its `description`. `None` when no harness the load is for admits the
   * component, because the load does not read such a note in full.
   */
  description: string | null;

  /**
   * Every skill or command its body calls, in body order. Empty when no
   * harness the load is for admits the component.
   */
  calls: SkillId[];

  /** The paths of the files it carries, relative to its own directory. */
  resources: string[];
}

/** A skill or command as a call names it. */
export interface SkillId {
  /** The plugin it ships in. */
  plugin: string;

  /** Its source name. */
  name: string;

  /** A skill or a command. */
  kind: EntityKind;
}

/** A file copied verbatim into a pack. */
export interface Asset {
  /** Where it lands, relative to the pack's own directory. */
  dest: string;

  /** Whether it is executable. */
  executable: boolean;
}

/** A harness name: one of `KNOWN_TARGETS`. */
export type Target = "claude" | "codex" | "cursor" | "pi";

/**
 * The request each export deserializes, by the name JavaScript calls it.
 */
export interface WasmRequests {
  discoverComponents: DiscoverRequest;
  exportSkills: ExportRequest;
  buildVault: VaultRequest;
  loadMarketplace: LoadRequest;
}

/**
 * The report each export serializes back.
 */
export interface WasmResponses {
  knownTargets: string[];
  parseRootFile: RootFile;
  discoverComponents: DiscoveryReport;
  exportSkills: ExportReport;
  buildVault: VaultReport;
  loadMarketplace: Marketplace;
}

/**
 * Exports with no JSON on either side, so nothing here can drift for them.
 */
export type WasmExportsWithoutJson = never;
