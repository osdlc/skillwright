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

/**
 * What crosses the boundary as `request_json`: the core's `PublishRequest`,
 * plus the one field this layer adds.
 *
 * A field on the request rather than a third parameter: hosts call
 * `publishSkill(vfs, requestJson)`.
 *
 * It is a `String` stem rather than a `Namespace` because the core
 * deliberately gives `Namespace` no `Deserialize`: every construction goes
 * through [`Namespace::new`], so a stem that arrives over the wire is
 * validated at the same gate a stem typed at a terminal is, and the same
 * [`InvalidStem`] comes back.
 */
export interface PublishRequest {
  /** Absolute path to the vault note. Not a directory — a skill is a note. */
  skill_path: string;

  /**
   * Absolute path to the destination root. Target-specific directories
   * (`.claude/skills/…`, `.agents/skills/…`) are computed under it.
   */
  destination_root: string;

  /** A name from [`KNOWN_TARGETS`]. See `runtime_target_for`. */
  target: string;

  /**
   * Write, or classify only. Absent on the wire means
   * [`PublishMode::Write`].
   */
  mode?: PublishMode;

  /**
   * The VAULT root this note lives under — the whole vault, never a
   * subfolder of it. Two duties ride on it.
   *
   * The kind gate asks the folders under this root what the note is — the
   * same root [`crate::discover`] scans. And the rail protects exactly
   * this tree: passing `/vault/skills` here would shrink protection to
   * that subtree and make the real vault root a publishable ancestor.
   *
   * Absent means "no structural layer": the default decides, and the rail
   * falls back to the note's own directory with the wider ancestor
   * refusal. A caller that has the vault root should pass it: without it,
   * a command sitting in `commands/` publishes as a `SKILL.md`.
   */
  skills_root?: string;

  /**
   * The root file's config, the top of the cascade: what a note and its
   * containers leave out comes from here. Absent means no root file.
   */
  config?: MarketplaceConfig;

  /**
   * When this run happened, RFC 3339, from the caller's clock.
   *
   * This crate reads no clock — the same rule [`crate::adopt::AdoptRequest`]
   * follows, and the reason both are deterministic under test. Absent, the
   * manifest records no time and a conflict report says so rather than
   * inventing one.
   */
  published_at?: string;

  /**
   * Destination-root-relative paths whose refusal the caller has explicitly
   * acknowledged, and which this run may therefore overwrite.
   *
   * Per file and named, never a run-wide force. A conflicted file holds the
   * only copy of an edit somebody made at the destination, so discarding it
   * is a decision about *that file* — a switch that resolved every conflict
   * at once would make the decision invisible at the moment it is taken.
   */
  overwrite?: string[];

  /**
   * `destination_root` is the home directory, and files land in the
   * harness's user folders rather than its project folders. The two differ
   * for Pi, whose user skills are `.pi/agent/skills`.
   */
  user_folders?: boolean;

  /**
   * The vault's frontmatter namespace, as a stem, such as `sw`, or whatever the
   * host configured. Absent means [`Namespace::default`].
   */
  namespace?: string;
}

/**
 * Whether a publish writes, or only reports what it would write.
 *
 * A field on the request, so there is one publish path. It is also how
 * previewing crosses the wasm boundary: that boundary serializes a
 * [`PublishRequest`], so `#[serde(default)]` lets a host opt in with one
 * extra key, and a request without it means `write`. A separate argument
 * would need a second wire shape for the same request.
 */
export type PublishMode = "write" | "dry-run";

/**
 * What the *build* says about the marketplace and its plugins — the fallback
 * under what the vault's own container notes say.
 *
 * [`crate::model::Cascade`] folds the two, note over request, field by field: a
 * marketplace note is the marketplace, and a build flag is one invocation of
 * it.
 *
 * A member outside every plugin folder belongs to the marketplace's own
 * plugin, which is named after the marketplace.
 */
export interface MarketplaceConfig {
  /**
   * The name both manifests publish under, **when the vault does not supply
   * one**. A marketplace note names itself, and that name wins.
   *
   * A fallback, not an assertion. Which marketplace to build has its own
   * field ([`ExportRequest::marketplace`]), validated against what the vault
   * actually holds; one selector is enough.
   *
   * `None` with no marketplace note is the one case with no answer, and it
   * is refused rather than defaulted: a distribution nobody named is a
   * configuration fault, not a marketplace called `default`.
   */
  name?: string;

  /** Codex's `interface.displayName`; the name when absent. */
  display_name?: string;

  /**
   * The marketplace `owner`. JSON rather than a string because an author
   * may spell it `{name, email, url}`, but a bare name is **not** a shape
   * any dialect accepts: Claude and Cursor both mark `owner` required and
   * both require an object. A string here is normalised to `{name: …}` on
   * the way into a manifest rather than refused, because the note's
   * spelling is a convenience and the manifest's shape is the dialect's.
   */
  owner?: unknown;

  /**
   * The marketplace's own one-line description, from its note. Optional in
   * every dialect, and placed differently by each — see
   * `claude_marketplace_manifest` and `cursor_marketplace_manifest`.
   */
  description?: string;

  /** Default plugin author, inherited by every plugin. `owner` is not. */
  author?: unknown;

  /**
   * Default plugin license, as an SPDX identifier. A distribution is
   * licensed as a whole, so the marketplace note states it once and every
   * plugin under it inherits; a plugin licensed differently says so on its
   * own note.
   */
  license?: string;

  /** The marketplace's own version, where a dialect carries one. */
  version?: string;

  /**
   * Which harnesses get what the marketplace holds. A plugin or member
   * with no `targets` of its own inherits it.
   */
  targets?: string[];

  /** The source repository URL. Every plugin under it inherits it. */
  repository?: string;

  /** The documentation URL. Every plugin under it inherits it. */
  homepage?: string;

  /**
   * The marketplace's harness blocks, keyed by harness (`codex`, …), each
   * a map of that harness's own manifest keys. Merged over the generated
   * marketplace manifest, and inherited by every plugin key by key.
   */
  blocks?: Record<string, Record<string, unknown>>;

  /** Per-plugin manifest overrides, keyed by plugin name. */
  plugins?: Record<string, PluginOverrides>;

  /**
   * The plugin's launcher, relative to the plugin's root (`bin/tool`).
   * Skill bodies call it as `${CLAUDE_PLUGIN_ROOT}/<launcher_path>`. On a
   * root whose harness does not expand that variable, the build rewrites
   * each call to the launcher's path relative to the skill's own folder,
   * when the plugin ships an asset at this path. Absent turns the rewrite
   * off. The root file declares it as `build.launcher_path`.
   */
  launcher_path?: string;
}

/**
 * Manifest fields one plugin overrides. Every absence has a documented
 * default, so an empty override block means the defaults wholesale.
 */
export interface PluginOverrides {
  /** Default `0.1.0`. */
  version?: string;

  /** Default `Skills: <names>.` over the root's own member list. */
  description?: string;

  /** Default the marketplace `author`. */
  author?: unknown;

  /** Default the marketplace `license`, then `UNLICENSED`. */
  license?: string;

  /**
   * Default empty. Also the Claude and Cursor marketplace entry's `tags`.
   * Pi's `package.json` adds `pi-package`.
   */
  keywords?: string[];

  /**
   * Codex always writes it, default `Productivity`. Claude and Cursor
   * write it on the marketplace entry only when it is set.
   */
  category?: string;

  /**
   * Codex's `interface.displayName`, default the plugin's name, and
   * Cursor's `displayName`, absent by default.
   */
  display_name?: string;

  /**
   * Which harnesses the plugin's members go to. Default the marketplace
   * `targets`.
   */
  targets?: string[];

  /** Default the marketplace `repository`. */
  repository?: string;

  /** Default the marketplace `homepage`. */
  homepage?: string;

  /**
   * The plugin note's harness blocks, keyed by harness. Merged over the
   * marketplace's blocks key by key, then over the plugin's manifest and
   * its marketplace entry.
   */
  blocks?: Record<string, Record<string, unknown>>;
}

/**
 * Every file the publish considered, in path order. A refused publish still
 * reports all of them — a silent skip is how a destination rots unnoticed.
 */
export interface PublishReport {
  /** Every file considered, in path order. */
  files: PublishedFile[];

  /**
   * Why there are no files: set exactly when `targets` excluded this
   * destination, and `None` on every other report.
   *
   * Prose, not one of the publish spec's reason tokens. Those
   * name outcomes, and exclusion is the absence of one — nothing here was a
   * candidate, so nothing here has a status. Omitted from the wire when
   * unset.
   */
  not_published?: string;

  /**
   * Vault-relative paths of members this projection could not carry.
   *
   * A pack declares only `skills/`, so an agent or a command belonging to
   * one has no place inside it. Reporting beats refusing the pack — a
   * plugin holding four skills and an agent is an ordinary vault and the
   * four should travel — and it beats dropping them, which is the failure
   * this crate is built around. Both kinds publish to a destination on
   * their own, which is what a reader does about it.
   *
   * Empty on every report but a pack's, and omitted from the wire when
   * empty.
   */
  not_carried?: string[];

  /**
   * Vault-relative path of the plugin folder's own manifest, when the
   * pack's rendered manifest replaced it and the two differ.
   *
   * The rendered manifest comes from the plugin note's frontmatter, so
   * fields only the folder's copy holds (hooks, MCP servers) do not
   * travel. Not a member and not publishable on its own. `None` on every
   * report but a pack's, and omitted from the wire when unset.
   */
  replaced_manifest?: string;

  /**
   * harness block keys this destination's render already wrote, each as
   * its own reason line.
   *
   * A diagnostic that travels with the report and never blocks a write:
   * the files below published exactly as they would have without the key.
   * Empty on every ordinary report, and omitted from the wire when empty.
   */
  redundant_overrides?: PublishReason[];

  /**
   * The unknown keys of the note, and of a pack's members: what the read
   * left out or carried, which a publish has no other way to report. The
   * same warning a scan gives. Omitted from the wire when empty.
   */
  warnings?: DiscoveryWarning[];
}

/** One line of a [`PublishReport`]. */
export interface PublishedFile {
  /** Destination-root-relative, forward-slashed. */
  path: string;

  /** What happened to the file. */
  status: PublishStatus;

  /**
   * Which rule decided [`PublishedFile::status`]. The distinction a status
   * cannot carry — a first write against a rewrite of stale output, a
   * destination edit against a foreign file.
   */
  outcome: PublishOutcome;

  /**
   * Why, when there is a why: every refusal carries one, and so does a file
   * claimed into the manifest because its bytes were already ours.
   */
  reason: PublishReason | null;
}

/** What happened to one file. */
export type PublishStatus = "written" | "unchanged" | "refused";

/**
 * Which of `classify`'s rules produced a file's [`PublishStatus`].
 *
 * A status alone collapses cases a reader has to tell apart. `written` is
 * both a first write into an empty destination and a rewrite of our own
 * output that the source note has moved past, and those two are the
 * difference between installing a unit and repairing a stale one. `refused`
 * is both a destination edit and a file we never wrote. The status says what
 * happened to the bytes; this says which state of the world it happened in.
 *
 * A closed set, and the same tokens on every surface — the wire spelling is
 * the kebab-case rename below, and no consumer re-words it.
 */
export type PublishOutcome = "first-write" | "rewrite" | "claimed" | "overwritten" | "unchanged" | "conflict" | "foreign" | "blocked" | "manifest";

/**
 * Why a report line reads the way it does: the closed-set token, and the
 * prose that varies with the occurrence.
 *
 * Both halves are needed. The token is what a script matches on; the detail
 * carries the path, the timestamp or the member name that tells a person what
 * to do about it, and no closed set can hold those.
 */
export interface PublishReason {
  /** The closed-set token a script matches on. */
  token: ReasonToken;

  /** Prose for a human. Omitted from the wire when there is none. */
  detail?: string;
}

/**
 * The closed set of reasons a report line can carry — the publish spec's
 * outbound vocabulary, plus the one non-refusal note a line has to say about
 * itself.
 *
 * Closed: a report, a `--json` document, `skillwright status` and a later UI
 * all read the same token, so it may not be free text that drifts from the
 * table it reproduces.
 */
export type ReasonToken = "conflict" | "foreign" | "unsafe-path" | "io" | "no-renderer" | "plugin-managed" | "stopped" | "cycle" | "unsafe-root" | "bad-manifest" | "path-collision" | "unknown-target" | "unknown-override-key" | "unknown-kind" | "member-unresolved" | "empty-plugin" | "mixed-frontmatter-shape" | "excluded-note" | "build-output" | "missing-description" | "invalid-name" | "malformed" | "malformed-root-file" | "claimed" | "redundant-override" | "overwritten";

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

/** A parsed root file. */
export interface RootFile {
  /** The marketplace's own fields. */
  marketplace: MarketplaceFields;

  /**
   * `workspace: true`: this file holds shared defaults for every
   * `marketplaces/<m>/skillwright.md` below it and builds them all.
   */
  workspace: boolean;

  /** The `build:` block. */
  build: BuildSettings;

  /** One line per key the file carries that skillwright does not read. */
  warnings?: string[];
}

/** The marketplace's own fields: the root file's top-level keys. */
export interface MarketplaceFields {
  /** The marketplace's name. Absent means the root folder's name. */
  name?: string;

  /** One line on what the marketplace holds. */
  description?: string;

  /** Codex's `interface.displayName`. */
  display_name?: string;

  /**
   * Claude's and Cursor's marketplace `owner`; a string or
   * `{name, email, url}`.
   */
  owner?: unknown;

  /** Default plugin author, inherited by every plugin. `owner` is not. */
  author?: unknown;

  /**
   * Default plugin license, as an SPDX identifier. Inherited by every
   * plugin.
   */
  license?: string;

  /** The marketplace's own version. */
  version?: string;

  /**
   * Which harnesses get what the marketplace holds. Inherited by every
   * plugin and member with none of its own.
   */
  targets?: string[];

  /** The source repository URL. Inherited by every plugin. */
  repository?: string;

  /** The documentation URL. Inherited by every plugin. */
  homepage?: string;

  /**
   * The harness blocks (`claude:`, `codex:`, `cursor:`, `pi:`), keyed by
   * harness. Inherited by every plugin, key by key.
   */
  blocks?: Record<string, Record<string, unknown>>;
}

/** The `build:` block: how skillwright runs, never what lands in a manifest. */
export interface BuildSettings {
  /**
   * Where the harness trees are written, relative to the root file.
   * Required, here or as `--out`: there is no default.
   */
  out?: string;

  /** Which harnesses to build. Absent means all of them. */
  harnesses?: string[];

  /** The frontmatter namespace stem. */
  namespace?: string;

  /**
   * The plugin's launcher, relative to the plugin's root. See
   * [`MarketplaceConfig::launcher_path`].
   */
  launcher_path?: string;

  /** How `bump` writes a version. */
  bump?: BumpSettings;

  /**
   * Where `publish` writes when `--to` is not given. Empty means each
   * harness's user skill folder.
   */
  destinations?: DestinationSetting[];

  /**
   * Folders a bare `adopt` reads from. Empty means each harness's user
   * skill folder.
   */
  scan_locations?: string[];

  /** Files copied into a plugin's root besides each plugin's `assets/`. */
  assets?: AssetSetting[];

  /** Canonical hooks files. */
  hooks?: HookSetting[];

  /**
   * Globs of what never ships, relative to the root file. Only this
   * file's own: a workspace's globs are not inherited into this list, but
   * added to it, read from the workspace folder. Experimental.
   */
  exclude?: string[];

  /** Globs that stand in for the conventional folders. Experimental. */
  patterns?: FolderPatterns;
}

/** `build.bump`: how `bump` writes a version. */
export interface BumpSettings {
  /** How a bump writes the patch number. */
  patch: PatchScheme;
}

/** How `bump` writes the patch number of the version it sets. */
export type PatchScheme = "increment" | "timestamp";

/** Where `publish` writes when `--to` is not given. */
export interface DestinationSetting {
  /** The destination root directory, relative to the root file. */
  root: string;

  /** The target rendered into the root. */
  target: string;

  /** `project` or `user`. Informational: nothing reads it. */
  scope?: string;
}

/** One `build.assets` entry: a file or folder copied into a plugin's root. */
export interface AssetSetting {
  /** The source file or folder, relative to the root file. */
  from: string;

  /** Where it lands, relative to the built plugin's own root. */
  to: string;

  /** Which plugin it lands in. Absent means the marketplace's own plugin. */
  plugin?: string;
}

/** One `build.hooks` entry: a Claude Code `hooks.json` rendered per harness. */
export interface HookSetting {
  /** The file, relative to the root file. */
  from: string;

  /**
   * Which plugin's packs carry it. Absent means the marketplace's own
   * plugin.
   */
  plugin?: string;
}

/**
 * `build.patterns`: for each conventional folder, the globs that name what
 * it holds. See [`crate::patterns`].
 */
export interface FolderPatterns {
  /** Each match is a plugin folder. */
  plugins?: string[];

  /** Each match is a skill: a note, or a folder holding its entry. */
  skills?: string[];

  /** Each match is an agent. */
  agents?: string[];

  /** Each match is a command. */
  commands?: string[];

  /** Each match is a hook note. */
  hooks?: string[];

  /** Each match is a plugin's assets folder. */
  assets?: string[];
}

/**
 * What crosses as `request_json` for [`discover_components`].
 *
 * The one wire shape in this crate that does not `flatten` a core struct,
 * because there is no core struct to flatten: `discover_components` takes its
 * vault root and skills root as loose parameters. So this is not a second
 * copy of anything — it is the parameter list, spelled once, in the only
 * place it has to cross a wire.
 */
export interface DiscoverRequest {
  /** Absolute path to the vault. */
  vault_root: string;

  /**
   * The content root to scan. Absent means the vault root, exactly as the
   * core reads it.
   */
  skills_root?: string;

  /**
   * Absolute path to a folder a build writes to. The scan leaves out what a
   * build owns there too. The configured `build.out` is left out either
   * way, unless it starts with `~`, which wasm cannot resolve.
   */
  out_root?: string;

  /**
   * Says where the packs folders are, under what the vault's own notes say.
   * Absent means the vault's root file.
   */
  config?: MarketplaceConfig;

  /**
   * The vault's frontmatter namespace, as a stem. Absent means
   * [`Namespace::default`].
   */
  namespace?: string;
}

/**
 * What one scan found, and what it could not read.
 *
 * Two lists rather than a `Result`, because a vault is a shared tree and a
 * note nobody else owns must not decide whether everybody else's publish
 * runs. Every caller reads both: the components are the roster, and
 * [`Self::skipped`] is the part of the vault this answer does not cover.
 */
export interface DiscoveryReport {
  /** Every publishable note, sorted by name. */
  components: DiscoveredComponent[];

  /** Every note the scan could not read, in path order. */
  skipped: SkippedNote[];

  /**
   * What the scan has to say about notes it admitted, in path order. An
   * entry [`DiscoveryWarning::is_error`] names is one a build refuses.
   */
  warnings: DiscoveryWarning[];
}

/**
 * One component, as discovery hands it to the verbs.
 *
 * `targets` keeps the three-outcome `targets` reading instead of being
 * expanded to an explicit all-targets list, so the layer that knows what
 * "every destination" means today is the one that decides it.
 */
export interface DiscoveredComponent {
  /** The frontmatter `name`, already validated against the Agent Skills spec. */
  name: string;

  /**
   * The frontmatter `description`, ≤1024 characters. Empty when the note
   * wrote none. A build refuses a skill, command or agent with none
   * ([`Self::lacks_description`]); a container or a hook need not have
   * one.
   */
  description: string;

  /** What this component is, resolved across the layers that can decide it. */
  kind: EntityKind;

  /**
   * Which layer produced [`Self::kind`], straight from
   * [`crate::frontmatter::resolve_kind`].
   */
  kind_source: KindSource;

  /**
   * What the structure said this was, before frontmatter was read. `None`
   * where structure said nothing — a component at the root has no folder
   * above it naming a kind.
   *
   * Kept beside the resolved value rather than discarded so a caller can
   * tell "the folder and the note agree" from "the note overruled the
   * folder".
   */
  folder_kind: EntityKind | null;

  /**
   * `targets` exactly as [`VaultFrontmatter::targets`] read it, every
   * name checked against [`KNOWN_TARGETS`]. `None` is absent — every
   * destination; `Some(vec![])` is nowhere.
   */
  targets: string[] | null;

  /**
   * The plugin holding this component. `None` is a bare component, sitting
   * at the root and belonging to no plugin.
   *
   * The containing folder decides it. Where a marketplace needs every
   * skill under *some* plugin directory, [`crate::export`] supplies the
   * plugin named after the marketplace: a publication concern, not a fact
   * about the vault.
   */
  plugin: string | null;

  /**
   * The marketplace this belongs to — the directory holding the `plugins/`
   * folder it sits under. `None` for a marketplace's own note, and for a
   * component with no marketplace folder above it.
   *
   * Structural, with no frontmatter counterpart.
   */
  marketplace: string | null;

  /** Absolute path of the entry note. */
  entry_path: string;

  /**
   * The directory this component owns — everything under it ships with a
   * skill. `None` for a lone note, which travels alone.
   */
  bundle_root: string | null;

  /**
   * Files this component owns, relative to [`Self::bundle_root`] and
   * forward-slashed, sorted.
   *
   * Ownership is *deepest* directory wins, so a plugin holding
   * `skills/code-review/scripts/check.py` does not claim it —
   * `code-review` does. Without that rule a plugin reads as a flat dump of
   * its own subtree.
   */
  resources: string[];

  /**
   * The entry's path relative to the vault root, forward-slashed — the
   * spelling reports and provenance markers print.
   */
  vault_relative_path: string;

  /**
   * A hook note's routing and payload, as a reader of the tree needs them:
   * `None` on every other kind.
   *
   * Carried here because a hook's file name says nothing about what fires
   * it — a lister that only had the name would have to open each note to
   * tell one hook from another.
   */
  hook: DiscoveredHook | null;

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
 * Which publishable unit a note is: what the folder it sits in names.
 *
 * default is for **absence only**. An unrecognised value is a typo, and
 * defaulting it would publish a `SKILL.md` at every destination for something
 * that is not a skill — silently, and as a write.
 *
 * `mcp-server` is named by the decision as a later kind and is deliberately
 * not a variant yet.
 *
 * # A kind in the vocabulary need not be publishable
 *
 * Entering the vocabulary makes a note *discoverable* — the workbench can
 * list it and say what it is — while [`crate::publish`]'s kind gate refuses it
 * `no-renderer` until a projection exists. `Marketplace` and `Hook` are the
 * kinds sitting there now. Outside the vocabulary a note is `unknown-kind`: a typo.
 */
export type EntityKind = "skill" | "plugin" | "command" | "agent" | "hook" | "marketplace";

/**
 * Which layer decided a note's kind.
 *
 * Carried alongside the kind because the answer to "why is this a plugin"
 * is a different sentence per layer, and a report that only had the kind
 * could not write any of them.
 */
export type KindSource = "override" | "directory" | "default";

/**
 * What a hook note routes on and what it runs, summarised for a listing.
 *
 * A summary, not the projection: [`crate::projection::to_adapt_hook`] stays
 * the authority on what a hook renders to and on every refusal. This reads
 * the same keys with none of the refusals, because a tree has to draw a
 * half-written hook too.
 */
export interface DiscoveredHook {
  /**
   * `event` — what fires the hook. `None` where the note names none, which
   * the projection refuses and a listing still has to show.
   */
  event: string | null;

  /** `matcher` — pipe-delimited tool names. `None` matches every tool. */
  matcher: string | null;

  /**
   * `type`, defaulted to `command` exactly as the projection defaults it.
   * An unrecognised spelling is carried verbatim; naming it a typo is the
   * projection's job.
   */
  hook_type: string;

  /**
   * The `command`, `prompt` or `url` this hook runs, whichever its type
   * licenses — the key when the note writes one, otherwise the body's
   * first non-empty line, which is where a `command` or `prompt` hook may
   * instead put it. `None` for `mcp_tool` and `agent`, which carry their
   * payload elsewhere.
   */
  payload: string | null;
}

/**
 * What a container note says about the thing it names — the manifest fields
 * a marketplace or a plugin publishes under.
 *
 * Read only on a container: the same words on a skill are the skill's own
 * (`license`) or unknown.
 *
 * **Every field is optional and every absence has a default**, resolved by
 * [`crate::export`] rather than here, because the defaults are a property of
 * the manifest dialect being written and not of the note.
 *
 * `owner` and `author` are [`Value`] rather than `String` because an author
 * may spell either as `{name, email, url}`, and narrowing them here would
 * refuse a shape the manifest accepts.
 */
export interface ContainerIdentity {
  /** `owner` — the marketplace's owner, a name or a mapping. */
  owner: unknown;

  /**
   * `author` — the author a plugin publishes under, a name or a mapping. On a
   * marketplace note it is the default for every plugin under it.
   */
  author: unknown;

  /**
   * `display_name` — the human-facing name where a dialect carries one
   * separately from the slug.
   */
  display_name: string | null;

  /** `license` — an SPDX identifier, unread and passed through. */
  license: string | null;

  /** `keywords`. */
  keywords: string[] | null;

  /** `category` — the plugin's marketplace category. */
  category: string | null;

  /** `version` — the plugin's or marketplace's own version. */
  version: string | null;

  /** `repository` — the source repository URL. */
  repository?: string;

  /** `homepage` — the documentation URL. */
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
   * (`unknown-target`, `unknown-kind`); everything else — frontmatter that
   * will not parse, a name the Agent Skills spec does not admit, an unreadable file —
   * is `unreadable-note`, the vocabulary's entry for "this note does not
   * parse".
   */
  reason: string;

  /** The sentence naming what is wrong, after the reason token. */
  detail: string;

  /**
   * The plugin folder holding this note.
   *
   * A pack publish checks its own members against this: the publish spec
   * forbids installing a subset silently. `None` where no plugin folder
   * holds it: the note joins no pack, and no pack is blocked.
   */
  plugin: string | null;
}

/**
 * What crosses as `request_json` for [`export_skills`]: the core's
 * [`ExportRequest`], flattened, plus the namespace stem this layer adds.
 */
export interface ExportRequest {
  /**
   * Absolute path to the vault. Discovery, compilation, and the provenance
   * markers all resolve against it.
   */
  vault_root: string;

  /**
   * The content root to scan; `None` means the vault root, exactly as
   * [`crate::discover::discover_components`] reads it.
   */
  skills_root?: string;

  /**
   * Which marketplace this build renders. `None` names the vault's only
   * one; a vault holding more than one is refused rather than flattened,
   * because one `out_root` holds one marketplace.
   */
  marketplace?: string;

  /**
   * The marketplace directory: one directory holding one marketplace,
   * which every harness this build renders reads from.
   *
   * Each harness owns exactly one top-level entry under it, its own tree,
   * and nothing else here is read, compared or replaced. So this may be a
   * directory holding other things, including a repository's own root,
   * which is what lets a repository be its own marketplace.
   */
  out_root: string;

  /**
   * Which harnesses to render, by name. Empty means every harness this
   * build supports.
   *
   * A name no harness answers to is refused rather than ignored: it is a
   * typo or a harness this build cannot produce, and silently publishing
   * less than was asked for is the failure this crate is built around.
   */
  harnesses?: string[];

  /**
   * Build, or check. Absent on the wire means [`ExportMode::Build`] — the
   * same default-to-the-write choice [`crate::publish::PublishMode`] makes,
   * for the same reason.
   */
  mode?: ExportMode;

  /** The marketplace's own identity and per-plugin overrides. */
  config: MarketplaceConfig;

  /**
   * Files copied verbatim into a built plugin's own root — a CLI, a wasm
   * artifact, a launcher: authored on disk, never as a vault note.
   *
   * Paths are absolute, resolved by the caller: this core never reads an
   * ambient root file, so it has nothing to resolve a relative asset
   * path against (see the crate doc's wasm-boundary note on
   * [`MarketplaceConfig`]). The Rust helper `marketplace_inputs` resolves
   * `build.assets` entries against the root file's own directory.
   */
  assets?: AssetSpec[];

  /** Canonical hooks files; see [`HookFileSpec`]. */
  hooks?: HookFileSpec[];

  /**
   * The vault's frontmatter namespace, as a stem. Absent means
   * [`Namespace::default`], exactly as on [`WireRequest`].
   */
  namespace?: string;
}

/** Whether an export writes the marketplace, or only reports its drift. */
export type ExportMode = "build" | "check";

/**
 * One file, or one directory tree, an export copies verbatim into a built
 * plugin's own root, rather than rendering it from a vault note.
 *
 * Bundle resources ([`crate::compile::CompileSource::bundle_root`]) already
 * cover a skill's own peers; this is for what a *plugin* carries once, no
 * matter which skill happens to shell out to it. A launcher script is the
 * motivating file case, and shared reference docs the skills read
 * (`conventions/`) the motivating directory case.
 */
export interface AssetSpec {
  /**
   * Absolute host path to the source file or directory. A directory
   * lands as its whole tree, under the same fixed `tests`/`fixtures`
   * exclude a skill's bundle sweep applies, and minus `*.test.*` files.
   */
  source: string;

  /**
   * Destination, relative to the built plugin's own root
   * (`<packs>/<plugin>/`), forward-slashed. For a directory source, the
   * directory its tree lands under.
   */
  dest: string;

  /**
   * Which plugin's root this asset lands in. `None` means the
   * marketplace's own plugin, named after the marketplace.
   */
  plugin?: string;
}

/**
 * One canonical hooks file a build renders into a plugin's packs. At most
 * one per plugin; its gap report goes in every pack. See
 * [`crate::hooks_file`].
 */
export interface HookFileSpec {
  /**
   * Absolute host path to the file, resolved by the caller the way an
   * [`AssetSpec::source`] is.
   */
  source: string;

  /**
   * The path relative to the root file.
   * Refusals and the gap report's `source` name the file by this, so the
   * report holds no host path.
   */
  from: string;

  /**
   * Which plugin's packs carry it. `None` means the marketplace's own
   * plugin, which holds the members outside `plugins/` and is named after
   * the marketplace.
   */
  plugin?: string;
}

/** What one export did, or would do — per plugin, per root. */
export interface ExportReport {
  /**
   * The mode the report describes, so `ok`/`drift` and
   * `written`/`unchanged` read against the right verb.
   */
  mode: ExportMode;

  /** How many publishable skills discovery found. */
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
   * A pack has a place for skills, for commands on a root whose
   * [`crate::publish::plugin_format_carries_commands`] is `true`, and for
   * hooks on a root whose [`crate::publish::plugin_format_carries_hooks`]
   * is. Everything else is named here rather than dropped; what a build
   * must not do is leave silently. This is the same fact
   * [`crate::publish::PublishReport::not_carried`] reports on the other
   * path, in this report's own vocabulary.
   *
   * `root` is the root that cannot carry it — a command on Codex, say.
   * `None` when no root can, as for an agent: the reason is the layout's,
   * so it is listed once. A member `targets` narrowed away from a root
   * is not named there; the author asked for that.
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
 * A plugin whose members changed since it was last published here, with its
 * own `version` unmoved since — the callout
 * [`PublishUnitStatus::version_bump_suggestion`] carries so a publish screen can
 * offer a one-click bump.
 *
 * Not a sixth [`PublishState`]: the state stays `stale`, exactly as it would
 * without this. This is a suggestion about what to do next, not a verdict
 * about the destination.
 *
 * # Where "the version at the last publish" comes from
 *
 * The manifest records no version of its own (that would be a schema
 * bump). A plugin's pack manifest
 * (`plugin.json`) is written with whatever `version` was current at
 * publish time, so the installed file's own `"version"` field IS that
 * record, read back rather than tracked twice.
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

/**
 * What crosses as `request_json` for [`load_marketplace`]: the core's
 * [`LoadRequest`], flattened, plus the namespace stem this layer adds.
 */
export interface LoadRequest {
  /** Absolute path to the vault. */
  vault_root: string;

  /** The content root to scan; `None` means the vault root. */
  skills_root?: string;

  /** Which marketplace to load. `None` names the vault's only one. */
  marketplace?: string;

  /**
   * Which harnesses the model is for, by canonical name. Empty means every
   * harness with a plugin format. A body no harness here admits is read
   * but not resolved.
   */
  harnesses?: string[];

  /**
   * The marketplace's identity and per-plugin overrides, under what the
   * vault's own notes say.
   */
  config: MarketplaceConfig;

  /** Files copied verbatim into a plugin's pack. */
  assets?: AssetSpec[];

  /** Canonical hooks files. */
  hooks?: HookFileSpec[];

  /**
   * Absolute path to a folder a build writes to. The load leaves out what a
   * build owns there too. The configured `build.out` is left out either
   * way, unless it starts with `~`, which wasm cannot resolve.
   */
  out_root?: string;

  /**
   * The vault's frontmatter namespace, as a stem. Absent means
   * [`Namespace::default`].
   */
  namespace?: string;
}

/** A vault, loaded as the one marketplace a build renders. */
export interface Marketplace {
  /** The name the marketplace publishes under. */
  name: string;

  /**
   * The vault's marketplace folder this was loaded from, `None` for a
   * vault that holds none.
   */
  folder: string | null;

  /** Every plugin, by name. */
  plugins: Record<string, Plugin>;

  /**
   * Components no pack has a place for, such as agents — named, never
   * dropped. In path order.
   */
  uncarried: Component[];

  /** What the scan and the resolve pass filled in or noticed, in path order. */
  warnings: DiscoveryWarning[];

  /** How many skills the marketplace holds. */
  checked: number;
}

/** One plugin and everything it carries. */
export interface Plugin {
  /** The plugin's name. */
  name: string;

  /** Its skills, in discovery's name order. */
  skills: Component[];

  /** Its commands. */
  commands: Component[];

  /** Its hook notes. */
  hooks: Component[];

  /** Files copied verbatim into the built pack. */
  assets: Asset[];

  /**
   * The launcher's path inside the pack, when the plugin ships an asset
   * at [`crate::export::MarketplaceConfig::launcher_path`]: the
   * `${CLAUDE_PLUGIN_ROOT}/<launcher>` its skills call.
   */
  launcher: string | null;

  /**
   * The harnesses that name this plugin's skills bare, whatever their
   * profile says: `pi: {naming: bare}`.
   */
  bare_on?: string[];
}

/**
 * A skill, a command, a hook, an agent or a container note, read and — when
 * a requested harness admits it — resolved.
 */
export interface Component {
  /** What it is. */
  kind: EntityKind;

  /** Its source name, never prefixed. */
  name: string;

  /** The note, vault-relative. */
  path: string;

  /** Its `targets`, as written. `None` publishes everywhere. */
  targets: string[] | null;

  /** Its harness-only fields, by harness. */
  extensions: Extensions;

  /**
   * Its content: name, description, the modeled fields and the
   * annotations every harness receives, with `sw-arguments` applied for a
   * skill or a command. The body is the note's own; the resolved one is
   * [`Self::resolved`]. `None` until the note is read in full, which a load
   * does only for what some requested harness admits.
   */
  content: SkillContent | null;

  /**
   * The body and its files, resolved. `None` when no harness this load is
   * for admits the component, or for a kind whose body is taken verbatim.
   */
  resolved: Resolved | null;
}

/**
 * A node's harness-only fields: harness blocks, folded onto canonical
 * harness names (D-ISOU).
 */
export interface Extensions {
  /** Each harness's block, as written. */
  blocks: Record<string, Record<string, unknown>>;
}

/**
 * The content plane shared by drafts and version snapshots. Fields are the
 * skill's modeled set plus `annotations` (frontmatter keys
 * outside the modeled set, preserved verbatim so adoption is not lossy).
 */
export interface SkillContent {
  /** The skill's name. */
  name: string;

  /** What the skill does and when to use it. */
  description: string;

  /** Hint shown for the arguments the skill takes. */
  argument_hint: string | null;

  /**
   * Claude Code's own `arguments:` frontmatter key — see
   * [`agent_adapt::Skill::argument_names`]. Hand-written (read from the
   * note's literal `arguments:` key, never `sw-arguments`), or derived by
   * [`SkillContent::apply_arguments`] the same way [`Self::argument_hint`]
   * is.
   */
  argument_names: string[] | null;

  /**
   * What this skill or command's invocation takes.
   *
   * The one field here sourced from skillwright's own keys rather than from
   * the standard's own frontmatter — see
   * [`crate::frontmatter::VaultFrontmatter::arguments`] for why that is
   * not the contradiction it looks like. `Vec::new()` by construction
   * whenever this `SkillContent` comes from
   * [`crate::vault::read_vault_skill`]; only [`SkillContent::apply_arguments`]
   * fills it in, called by `publish` and `export` after the read, before
   * the projection.
   */
  arguments: Argument[];

  /** The markdown body, without frontmatter. */
  body: string;

  /** Tools the skill may use without asking. */
  allowed_tools: string[];

  /**
   * The Agent Skills spec's `license:` — a licence name, or a reference to a bundled
   * licence file. Not `sw-license`, which is a container note's package
   * identity and never travels.
   */
  license: string | null;

  /**
   * The Agent Skills spec's `compatibility:` — what environment this needs, capped at
   * 500 characters by [`crate::discover`].
   */
  compatibility: string | null;

  /**
   * The Agent Skills spec's `metadata:`. Only the entries the note wrote under that key;
   * the unmodeled top-level keys in [`Self::annotations`] join them in one block at render
   * time, as YAML, so an unquoted `true` publishes as a boolean.
   */
  metadata: Record<string, string>;

  /** Model the skill asks to run on, when it names one. */
  model_override: string | null;

  /** Context mode the skill asks for, when it names one. */
  context_mode: string | null;

  /** Whether a user can invoke the skill directly. */
  user_invocable: boolean;

  /** Whether the model is barred from invoking the skill on its own. */
  disable_model_invocation: boolean;

  /** Free-form tags. */
  tags: string[];

  /** Unmodeled frontmatter, preserved as-is. A JSON object. */
  annotations: Record<string, unknown>;

  /**
   * harness blocks, keyed by **canonical** destination name — what one
   * harness receives that the shared model cannot say.
   *
   * The second field here sourced from skillwright's own keys, and it is
   * here for the same reason [`Self::arguments`] is: a renderer needs it.
   * `BTreeMap::new()` by construction from
   * [`crate::vault::read_vault_skill`]; only
   * [`SkillContent::apply_overrides`] fills it in, and only after
   * [`crate::publish::overrides_for`] has folded the target names and
   * checked every key against the harness that will receive it.
   *
   * The raw harness block key still reaches no destination — the strip
   * is unaffected, because this is not `annotations`.
   */
  overrides: Record<string, Record<string, unknown>>;
}

/**
 * One argument a [`crate::Skill`] or [`crate::Command`] declares.
 *
 * Rendered two ways: [`crate::render::derive_argument_hint`] folds the list
 * into a single hint string for a dialect that has one field for it
 * (Claude Code's `argument-hint`), and
 * [`crate::render::render_arguments_section`] writes the whole list into the
 * body as prose, in the syntax the target runtime's invocation mechanism
 * uses — or as a plain description when it has none.
 */
export interface Argument {
  /** A slug: unique among the arguments on the same skill or command. */
  name: string;

  /**
   * What the argument is for, shown beside its name wherever the list is
   * rendered.
   */
  description: string | null;

  /** Whether the caller must supply this argument. Defaults to `false`. */
  required: boolean;

  /**
   * The closed set of values this argument accepts, when it is an enum.
   * `None` means any value; `Some` is never empty — an argument with no
   * allowed values is not an enum, and authoring that is refused before
   * this struct exists.
   */
  values: string[] | null;

  /**
   * The value assumed when the caller supplies none. When `values` is
   * `Some`, this is one of them — also enforced before this struct exists,
   * not here.
   */
  default: string | null;
}

/** A body and the files it reaches, every link and call pointed at its node. */
export interface Resolved {
  /** The entry's body. */
  body: Body;

  /** Every file that travels with it, by output path. */
  resources: Resource[];
}

/**
 * A body as a sequence of segments.
 *
 * A named field rather than a newtype so it crosses the wasm boundary as an
 * object, which is the only struct shape the TypeScript generator emits.
 */
export interface Body {
  /** The segments, in order. */
  segments: Segment[];
}

/** One piece of a resolved body. */
export type Segment =
  /** Text, written as is. */
  | {
      type: "text";
      /** The text. */
      text: string;
    }
  /** A link to a file that travels with the component. */
  | {
      type: "link";
      /** Where it points. */
      target: ResolvedRef;
    }
  /**
   * A call to a skill. The surrounding text already holds the backticks
   * a call outside code is written in.
   */
  | {
      type: "call";
      /** The call. */
      call: Call;
    }
  /**
   * A call to the plugin's launcher, which the vault writes as
   * `${CLAUDE_PLUGIN_ROOT}/<launcher>`.
   */
  | {
      type: "launcher-call";
      /** The launcher's path inside the plugin. */
      launcher: string;
    };

/** A resolved link: the file it reaches and the markdown it renders as. */
export interface ResolvedRef {
  /** The linked file, vault-relative. */
  path: string;

  /** The link text, escaped. */
  label: string;

  /** The relative path from the emitting file, encoded, with its fragment. */
  href: string;

  /** Written as an image. */
  image: boolean;
}

/** A resolved call. */
export interface Call {
  /** The skill it names. */
  skill: SkillId;

  /** That skill's `targets`, which decide whether a harness ships it. */
  targets: string[] | null;

  /** The harnesses that name the skill bare, whatever their profile says. */
  bare_on?: string[];

  /** The call as written, for a refusal. */
  destination: string;

  /**
   * A plain `[[skill]]`: written as the skill's published name, with no
   * call sigil, rather than as an invocation.
   */
  mention: boolean;

  /** The note the call is in, for a refusal. */
  caller: string;
}

/** A skill as a call names it. */
export interface SkillId {
  /** The plugin it ships in, `None` for a skill published on its own. */
  plugin: string | null;

  /** Its source name. */
  name: string;
}

/** One file a component carries. */
export interface Resource {
  /** Where it lands, relative to the component's directory. */
  path: string;

  /** Its content. */
  content: ResourceContent;

  /** Whether it is executable. */
  executable: boolean;
}

/** A resource's content: a compiled markdown file, or bytes as found. */
export type ResourceContent =
  /** A markdown file, whose links and calls a translator writes. */
  | {
      type: "body";
      /** The file, frontmatter and all. */
      body: Body;
    }
  /** Anything else, copied byte for byte. */
  | {
      type: "bytes";
    };

/** A file copied verbatim into a pack. */
export interface Asset {
  /** Where it lands, relative to the pack's own directory. */
  dest: string;

  /** Whether it is executable. */
  executable: boolean;
}

/**
 * What crosses as `request_json` for [`adopt_skills`]: the core's
 * [`AdoptRequest`], flattened, plus the namespace stem this layer adds.
 */
export interface AdoptRequest {
  /**
   * Absolute path to what is being adopted: a skill directory (holds a
   * `SKILL.md`), a plugin directory (holds `skills/`), or a directory of
   * skill directories.
   */
  source: string;

  /** Absolute path to the vault root. */
  vault_root: string;

  /**
   * The content root written under — the vault root when `None`, which is
   * the same root [`crate::discover::discover_components`] scans by default.
   * The two must name the same tree: an adoption the scan cannot find is an
   * import that silently went nowhere.
   *
   * This is a *content* root, not a plugins directory. Adoption spells the
   * `plugins/<plugin>/skills/` levels under it, so a vault gets
   * `<vault>/plugins/adopted/skills/…` and a root configured to `library/`
   * gets `library/plugins/adopted/skills/…` — each one a tree a scan from
   * that root reads.
   */
  skills_root?: string;

  /**
   * The plugin folder the skill lands in;
   * [`DEFAULT_ADOPTED_PLUGIN`] when `None`. Validated with the skills
   * spec's name rule — it becomes a directory name.
   */
  plugin?: string;

  /**
   * What [`Self::source`] is CALLED in `adopted.source`, when the path is not
   * the honest answer.
   *
   * A source fetched from a repository lives in a cache directory whose
   * path says nothing about where the skill came from and will not
   * survive a cache sweep. The caller that fetched it knows the durable
   * spelling — `<url>@<sha>` — and hands it over here. The re-adopt
   * comparison reads this too, so a note stamped with a URL is compared
   * against that URL rather than against the checkout it happens to sit
   * in this run. The comparison drops a repository label's trailing
   * `@<commit>`, so a later commit of the same repository reads as the
   * source moved on.
   *
   * `None` stamps the source path, which is what a local adoption wants.
   */
  source_label?: string;

  /**
   * The harness layout stamped as `adopted.format` — `claude`, `codex`, and
   * the rest of [`crate::skill::RUNTIME_DIRS`]'s second column.
   *
   * `None` derives one from the source path, so a caller that knows the
   * answer (a picker naming the harness) states it and a caller that does
   * not gets the heuristic. See [`detect_format`].
   */
  format?: string;

  /**
   * Further roots to cross-reference for `adopted.also_at`: every other place
   * this machine holds the same skill. Three outcomes, like
   * [`Self::targets`] — see the module doc's `adopted.also_at` row.
   */
  also_scan?: string[];

  /**
   * What lands in `targets`, with the key's own three-outcome contract:
   * `None` writes no key (every destination), an empty list writes `[]`
   * (nowhere), names are validated against [`KNOWN_TARGETS`].
   */
  targets?: string[];

  /**
   * Adopt only these skills, matched by source directory basename or
   * source-relative path. A name that matches nothing is refused.
   */
  skills?: string[];

  /**
   * Vault notes whose replacement the caller has explicitly acknowledged,
   * and which this run may therefore overwrite.
   *
   * Per note and named, never a run-wide force — the same rule and the
   * same spelling as [`crate::publish::PublishRequest::overwrite`]. A note
   * matches by its entry path: the absolute spelling the report prints, or
   * that path relative to the vault root or the content root.
   */
  overwrite?: string[];

  /** Write, or classify only. Absent on the wire means [`AdoptMode::Write`]. */
  mode?: AdoptMode;

  /**
   * The timestamp stamped as `adopted.at`, supplied by the caller —
   * the layer that has a clock. See the module doc.
   */
  adopted_at: string;

  /**
   * The vault's frontmatter namespace, as a stem. Absent means
   * [`Namespace::default`]. It decides how the provenance keys the adoption
   * stamps are spelled, so a vault configured to `x` gets `x-source` and
   * reads it back as its own.
   */
  namespace?: string;
}

/**
 * Whether an adoption writes, or only reports what it would write.
 *
 * The same shape as [`crate::publish::PublishMode`], for the same reasons:
 * one code path, previewing a property of the request so it rides the wire,
 * and a default that does the thing harder to undo rather than the thing
 * that silently does nothing.
 */
export type AdoptMode = "write" | "dry-run";

/**
 * Every skill the adoption considered, sorted by name. A refused skill is
 * still a line — a silent skip is how an import quietly loses half its
 * skills.
 */
export interface AdoptReport {
  /** One entry per skill considered, sorted by name. */
  skills: AdoptedSkill[];
}

/** One line of an [`AdoptReport`]. */
export interface AdoptedSkill {
  /**
   * The adopted name: the source frontmatter's `name` slugified, or the
   * source directory's basename where the frontmatter is silent.
   */
  name: string;

  /** The plugin folder it landed in. */
  plugin: string;

  /** Whether the source carried resources beside its `SKILL.md`. */
  bundle: boolean;

  /** The source directory this skill was read from. */
  source_dir: string;

  /**
   * Where the entry note landed (or would land):
   * `<content-root>/plugins/<plugin>/skills/<name>/<name>.md`.
   */
  entry_path: string;

  /** What the adoption did, or would do, with this skill. */
  status: AdoptStatus;

  /**
   * Why. Always set on [`AdoptStatus::Refused`]; also set on an
   * [`AdoptStatus::Unchanged`] the note's own edits produced, and on an
   * [`AdoptStatus::Updated`] that an acknowledged overwrite forced, so a
   * discarded edit is stated rather than silent.
   */
  reason: string | null;
}

/** What happened to one skill. */
export type AdoptStatus = "adopted" | "updated" | "unchanged" | "refused";

/**
 * What crosses as `request_json` for [`status_of`]: the core's
 * [`StatusRequest`], flattened, plus the namespace stem this layer adds.
 */
export interface StatusRequest {
  /** Path of the skill's note. */
  skill_path: string;

  /** Root of the destination to compare against. */
  destination_root: string;

  /** The runtime target to project for. */
  target: string;

  /** Root of the vault, when the caller knows it. */
  skills_root?: string;

  /** See [`PublishRequest::user_folders`]. */
  user_folders?: boolean;

  /** The root file's config; see [`PublishRequest::config`]. */
  config?: MarketplaceConfig;

  /**
   * The vault's frontmatter namespace, as a stem. Absent means
   * [`Namespace::default`], exactly as on [`WireRequest`].
   */
  namespace?: string;
}

/**
 * The answer, and enough of the working to act on it.
 *
 * Named apart from [`crate::export::UnitStatus`], the marketplace's
 * four-word status: two public types with one name cannot both cross the
 * wasm boundary.
 */
export interface PublishUnitStatus {
  /** Whether the destination is current. */
  state: PublishState;

  /**
   * A sentence naming what makes the state what it is, when there is one to
   * name. `stale` alone is not actionable.
   */
  reason: string | null;

  /** When we last wrote anything here, from the manifest. */
  published_at: string | null;

  /**
   * Every projected file, classified. This is the drift: what would be
   * written, what is already right, and what is refused and why.
   */
  files: PublishedFile[];

  /**
   * The notes that fed this unit's output, as the manifest recorded them.
   * A reverse lookup over these answers *what did editing this make stale*.
   */
  contributing_notes: string[];

  /** Recorded contributors that are no longer in the vault. */
  missing_notes: string[];

  /**
   * Set when `targets` excludes this destination — the `n/a` cell, which
   * is not a state.
   */
  not_published: string | null;

  /**
   * A plugin whose members drifted since it was last published here, with
   * its own `version` unmoved since. `None` for every non-plugin unit,
   * and for a plugin that does not meet the rule — see
   * [`VersionBumpSuggestion`].
   */
  version_bump_suggestion: VersionBumpSuggestion | null;
}

/**
 * What a unit's publish state is at one destination.
 *
 * The closed set the status spec specifies, and the same five words on every
 * surface. Two things that also need a cell in a grid are deliberately not
 * members: a destination whose target has no renderer, and a destination this
 * note's `targets` excludes. Neither is a *manifest* state — one is
 * incapability and the other is deliberate exclusion — and folding them in
 * here would make them look like conditions a publish could fix.
 */
export type PublishState = "never-published" | "current" | "stale" | "conflict" | "unknown";

/**
 * The request each export deserializes, by the name JavaScript calls it.
 */
export interface WasmRequests {
  publishSkill: PublishRequest;
  discoverComponents: DiscoverRequest;
  exportSkills: ExportRequest;
  loadMarketplace: LoadRequest;
  adoptSkills: AdoptRequest;
  statusOf: StatusRequest;
}

/**
 * The report each export serializes back.
 */
export interface WasmResponses {
  publishSkill: PublishReport;
  knownTargets: string[];
  parseRootFile: RootFile;
  discoverComponents: DiscoveryReport;
  exportSkills: ExportReport;
  loadMarketplace: Marketplace;
  adoptSkills: AdoptReport;
  statusOf: PublishUnitStatus;
}

/**
 * Exports with no JSON on either side, so nothing here can drift for them.
 * `validateNamespace(stem: string): void` is the whole signature.
 */
export type WasmExportsWithoutJson = "validateNamespace" | "canonicalTarget" | "publishableKinds" | "rootFileConfig";
