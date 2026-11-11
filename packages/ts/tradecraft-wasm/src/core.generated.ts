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

  /** `targets`: which harnesses get each component. Absent means all. */
  targets?: string[];

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
 * The one wire shape in this crate that does not `flatten` a core struct,
 * because there is no core struct to flatten: `discover_components` takes its
 * vault root as a loose parameter. So this is not a second copy of anything —
 * it is the parameter list, spelled once, in the only place it has to cross a
 * wire.
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

  /**
   * Which layer produced [`Self::kind`], straight from
   * [`crate::frontmatter::resolve_kind`].
   */
  kind_source: KindSource;

  /** What the position said this was. Always the same as [`Self::kind`]. */
  folder_kind: EntityKind | null;

  /**
   * `targets` exactly as [`VaultFrontmatter::targets`] read it, every
   * name checked against [`KNOWN_TARGETS`]. `None` is absent — every
   * destination; `Some(vec![])` is nowhere.
   */
  targets: string[] | null;

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
 * Entering the vocabulary makes a note *discoverable*: a scan can list it and
 * say what it is. A build ships only the kinds a pack has a place for.
 * Outside the vocabulary a note is `unknown-kind`: a typo.
 */
export type EntityKind = "skill" | "plugin" | "command" | "agent" | "marketplace";

/**
 * Which layer decided a note's kind.
 *
 * Carried alongside the kind because the answer to "why is this a plugin"
 * is a different sentence per layer, and a report that only had the kind
 * could not write any of them.
 */
export type KindSource = "override" | "directory" | "default";

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
 * What crosses as `request_json` for [`export_skills`]: the core's
 * [`ExportRequest`], flattened.
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
   * Which harnesses to render, by name. Empty means every harness this
   * build supports.
   *
   * A name no harness answers to is refused rather than ignored: it is a
   * typo or a harness this build cannot produce, and silently publishing
   * less than was asked for is the failure this crate is built around.
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

  /**
   * The root file's shared defaults, which every marketplace and plugin
   * takes where its note is silent. Absent on the wire means none.
   */
  defaults?: SharedDefaults;
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
   * A pack has a place for skills, and for commands on a root whose
   * [`crate::harness::PluginFormat::carries_commands`] is `true`.
   * Everything else is named here rather than dropped; what a build must
   * not do is leave silently.
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

/**
 * What crosses as `request_json` for [`load_marketplace`]: the core's
 * [`LoadRequest`], flattened.
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
   * Which harnesses the model is for, by canonical name. Empty means every
   * harness with a plugin format. A body no harness here admits is read
   * but not resolved.
   */
  harnesses?: string[];

  /**
   * The root file's shared defaults, under what the vault's own notes say.
   * Absent on the wire means none.
   */
  defaults?: SharedDefaults;
}

/** A vault, loaded as the one marketplace a build renders. */
export interface Marketplace {
  /** The name the marketplace publishes under. */
  name: string;

  /** The vault's marketplace folder this was loaded from. Always set. */
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

  /** Files copied verbatim into the built pack. */
  assets: Asset[];

  /**
   * The harnesses that name this plugin's skills bare against their
   * profile's default: `<harness>: {naming: bare}` where the default
   * prefixes.
   */
  bare_on?: string[];
}

/**
 * A skill, a command, an agent or a container note, read and — when
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
   * annotations every harness receives, with `arguments` applied for a
   * skill or a command. The body is the note's own; the resolved one is
   * [`Self::resolved`]. `None` until the note is read in full, which a load
   * does only for what some requested harness admits.
   */
  content: SkillContent | null;

  /**
   * The body and its files, resolved. `None` when no harness this load is
   * for admits the component.
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
 * outside the modeled set, preserved verbatim).
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
   * fills it in, called by `export` after the read, before
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
   * [`crate::model::cascade::fold_harness_blocks`] has folded the target names and
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

  /**
   * The harnesses that name the skill bare against their profile's
   * default, as its plugin's [`Plugin::bare_on`].
   */
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
 * The request each export deserializes, by the name JavaScript calls it.
 */
export interface WasmRequests {
  discoverComponents: DiscoverRequest;
  exportSkills: ExportRequest;
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
  loadMarketplace: Marketplace;
}

/**
 * Exports with no JSON on either side, so nothing here can drift for them.
 */
export type WasmExportsWithoutJson = never;
