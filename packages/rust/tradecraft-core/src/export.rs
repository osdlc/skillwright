//! The marketplace renderer — every publishable note built into one
//! marketplace directory serving every harness, and the drift check over it.
//!
//! The engine behind the `build` and `check` verbs.
//! Skills group by the plugin folder they sit in, each group
//! becomes a pack directory with a manifest, and the groups assemble into a
//! marketplace. This is the standalone rendering; a per-destination plugin
//! publish is [`crate::publish::publish_skill`]'s `EntityKind::Plugin` arm,
//! which writes the pack manifest at the destination.
//!
//! # One directory, one tree per harness
//!
//! `build` writes into one marketplace directory ([`ExportRequest::out_root`])
//! and gives each harness its own top-level tree inside it
//! (`HarnessProfile::tree`) — `.claude-plugin/`, `.agents/`, `.cursor-plugin/` and
//! `.pi/` today. Each tree carries that harness's marketplace manifest, at the
//! fixed path that harness reads (Pi has none: its packs install by path),
//! its pack manifests in its own dialect, and only the skills
//! whose `targets` admit it, at `<packs>/<plugin>/skills/<name>/`
//! (the marketplace layout spec). A skill narrowed away from a harness is absent
//! from its tree entirely, not merely unlisted.
//!
//! The trees are disjoint by construction, and they have to be: the harnesses
//! translate a skill's tool names differently and spell its frontmatter
//! differently, so the same skill is genuinely one file per harness. A tree per harness is what lets one directory serve
//! all of them — including a directory that is a repository's own root, which
//! is how a repository becomes a marketplace installable from its bare URL.
//!
//! **A build owns its trees and nothing else.** Nothing at `out_root` outside
//! them is read, compared or replaced, because the render produces no path
//! beginning with anything else. That is a structural guarantee rather than a
//! denylist, and it is what makes `out_root` safe to point at a directory
//! holding a `.git`, a vault and its root file.
//!
//! A plugin's commands render alongside its skills, at
//! `<packs>/<plugin>/commands/<name>.<ext>`, on the one harness whose plugin
//! format has a place for one — see
//! [`crate::publish::plugin_format_carries_commands`] for which. A command
//! narrowed out this way has no file and no manifest mention there, and the
//! report names it as [`ExportUnit::NotCarried`] for that root. One narrowed
//! out by `targets` is absent with no report line: the author asked.
//!
//! Which harnesses a build renders is [`ExportRequest::harnesses`]; empty
//! means all of them. A name no harness answers to is refused rather than
//! ignored — publishing less than was asked for, quietly, is the failure this
//! crate is built around.
//!
//! # One model, one tree per harness
//!
//! The vault is loaded once into the marketplace model ([`crate::model`]) —
//! every note read, every body resolved — and each harness's translator
//! ([`crate::harness::HarnessTranslator`]) writes that model as its tree.
//! These are the same translators `publish` writes a pack and a note through,
//! so a note renders the same bytes on both paths, harness blocks included.
//!
//! # One render, two verbs
//!
//! `check` is not a second reading of `build`'s rules. Both modes translate
//! the whole marketplace once, into a [`crate::harness::FileTree`] per
//! harness, and diff those bytes against the destination. `check` returns that diff, having written nothing under any
//! outcome. `build` acts on it: an up-to-date destination is reported
//! unchanged and nothing is written or renamed; a drifted root is staged from
//! the very bytes the diff compared and swapped in. The two verdicts cannot
//! disagree, because there is only one render and one comparison.
//!
//! # The commit is stage-then-swap, one path at a time, with backup
//!
//! A changed harness is written under `<out>/.skillwright-stage/new/`, each
//! path it owns is renamed aside to `…/backup/<path>`, and the staged path is
//! renamed into place — the temp-and-rename [`crate::vfs::Vfs::rename`] and
//! [`crate::vfs::Vfs::remove`] exist for. The stage lives under `out_root`
//! itself, because a rename is only atomic within one filesystem and the one
//! directory guaranteed to share a filesystem with the thing being replaced
//! is its own parent. A failure mid-swap rolls back every path already
//! swapped, restores the failing path's own backup, and surfaces as
//! [`CoreError::SwapFailed`] saying so.
//!
//! # A build replaces only what it owns
//!
//! The swap unit is what [`HarnessProfile::owned_paths`] names: a harness's
//! packs folder and its marketplace manifest. Those are generated output,
//! replaced on every build. Anything inside them the render did not produce —
//! a stale pack, a stray file, an empty directory — goes with the swap, and
//! `check` reports exactly that set as stale, which is what keeps its verdict
//! honest about what `build` would change.
//!
//! The folders around them are not swapped: `.agents/` and `.pi/` also hold
//! skills and config people author, and `out: .` lands the output beside the
//! root file. Anything outside the owned paths is never looked at.
//!
//! # Manifests are byte-deterministic
//!
//! Stable key order (serde struct order), plugins and skills sorted by name,
//! two-space indentation, one trailing newline — so `check` can compare
//! bytes, and two builds of one vault are identical. Configuration enters
//! through [`MarketplaceConfig`] on the request, typed and serializable,
//! because everything here crosses the wasm boundary and an
//! ambient config file is a thing a wasm host does not have.
//!
//! # The rail holds here too
//!
//! On a host whose [`crate::vfs::Vfs::sees_links`] is `true`,
//! [`crate::guard::assert_destination_allowed_for_vault`] runs before anything
//! is staged: a destination inside the vault's kind folders is refused,
//! and so is an ancestor of the vault when a path this build replaces would
//! land inside the vault. On any other host, such as the wasm bridge over a host object without
//! link methods, [`crate::guard::assert_destination_allowed`] also refuses every ancestor of
//! the vault, because a link the host cannot see is what the narrowed check
//! would miss. `publish` takes the same two rails on the same condition.
//! A destination inside the vault but outside its kind folders, the vault root
//! itself included, is accepted: the vault walk leaves out what a
//! build writes there.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use agent_adapt::RuntimeId;
use serde::{Deserialize, Serialize};

use crate::discover::{DiscoveryWarning, name_refusal};
use crate::error::CoreError;
use crate::guard::{assert_destination_allowed, assert_destination_allowed_for_vault, within};
use crate::harness::manifest::{overrides_for, packs_of};
use crate::harness::translator::GENERATED_MARKER;
pub use crate::harness::{CliInvocation, PluginDialect, SkillNaming};
use crate::harness::{HarnessProfile, HarnessTranslator, translator_for, with_plugin_format};
use crate::model::load::{LoadRequest, kind_carried, selected_harnesses, targets_admit};
use crate::model::{Component, Marketplace, load};
use crate::namespace::Namespace;
use crate::skill::forward_slashed;
use crate::status::{VersionBumpSuggestion, version_bump_if_unmoved};
use crate::vfs::{PreviewWrite, Vfs, normalize};

/// One marketplace export: this vault, onto this output root, in this mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    /// Absolute path to the vault. Discovery, compilation, and the provenance
    /// markers all resolve against it.
    pub vault_root: PathBuf,
    /// The content root to scan; `None` means the vault root, exactly as
    /// [`crate::discover::discover_components`] reads it.
    #[serde(default)]
    pub skills_root: Option<PathBuf>,
    /// Which marketplace this build renders. `None` names the vault's only
    /// one; a vault holding more than one is refused rather than flattened,
    /// because one `out_root` holds one marketplace.
    #[serde(default)]
    pub marketplace: Option<String>,
    /// The marketplace directory: one directory holding one marketplace,
    /// which every harness this build renders reads from.
    ///
    /// Each harness owns exactly one top-level entry under it — its own tree
    /// (`HarnessProfile::tree`) — and nothing else here is read, compared or
    /// replaced. So this may be a directory holding other things, including
    /// a repository's own root, which is what lets a repository be its own
    /// marketplace.
    pub out_root: PathBuf,
    /// Which harnesses to render, by name. Empty means every harness this
    /// build supports.
    ///
    /// A name no harness answers to is refused rather than ignored: it is a
    /// typo or a harness this build cannot produce, and silently publishing
    /// less than was asked for is the failure this crate is built around.
    #[serde(default)]
    pub harnesses: BTreeSet<String>,
    /// Build, or check. Absent on the wire means [`ExportMode::Build`] — the
    /// same default-to-the-write choice [`crate::publish::PublishMode`] makes,
    /// for the same reason.
    #[serde(default)]
    pub mode: ExportMode,
    /// The marketplace's own identity and per-plugin overrides.
    pub config: MarketplaceConfig,
    /// Files copied verbatim into a built plugin's own root — a CLI, a wasm
    /// artifact, a launcher: authored on disk, never as a vault note.
    ///
    /// Paths are absolute, resolved by the caller: this core never reads an
    /// ambient root file, so it has nothing to resolve a relative asset
    /// path against (see the crate doc's wasm-boundary note on
    /// [`MarketplaceConfig`]). The CLI resolves `build.assets` entries against the
    /// root file's own directory before building this request.
    #[serde(default)]
    pub assets: Vec<AssetSpec>,
    /// Canonical hooks files; see [`HookFileSpec`].
    #[serde(default)]
    pub hooks: Vec<HookFileSpec>,
}

/// One canonical hooks file a build renders into a plugin's packs. At most
/// one per plugin; its gap report goes in every pack. See
/// [`crate::hooks_file`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookFileSpec {
    /// Absolute host path to the file, resolved by the caller the way an
    /// [`AssetSpec::source`] is.
    pub source: PathBuf,
    /// The path relative to the root file.
    /// Refusals and the gap report's `source` name the file by this, so the
    /// report holds no host path.
    pub from: String,
    /// Which plugin's packs carry it. `None` means the marketplace's own
    /// plugin, which holds the members outside `plugins/` and is named after
    /// the marketplace.
    #[serde(default)]
    pub plugin: Option<String>,
}

/// One file, or one directory tree, an export copies verbatim into a built
/// plugin's own root, rather than rendering it from a vault note.
///
/// Bundle resources ([`crate::compile::CompileSource::bundle_root`]) already
/// cover a skill's own peers; this is for what a *plugin* carries once, no
/// matter which skill happens to shell out to it. A launcher script is the
/// motivating file case, and shared reference docs the skills read
/// (`conventions/`) the motivating directory case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSpec {
    /// Absolute host path to the source file or directory. A directory
    /// lands as its whole tree, under the same fixed `tests`/`fixtures`
    /// exclude a skill's bundle sweep applies, and minus `*.test.*` files.
    pub source: PathBuf,
    /// Destination, relative to the built plugin's own root
    /// (`<packs>/<plugin>/`), forward-slashed. For a directory source, the
    /// directory its tree lands under.
    pub dest: String,
    /// Which plugin's root this asset lands in. `None` means the
    /// marketplace's own plugin, named after the marketplace.
    #[serde(default)]
    pub plugin: Option<String>,
}

/// Whether an export writes the marketplace, or only reports its drift.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExportMode {
    /// Render, and swap any drifted root into place.
    #[default]
    Build,
    /// Render into the overlay, compare, write nothing — under any outcome.
    Check,
}

/// What the *build* says about the marketplace and its plugins — the fallback
/// under what the vault's own container notes say.
///
/// `resolve_config` folds the two, note over request, field by field: a
/// marketplace note is the marketplace, and a build flag is one invocation of
/// it.
///
/// A member outside every plugin folder belongs to the marketplace's own
/// plugin, which is named after the marketplace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceConfig {
    /// The name both manifests publish under, **when the vault does not supply
    /// one**. A marketplace note names itself, and that name wins.
    ///
    /// A fallback, not an assertion. Which marketplace to build has its own
    /// field ([`ExportRequest::marketplace`]), validated against what the vault
    /// actually holds; one selector is enough.
    ///
    /// `None` with no marketplace note is the one case with no answer, and it
    /// is refused rather than defaulted: a distribution nobody named is a
    /// configuration fault, not a marketplace called `default`.
    #[serde(default)]
    pub name: Option<String>,
    /// Codex's `interface.displayName`; the name when absent.
    #[serde(default)]
    pub display_name: Option<String>,
    /// The marketplace `owner`. JSON rather than a string because an author
    /// may spell it `{name, email, url}`, but a bare name is **not** a shape
    /// any dialect accepts: Claude and Cursor both mark `owner` required and
    /// both require an object. A string here is normalised to `{name: …}` on
    /// the way into a manifest rather than refused, because the note's
    /// spelling is a convenience and the manifest's shape is the dialect's.
    #[serde(default)]
    pub owner: Option<serde_json::Value>,
    /// The marketplace's own one-line description, from its note. Optional in
    /// every dialect, and placed differently by each — see
    /// `claude_marketplace_manifest` and `cursor_marketplace_manifest`.
    #[serde(default)]
    pub description: Option<String>,
    /// Default plugin author; falls back to `owner` when absent.
    #[serde(default)]
    pub author: Option<serde_json::Value>,
    /// Default plugin license, as an SPDX identifier. A distribution is
    /// licensed as a whole, so the marketplace note states it once and every
    /// plugin under it inherits; a plugin licensed differently says so on its
    /// own note.
    #[serde(default)]
    pub license: Option<String>,
    /// The marketplace's own version, where a dialect carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Which harnesses get what the marketplace holds. A plugin or member
    /// with no `targets` of its own inherits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<String>>,
    /// The source repository URL. Every plugin under it inherits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// The documentation URL. Every plugin under it inherits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// The marketplace's harness blocks, keyed by harness (`codex`, …), each
    /// a map of that harness's own manifest keys. Merged over the generated
    /// marketplace manifest, and inherited by every plugin key by key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub blocks: BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
    /// Per-plugin manifest overrides, keyed by plugin name.
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginOverrides>,
    /// Refuse the build when a note has a `name` and no `description` and
    /// this config supplies none for it, instead of publishing
    /// [`crate::discover::MISSING_DESCRIPTION`] and reporting it in
    /// [`ExportReport::warnings`].
    #[serde(default)]
    pub require_descriptions: bool,
    /// The plugin's launcher, relative to the plugin's root (`bin/tool`).
    /// Skill bodies call it as `${CLAUDE_PLUGIN_ROOT}/<launcher_path>`. On a
    /// root whose harness does not expand that variable, the build rewrites
    /// each call to the launcher's path relative to the skill's own folder,
    /// when the plugin ships an asset at this path. Absent turns the rewrite
    /// off. The root file declares it as `build.launcher_path`.
    #[serde(default)]
    pub launcher_path: Option<String>,
}

/// Manifest fields one plugin overrides. Every absence has a documented
/// default, so an empty override block means the defaults wholesale.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginOverrides {
    /// Default `0.1.0`.
    pub version: Option<String>,
    /// Default `Skills: <names>.` over the root's own member list.
    pub description: Option<String>,
    /// Default the marketplace `author`, then `owner`.
    pub author: Option<serde_json::Value>,
    /// Default the marketplace `license`, then `UNLICENSED`.
    pub license: Option<String>,
    /// Default empty.
    pub keywords: Option<Vec<String>>,
    /// Codex only. Default `Productivity`.
    pub category: Option<String>,
    /// Codex only. Default the plugin's name.
    pub display_name: Option<String>,
    /// Default the marketplace `repository`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Default the marketplace `homepage`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// The plugin note's harness blocks, keyed by harness. Merged over the
    /// marketplace's blocks key by key, then over the plugin's manifest and
    /// its marketplace entry.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub blocks: BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
}

/// What one export did, or would do — per plugin, per root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportReport {
    /// The mode the report describes, so `ok`/`drift` and
    /// `written`/`unchanged` read against the right verb.
    pub mode: ExportMode,
    /// How many publishable skills discovery found.
    pub checked: usize,
    /// Check: anything drifted or stale. Build: anything was swapped.
    pub changed: bool,
    /// One unit per plugin per root, one per marketplace manifest, plus whatever the
    /// destination holds that the vault no longer produces, plus the members a
    /// layout cannot carry. Ordered: the members no root carries first; then roots in
    /// `claude`, `codex`, `cursor`, `pi` order; within a root, plugins by name, then the
    /// manifest, then stale paths, then the members that root cannot carry.
    pub units: Vec<ExportUnit>,
    /// Notes the scan admitted with something filled in, in path order.
    #[serde(default)]
    pub warnings: Vec<DiscoveryWarning>,
}

/// One comparable unit of the marketplace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "kebab-case")]
pub enum ExportUnit {
    /// One plugin directory in one root — its manifest and its skills.
    Plugin {
        /// The harness root the plugin sits in.
        root: String,
        /// The plugin's name.
        name: String,
        /// Member skills admitted to this root.
        skills: usize,
        /// Whether the plugin matches what is installed.
        status: UnitStatus,
        /// This plugin's members drifted from what is installed here, with
        /// its own `version` unmoved since — the one-click bump a build
        /// report can offer alongside `drift`/`written`. `None` for a plugin
        /// that does not meet the rule, or that carries no `version` to
        /// compare. See [`VersionBumpSuggestion`].
        version_bump_suggestion: Option<VersionBumpSuggestion>,
    },
    /// One root's marketplace manifest.
    Manifest {
        /// The harness root the manifest sits in.
        root: String,
        /// Whether the manifest matches what is installed.
        status: UnitStatus,
    },
    /// Present at the destination, absent from the render. `check` reports
    /// it as drift; `build`'s swap removes it.
    ///
    /// `kind` is the difference between losing a whole pack and losing a
    /// leftover file, which is not a difference a reader should have to
    /// infer from the path.
    Stale {
        /// The harness root the path sits in.
        root: String,
        /// The stale path, relative to the destination.
        path: String,
        /// What the stale path is.
        kind: StaleKind,
    },
    /// A component the marketplace layout has no place for, named rather than
    /// dropped.
    ///
    /// A pack has a place for skills, for commands on a root whose
    /// [`crate::publish::plugin_format_carries_commands`] is `true`, and for
    /// hooks on a root whose [`crate::publish::plugin_format_carries_hooks`]
    /// is. Everything else is named here rather than dropped; what a build
    /// must not do is leave silently. This is the same fact
    /// [`crate::publish::PublishReport::not_carried`] reports on the other
    /// path, in this report's own vocabulary.
    ///
    /// `root` is the root that cannot carry it — a command on Codex, say.
    /// `None` when no root can, as for an agent: the reason is the layout's,
    /// so it is listed once. A member `targets` narrowed away from a root
    /// is not named there; the author asked for that.
    NotCarried {
        /// The root that cannot carry the component, or `None` when no root can.
        root: Option<String>,
        /// The component's path in the vault.
        path: String,
        /// The component's kind.
        kind: String,
    },
}

/// What a stale path is, which decides how much a reader should care.
///
/// The swap removes all three the same way. They are not the same news: a
/// plugin is a whole pack the marketplace stops carrying, a file is one
/// leftover, and a directory is a shape the destination kept after its
/// contents went. Reported separately rather than left to be read off the
/// path, which only says `plugins/<name>` for the first of them by
/// coincidence of layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StaleKind {
    /// A plugin directory this root no longer carries — one unit for the
    /// whole tree, never a unit per file inside it.
    Plugin,
    /// A file the render does not produce.
    File,
    /// A directory holding nothing, invisible to a file comparison.
    EmptyDir,
}

/// One unit's outcome. `check` speaks `ok`/`drift`; `build` speaks
/// `written`/`unchanged` — the same comparison, two verbs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitStatus {
    /// The unit matches what is installed (`check`).
    Ok,
    /// The unit differs from what is installed (`check`).
    Drift,
    /// The unit was written (`build`).
    Written,
    /// The unit was already up to date (`build`).
    Unchanged,
}

/// The name a skill publishes under: its directory, its frontmatter `name`,
/// and the name a call to it renders. One function, so the three agree.
///
/// # Errors
///
/// `refused: published-name-invalid` — a prefixed name the Agent Skills spec does
/// not admit. Never truncated: a shortened name would not match its calls.
pub fn published_skill_name(
    naming: SkillNaming,
    plugin: &str,
    skill: &str,
) -> Result<String, CoreError> {
    match naming {
        SkillNaming::HostNamespaced | SkillNaming::Bare => Ok(skill.to_owned()),
        SkillNaming::PluginPrefixed => {
            let name = format!("{plugin}-{skill}");
            match name_refusal(&name, "published skill name") {
                None => Ok(name),
                Some(why) => Err(CoreError::refused(
                    "published-name-invalid",
                    format!("skill `{skill}` of plugin `{plugin}` cannot publish as {why}"),
                )),
            }
        }
    }
}

/// The call one runtime writes for skill `skill` of plugin `plugin`.
pub fn rendered_skill_call(
    runtime: RuntimeId,
    naming: SkillNaming,
    plugin: &str,
    skill: &str,
) -> Result<String, CoreError> {
    let name = published_skill_name(naming, plugin, skill)?;
    match naming {
        SkillNaming::HostNamespaced => checked_skill_call(runtime, Some(plugin), &name),
        SkillNaming::Bare | SkillNaming::PluginPrefixed => checked_skill_call(runtime, None, &name),
    }
}

/// `RuntimeId::skill_call`, with its error as a `skill-call-invalid` refusal.
pub(crate) fn checked_skill_call(
    runtime: RuntimeId,
    namespace: Option<&str>,
    name: &str,
) -> Result<String, CoreError> {
    runtime
        .skill_call(namespace, name)
        .map_err(|err| CoreError::refused("skill-call-invalid", err.to_string()))
}

/// Does `bytes` hold `marker` anywhere?
fn contains(bytes: &[u8], marker: &str) -> bool {
    bytes.windows(marker.len()).any(|w| w == marker.as_bytes())
}

/// Where a build stages and backs up, under the root's own base — created
/// only when a swap is actually needed, removed when the build ends.
///
/// Under the base rather than beside it, because a swap is a rename and a
/// rename is only atomic within one filesystem. A root can be told to land
/// anywhere, so the one directory guaranteed to share a filesystem with the
/// thing being replaced is the thing's own parent. It stays invisible to the
/// diff because [`scan_destination`] reads only the entries a root owns, and
/// this is never one of them.
pub(crate) const STAGE_DIR: &str = ".skillwright-stage";

/// Renders the vault into every selected marketplace root and, per
/// [`ExportRequest::mode`], either swaps the drifted roots into place or
/// reports the drift and writes nothing.
///
/// `ns` is the vault's frontmatter namespace, handed to every read and every
/// emit below — the single-namespace rule [`crate::publish::publish_skill`] states.
///
/// # Errors
///
/// * [`CoreError::Refused`] — the rail: an output root that is, contains, or
///   sits inside the vault, before anything is staged. Also everything
///   discovery and compilation refuse, unchanged.
/// * `refused: empty-marketplace` — the marketplace built holds nothing
///   publishable. A marketplace of nothing is a configuration fault, not an
///   empty success.
/// * `refused: ambiguous-marketplace` — the vault holds more than one and
///   [`ExportRequest::marketplace`] named none of them.
/// * `refused: unknown-marketplace` — it named one the vault does not hold.
/// * `refused: path-collision` — a promoted skill and a marketplace's own
///   skill of that name meeting in one plugin directory.
/// * [`CoreError::SwapFailed`] — the commit failed part-way; the message says
///   what was rolled back.
fn selected_roots(request: &ExportRequest) -> Result<Vec<&'static HarnessProfile>, CoreError> {
    selected_harnesses(&request.harnesses)
}

/// Which harnesses this request renders, each with every path it owns in the
/// marketplace directory.
///
/// So a caller can say what a build is about to replace without owning a
/// second copy of the roots table or of the harness selection.
pub fn owned_paths(request: &ExportRequest) -> Result<Vec<(&'static str, &str)>, CoreError> {
    Ok(selected_roots(request)?
        .into_iter()
        .flat_map(|spec| {
            spec.owned_paths_at(packs_of(&request.config, spec))
                .into_iter()
                .map(|path| (spec.name, path))
        })
        .collect())
}

/// Where `root`'s `marketplace.json` sits under a built marketplace, read
/// from the harness profile. `None` for an unknown root or one that writes no manifest.
pub fn marketplace_manifest_path(root: &str) -> Option<&'static str> {
    with_plugin_format()
        .find(|spec| spec.name == root)
        .and_then(|spec| spec.format().marketplace_manifest)
}

/// `root`'s plugin manifest path, relative to a pack's directory. `None` for
/// an unknown root.
pub fn plugin_manifest_path(root: &str) -> Option<&'static str> {
    with_plugin_format()
        .find(|spec| spec.name == root)
        .map(|spec| spec.format().plugin_manifest)
}

/// The entry under `out_root` (one of `owned`, or the stage) that holds
/// `path`, if any. A swap replaces each of them whole.
fn replaced_entry_holding(
    vfs: &dyn Vfs,
    out_root: &Path,
    owned: &[&str],
    path: &Path,
) -> Result<Option<PathBuf>, CoreError> {
    for entry in owned.iter().copied().chain([STAGE_DIR]) {
        let replaced = out_root.join(entry);
        if within(vfs, &replaced, path)? {
            return Ok(Some(replaced));
        }
    }
    Ok(None)
}

/// Whether `out_root` is the vault root, or a folder inside it with no kind
/// folder on the way, where a build may write its trees.
fn output_beside_the_notes(
    vfs: &dyn Vfs,
    vault_root: &Path,
    out_root: &Path,
) -> Result<bool, CoreError> {
    if !within(vfs, vault_root, out_root)? {
        return Ok(false);
    }
    let Ok(rest) = out_root.strip_prefix(vault_root) else {
        // Inside only once links are resolved: not a spelling to trust.
        return Ok(false);
    };
    Ok(!rest
        .iter()
        .any(|part| crate::discover::kind_dir(&part.to_string_lossy()).is_some()))
}

/// Renders the vault into one marketplace directory, or checks it against what is already there.
///
/// A note's block for a harness is applied in that harness's
/// tree, as a publish to it applies it.
pub fn export_skills(
    vfs: &dyn Vfs,
    request: &ExportRequest,
    ns: &Namespace,
) -> Result<ExportReport, CoreError> {
    let vault_root = normalize(&request.vault_root);
    let out_root = normalize(&request.out_root);

    // Which harnesses this build renders, before anything reads or writes.
    let roots = selected_roots(request)?;
    crate::harness::manifest::check_packs(&request.config)?;
    let owned: Vec<&str> = roots
        .iter()
        .flat_map(|spec| spec.owned_paths_at(packs_of(&request.config, spec)))
        .collect();

    // The rail, before discovery reads a byte: both sides resolved as far as
    // the filesystem goes, so a symlinked spelling of the vault cannot slip a
    // destination inside it.
    //
    // The question is narrow, and it has to be. The case this exists for is a
    // repository root, which legitimately contains its own vault, so an
    // ancestor rule would refuse exactly what the feature is for. What
    // matters is what the swap touches — would any path this build replaces
    // be, sit inside, or contain the vault? Same refusal token, same symlink
    // resolution, and the first two are the rail `publish` already runs
    // against a destination's own relative paths.
    //
    // Narrowed only where the host can see links, as `publish` does. Anywhere
    // else an ancestor of the vault stays refused, because a link under it
    // into the vault is exactly what the narrowed check cannot see.
    //
    // The output may sit inside the vault — beside the root file with
    // `out: .`, or in a folder such as `dist/` — when no kind folder is on the way
    // to it: the vault walk never reads what a build writes there. A folder
    // inside the structure (`plugins/tools/`) stays refused, because its
    // `.claude-plugin/` can be a plugin's own authored manifest.
    if output_beside_the_notes(vfs, &vault_root, &out_root)? {
        // Nothing to check: the vault cannot sit inside a tree it holds.
    } else if vfs.sees_links() {
        assert_destination_allowed_for_vault(vfs, &out_root, &vault_root, owned.iter().copied())?;
        // The third: a swap removes each owned path, and the stage, whole.
        if let Some(replaced) = replaced_entry_holding(vfs, &out_root, &owned, &vault_root)? {
            return Err(CoreError::refused(
                "unsafe-root",
                format!(
                    "the vault {} is inside {}, which a build replaces whole",
                    vault_root.display(),
                    replaced.display()
                ),
            ));
        }
    } else {
        assert_destination_allowed(vfs, &out_root, std::slice::from_ref(&vault_root))?;
    }
    // The swap deletes each path it replaces, so an asset sourced from inside
    // one would be gone after the build that copied it.
    for asset in &request.assets {
        if asset.source.is_absolute()
            && let Some(replaced) = replaced_entry_holding(vfs, &out_root, &owned, &asset.source)?
        {
            return Err(CoreError::refused(
                "unsafe-root",
                format!(
                    "the asset source `{}` is inside {}, which a build replaces \
                     whole. Move the source out of it. Nothing was written",
                    asset.source.display(),
                    replaced.display()
                ),
            ));
        }
    }

    // Load once, for every harness this build renders: the vault read, the
    // marketplace chosen and validated, every body resolved. Everything a
    // build refuses before it renders, it refuses here.
    let market = load(vfs, &request.load_request(), ns)?;

    // One tree per harness, translated from the one model, at the final
    // destination paths. Both modes read this and nothing else; `build`
    // even writes from it.
    let mut rendered: Vec<BTreeMap<String, PreviewWrite>> = Vec::with_capacity(roots.len());
    let mut translators: Vec<&dyn HarnessTranslator> = Vec::with_capacity(roots.len());
    for spec in &roots {
        let translator =
            translator_for(spec.name).expect("every harness with a plugin format translates");
        let tree = translator.translate(&market, ns)?;
        rendered.push(
            tree.files
                .into_iter()
                .map(|(path, file)| {
                    (
                        path,
                        PreviewWrite {
                            bytes: file.bytes,
                            executable: file.executable,
                        },
                    )
                })
                .collect(),
        );
        translators.push(translator);
    }

    // The components no root could take, listed once rather than per root:
    // the reason is the layout's, not any one root's. A component only some
    // roots cannot take is named under each of those roots.
    let mut units: Vec<ExportUnit> = market
        .uncarried
        .iter()
        .map(|c| ExportUnit::NotCarried {
            root: None,
            path: c.path.clone(),
            kind: c.kind.as_str().to_owned(),
        })
        .collect();
    let mut to_swap: Vec<usize> = Vec::new();
    for (index, spec) in roots.iter().enumerate() {
        let admitted = admitted_plugins(translators[index], &market);
        let diff = diff_root(
            vfs,
            &out_root,
            spec,
            &admitted,
            &rendered[index],
            &market.config,
        )?;
        // The one thing a build landing in a directory it does not own must
        // not do quietly. The swap replaces this root's owned paths; holding
        // work this build did not produce, they are far likelier to be a
        // misconfiguration than an invitation.
        //
        // A rebuild is told from a stranger by the rule in
        // [`DestTree::built_here`]. Refused on `check` too — it exists to say
        // what a build would do.
        if diff.foreign {
            return Err(CoreError::refused(
                "foreign-tree",
                format!(
                    "{} already holds `{}`, and it holds files this build does not produce \
                     (and none carries this tool's generated-file marker): rendering `{}` here \
                     would replace it wholesale. Move it aside, or point `build.out` (or \
                     `--out`) somewhere else. Nothing was written",
                    out_root.display(),
                    spec.owned_paths_at(packs_of(&market.config, spec))
                        .join("` and `"),
                    spec.name,
                ),
            ));
        }
        units.extend(diff.units);
        let mut uncarried: Vec<&Component> = market
            .plugins
            .values()
            .flat_map(|plugin| plugin.commands.iter().chain(&plugin.hooks))
            .filter(|c| {
                !kind_carried(&c.discovered, spec) && targets_admit(&c.discovered, spec.name)
            })
            .collect();
        uncarried.sort_by(|a, b| a.path.cmp(&b.path));
        units.extend(uncarried.into_iter().map(|c| ExportUnit::NotCarried {
            root: Some(spec.name.to_owned()),
            path: c.path.clone(),
            kind: c.kind.as_str().to_owned(),
        }));
        if !diff.equal {
            to_swap.push(index);
        }
    }
    let changed = !to_swap.is_empty();

    if request.mode == ExportMode::Check {
        return Ok(ExportReport {
            mode: ExportMode::Check,
            checked: market.checked,
            changed,
            units,
            warnings: market.warnings,
        });
    }

    if changed {
        let swapping: Vec<(&HarnessProfile, &BTreeMap<String, PreviewWrite>)> = to_swap
            .iter()
            .map(|index| (roots[*index], &rendered[*index]))
            .collect();
        commit(vfs, &out_root, &swapping, &market.config)?;
    }
    // The build report is the check's comparison under the write verbs: what
    // matched is unchanged, what differed was (just) written.
    for unit in &mut units {
        match unit {
            ExportUnit::Plugin { status, .. } | ExportUnit::Manifest { status, .. } => {
                *status = match status {
                    UnitStatus::Ok | UnitStatus::Unchanged => UnitStatus::Unchanged,
                    UnitStatus::Drift | UnitStatus::Written => UnitStatus::Written,
                };
            }
            // Neither has a status: one is what a swap removes, the other is
            // what no layout could take.
            ExportUnit::Stale { .. } | ExportUnit::NotCarried { .. } => {}
        }
    }
    Ok(ExportReport {
        mode: ExportMode::Build,
        checked: market.checked,
        changed,
        units,
        warnings: market.warnings,
    })
}

impl ExportRequest {
    /// What this build loads: its vault, its marketplace, its harnesses and
    /// its configuration.
    pub fn load_request(&self) -> LoadRequest {
        LoadRequest {
            vault_root: self.vault_root.clone(),
            skills_root: self.skills_root.clone(),
            marketplace: self.marketplace.clone(),
            harnesses: self.harnesses.clone(),
            config: self.config.clone(),
            assets: self.assets.clone(),
            hooks: self.hooks.clone(),
        }
    }
}

// --- Comparison --------------------------------------------------------------

/// One root's comparison: equality across what it owns (the swap's grain) and
/// the per-unit stories a report tells about it.
struct RootDiff {
    equal: bool,
    units: Vec<ExportUnit>,
    /// What this root owns is already there, and none of it is this build's
    /// — so it was put there by someone else. See [`DestTree::built_here`].
    foreign: bool,
}

/// What the destination root actually holds.
#[derive(Default)]
struct DestState {
    /// Root-relative file paths → is-symlink. A symlink never matches: the
    /// render emits bytes, and a link that reads the same resolves elsewhere.
    files: BTreeMap<String, bool>,
    /// Directories holding nothing — invisible to a file diff, removed by the
    /// swap, so the report has to carry them.
    empty_dirs: Vec<String>,
    /// Immediate directories under `plugins/`, for the stale-plugin sweep.
    plugin_dirs: Vec<String>,
}

/// Reads `base`, but only the paths this harness owns — never the whole
/// directory.
///
/// The scope is the swap's: what the swap would replace is exactly what the
/// report may call stale, or `check` would claim a build removes something it
/// leaves alone. It matters most where a root shares its directory with
/// things that are not marketplace — at a repository root, walking the whole
/// base would read `.git` as drift and rewrite it on every build, and `.agents/`
/// holds a person's skills beside the packs.
fn scan_destination(
    vfs: &dyn Vfs,
    base: &Path,
    spec: &HarnessProfile,
    packs_dir: &str,
) -> Result<DestState, CoreError> {
    let mut state = DestState::default();
    for owned in spec.owned_paths_at(packs_dir) {
        // A link or a file above an owned path is recorded and goes no
        // further: a swap through it would land somewhere else, or fail.
        if let Some((blocker, is_link)) = blocked_above(vfs, base, owned) {
            state.files.insert(blocker, is_link);
            continue;
        }
        let path = base.join(owned);
        // A symlink AT an owned path is one unmatched path, never descended
        // into — the same rule `walk_destination` applies further down.
        if vfs.is_symlink(&path) {
            state.files.insert(owned.to_owned(), true);
        } else if !vfs.exists(&path) {
            continue;
        } else if vfs.read_dir(&path).is_err() {
            // A file: a manifest, or something where the render wants a folder.
            state.files.insert(owned.to_owned(), false);
        } else {
            walk_destination(vfs, base, &path, &mut state)?;
        }
    }
    let packs = base.join(packs_dir);
    if vfs.exists(&packs) {
        let mut entries = vfs.read_dir(&packs)?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        state.plugin_dirs = entries
            .into_iter()
            .filter(|entry| entry.is_dir && !vfs.is_symlink(&packs.join(&entry.name)))
            .map(|entry| entry.name)
            .collect();
    }
    Ok(state)
}

/// The first folder on the way to `owned` that is a link or not a folder, with
/// whether it is a link.
fn blocked_above(vfs: &dyn Vfs, base: &Path, owned: &str) -> Option<(String, bool)> {
    let parent = Path::new(owned).parent()?;
    let mut ancestors: Vec<&Path> = parent
        .ancestors()
        .filter(|path| !path.as_os_str().is_empty())
        .collect();
    ancestors.reverse();
    ancestors.into_iter().find_map(|relative| {
        let path = base.join(relative);
        let is_link = vfs.is_symlink(&path);
        (is_link || (vfs.exists(&path) && vfs.read_dir(&path).is_err()))
            .then(|| (forward_slashed(relative), is_link))
    })
}

fn walk_destination(
    vfs: &dyn Vfs,
    base: &Path,
    dir: &Path,
    state: &mut DestState,
) -> Result<(), CoreError> {
    let mut entries = vfs.read_dir(dir)?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    if entries.is_empty() && dir != base {
        let relative = dir.strip_prefix(base).unwrap_or(dir);
        state.empty_dirs.push(forward_slashed(relative));
        return Ok(());
    }
    for entry in entries {
        let path = dir.join(&entry.name);
        let relative = path.strip_prefix(base).unwrap_or(&path);
        if vfs.is_symlink(&path) {
            // The link itself, wherever it points — a symlinked directory is
            // never descended into, so it records as one unmatched entry.
            state.files.insert(forward_slashed(relative), true);
        } else if entry.is_dir {
            walk_destination(vfs, base, &path, state)?;
        } else {
            state.files.insert(forward_slashed(relative), false);
        }
    }
    Ok(())
}

/// One root's destination, scanned once, with the byte-and-bit comparison
/// every caller shares.
///
/// The comparison lives here rather than in a closure per caller because two
/// callers ask it: the swap, which wants one boolean about all a root owns,
/// and the report, which wants a story per plugin. Read off one scan and one
/// `matches`, they cannot disagree.
struct DestTree<'v> {
    vfs: &'v dyn Vfs,
    base: PathBuf,
    state: DestState,
}

impl<'v> DestTree<'v> {
    fn scan(
        vfs: &'v dyn Vfs,
        base: PathBuf,
        spec: &HarnessProfile,
        packs: &str,
    ) -> Result<Self, CoreError> {
        let state = scan_destination(vfs, &base, spec, packs)?;
        Ok(Self { vfs, base, state })
    }

    /// Does the destination hold exactly this rendered file — a real file
    /// rather than a symlink that happens to read the same, the same bytes,
    /// and the same execute bit?
    fn matches(&self, relative: &str, write: &PreviewWrite) -> bool {
        if self.state.files.get(relative) != Some(&false) {
            return false;
        }
        let path = self.base.join(Path::new(relative));
        self.vfs
            .read_file(&path)
            .is_ok_and(|bytes| bytes == write.bytes)
            && self.vfs.is_executable(&path) == write.executable
    }

    /// The render and the destination bucketed against each other, once.
    fn compare(&self, rendered: &BTreeMap<String, PreviewWrite>) -> FileSets {
        let mut sets = FileSets::default();
        for (relative, write) in rendered {
            let bucket = if self.matches(relative, write) {
                &mut sets.same
            } else {
                &mut sets.differs
            };
            bucket.insert(relative.clone());
        }
        sets.extra = self
            .state
            .files
            .keys()
            .filter(|relative| !rendered.contains_key(*relative))
            .cloned()
            .collect();
        sets
    }

    /// Did a previous build of this vault write what this root owns here?
    ///
    /// **Provenance, not path overlap.** A rendered path being occupied says
    /// only that something is where this build would put something — and a
    /// hand-written marketplace puts its `marketplace.json` at exactly the
    /// path this build renders one to. Asking "is any rendered path present"
    /// therefore answers *yes* for the case that most needs a *no*: a
    /// repository that already is a marketplace, pointed at by `out: "."`,
    /// which is the adoption this feature invites.
    ///
    /// [`GENERATED_MARKER`] is the one thing here that only this crate
    /// writes. Every emitted skill body opens with it, so packs this vault
    /// built carry it and packs someone wrote by hand do not.
    ///
    /// A marker is not always available: only one plugin format carries
    /// commands ([`crate::publish::plugin_format_carries_commands`]), so a
    /// commands-only plugin leaves the other harness manifests and nothing
    /// else. The second clause covers that — holding nothing but paths this
    /// render also produces is holding what this render owns. It also answers
    /// for nothing being here at all.
    ///
    /// **The residue, stated rather than hidden.** A hand-written Claude
    /// marketplace laid out the documented way keeps its packs in a *sibling*
    /// `plugins/`, so all that sits here is its `marketplace.json` — a path
    /// this build renders. That satisfies the second clause and is still
    /// replaced.
    fn built_here(&self, rendered: &BTreeMap<String, PreviewWrite>) -> bool {
        self.state
            .files
            .keys()
            .any(|relative| self.carries_marker(relative))
            || self
                .state
                .files
                .keys()
                .all(|relative| rendered.contains_key(relative))
    }

    /// Does this installed file carry the provenance marker? Read lazily and short-circuited by
    /// the `any` above, so an ordinary rebuild stops at the first skill it
    /// finds.
    fn carries_marker(&self, relative: &str) -> bool {
        self.read(relative)
            .is_some_and(|bytes| contains(&bytes, GENERATED_MARKER))
    }

    /// One installed file's bytes, for the report alone. The comparison
    /// never needs them — `matches` has already answered.
    fn read(&self, relative: &str) -> Option<Vec<u8>> {
        self.vfs
            .read_file(&self.base.join(Path::new(relative)))
            .ok()
    }
}

/// Every path either tree holds, bucketed once: `same` and `differs`
/// partition the render, and `extra` is what the destination holds beyond it.
#[derive(Default)]
struct FileSets {
    same: BTreeSet<String>,
    differs: BTreeSet<String>,
    extra: BTreeSet<String>,
}

/// The swap's own question: is what this root owns exactly the render?
///
/// Every rendered file matched, nothing else present, no empty directory.
/// Given every rendered path matched, the destination holds all of them, so
/// "the destination holds no more files than the render" is exactly `extra`
/// being empty.
fn root_is_equal(sets: &FileSets, dest: &DestTree<'_>) -> bool {
    sets.differs.is_empty() && sets.extra.is_empty() && dest.state.empty_dirs.is_empty()
}

/// One carried plugin's share of the comparison.
#[derive(Default)]
struct PluginSlice<'a> {
    differs: Vec<&'a str>,
    extra: Vec<&'a str>,
    /// An empty directory inside this plugin's tree — invisible to a file
    /// comparison, removed by the swap, so it drifts the plugin holding it.
    dirty: bool,
}

/// Which plugins this root carries, and how many skills each contributes.
///
/// Admission folds all three kinds through the translator's own `admits` —
/// the same call, so the diff cannot admit a plugin the render left out. The
/// count stays skills-only because that is what [`ExportUnit::Plugin`]'s
/// `skills` reports.
fn admitted_plugins<'m>(
    translator: &dyn HarnessTranslator,
    market: &'m Marketplace,
) -> BTreeMap<&'m str, usize> {
    market
        .plugins
        .iter()
        .filter_map(|(name, plugin)| {
            let carried = plugin.components().any(|c| translator.admits(c));
            let skills = plugin
                .skills
                .iter()
                .filter(|c| translator.admits(c))
                .count();
            carried.then_some((name.as_str(), skills))
        })
        .collect()
}

/// The report's side of the comparison: one unit per carried plugin, then
/// this root's manifest, then whatever the destination holds that no unit
/// already speaks for.
///
/// `rendered` is still needed beside `sets`, for one rule: an empty
/// directory is suppressed when the render owns files beneath it, and that
/// is a prefix query rather than a set membership.
fn describe_root(
    dest: &DestTree<'_>,
    spec: &HarnessProfile,
    admitted: &BTreeMap<&str, usize>,
    rendered: &BTreeMap<String, PreviewWrite>,
    sets: &FileSets,
    config: &MarketplaceConfig,
) -> Vec<ExportUnit> {
    let packs = packs_of(config, spec);
    // A destination plugin directory this root no longer carries. It is one
    // unit, and its files are never reported on their own: the swap removes
    // the directory whole, and a line per file inside it would say the same
    // thing many times.
    let stale_plugins: BTreeSet<&str> = dest
        .state
        .plugin_dirs
        .iter()
        .map(String::as_str)
        .filter(|name| !admitted.contains_key(name))
        .collect();

    // Bucket what carries a story by the plugin that owns it. Membership is
    // decided against the two NAME sets, never by path shape alone: a
    // symlinked plugin directory is absent from `plugin_dirs` and present
    // among the files, so it is neither carried nor stale, and falls to the
    // loose pile like any other stray.
    let mut slices: BTreeMap<&str, PluginSlice> = BTreeMap::new();
    let mut loose: Vec<&str> = Vec::new();
    for relative in &sets.extra {
        match HarnessProfile::split_pack(packs, relative) {
            Some((name, tail)) if admitted.contains_key(name) && !tail.is_empty() => {
                slices.entry(name).or_default().extra.push(relative);
            }
            Some((name, _)) if stale_plugins.contains(name) => {}
            _ => loose.push(relative),
        }
    }
    for relative in &sets.differs {
        if let Some((name, tail)) = HarnessProfile::split_pack(packs, relative)
            && !tail.is_empty()
            && admitted.contains_key(name)
        {
            slices.entry(name).or_default().differs.push(relative);
        }
    }

    // Empty directories, in the one order that keeps each to a single story:
    // inside a carried plugin it drifts that plugin; inside a stale one that
    // plugin's unit already speaks for it; where the render owns files
    // beneath it the owning unit reports the gap; otherwise it is stale on
    // its own.
    let mut stale_paths: Vec<(String, StaleKind)> = Vec::new();
    for dir in &dest.state.empty_dirs {
        match HarnessProfile::split_pack(packs, dir) {
            Some((name, tail)) if admitted.contains_key(name) && !tail.is_empty() => {
                slices.entry(name).or_default().dirty = true;
            }
            Some((name, _)) if stale_plugins.contains(name) => {}
            _ if rendered.keys().any(|k| k.starts_with(&format!("{dir}/"))) => {}
            _ => stale_paths.push((dir.clone(), StaleKind::EmptyDir)),
        }
    }

    let mut units = Vec::new();
    for (plugin, skills) in admitted {
        let slice = slices.get(plugin);
        let ok = slice.is_none_or(|s| s.differs.is_empty() && s.extra.is_empty() && !s.dirty);
        units.push(ExportUnit::Plugin {
            root: spec.name.to_owned(),
            name: (*plugin).to_owned(),
            skills: *skills,
            status: if ok {
                UnitStatus::Ok
            } else {
                UnitStatus::Drift
            },
            version_bump_suggestion: plugin_version_bump(
                dest, spec, plugin, rendered, slice, config,
            ),
        });
    }

    if let Some(manifest) = spec.format().marketplace_manifest {
        units.push(ExportUnit::Manifest {
            root: spec.name.to_owned(),
            status: if sets.same.contains(manifest) {
                UnitStatus::Ok
            } else {
                UnitStatus::Drift
            },
        });
    }

    for name in &stale_plugins {
        units.push(ExportUnit::Stale {
            root: spec.name.to_owned(),
            path: format!("{packs}/{name}"),
            kind: StaleKind::Plugin,
        });
    }
    stale_paths.extend(
        loose
            .into_iter()
            .map(|path| (path.to_owned(), StaleKind::File)),
    );
    stale_paths.sort_by(|a, b| a.0.cmp(&b.0));
    stale_paths.dedup_by(|a, b| a.0 == b.0);
    for (path, kind) in stale_paths {
        units.push(ExportUnit::Stale {
            root: spec.name.to_owned(),
            path,
            kind,
        });
    }
    units
}

/// The version-bump signal for one plugin at one root: its MEMBER files
/// drifted, and the installed pack manifest still carries the version the
/// vault currently declares.
///
/// `None` when the plugin's note carries no `version`, or when nothing
/// among its members actually changed — the pack manifest can drift on its
/// own (a description, an author) without a member having moved, and that is
/// not this rule. The tail of the rule, the one that reads the installed
/// manifest back, is [`crate::status::version_bump_if_unmoved`], shared with
/// `status_of`.
fn plugin_version_bump(
    dest: &DestTree<'_>,
    spec: &HarnessProfile,
    plugin: &str,
    rendered: &BTreeMap<String, PreviewWrite>,
    slice: Option<&PluginSlice<'_>>,
    config: &MarketplaceConfig,
) -> Option<VersionBumpSuggestion> {
    let current_version = overrides_for(config, plugin).and_then(|o| o.version.clone())?;
    let manifest_key = format!(
        "{}/{plugin}/{}",
        packs_of(config, spec),
        spec.format().plugin_manifest
    );
    if !rendered.contains_key(&manifest_key) {
        return None;
    }
    // Members changed when a rendered member did not match, or when the
    // destination holds a member the render does not.
    let slice = slice?;
    let members_changed =
        slice.differs.iter().any(|p| *p != manifest_key) || !slice.extra.is_empty();
    if !members_changed {
        return None;
    }
    version_bump_if_unmoved(dest.read(&manifest_key).as_deref(), current_version)
}

/// One root's comparison, both answers from one reading of the destination.
///
/// The two halves are computed side by side here, and nowhere else, because
/// `check` promises its verdict is `build`'s: a report saying `drift` while
/// the swap sees an equal tree, or the reverse, is the one way this file
/// could lie.
fn diff_root(
    vfs: &dyn Vfs,
    out_root: &Path,
    spec: &HarnessProfile,
    admitted: &BTreeMap<&str, usize>,
    rendered: &BTreeMap<String, PreviewWrite>,
    config: &MarketplaceConfig,
) -> Result<RootDiff, CoreError> {
    let dest = DestTree::scan(vfs, out_root.to_path_buf(), spec, packs_of(config, spec))?;
    let sets = dest.compare(rendered);
    Ok(RootDiff {
        equal: root_is_equal(&sets, &dest),
        units: describe_root(&dest, spec, admitted, rendered, &sets, config),
        foreign: !dest.built_here(rendered),
    })
}

fn commit(
    vfs: &dyn Vfs,
    out_root: &Path,
    swapping: &[(&HarnessProfile, &BTreeMap<String, PreviewWrite>)],
    config: &MarketplaceConfig,
) -> Result<(), CoreError> {
    let stage = out_root.join(STAGE_DIR);
    // A stage left by a crashed run is dead weight; a fresh build owns the
    // path.
    if vfs.exists(&stage) {
        vfs.remove(&stage)?;
    }
    vfs.create_dir_all(&stage.join("backup"))?;
    vfs.create_dir_all(out_root)?;

    for (_, rendered) in swapping {
        for (relative, write) in *rendered {
            let path = stage.join("new").join(Path::new(relative));
            if let Some(parent) = path.parent() {
                vfs.create_dir_all(parent)?;
            }
            vfs.write_file(&path, &write.bytes)?;
            if write.executable {
                vfs.set_executable(&path, true)?;
            }
        }
    }

    let entries = swapping
        .iter()
        .flat_map(|(spec, _)| spec.owned_paths_at(packs_of(config, spec)));
    let mut swapped: Vec<(PathBuf, PathBuf)> = Vec::new();
    for entry in entries {
        let dest = out_root.join(entry);
        let staged = stage.join("new").join(entry);
        let saved = stage.join("backup").join(entry);
        let staged = vfs.exists(&staged).then_some(staged);
        // Nothing here and nothing to put here: a root that renders nothing.
        if staged.is_none() && !vfs.exists(&dest) {
            continue;
        }
        for path in [&saved, &dest] {
            if let Some(parent) = path.parent() {
                vfs.create_dir_all(parent)?;
            }
        }
        if let Err(error) = swap_one(vfs, &dest, &saved, staged.as_deref()) {
            // Everything the rollback could not put back, the failing path
            // included: `swap_one` restores its own backup best-effort, and
            // this is the check on whether it managed.
            let mut unrestored: Vec<PathBuf> = Vec::new();
            if !vfs.exists(&dest) && vfs.exists(&saved) {
                unrestored.push(dest.clone());
            }
            if let Err(failed) = roll_back(vfs, &swapped) {
                unrestored.push(failed);
            }
            let mut message = format!(
                "swapping `{entry}` into {}: {error}; everything already swapped was rolled back \
                 and the destination is as it was",
                out_root.display()
            );
            if unrestored.is_empty() {
                let _ = vfs.remove(&stage);
            } else {
                // The one state this function cannot leave clean: name what
                // was not restored and keep its backup instead of deleting it.
                let names: Vec<String> =
                    unrestored.iter().map(|p| p.display().to_string()).collect();
                message.push_str(&format!(
                    " — except {}, whose rollback failed; the backup survives under {}",
                    names.join(", "),
                    stage.join("backup").display()
                ));
            }
            return Err(CoreError::SwapFailed(message));
        }
        swapped.push((dest, saved));
    }
    for (dest, _) in &swapped {
        remove_emptied_folders(vfs, out_root, dest)?;
    }
    vfs.remove(&stage)?;
    Ok(())
}

/// Removes the folders above a swapped-out `path` that it left empty, up to
/// `out_root`. A swap that kept the path leaves them all in place.
fn remove_emptied_folders(vfs: &dyn Vfs, out_root: &Path, path: &Path) -> Result<(), CoreError> {
    if vfs.exists(path) {
        return Ok(());
    }
    for folder in path.ancestors().skip(1) {
        if folder == out_root || !vfs.read_dir(folder)?.is_empty() {
            break;
        }
        vfs.remove(folder)?;
    }
    Ok(())
}

/// One path's swap: the live entry aside, the staged entry in. On failure its
/// own backup is restored before the error surfaces, so the caller only ever
/// rolls back *completed* swaps.
fn swap_one(
    vfs: &dyn Vfs,
    dest: &Path,
    saved: &Path,
    staged: Option<&Path>,
) -> Result<(), CoreError> {
    if vfs.exists(dest) {
        vfs.rename(dest, saved)?;
    }
    // Nothing staged is not nothing happening: the entry was owned, it is
    // backed up, and leaving it unrestored is how what the vault no longer
    // produces goes away.
    let Some(staged) = staged else {
        return Ok(());
    };
    if let Err(error) = vfs.rename(staged, dest) {
        if vfs.exists(saved) && !vfs.exists(dest) {
            let _ = vfs.rename(saved, dest);
        }
        return Err(error.into());
    }
    Ok(())
}

/// Unwinds completed swaps, newest first. `Err` names the first destination
/// whose restore failed; its backup is deliberately left in place.
fn roll_back(vfs: &dyn Vfs, swapped: &[(PathBuf, PathBuf)]) -> Result<(), PathBuf> {
    for (dest, saved) in swapped.iter().rev() {
        if vfs.exists(dest) && vfs.remove(dest).is_err() {
            return Err(dest.clone());
        }
        if vfs.exists(saved) && vfs.rename(saved, dest).is_err() {
            return Err(dest.clone());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::harness::translator::{
        relative_path, rewrite_launcher_calls, rewrite_sibling_paths,
    };
    use std::cell::Cell;

    use serde_json::json;

    use super::*;
    use crate::discover::{MISSING_DESCRIPTION, MISSING_DESCRIPTION_REASON};
    use crate::vfs::{MemVfs, NativeVfs};

    fn sw() -> Namespace {
        Namespace::sw()
    }

    fn write(vfs: &dyn Vfs, path: &str, content: &str) {
        let path = Path::new(path);
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        vfs.write_file(path, content.as_bytes()).unwrap();
    }

    /// A publishable note in this vault's vocabulary, with a caller-supplied
    /// body — for a test that needs to control what a skill's rendered text
    /// actually says (e.g. whether it calls the CLI at all).
    fn note_with_body(name: &str, targets: &[&str], body: &str) -> String {
        let mut out = format!("---\nname: {name}\ndescription: Use {name}.\n");
        if !targets.is_empty() {
            out.push_str("sw-targets:\n");
            for target in targets {
                out.push_str(&format!("  - {target}\n"));
            }
        }
        out.push_str("---\n\n");
        out.push_str(body);
        out
    }

    /// A publishable note in this vault's vocabulary.
    fn note(name: &str, targets: &[&str]) -> String {
        note_with_body(name, targets, &format!("# {name}\n"))
    }

    /// The marketplace layout spec example 4: two plugins, one target narrowing.
    fn example_vault() -> MemVfs {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper.md",
            &note("commit-helper", &[]),
        );
        // The runtime spelling on purpose: `claude-code` must narrow the
        // `claude` root, the same folding a publish grants.
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/rebase-guide.md",
            &note("rebase-guide", &["claude-code"]),
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide.md",
            &note("review-guide", &[]),
        );
        vfs
    }

    /// The launcher path the launcher tests configure and call.
    const LAUNCHER: &str = "bin/tool";

    /// Overwrites `example_vault()`'s `review-guide` note so its body calls
    /// the launcher at `launcher` the vault-standard way.
    fn write_review_guide_calling(vfs: &dyn Vfs, launcher: &str) {
        write(
            vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide.md",
            &note_with_body(
                "review-guide",
                &[],
                &format!(
                    "# review-guide\n\nRun `${{CLAUDE_PLUGIN_ROOT}}/{launcher} entities validate`.\n"
                ),
            ),
        );
    }

    /// A build of `example_vault()` whose `review-guide` calls [`LAUNCHER`],
    /// with `launcher_path` set to it and `assets` shipped.
    fn launcher_build(vfs: &MemVfs, assets: Vec<AssetSpec>) {
        write_review_guide_calling(vfs, LAUNCHER);
        write(vfs, "/host/bin/tool", "#!/bin/sh\nexec tool \"$@\"\n");
        let mut request = request(ExportMode::Build);
        request.config.launcher_path = Some(LAUNCHER.to_owned());
        request.assets = assets;
        export_skills(vfs, &request, &sw()).unwrap();
    }

    fn launcher_asset() -> AssetSpec {
        AssetSpec {
            source: PathBuf::from("/host/bin/tool"),
            dest: LAUNCHER.to_owned(),
            plugin: Some("tools".to_owned()),
        }
    }

    fn config() -> MarketplaceConfig {
        MarketplaceConfig {
            name: Some("demo-market".to_owned()),
            display_name: None,
            owner: None,
            description: None,
            author: None,
            license: None,
            version: None,
            targets: None,
            repository: None,
            homepage: None,
            blocks: BTreeMap::new(),
            plugins: BTreeMap::new(),
            require_descriptions: false,
            launcher_path: None,
        }
    }

    fn request(mode: ExportMode) -> ExportRequest {
        ExportRequest {
            vault_root: PathBuf::from("/vault"),
            skills_root: None,
            marketplace: None,
            out_root: PathBuf::from("/market"),
            harnesses: BTreeSet::new(),
            mode,
            config: config(),
            assets: Vec::new(),
            hooks: Vec::new(),
        }
    }

    fn build(vfs: &dyn Vfs) -> ExportReport {
        export_skills(vfs, &request(ExportMode::Build), &sw()).unwrap()
    }

    /// A build whose root file says something, for the configuration rungs
    /// a vault note cannot supply.
    fn build_with(vfs: &dyn Vfs, config: MarketplaceConfig) -> ExportReport {
        let mut request = request(ExportMode::Build);
        request.config = config;
        export_skills(vfs, &request, &sw()).unwrap()
    }

    fn check(vfs: &dyn Vfs) -> ExportReport {
        export_skills(vfs, &request(ExportMode::Check), &sw()).unwrap()
    }

    /// Every file under `root`: relative path, bytes, execute bit — sorted.
    fn files_under(vfs: &dyn Vfs, root: &Path) -> Vec<(String, Vec<u8>, bool)> {
        let mut out: Vec<_> = crate::walk::readable_files(vfs, root)
            .into_iter()
            .map(|entry| {
                (
                    entry.rel,
                    vfs.read_file(&entry.abs).unwrap(),
                    vfs.is_executable(&entry.abs),
                )
            })
            .collect();
        out.sort();
        out
    }

    fn paths_under(vfs: &dyn Vfs, root: &Path) -> Vec<String> {
        files_under(vfs, root)
            .into_iter()
            .map(|(path, _, _)| path)
            .collect()
    }

    fn text_at(vfs: &dyn Vfs, path: &str) -> String {
        String::from_utf8(vfs.read_file(Path::new(path)).unwrap()).unwrap()
    }

    /// Which (root, unit-name) pairs a report says differ — `drift` for a
    /// check, `written` for a build. Manifest units count under the name
    /// `marketplace.json`.
    fn differing(report: &ExportReport) -> BTreeSet<(String, String)> {
        report
            .units
            .iter()
            .filter_map(|unit| match unit {
                ExportUnit::Plugin {
                    root,
                    name,
                    status: UnitStatus::Drift | UnitStatus::Written,
                    ..
                } => Some((root.clone(), name.clone())),
                ExportUnit::Manifest {
                    root,
                    status: UnitStatus::Drift | UnitStatus::Written,
                } => Some((root.clone(), "marketplace.json".to_owned())),
                _ => None,
            })
            .collect()
    }

    fn stale_set(report: &ExportReport) -> BTreeSet<(String, String, StaleKind)> {
        report
            .units
            .iter()
            .filter_map(|unit| match unit {
                ExportUnit::Stale { root, path, kind } => Some((root.clone(), path.clone(), *kind)),
                _ => None,
            })
            .collect()
    }

    fn plugin_status(report: &ExportReport, root: &str, name: &str) -> UnitStatus {
        report
            .units
            .iter()
            .find_map(|unit| match unit {
                ExportUnit::Plugin {
                    root: r,
                    name: n,
                    status,
                    ..
                } if r == root && n == name => Some(*status),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no plugin unit for {root}/{name}: {report:?}"))
    }

    fn plugin_version_bump(
        report: &ExportReport,
        root: &str,
        name: &str,
    ) -> Option<VersionBumpSuggestion> {
        report
            .units
            .iter()
            .find_map(|unit| match unit {
                ExportUnit::Plugin {
                    root: r,
                    name: n,
                    version_bump_suggestion,
                    ..
                } if r == root && n == name => Some(version_bump_suggestion.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no plugin unit for {root}/{name}: {report:?}"))
    }

    // A build swaps whole roots and sweeps what the vault no longer produces,
    // so a note it could not read would look exactly like a note that was
    // deleted. It refuses instead, and `check` refuses with it — one path,
    // one verdict.
    #[test]
    fn a_note_the_scan_could_not_read_stops_the_build_and_the_check() {
        let vfs = example_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/draft.md",
            "---\nname: Not A Legal Name\ndescription: A draft.\n---\n\nHalf written.\n",
        );

        for mode in [ExportMode::Build, ExportMode::Check] {
            let err = export_skills(&vfs, &request(mode), &sw()).unwrap_err();
            let message = err.to_string();
            assert!(message.contains("unreadable-note"), "{message}");
            assert!(message.contains("skills/draft.md"), "{message}");
        }
        // The refusal lands before anything is staged.
        assert!(paths_under(&vfs, Path::new("/market")).is_empty());
    }

    // The opt-in is `description` alone, so a stray carrying one reaches the
    // build's refusal the same way a malformed entry note does. A resource
    // declares neither key and is still silent, which is what keeps the build
    // buildable at all.
    #[test]
    fn a_stray_declaring_only_a_description_stops_the_build_and_a_resource_does_not() {
        let vfs = example_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/notes/stray.md",
            "---\ndescription: half a draft\n---\n\nHalf written.\n",
        );

        let err = export_skills(&vfs, &request(ExportMode::Build), &sw()).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("unreadable-note"), "{message}");
        assert!(message.contains("skills/notes/stray.md"), "{message}");
        assert!(paths_under(&vfs, Path::new("/market")).is_empty());

        let vfs = example_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/notes/prose.md",
            "Just prose.\n",
        );
        build(&vfs);
    }

    #[test]
    fn build_produces_the_standalone_trees_and_nothing_else() {
        let vfs = example_vault();
        let report = build(&vfs);

        assert_eq!(report.checked, 3);
        assert!(report.changed);
        // The whole output, byte-listed: one tree per harness, each with its
        // own manifest dialect and, where the harness has one, its own
        // marketplace manifest; the narrowed skill in claude's alone — and no
        // shared tree, no root-level manifests, no stage residue.
        assert_eq!(
            paths_under(&vfs, Path::new("/market")),
            vec![
                ".agents/packs/git-tools/.codex-plugin/plugin.json",
                ".agents/packs/git-tools/skills/commit-helper/SKILL.md",
                ".agents/packs/tools/.codex-plugin/plugin.json",
                ".agents/packs/tools/skills/review-guide/SKILL.md",
                ".agents/plugins/marketplace.json",
                ".claude-plugin/marketplace.json",
                ".claude-plugin/packs/git-tools/.claude-plugin/plugin.json",
                ".claude-plugin/packs/git-tools/skills/commit-helper/SKILL.md",
                ".claude-plugin/packs/git-tools/skills/rebase-guide/SKILL.md",
                ".claude-plugin/packs/tools/.claude-plugin/plugin.json",
                ".claude-plugin/packs/tools/skills/review-guide/SKILL.md",
                ".cursor-plugin/marketplace.json",
                ".cursor-plugin/packs/git-tools/.cursor-plugin/plugin.json",
                ".cursor-plugin/packs/git-tools/skills/commit-helper/SKILL.md",
                ".cursor-plugin/packs/tools/.cursor-plugin/plugin.json",
                ".cursor-plugin/packs/tools/skills/review-guide/SKILL.md",
                ".pi/packs/git-tools/package.json",
                ".pi/packs/git-tools/skills/git-tools-commit-helper/SKILL.md",
                ".pi/packs/tools/package.json",
                ".pi/packs/tools/skills/tools-review-guide/SKILL.md",
            ]
        );
        // The per-root member counts say the narrowing happened where the
        // listing says it did.
        let git_claude = report.units.iter().any(|u| {
            matches!(u, ExportUnit::Plugin { root, name, skills, .. }
                if root == "claude" && name == "git-tools" && *skills == 2)
        });
        let git_codex = report.units.iter().any(|u| {
            matches!(u, ExportUnit::Plugin { root, name, skills, .. }
                if root == "codex" && name == "git-tools" && *skills == 1)
        });
        assert!(git_claude && git_codex, "{report:?}");
    }

    #[test]
    fn a_plugin_folder_s_assets_and_changelog_land_at_its_pack_root() {
        let vfs = example_vault();
        let folder = "/vault/marketplaces/house/plugins/tools";
        write(&vfs, &format!("{folder}/assets/bin/tool"), "#!/bin/sh\n");
        write(
            &vfs,
            &format!("{folder}/CHANGELOG.md"),
            "# Changelog\n\n## 0.9.0\n",
        );
        write(
            &vfs,
            &format!("{folder}/tools.md"),
            "---\nname: tools\ndescription: Tools.\nsw-version: 1.0.0\n---\n",
        );
        let report = export_skills(&vfs, &request(ExportMode::Build), &sw()).unwrap();

        let paths = paths_under(&vfs, Path::new("/market"));
        for path in [
            ".claude-plugin/packs/tools/bin/tool",
            ".claude-plugin/packs/tools/CHANGELOG.md",
            ".pi/packs/tools/CHANGELOG.md",
        ] {
            assert!(paths.contains(&path.to_owned()), "{path}: {paths:?}");
        }
        // The folder itself never ships under its own name.
        assert!(!paths.iter().any(|p| p.contains("/assets/")), "{paths:?}");
        // 1.0.0 has no heading in the changelog.
        let warned: Vec<&str> = report.warnings.iter().map(|w| w.reason.as_str()).collect();
        assert!(warned.contains(&"changelog-missing-version"), "{warned:?}");
    }

    /// `assets/` is copied as it is: not read as notes, and with nothing left
    /// out, whatever the folders are called.
    #[test]
    fn a_plugin_folder_s_assets_land_whole_and_are_not_notes() {
        let vfs = example_vault();
        let assets = "/vault/marketplaces/house/plugins/tools/assets";
        write(
            &vfs,
            &format!("{assets}/skills/helper.md"),
            &note("helper", &[]),
        );
        write(&vfs, &format!("{assets}/docs/guide.txt"), "Guide.\n");
        write(&vfs, &format!("{assets}/tests/case.test.txt"), "Case.\n");
        let report = export_skills(&vfs, &request(ExportMode::Build), &sw()).unwrap();

        assert_eq!(report.checked, 3, "the asset is not a fourth note");
        let paths = paths_under(&vfs, Path::new("/market"));
        for path in [
            ".claude-plugin/packs/tools/skills/helper.md",
            ".claude-plugin/packs/tools/docs/guide.txt",
            ".claude-plugin/packs/tools/tests/case.test.txt",
        ] {
            assert!(paths.contains(&path.to_owned()), "{path}: {paths:?}");
        }
    }

    /// The warnings a build with this `CHANGELOG.md` and plugin note gives.
    fn changelog_reasons(changelog: &str, plugin_note: Option<&str>) -> Vec<String> {
        let vfs = example_vault();
        let folder = "/vault/marketplaces/house/plugins/tools";
        write(&vfs, &format!("{folder}/CHANGELOG.md"), changelog);
        if let Some(plugin_note) = plugin_note {
            write(&vfs, &format!("{folder}/tools.md"), plugin_note);
        }
        let report = export_skills(&vfs, &request(ExportMode::Build), &sw()).unwrap();
        report.warnings.into_iter().map(|w| w.reason).collect()
    }

    #[test]
    fn a_changelog_heading_names_a_version_only_as_a_whole_token() {
        let plugin = "---\nname: tools\ndescription: Tools.\nsw-version: 1.0.0\n---\n";
        let warned = |changelog: &str| {
            changelog_reasons(changelog, Some(plugin))
                .contains(&"changelog-missing-version".to_owned())
        };
        for changelog in ["## 1.0.0\n", "## [1.0.0] - 2026-01-02\n", "## v1.0.0\n"] {
            assert!(!warned(changelog), "{changelog}");
        }
        for changelog in ["## 11.0.0\n", "## 1.0.0-rc.1\n", "1.0.0\n"] {
            assert!(warned(changelog), "{changelog}");
        }
    }

    #[test]
    fn a_plugin_with_no_note_is_checked_at_the_version_it_ships() {
        let warned = |changelog: &str| {
            changelog_reasons(changelog, None).contains(&"changelog-missing-version".to_owned())
        };
        assert!(warned("# Changelog\n"));
        assert!(!warned("## 0.1.0\n"));
    }

    #[test]
    fn members_outside_plugins_land_in_a_plugin_named_after_the_marketplace() {
        let vfs = MemVfs::new();
        write(&vfs, "/vault/skills/tone.md", &note("tone", &[]));
        export_skills(&vfs, &request(ExportMode::Build), &sw()).unwrap();
        assert!(
            paths_under(&vfs, Path::new("/market"))
                .contains(&".claude-plugin/packs/demo-market/skills/tone/SKILL.md".to_owned())
        );

        // A plugin folder of that name as well is two sources for one plugin.
        write(
            &vfs,
            "/vault/plugins/demo-market/skills/voice.md",
            &note("voice", &[]),
        );
        let err = export_skills(&vfs, &request(ExportMode::Build), &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("own-plugin-clash"), "{err}");
    }

    /// The default output is beside the root file, inside the vault. What the
    /// first build writes there is not read back as notes by the second.
    #[test]
    fn output_inside_the_vault_is_not_read_back_as_notes() {
        let vfs = example_vault();
        let request = ExportRequest {
            out_root: PathBuf::from("/vault"),
            ..request(ExportMode::Build)
        };
        export_skills(&vfs, &request, &sw()).unwrap();
        let again = export_skills(
            &vfs,
            &ExportRequest {
                mode: ExportMode::Check,
                ..request.clone()
            },
            &sw(),
        )
        .unwrap();
        assert!(!again.changed, "{:?}", again.units);
        assert_eq!(again.checked, 3);
    }

    #[test]
    fn a_plugin_asset_lands_verbatim_in_its_own_plugin() {
        let vfs = example_vault();
        write(&vfs, "/host/bin/tool", "#!/bin/sh\nexec tool \"$@\"\n");
        vfs.set_executable(Path::new("/host/bin/tool"), true)
            .unwrap();
        let mut request = request(ExportMode::Build);
        request.assets = vec![launcher_asset()];
        assert!(export_skills(&vfs, &request, &sw()).unwrap().changed);

        let asset_at = Path::new("/market/.claude-plugin/packs/tools/bin/tool");
        assert_eq!(
            vfs.read_file(asset_at).unwrap(),
            b"#!/bin/sh\nexec tool \"$@\"\n"
        );
        assert!(vfs.is_executable(asset_at));
        assert!(!vfs.exists(Path::new("/market/.claude-plugin/packs/git-tools/bin/tool")));
    }

    /// Codex, Cursor and Pi do not expand `${CLAUDE_PLUGIN_ROOT}`, so their
    /// launcher call becomes a path from the skill's folder; Claude keeps the
    /// token. Nothing else lands in the skill's folder.
    #[test]
    fn each_harness_without_the_token_gets_a_skill_relative_launcher_path() {
        let vfs = example_vault();
        launcher_build(&vfs, vec![launcher_asset()]);

        let claude = text_at(
            &vfs,
            "/market/.claude-plugin/packs/tools/skills/review-guide/SKILL.md",
        );
        assert!(
            claude.contains("`${CLAUDE_PLUGIN_ROOT}/bin/tool entities validate`"),
            "{claude}"
        );

        for skill in [
            "/market/.agents/packs/tools/skills/review-guide",
            "/market/.cursor-plugin/packs/tools/skills/review-guide",
            "/market/.pi/packs/tools/skills/tools-review-guide",
        ] {
            let body = text_at(&vfs, &format!("{skill}/SKILL.md"));
            assert!(
                body.contains("`../../bin/tool entities validate`"),
                "{skill}: {body}"
            );
            assert!(!body.contains("CLAUDE_PLUGIN_ROOT"), "{skill}: {body}");
            assert!(!vfs.exists(&Path::new(skill).join("scripts")), "{skill}");
        }
    }

    /// With no `launcher_path`, nothing is rewritten on any root.
    #[test]
    fn no_launcher_path_leaves_every_call_as_written() {
        let vfs = example_vault();
        write_review_guide_calling(&vfs, LAUNCHER);
        write(&vfs, "/host/bin/tool", "#!/bin/sh\n");
        let mut request = request(ExportMode::Build);
        request.assets = vec![launcher_asset()];
        export_skills(&vfs, &request, &sw()).unwrap();

        let body = text_at(
            &vfs,
            "/market/.agents/packs/tools/skills/review-guide/SKILL.md",
        );
        assert!(body.contains("${CLAUDE_PLUGIN_ROOT}/bin/tool"), "{body}");
    }

    /// A plugin that ships nothing at `launcher_path` keeps its calls: a
    /// relative path would name a file that is not there.
    #[test]
    fn a_plugin_without_the_launcher_asset_keeps_its_calls() {
        let vfs = example_vault();
        launcher_build(
            &vfs,
            vec![AssetSpec {
                dest: "bin/other".to_owned(),
                ..launcher_asset()
            }],
        );
        let body = text_at(
            &vfs,
            "/market/.agents/packs/tools/skills/review-guide/SKILL.md",
        );
        assert!(body.contains("${CLAUDE_PLUGIN_ROOT}/bin/tool"), "{body}");
    }

    /// The launcher lands at `bin/tool` from a `./`-spelled dest and from a
    /// directory asset holding it, so both get the rewritten call.
    #[test]
    fn a_launcher_from_a_dot_dest_or_a_directory_asset_is_rewritten() {
        let file = AssetSpec {
            dest: "./bin/tool".to_owned(),
            ..launcher_asset()
        };
        let directory = AssetSpec {
            source: PathBuf::from("/host/bin"),
            dest: "bin".to_owned(),
            plugin: Some("tools".to_owned()),
        };
        for asset in [file, directory] {
            let vfs = example_vault();
            let dest = asset.dest.clone();
            launcher_build(&vfs, vec![asset]);

            assert!(vfs.exists(Path::new("/market/.agents/packs/tools/bin/tool")));
            let body = text_at(
                &vfs,
                "/market/.agents/packs/tools/skills/review-guide/SKILL.md",
            );
            assert!(body.contains("`../../bin/tool entities"), "{dest}: {body}");
        }
    }

    /// `<plugin>-<skill>` is not injective: plugin `a` skill `b-c` and plugin
    /// `a-b` skill `c` both publish as `a-b-c` on pi, so the build refuses.
    #[test]
    fn two_plugins_whose_prefixed_skill_names_meet_are_refused() {
        let vfs = MemVfs::new();
        write(&vfs, "/vault/plugins/a/skills/b-c.md", &note("b-c", &[]));
        write(&vfs, "/vault/plugins/a-b/skills/c.md", &note("c", &[]));
        let message = export_skills(&vfs, &request(ExportMode::Build), &sw())
            .unwrap_err()
            .to_string();
        assert!(message.contains("path-collision"), "{message}");
        assert!(message.contains("a-b-c"), "{message}");
        assert!(message.contains("plugins/a/skills/b-c.md"), "{message}");
        assert!(message.contains("plugins/a-b/skills/c.md"), "{message}");
        assert!(!vfs.exists(Path::new("/market")));
    }

    /// The swap replaces each tree whole, so a rendered path that turns from
    /// a file into a directory (or back) between builds must rebuild, and
    /// `check` must report it rather than fail.
    #[test]
    fn a_rendered_path_that_switches_between_file_and_directory_rebuilds() {
        let file = AssetSpec {
            source: PathBuf::from("/src/docs"),
            dest: "docs".to_owned(),
            plugin: Some("tools".to_owned()),
        };
        let directory = AssetSpec {
            source: PathBuf::from("/src2/docs"),
            dest: "docs".to_owned(),
            plugin: Some("tools".to_owned()),
        };
        // Each case carries the path the second build lands and its bytes.
        for (first, second, landed, bytes) in [
            (
                file.clone(),
                directory.clone(),
                "docs/x.md",
                &b"a directory\n"[..],
            ),
            (directory, file, "docs", &b"a file\n"[..]),
        ] {
            let vfs = example_vault();
            write(&vfs, "/src/docs", "a file\n");
            write(&vfs, "/src2/docs/x.md", "a directory\n");
            let mut request = request(ExportMode::Build);
            request.assets = vec![first];
            assert!(export_skills(&vfs, &request, &sw()).unwrap().changed);

            request.assets = vec![second];
            request.mode = ExportMode::Check;
            let report = export_skills(&vfs, &request, &sw()).unwrap();
            assert!(report.changed, "{report:?}");
            request.mode = ExportMode::Build;
            assert!(export_skills(&vfs, &request, &sw()).unwrap().changed);

            let path = Path::new("/market/.claude-plugin/packs/tools").join(landed);
            assert_eq!(vfs.read_file(&path).unwrap(), bytes);
        }
    }

    // --- Skill calls ---------------------------------------------------------

    const CALLS_SKILL_A: &str = "/vault/marketplaces/house/plugins/tools/skills/a.md";

    /// Plugin `tools` holds skills `a` and `b`, plugin `other` holds `c`, and
    /// `notes/plain.md` is a note that is not a skill. `a`'s body is the
    /// caller's.
    fn calls_vault(a_body: &str) -> MemVfs {
        let vfs = MemVfs::new();
        write(&vfs, CALLS_SKILL_A, &note_with_body("a", &[], a_body));
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/b.md",
            &note_with_body("b", &[], "# b\n\n## Steps\n\nB's own prose.\n"),
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/other/skills/c.md",
            &note("c", &[]),
        );
        write(&vfs, "/vault/notes/plain.md", "# plain\n");
        vfs
    }

    fn skill_md(vfs: &dyn Vfs, tree: &str, skill: &str) -> String {
        text_at(
            vfs,
            &format!("/market/{tree}/packs/tools/skills/{skill}/SKILL.md"),
        )
    }

    fn export_refusal(vfs: &dyn Vfs) -> String {
        export_skills(vfs, &request(ExportMode::Build), &sw())
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn a_call_renders_each_harness_spelling_and_copies_nothing() {
        let vfs = calls_vault(
            "# a\n\nNext, run /[[b]] to finish.\n\nUsage: `/[[b|the b skill]] ISSUE-1 --flag`\n\n\
             ```sh\n/[[b#Steps]] now\n```\n\nSee also [[b]].\n\nThen /[[c]].\n",
        );
        let report = build(&vfs);

        for (tree, b, c) in [
            (".claude-plugin", "/tools:b", "/other:c"),
            (".agents", "$tools:b", "$other:c"),
            (".cursor-plugin", "/b", "/c"),
        ] {
            let body = skill_md(&vfs, tree, "a");
            assert!(
                body.contains(&format!("Next, run `{b}` to finish.")),
                "{tree}: {body}"
            );
            assert!(
                body.contains(&format!("Usage: `{b} ISSUE-1 --flag`")),
                "{tree}: {body}"
            );
            assert!(
                body.contains(&format!("```sh\n{b} now\n```")),
                "{tree}: {body}"
            );
            assert!(body.contains(&format!("See also `{b}`.")), "{tree}: {body}");
            assert!(body.contains(&format!("Then `{c}`.")), "{tree}: {body}");
            assert!(!body.contains("B's own prose"), "{tree}: {body}");
            let copied: Vec<String> = paths_under(
                &vfs,
                Path::new(&format!("/market/{tree}/packs/tools/skills/a")),
            );
            assert_eq!(copied, vec!["SKILL.md".to_owned()], "{tree}");
        }

        let implicit: Vec<&DiscoveryWarning> = report
            .warnings
            .iter()
            .filter(|w| w.reason == "implicit-skill-call")
            .collect();
        assert_eq!(implicit.len(), 1, "{:?}", report.warnings);
        assert_eq!(
            implicit[0].path,
            "marketplaces/house/plugins/tools/skills/a.md"
        );
        assert!(
            implicit[0].detail.contains("/[[b]]"),
            "{}",
            implicit[0].detail
        );
    }

    #[test]
    fn a_call_naming_nothing_or_a_non_skill_is_refused() {
        let vfs = calls_vault("# a\n\nRun /[[nope]].\n");
        let err = export_refusal(&vfs);
        assert!(err.contains("skill-call-unresolved"), "{err}");

        let vfs = calls_vault("# a\n\nRun /[[plain]].\n");
        let err = export_refusal(&vfs);
        assert!(err.contains("skill-call-not-skill"), "{err}");
    }

    #[test]
    fn a_call_fragment_is_checked_then_dropped() {
        let vfs = calls_vault("# a\n\nRun /[[b#Missing]].\n");
        let err = export_refusal(&vfs);
        assert!(err.contains("fragment-unresolved"), "{err}");

        let vfs = calls_vault("# a\n\nRun /[[b#Steps]].\n");
        build(&vfs);
        let body = skill_md(&vfs, ".claude-plugin", "a");
        assert!(body.contains("Run `/tools:b`."), "{body}");
    }

    #[test]
    fn an_escaped_sigil_shows_the_notation() {
        let vfs = calls_vault(
            "# a\n\nWrite \\/[[plain]] in prose and `\\/[[b]]` in code.\n\nCite \\/[[b]] too.\n",
        );
        let report = build(&vfs);
        let body = skill_md(&vfs, ".claude-plugin", "a");
        // In prose and code alike the notation is written out, and nothing is
        // copied, even when it names a skill.
        assert!(body.contains("Write /[[plain]] in prose"), "{body}");
        assert!(body.contains("and `/[[b]]` in code."), "{body}");
        assert!(body.contains("Cite /[[b]] too."), "{body}");
        assert!(!body.contains("/tools:b"), "{body}");
        let copied = "/market/.claude-plugin/packs/tools/skills/a/references/vault";
        for note in [
            "notes/plain.md",
            "marketplaces/house/plugins/tools/skills/b.md",
        ] {
            assert!(
                !vfs.exists(Path::new(&format!("{copied}/{note}"))),
                "{note}"
            );
        }
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| w.reason == "implicit-skill-call"),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_call_to_a_skill_a_root_narrows_away_is_refused() {
        let vfs = calls_vault("# a\n\nRun /[[b]].\n");
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/b.md",
            &note_with_body("b", &["claude-code"], "# b\n"),
        );
        let err = export_refusal(&vfs);
        assert!(err.contains("skill-call-not-shipped"), "{err}");
        assert!(err.contains("tools:b"), "{err}");
        assert!(err.contains("skills/a.md"), "{err}");
    }

    #[test]
    fn a_literal_call_to_a_known_skill_is_refused() {
        let vfs = calls_vault("# a\n\nRun `$other:c` now.\n");
        let err = export_refusal(&vfs);
        assert!(err.contains("literal-skill-call"), "{err}");
        assert!(err.contains("`$other:c`"), "{err}");

        let vfs = calls_vault("# a\n\nRun `/tools:b` now.\n");
        assert!(export_refusal(&vfs).contains("`/tools:b`"));

        // Cursor's bare form, for a skill in a's own plugin.
        let vfs = calls_vault("# a\n\nRun /b now.\n");
        assert!(export_refusal(&vfs).contains("`/b`"));

        // A plugin or skill this vault does not hold is left alone: it may be
        // another vault's. So is a bare name from another plugin, and a path.
        let vfs = calls_vault(
            "# a\n\nRun `/tools:nope`, `/elsewhere:task-work`, /c, or see src/b and /b/x.\n",
        );
        build(&vfs);
    }

    #[test]
    fn a_prefixed_name_is_the_directory_and_the_call() {
        assert_eq!(
            published_skill_name(SkillNaming::PluginPrefixed, "tools", "b").unwrap(),
            "tools-b"
        );
        assert_eq!(
            rendered_skill_call(RuntimeId::Pi, SkillNaming::PluginPrefixed, "tools", "b").unwrap(),
            "/skill:tools-b"
        );
        for naming in [SkillNaming::HostNamespaced, SkillNaming::Bare] {
            assert_eq!(published_skill_name(naming, "tools", "b").unwrap(), "b");
        }
        assert_eq!(
            rendered_skill_call(
                RuntimeId::ClaudeCode,
                SkillNaming::HostNamespaced,
                "tools",
                "b"
            )
            .unwrap(),
            "/tools:b"
        );
        assert_eq!(
            rendered_skill_call(RuntimeId::Cursor, SkillNaming::Bare, "tools", "b").unwrap(),
            "/b"
        );

        // 33 + 1 + 31 = 65 characters: refused, never truncated.
        let plugin = "p".repeat(33);
        let skill = "s".repeat(31);
        let err = published_skill_name(SkillNaming::PluginPrefixed, &plugin, &skill).unwrap_err();
        assert!(err.to_string().contains("published-name-invalid"), "{err}");
        let err = rendered_skill_call(RuntimeId::Pi, SkillNaming::PluginPrefixed, &plugin, &skill)
            .unwrap_err();
        assert!(err.to_string().contains("published-name-invalid"), "{err}");
    }

    #[test]
    fn the_pi_root_writes_a_package_per_plugin_with_prefixed_names() {
        let vfs = calls_vault(
            "# a\n\nRun /[[b]], then `${CLAUDE_PLUGIN_ROOT}/bin/tool go`.\n\n\
             Read `${CLAUDE_PLUGIN_ROOT}/skills/b/ref.md` and `${CLAUDE_PLUGIN_ROOT}/skills/c/x`.\n",
        );
        write(&vfs, "/host/bin/tool", "#!/bin/sh\n");
        let mut request = request(ExportMode::Build);
        request.config.launcher_path = Some(LAUNCHER.to_owned());
        request.assets = vec![AssetSpec {
            plugin: Some("tools".to_owned()),
            ..launcher_asset()
        }];
        let report = export_skills(&vfs, &request, &sw()).unwrap();

        assert_eq!(
            paths_under(&vfs, Path::new("/market/.pi")),
            vec![
                "packs/other/package.json",
                "packs/other/skills/other-c/SKILL.md",
                "packs/tools/bin/tool",
                "packs/tools/package.json",
                "packs/tools/skills/tools-a/SKILL.md",
                "packs/tools/skills/tools-b/SKILL.md",
            ]
        );
        assert_eq!(
            text_at(&vfs, "/market/.pi/packs/tools/package.json"),
            "{\n  \"name\": \"tools\",\n  \"version\": \"0.1.0\",\n  \"description\": \
             \"Skills: a, b.\",\n  \"license\": \"UNLICENSED\",\n  \"pi\": {\n    \
             \"skills\": [\n      \"./skills\"\n    ]\n  }\n}\n"
        );
        let b = text_at(&vfs, "/market/.pi/packs/tools/skills/tools-b/SKILL.md");
        assert!(b.contains("name: tools-b\n"), "{b}");
        let a = text_at(&vfs, "/market/.pi/packs/tools/skills/tools-a/SKILL.md");
        assert!(
            a.contains("Run `/skill:tools-b`, then `../../bin/tool go`."),
            "{a}"
        );
        // A sibling's path follows its rename; another plugin's skill does not.
        assert!(
            a.contains(
                "Read `${CLAUDE_PLUGIN_ROOT}/skills/tools-b/ref.md` and \
                 `${CLAUDE_PLUGIN_ROOT}/skills/c/x`."
            ),
            "{a}"
        );
        let claude = text_at(&vfs, "/market/.claude-plugin/packs/tools/skills/a/SKILL.md");
        assert!(
            claude.contains("${CLAUDE_PLUGIN_ROOT}/skills/b/ref.md"),
            "{claude}"
        );
        // No marketplace index, so no manifest unit either.
        assert!(!report.units.iter().any(|u| matches!(
            u,
            ExportUnit::Manifest { root, .. } if root == "pi"
        )));
        // A rebuild finds the tree its own, and unchanged.
        request.mode = ExportMode::Check;
        assert!(!export_skills(&vfs, &request, &sw()).unwrap().changed);
    }

    #[test]
    fn a_prefixed_name_over_the_limit_refuses_the_build() {
        let plugin = "p".repeat(33);
        let skill = "s".repeat(31);
        let vfs = MemVfs::new();
        let path = format!("/vault/marketplaces/house/plugins/{plugin}/skills/{skill}.md");
        write(&vfs, &path, &note(&skill, &[]));
        let err = export_refusal(&vfs);
        assert!(err.contains("published-name-invalid"), "{err}");
        assert!(paths_under(&vfs, Path::new("/market")).is_empty());

        // Narrowed away from Pi, the same skill builds.
        write(&vfs, &path, &note(&skill, &["claude"]));
        build(&vfs);
    }

    #[test]
    fn a_build_applies_each_harness_its_own_override_block() {
        // Intended change: a build renders a note the way a publish does, so a
        // harness's block reaches that harness's tree and no
        // other.
        let vfs = example_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide.md",
            "---\nname: review-guide\ndescription: Use review-guide.\nsw-claude:\n  \
             permissionMode: plan\n---\n\n# review-guide\n",
        );
        build(&vfs);
        let read = |path: &str| String::from_utf8(vfs.read_file(Path::new(path)).unwrap()).unwrap();
        let claude = read("/market/.claude-plugin/packs/tools/skills/review-guide/SKILL.md");
        assert!(claude.contains("permissionMode: plan"), "{claude}");
        let codex = read("/market/.agents/packs/tools/skills/review-guide/SKILL.md");
        assert!(!codex.contains("permissionMode"), "{codex}");
    }

    #[test]
    fn a_build_refuses_an_override_key_the_harness_does_not_read() {
        let vfs = example_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide.md",
            "---\nname: review-guide\ndescription: Use review-guide.\nsw-codex:\n  \
             argument-hint: x\n---\n\n# review-guide\n",
        );
        let err = export_refusal(&vfs);
        assert!(err.contains("unknown-override-key"), "{err}");
        assert!(paths_under(&vfs, Path::new("/market")).is_empty());
    }

    #[test]
    fn rewrite_sibling_paths_renames_only_whole_sibling_segments() {
        let siblings: BTreeSet<&str> = ["pr", "pr-check"].into_iter().collect();
        assert_eq!(
            rewrite_sibling_paths(
                "${CLAUDE_PLUGIN_ROOT}/skills/pr-check/a.sh ${CLAUDE_PLUGIN_ROOT}/skills/pr \
                 ${CLAUDE_PLUGIN_ROOT}/skills/prx/b plugins/tools/skills/pr/pr.md \
                 .claude/skills/pr/ ${CLAUDE_PLUGIN_ROOT}/skills/",
                "tools",
                &siblings,
            ),
            "${CLAUDE_PLUGIN_ROOT}/skills/tools-pr-check/a.sh ${CLAUDE_PLUGIN_ROOT}/skills/tools-pr \
             ${CLAUDE_PLUGIN_ROOT}/skills/prx/b plugins/tools/skills/pr/pr.md \
             .claude/skills/pr/ ${CLAUDE_PLUGIN_ROOT}/skills/"
        );
    }

    #[test]
    fn rewrite_launcher_calls_matches_only_the_exact_call_site() {
        let rewrite = |body| rewrite_launcher_calls(body, "bin/tool", "skills/a");
        assert_eq!(
            rewrite("Run `${CLAUDE_PLUGIN_ROOT}/bin/tool task list`."),
            "Run `../../bin/tool task list`."
        );
        assert_eq!(
            rewrite("${CLAUDE_PLUGIN_ROOT}/bin/tool a; then ${CLAUDE_PLUGIN_ROOT}/bin/tool b"),
            "../../bin/tool a; then ../../bin/tool b"
        );
        // A longer path sharing the prefix is a different file.
        for untouched in [
            "See `${CLAUDE_PLUGIN_ROOT}/bin/tool.js`.",
            "See `${CLAUDE_PLUGIN_ROOT}/conventions/branch-naming.md`.",
            "Nothing to see here.",
        ] {
            assert_eq!(rewrite(untouched), untouched);
        }
    }

    #[test]
    fn relative_path_walks_up_from_any_depth() {
        assert_eq!(relative_path("skills/a", "bin/tool"), "../../bin/tool");
        assert_eq!(
            relative_path("skills/group/a", "./bin//tool"),
            "../../../bin/tool"
        );
        assert_eq!(relative_path("skills/a", "skills/a/run.sh"), "run.sh");
        assert_eq!(relative_path("skills/a", "skills/b/run.sh"), "../b/run.sh");
        assert_eq!(relative_path("", "bin/tool"), "bin/tool");
    }

    #[test]
    fn a_directory_asset_lands_as_a_tree_without_its_tests_or_fixtures() {
        let vfs = example_vault();
        write(&vfs, "/host/conventions/branch-naming.md", "# Branches\n");
        write(&vfs, "/host/conventions/nested/deep.md", "deep\n");
        write(&vfs, "/host/conventions/run.sh", "#!/bin/sh\n");
        vfs.set_executable(Path::new("/host/conventions/run.sh"), true)
            .unwrap();
        write(&vfs, "/host/conventions/tests/probe.test.ts", "test\n");
        write(&vfs, "/host/conventions/nested/deep.test.ts", "test\n");
        write(&vfs, "/host/conventions/nested/fixtures/f.md", "fixture\n");

        let mut request = request(ExportMode::Build);
        request.assets = vec![AssetSpec {
            source: PathBuf::from("/host/conventions"),
            dest: "conventions".to_owned(),
            plugin: Some("tools".to_owned()),
        }];
        export_skills(&vfs, &request, &sw()).unwrap();

        let root = Path::new("/market/.claude-plugin/packs/tools/conventions");
        assert_eq!(
            vfs.read_file(&root.join("branch-naming.md")).unwrap(),
            b"# Branches\n"
        );
        assert_eq!(
            vfs.read_file(&root.join("nested/deep.md")).unwrap(),
            b"deep\n"
        );
        assert!(vfs.is_executable(&root.join("run.sh")));
        assert!(!vfs.exists(&root.join("tests")));
        assert!(!vfs.exists(&root.join("nested/fixtures")));
        assert!(!vfs.exists(&root.join("nested/deep.test.ts")));

        // A file added under the source directory is drift, same as an
        // edited single-file asset.
        let mut check_request = request.clone();
        check_request.mode = ExportMode::Check;
        assert!(!export_skills(&vfs, &check_request, &sw()).unwrap().changed);
        write(&vfs, "/host/conventions/new.md", "new\n");
        assert!(export_skills(&vfs, &check_request, &sw()).unwrap().changed);
    }

    #[test]
    fn an_asset_dest_escaping_the_plugin_root_via_dotdot_is_refused() {
        let vfs = example_vault();
        write(&vfs, "/host/bin/tool", "launcher\n");

        let mut request = request(ExportMode::Build);
        request.assets = vec![AssetSpec {
            source: PathBuf::from("/host/bin/tool"),
            dest: "../../escaped".to_owned(),
            plugin: Some("tools".to_owned()),
        }];
        let err = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("unsafe-asset-dest"), "{err}");
        assert!(!vfs.exists(Path::new("/escaped")));
    }

    #[test]
    fn an_absolute_asset_dest_is_refused() {
        let vfs = example_vault();
        write(&vfs, "/host/bin/tool", "launcher\n");

        let mut request = request(ExportMode::Build);
        request.assets = vec![AssetSpec {
            source: PathBuf::from("/host/bin/tool"),
            dest: "/etc/escaped".to_owned(),
            plugin: Some("tools".to_owned()),
        }];
        let err = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("unsafe-asset-dest"), "{err}");
        assert!(!vfs.exists(Path::new("/etc/escaped")));
    }

    #[test]
    fn an_asset_at_a_pack_manifest_path_is_refused() {
        let vfs = example_vault();
        write(&vfs, "/host/package.json", "{}\n");
        write(&vfs, "/host/tree/package.json", "{}\n");
        for (source, dest) in [
            ("/host/package.json", "package.json"),
            ("/host/package.json", "./package.json"),
            ("/host/tree", "."),
        ] {
            let mut request = request(ExportMode::Build);
            request.assets = vec![AssetSpec {
                source: PathBuf::from(source),
                dest: dest.to_owned(),
                plugin: Some("tools".to_owned()),
            }];
            let err = export_skills(&vfs, &request, &sw())
                .unwrap_err()
                .to_string();
            assert!(err.contains("asset-over-manifest"), "{dest}: {err}");
            assert!(err.contains("pi root"), "{dest}: {err}");
        }
        assert!(paths_under(&vfs, Path::new("/market")).is_empty());
    }

    #[test]
    fn an_asset_naming_a_plugin_the_vault_holds_nothing_for_is_refused() {
        let vfs = example_vault();
        write(&vfs, "/host/bin/tool", "launcher\n");

        let mut request = request(ExportMode::Build);
        request.assets = vec![AssetSpec {
            source: PathBuf::from("/host/bin/tool"),
            dest: LAUNCHER.to_owned(),
            plugin: Some("nonexistent".to_owned()),
        }];
        let err = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown-asset-plugin"), "{err}");
        assert!(err.contains("nonexistent"), "{err}");
    }

    #[test]
    fn check_reports_drift_when_an_asset_changes_on_disk() {
        let vfs = example_vault();
        write(&vfs, "/host/bin/tool", "v1\n");
        let mut request = request(ExportMode::Build);
        request.assets = vec![AssetSpec {
            source: PathBuf::from("/host/bin/tool"),
            dest: LAUNCHER.to_owned(),
            plugin: Some("tools".to_owned()),
        }];
        export_skills(&vfs, &request, &sw()).unwrap();

        // Unchanged: `check` agrees with what was just built.
        let mut check_request = request.clone();
        check_request.mode = ExportMode::Check;
        assert!(!export_skills(&vfs, &check_request, &sw()).unwrap().changed);

        // The source moves, out from under the build; `check` catches it the
        // same way it catches a rendered skill drifting.
        vfs.write_file(Path::new("/host/bin/tool"), b"v2\n")
            .unwrap();
        assert!(export_skills(&vfs, &check_request, &sw()).unwrap().changed);
    }

    #[test]
    fn a_plugin_narrowed_out_of_a_root_is_absent_from_it_entirely() {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/rebase-guide.md",
            &note("rebase-guide", &["claude"]),
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide.md",
            &note("review-guide", &[]),
        );
        build(&vfs);

        // No directory, no plugin manifest, no marketplace listing.
        assert!(!vfs.exists(Path::new("/market/.agents/packs/git-tools")));
        let manifest = text_at(&vfs, "/market/.agents/plugins/marketplace.json");
        assert!(!manifest.contains("git-tools"), "{manifest}");
        // And in the root that admits it, all three are present.
        assert!(vfs.exists(Path::new(
            "/market/.claude-plugin/packs/git-tools/.claude-plugin/plugin.json"
        )));
        let manifest = text_at(&vfs, "/market/.claude-plugin/marketplace.json");
        assert!(manifest.contains("git-tools"), "{manifest}");
    }

    #[test]
    fn the_emitted_skill_md_is_compiled_stamped_and_stripped() {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide.md",
            "---\nname: review-guide\ndescription: Reviews things.\n---\n\n\
             # Review Guide\n\n![[snippets/steps]]\n\nSee [[guides/style-guide]].\n",
        );
        write(&vfs, "/vault/snippets/steps.md", "Step prose to inline.\n");
        write(&vfs, "/vault/guides/style-guide.md", "# Style\n");
        build(&vfs);

        let text = text_at(
            &vfs,
            "/market/.claude-plugin/packs/tools/skills/review-guide/SKILL.md",
        );
        assert!(text.contains("name: review-guide"), "{text}");
        assert!(text.contains("description: Reviews things."), "{text}");
        // The provenance marker, naming the vault-relative source.
        assert!(
            text.contains(
                "<!-- Generated by skillwright from \
                 marketplaces/house/plugins/tools/skills/review-guide.md; do not edit. -->"
            ),
            "{text}"
        );
        // Compiled: the embed inlined, the link rewritten, nothing left to
        // resolve, and nothing of our namespace.
        assert!(text.contains("Step prose to inline."), "{text}");
        assert!(
            text.contains("(references/vault/guides/style-guide.md)"),
            "{text}"
        );
        assert!(!text.contains("[["), "{text}");
        assert!(!text.contains("sw-"), "{text}");
        // The linked file travelled as a resource, in the claude and codex roots.
        for packs in [".claude-plugin/packs", ".agents/packs"] {
            assert_eq!(
                vfs.read_file(
                    &Path::new("/market")
                        .join(packs)
                        .join("tools/skills/review-guide/references/vault/guides/style-guide.md")
                )
                .unwrap(),
                b"# Style\n",
                "{packs}"
            );
        }
    }

    fn nameonly_vault() -> MemVfs {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/tools.md",
            "---\nname: tools\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/quiet.md",
            "---\nname: quiet\n---\n\n# Quiet\n",
        );
        vfs
    }

    #[test]
    fn a_name_without_a_description_publishes_the_filler_and_warns() {
        let vfs = nameonly_vault();
        let report = build(&vfs);
        let warned: Vec<(&str, &str)> = report
            .warnings
            .iter()
            .map(|w| (w.path.as_str(), w.reason.as_str()))
            .collect();
        assert_eq!(
            warned,
            [
                (
                    "marketplaces/house/plugins/tools/skills/quiet.md",
                    "missing-description"
                ),
                (
                    "marketplaces/house/plugins/tools/tools.md",
                    "missing-description"
                ),
                (
                    "marketplaces/house/plugins/tools/tools.md",
                    "missing-version"
                ),
            ]
        );
        let skill = text_at(
            &vfs,
            "/market/.claude-plugin/packs/tools/skills/quiet/SKILL.md",
        );
        assert!(
            skill.contains(&format!("description: {MISSING_DESCRIPTION}")),
            "{skill}"
        );
        let manifest = text_at(
            &vfs,
            "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
        );
        assert!(manifest.contains(MISSING_DESCRIPTION), "{manifest}");
    }

    /// The filler is the vault being silent, so a plugin description the
    /// root file sets outranks it.
    #[test]
    fn a_configured_plugin_description_outranks_a_silent_plugin_note() {
        let vfs = nameonly_vault();
        let mut config = config();
        config.plugins.insert(
            "tools".to_owned(),
            PluginOverrides {
                description: Some("Configured description.".to_owned()),
                ..PluginOverrides::default()
            },
        );
        build_with(&vfs, config);
        let plugin: serde_json::Value = serde_json::from_str(&text_at(
            &vfs,
            "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
        ))
        .unwrap();
        assert_eq!(plugin["description"], "Configured description.");
        let market: serde_json::Value =
            serde_json::from_str(&text_at(&vfs, "/market/.claude-plugin/marketplace.json"))
                .unwrap();
        assert_eq!(
            market["plugins"][0]["description"],
            "Configured description."
        );
    }

    /// The marketplace-level twin: a silent marketplace note leaves the
    /// configured marketplace description in place.
    #[test]
    fn a_configured_marketplace_description_outranks_a_silent_marketplace_note() {
        let vfs = nameonly_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/house.md",
            "---\nname: house\n---\n",
        );
        let mut config = config();
        config.description = Some("Configured market description".to_owned());
        build_with(&vfs, config);
        let market: serde_json::Value =
            serde_json::from_str(&text_at(&vfs, "/market/.claude-plugin/marketplace.json"))
                .unwrap();
        assert_eq!(market["description"], "Configured market description");
    }

    /// A version from the note or from the config silences the warning, and
    /// the warning alone never trips `require_descriptions`.
    #[test]
    fn a_plugin_without_a_version_warns_until_one_is_stated() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/tools.md",
            "---\nname: tools\ndescription: Tools.\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/said.md",
            "---\nname: said\ndescription: d\n---\n",
        );
        let mut request = request(ExportMode::Check);
        request.config.require_descriptions = true;
        let report = export_skills(&vfs, &request, &sw()).unwrap();
        let reasons: Vec<&str> = report.warnings.iter().map(|w| w.reason.as_str()).collect();
        assert_eq!(reasons, ["missing-version"]);

        request.config.plugins.insert(
            "tools".to_owned(),
            PluginOverrides {
                version: Some("1.0.0".to_owned()),
                ..PluginOverrides::default()
            },
        );
        let report = export_skills(&vfs, &request, &sw()).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);

        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/tools.md",
            "---\nname: tools\ndescription: Tools.\nsw-version: 2.0.0\n---\n",
        );
        request.config.plugins.clear();
        let report = export_skills(&vfs, &request, &sw()).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// A configured description is what ships for a silent plugin or
    /// marketplace note, so that note neither warns nor trips
    /// `require_descriptions`. A silent skill still does.
    #[test]
    fn a_configured_description_neither_warns_nor_refuses() {
        let vfs = nameonly_vault();
        write(
            &vfs,
            "/vault/marketplaces/house/house.md",
            "---\nname: house\n---\n",
        );
        let mut request = request(ExportMode::Check);
        request.config.description = Some("Configured market description".to_owned());
        request.config.plugins.insert(
            "tools".to_owned(),
            PluginOverrides {
                description: Some("Configured description.".to_owned()),
                ..PluginOverrides::default()
            },
        );
        let report = export_skills(&vfs, &request, &sw()).unwrap();
        let warned: Vec<&str> = report
            .warnings
            .iter()
            .filter(|w| w.reason == MISSING_DESCRIPTION_REASON)
            .map(|w| w.path.as_str())
            .collect();
        assert_eq!(warned, ["marketplaces/house/plugins/tools/skills/quiet.md"]);

        request.config.require_descriptions = true;
        let message = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(message.contains("skills/quiet.md"), "{message}");
        assert!(!message.contains("tools/tools.md"), "{message}");
        assert!(!message.contains("house/house.md"), "{message}");
    }

    #[test]
    fn another_marketplaces_missing_description_neither_refuses_nor_warns() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/said.md",
            "---\nname: said\ndescription: d\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/other/other.md",
            "---\nname: other\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/other/plugins/kit/skills/quiet.md",
            "---\nname: quiet\n---\n",
        );
        let mut request = request(ExportMode::Build);
        request.marketplace = Some("house".to_owned());
        request.config.require_descriptions = true;
        let report = export_skills(&vfs, &request, &sw()).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    #[test]
    fn require_descriptions_refuses_the_build_naming_each_note() {
        let vfs = nameonly_vault();
        let mut request = request(ExportMode::Build);
        request.config.require_descriptions = true;
        let message = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(message.contains("missing-description"), "{message}");
        assert!(message.contains("skills/quiet.md"), "{message}");
        assert!(message.contains("tools/tools.md"), "{message}");
        assert!(!vfs.exists(Path::new("/market")));
    }

    #[test]
    fn a_description_holding_a_colon_space_publishes_as_valid_yaml() {
        let vfs = MemVfs::new();
        let description = "Turn a review into a page: diagrams of the flows. Use when asked.";
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/flow.md",
            &format!("---\nname: flow\ndescription: \"{description}\"\n---\n\n# Flow\n"),
        );
        build(&vfs);
        let text = text_at(
            &vfs,
            "/market/.claude-plugin/packs/tools/skills/flow/SKILL.md",
        );
        let frontmatter = text
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .map(|(fm, _)| fm)
            .unwrap();
        let parsed: serde_yaml::Mapping = serde_yaml::from_str(frontmatter).unwrap();
        assert_eq!(
            parsed.get("description").and_then(|v| v.as_str()),
            Some(description)
        );
    }

    // `read_vault_skill` never fills `SkillContent::arguments` — the translator
    // has to call `apply_arguments` itself, the same seam `publish` uses, or a
    // marketplace-published skill would carry no hint and no section at all.
    #[test]
    fn a_marketplace_skill_derives_its_hint_and_renders_the_section() {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review-guide.md",
            "---\nname: review-guide\ndescription: Reviews things.\n\
             sw-arguments:\n  - name: pr-number\n    required: true\n    description: The PR.\n\
             ---\n\n# Review Guide\n",
        );
        build(&vfs);

        // Claude Code's dialect keeps `argument-hint`; Codex's dialect omits
        // every extension field — see
        // `agent_adapt::AGENT_SKILLS_EXTENSION_FIELDS` — so only the body
        // section, not the frontmatter hint, is common to both.
        let claude = text_at(
            &vfs,
            "/market/.claude-plugin/packs/tools/skills/review-guide/SKILL.md",
        );
        assert!(
            claude.contains("argument-hint: \"<pr-number>\""),
            "{claude}"
        );
        assert!(claude.contains("arguments:\n  - pr-number"), "{claude}");
        assert!(claude.contains("$ARGUMENTS"), "{claude}");

        for tree in [".claude-plugin", ".agents"] {
            let text = text_at(
                &vfs,
                &format!("/market/{tree}/packs/tools/skills/review-guide/SKILL.md"),
            );
            assert!(text.contains("## Arguments"), "{tree}: {text}");
            assert!(
                text.contains("- `pr-number` (required): The PR."),
                "{tree}: {text}"
            );
        }

        let codex = text_at(
            &vfs,
            "/market/.agents/packs/tools/skills/review-guide/SKILL.md",
        );
        assert!(!codex.contains("argument-hint"), "{codex}");
        assert!(!codex.contains("arguments:"), "{codex}");
        assert!(!codex.contains("$ARGUMENTS"), "{codex}");
    }

    #[test]
    fn manifests_are_byte_deterministic() {
        let vfs = example_vault();
        build(&vfs);

        // Golden bytes: stable key order, plugins sorted by name, two-space
        // indentation, trailing newline.
        //
        // This vault's note names no owner, so the marketplace's own name
        // stands in. Claude marks `owner` required and rejects a bare string,
        // so an absent one is not an option — see `claude_marketplace_manifest`.
        assert_eq!(
            text_at(&vfs, "/market/.claude-plugin/marketplace.json"),
            r#"{
  "name": "demo-market",
  "owner": {
    "name": "demo-market"
  },
  "plugins": [
    {
      "name": "git-tools",
      "source": "./.claude-plugin/packs/git-tools",
      "description": "Skills: commit-helper, rebase-guide.",
      "version": "0.1.0"
    },
    {
      "name": "tools",
      "source": "./.claude-plugin/packs/tools",
      "description": "Skills: review-guide.",
      "version": "0.1.0"
    }
  ]
}
"#
        );
        assert_eq!(
            text_at(
                &vfs,
                "/market/.claude-plugin/packs/git-tools/.claude-plugin/plugin.json"
            ),
            r#"{
  "name": "git-tools",
  "version": "0.1.0",
  "description": "Skills: commit-helper, rebase-guide.",
  "license": "UNLICENSED",
  "keywords": [],
  "skills": "./skills/"
}
"#
        );
        // The codex output describes its *own* members: rebase-guide is
        // narrowed out of this root, so its manifest does not name it.
        assert_eq!(
            text_at(
                &vfs,
                "/market/.agents/packs/git-tools/.codex-plugin/plugin.json"
            ),
            r#"{
  "name": "git-tools",
  "version": "0.1.0",
  "description": "Skills: commit-helper.",
  "license": "UNLICENSED",
  "keywords": [],
  "skills": "./skills/",
  "interface": {
    "displayName": "git-tools",
    "category": "Productivity"
  }
}
"#
        );
        assert_eq!(
            text_at(&vfs, "/market/.agents/plugins/marketplace.json"),
            r#"{
  "name": "demo-market",
  "interface": {
    "displayName": "demo-market"
  },
  "plugins": [
    {
      "name": "git-tools",
      "source": {
        "source": "local",
        "path": "./.agents/packs/git-tools"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Productivity"
    },
    {
      "name": "tools",
      "source": {
        "source": "local",
        "path": "./.agents/packs/tools"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Productivity"
    }
  ]
}
"#
        );

        // And the whole output is byte-stable: a second vault, same content,
        // builds to identical bytes.
        let again = example_vault();
        build(&again);
        assert_eq!(
            files_under(&vfs, Path::new("/market")),
            files_under(&again, Path::new("/market"))
        );
    }

    #[test]
    fn config_overrides_and_the_author_fallback_reach_the_manifests() {
        let vfs = example_vault();
        let mut request = request(ExportMode::Build);
        request.config.owner = Some(json!({ "name": "Kelly" }));
        request.config.display_name = Some("Demo Market".to_owned());
        request.config.plugins.insert(
            "git-tools".to_owned(),
            PluginOverrides {
                version: Some("2.0.0".to_owned()),
                description: Some("Git helpers.".to_owned()),
                license: Some("MIT".to_owned()),
                keywords: Some(vec!["git".to_owned()]),
                category: Some("Development".to_owned()),
                display_name: Some("Git Tools".to_owned()),
                author: None,
                ..PluginOverrides::default()
            },
        );
        export_skills(&vfs, &request, &sw()).unwrap();

        let plugin = text_at(
            &vfs,
            "/market/.claude-plugin/packs/git-tools/.claude-plugin/plugin.json",
        );
        assert!(plugin.contains("\"version\": \"2.0.0\""), "{plugin}");
        assert!(
            plugin.contains("\"description\": \"Git helpers.\""),
            "{plugin}"
        );
        assert!(plugin.contains("\"license\": \"MIT\""), "{plugin}");
        assert!(plugin.contains("\"git\""), "{plugin}");
        // No author, no plugin-level author override — the owner steps in,
        // already an object and passed through.
        assert!(plugin.contains("\"author\": {"), "{plugin}");
        assert!(plugin.contains("\"name\": \"Kelly\""), "{plugin}");

        let codex = text_at(
            &vfs,
            "/market/.agents/packs/git-tools/.codex-plugin/plugin.json",
        );
        assert!(codex.contains("\"displayName\": \"Git Tools\""), "{codex}");
        assert!(codex.contains("\"category\": \"Development\""), "{codex}");

        let market = text_at(&vfs, "/market/.agents/plugins/marketplace.json");
        assert!(
            market.contains("\"displayName\": \"Demo Market\""),
            "{market}"
        );
        assert!(market.contains("\"category\": \"Development\""), "{market}");

        let market = text_at(&vfs, "/market/.claude-plugin/marketplace.json");
        assert!(market.contains("\"owner\""), "{market}");
        assert!(
            market.contains("\"description\": \"Git helpers.\""),
            "{market}"
        );
        assert!(market.contains("\"version\": \"2.0.0\""), "{market}");
    }

    #[test]
    fn an_up_to_date_destination_is_reported_unchanged_and_untouched() {
        let vfs = example_vault();
        build(&vfs);
        let before = files_under(&vfs, Path::new("/market"));

        let report = build(&vfs);

        assert!(!report.changed, "{report:?}");
        assert!(
            report.units.iter().all(|unit| matches!(
                unit,
                ExportUnit::Plugin {
                    status: UnitStatus::Unchanged,
                    ..
                } | ExportUnit::Manifest {
                    status: UnitStatus::Unchanged,
                    ..
                }
            )),
            "{report:?}"
        );
        // Not one byte, not one rename — no stage was even created.
        assert_eq!(files_under(&vfs, Path::new("/market")), before);
        assert!(!vfs.exists(Path::new("/market/.skillwright-stage")));
        assert!(!vfs.exists(Path::new("/market/.skillwright-stage")));
    }

    // --- Version bump suggestions -----------------------------------------

    /// One plugin with its own container note — carrying `version`, which
    /// [`resolve_config`] folds into `MarketplaceConfig.plugins` — and one
    /// member skill.
    fn vault_with_versioned_plugin() -> MemVfs {
        let vfs = MemVfs::new();
        let base = "/vault/marketplaces/house/plugins/git-tools";
        write(
            &vfs,
            &format!("{base}/git-tools.md"),
            "---\nname: git-tools\ndescription: Git helpers.\nsw-version: 2.1.0\n---\n\nThe pack.\n",
        );
        write(
            &vfs,
            &format!("{base}/skills/commit-helper.md"),
            "---\nname: commit-helper\ndescription: Writes commits.\n---\n\nCommit well.\n",
        );
        vfs
    }

    // The case the callout exists for: a member changed and nothing moved
    // `version` to say so.
    #[test]
    fn a_changed_member_with_an_unmoved_version_suggests_a_bump() {
        let vfs = vault_with_versioned_plugin();
        build(&vfs);

        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper.md",
            "---\nname: commit-helper\ndescription: Writes better commits.\n---\n\nCommit well.\n",
        );

        let report = check(&vfs);
        assert_eq!(
            plugin_status(&report, "claude", "git-tools"),
            UnitStatus::Drift
        );
        assert_eq!(
            plugin_version_bump(&report, "claude", "git-tools"),
            Some(VersionBumpSuggestion {
                current_version: "2.1.0".to_owned(),
                published_version: "2.1.0".to_owned(),
            })
        );
    }

    // The author already answered the drift, so there is nothing left to
    // suggest.
    #[test]
    fn a_changed_member_with_a_moved_version_suggests_nothing() {
        let vfs = vault_with_versioned_plugin();
        build(&vfs);

        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper.md",
            "---\nname: commit-helper\ndescription: Writes better commits.\n---\n\nCommit well.\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/git-tools.md",
            "---\nname: git-tools\ndescription: Git helpers.\nsw-version: 2.2.0\n---\n\nThe pack.\n",
        );

        let report = check(&vfs);
        assert_eq!(
            plugin_status(&report, "claude", "git-tools"),
            UnitStatus::Drift
        );
        assert_eq!(plugin_version_bump(&report, "claude", "git-tools"), None);
    }

    // Nothing changed, so the plugin is `ok` and there is nothing to suggest
    // either.
    #[test]
    fn an_unchanged_plugin_suggests_nothing() {
        let vfs = vault_with_versioned_plugin();
        build(&vfs);

        let report = check(&vfs);
        assert_eq!(
            plugin_status(&report, "claude", "git-tools"),
            UnitStatus::Ok
        );
        assert_eq!(plugin_version_bump(&report, "claude", "git-tools"), None);
    }

    // `example_vault`'s plugins have no container note at all, so there is
    // no `version` anywhere to compare a bump against.
    #[test]
    fn a_plugin_with_no_container_note_never_suggests_a_bump() {
        let vfs = example_vault();
        build(&vfs);

        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper.md",
            "---\nname: commit-helper\ndescription: Use commit-helper, but better.\n---\n\n# commit-helper\n",
        );

        let report = check(&vfs);
        assert_eq!(
            plugin_status(&report, "claude", "git-tools"),
            UnitStatus::Drift
        );
        assert_eq!(plugin_version_bump(&report, "claude", "git-tools"), None);
    }

    #[test]
    fn check_writes_nothing_under_any_outcome() {
        // Outcome one: everything drifted (no destination at all).
        let vfs = example_vault();
        let report = check(&vfs);
        assert!(report.changed);
        assert!(
            !vfs.exists(Path::new("/market")),
            "check created the destination"
        );

        // Outcome two: clean.
        build(&vfs);
        let before = files_under(&vfs, Path::new("/market"));
        let report = check(&vfs);
        assert!(!report.changed, "{report:?}");
        assert_eq!(files_under(&vfs, Path::new("/market")), before);

        // Outcome three: drifted over an existing destination.
        vfs.write_file(
            Path::new("/market/.claude-plugin/packs/tools/skills/review-guide/SKILL.md"),
            b"tampered\n",
        )
        .unwrap();
        let before = files_under(&vfs, Path::new("/market"));
        let report = check(&vfs);
        assert!(report.changed);
        assert_eq!(files_under(&vfs, Path::new("/market")), before);
    }

    #[test]
    fn check_reports_ok_and_drift_per_plugin_per_root() {
        let vfs = example_vault();
        build(&vfs);
        // One plugin, one root: everything else must stay OK.
        vfs.write_file(
            Path::new("/market/.claude-plugin/packs/tools/skills/review-guide/SKILL.md"),
            b"tampered\n",
        )
        .unwrap();

        let report = check(&vfs);

        assert!(report.changed);
        assert_eq!(plugin_status(&report, "claude", "tools"), UnitStatus::Drift);
        assert_eq!(
            plugin_status(&report, "claude", "git-tools"),
            UnitStatus::Ok
        );
        assert_eq!(plugin_status(&report, "codex", "tools"), UnitStatus::Ok);
        assert_eq!(plugin_status(&report, "codex", "git-tools"), UnitStatus::Ok);
        assert!(
            report.units.iter().all(|u| !matches!(
                u,
                ExportUnit::Manifest {
                    status: UnitStatus::Drift,
                    ..
                }
            )),
            "{report:?}"
        );
    }

    #[test]
    fn stale_content_is_reported_by_check_and_swept_by_build() {
        let vfs = example_vault();
        build(&vfs);
        // A stale plugin and an empty directory among the packs. Beside the
        // packs, nothing is stale.
        write(
            &vfs,
            "/market/.claude-plugin/packs/legacy/junk.md",
            "junk\n",
        );
        vfs.create_dir_all(Path::new("/market/.agents/packs/empty"))
            .unwrap();
        write(&vfs, "/market/.claude-plugin/README.md", "kept\n");
        vfs.create_dir_all(Path::new("/market/.agents/empty"))
            .unwrap();

        let report = check(&vfs);
        assert!(report.changed);
        assert_eq!(
            stale_set(&report),
            BTreeSet::from([
                (
                    "claude".to_owned(),
                    ".claude-plugin/packs/legacy".to_owned(),
                    StaleKind::Plugin,
                ),
                (
                    "codex".to_owned(),
                    ".agents/packs/empty".to_owned(),
                    StaleKind::Plugin,
                ),
            ]),
            "one unit per stale thing — the stale plugin's files do not \
             double-report: {report:?}"
        );
        // The vault's own plugins are still OK; staleness is the only drift.
        assert_eq!(plugin_status(&report, "claude", "tools"), UnitStatus::Ok);

        let report = build(&vfs);
        assert!(report.changed);
        assert_eq!(stale_set(&report).len(), 2, "{report:?}");
        assert!(!vfs.exists(Path::new("/market/.claude-plugin/packs/legacy")));
        assert!(!vfs.exists(Path::new("/market/.agents/packs/empty")));
        assert!(vfs.exists(Path::new("/market/.claude-plugin/README.md")));
        assert!(vfs.exists(Path::new("/market/.agents/empty")));
        assert!(!check(&vfs).changed);
    }

    #[test]
    fn check_build_check_verdicts_agree() {
        let vfs = example_vault();
        build(&vfs);
        // Three kinds of drift at once: tampered bytes in one root, a missing
        // plugin manifest in the other, and a stale plugin.
        vfs.write_file(
            Path::new("/market/.agents/packs/tools/skills/review-guide/SKILL.md"),
            b"tampered\n",
        )
        .unwrap();
        vfs.remove(Path::new(
            "/market/.claude-plugin/packs/git-tools/.claude-plugin/plugin.json",
        ))
        .unwrap();
        write(
            &vfs,
            "/market/.claude-plugin/packs/legacy/junk.md",
            "junk\n",
        );

        let verdict = check(&vfs);
        assert!(verdict.changed);

        let built = build(&vfs);
        assert!(built.changed);
        // What check called drift is exactly what build wrote, unit for unit,
        // and the stale sets are the same set.
        assert_eq!(differing(&verdict), differing(&built));
        assert_eq!(stale_set(&verdict), stale_set(&built));
        assert_eq!(
            differing(&verdict),
            BTreeSet::from([
                ("claude".to_owned(), "git-tools".to_owned()),
                ("codex".to_owned(), "tools".to_owned()),
            ])
        );

        // And after the build, the same check finds nothing.
        let after = check(&vfs);
        assert!(!after.changed, "{after:?}");
        assert!(differing(&after).is_empty());
        assert!(stale_set(&after).is_empty());
    }

    #[test]
    fn a_stale_execute_bit_is_drift_and_build_repairs_it() {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/tool-pack/tool-pack.md",
            &note("tool-pack", &[]),
        );
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/tool-pack/scripts/run.sh",
            "#!/bin/sh\n",
        );
        vfs.set_executable(
            Path::new("/vault/marketplaces/house/plugins/tools/skills/tool-pack/scripts/run.sh"),
            true,
        )
        .unwrap();
        build(&vfs);

        let published =
            Path::new("/market/.claude-plugin/packs/tools/skills/tool-pack/scripts/run.sh");
        assert!(vfs.is_executable(published), "the bit travelled");

        vfs.set_executable(published, false).unwrap();
        let report = check(&vfs);
        assert!(report.changed, "{report:?}");
        assert_eq!(plugin_status(&report, "claude", "tools"), UnitStatus::Drift);
        assert_eq!(plugin_status(&report, "codex", "tools"), UnitStatus::Ok);

        build(&vfs);
        assert!(vfs.is_executable(published), "the bit was repaired");
    }

    /// A wrapper that fails the next `remaining` renames onto one path — the
    /// injection a mid-swap failure test needs.
    struct FailingRename<'a> {
        inner: &'a MemVfs,
        to: PathBuf,
        remaining: Cell<u32>,
    }

    impl Vfs for FailingRename<'_> {
        fn read_file(&self, path: &Path) -> Result<Vec<u8>, crate::vfs::VfsError> {
            self.inner.read_file(path)
        }
        fn read_dir(&self, path: &Path) -> Result<Vec<crate::vfs::DirEntry>, crate::vfs::VfsError> {
            self.inner.read_dir(path)
        }
        fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<(), crate::vfs::VfsError> {
            self.inner.write_file(path, bytes)
        }
        fn create_dir_all(&self, path: &Path) -> Result<(), crate::vfs::VfsError> {
            self.inner.create_dir_all(path)
        }
        fn exists(&self, path: &Path) -> bool {
            self.inner.exists(path)
        }
        fn metadata(&self, path: &Path) -> Result<crate::vfs::Metadata, crate::vfs::VfsError> {
            self.inner.metadata(path)
        }
        fn is_executable(&self, path: &Path) -> bool {
            self.inner.is_executable(path)
        }
        fn set_executable(&self, path: &Path, yes: bool) -> Result<(), crate::vfs::VfsError> {
            self.inner.set_executable(path, yes)
        }
        fn rename(&self, from: &Path, to: &Path) -> Result<(), crate::vfs::VfsError> {
            if normalize(to) == self.to && self.remaining.get() > 0 {
                self.remaining.set(self.remaining.get() - 1);
                return Err(crate::vfs::VfsError::io(
                    to,
                    std::io::Error::other("injected failure"),
                ));
            }
            self.inner.rename(from, to)
        }
        fn remove(&self, path: &Path) -> Result<(), crate::vfs::VfsError> {
            self.inner.remove(path)
        }
    }

    #[test]
    fn a_failed_swap_rolls_back_every_root_already_swapped() {
        let vfs = example_vault();
        build(&vfs);
        // Drift every root, so the swap has several units and a later one can fail
        // after the first landed.
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper.md",
            "---\nname: commit-helper\ndescription: Use commit-helper.\n---\n\nRevised body.\n",
        );
        let before = files_under(&vfs, Path::new("/market"));

        // The swap unit is an entry, so the injected failure names one.
        let failing = FailingRename {
            inner: &vfs,
            to: PathBuf::from("/market/.agents/packs"),
            remaining: Cell::new(1),
        };
        let err = export_skills(&failing, &request(ExportMode::Build), &sw()).unwrap_err();
        let message = err.to_string();

        assert!(matches!(err, CoreError::SwapFailed(_)), "{message}");
        assert!(message.contains("rolled back"), "{message}");
        // Claude had already swapped; the rollback put it back. Byte for
        // byte, bit for bit, the destination is as it was — and the stage is
        // gone.
        assert_eq!(files_under(&vfs, Path::new("/market")), before);
        assert!(!vfs.exists(Path::new("/market/.skillwright-stage")));
        assert!(!vfs.exists(Path::new("/market/.skillwright-stage")));

        // The failure was transient, not structural: the same build succeeds
        // once the filesystem cooperates.
        let report = build(&vfs);
        assert!(report.changed);
        assert!(!check(&vfs).changed);
    }

    #[test]
    fn a_failed_rollback_keeps_the_backup_and_names_it() {
        let vfs = example_vault();
        build(&vfs);
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/git-tools/skills/commit-helper.md",
            "---\nname: commit-helper\ndescription: Use commit-helper.\n---\n\nRevised body.\n",
        );

        // Two failures on the same path: the swap-in, then the restore.
        let failing = FailingRename {
            inner: &vfs,
            to: PathBuf::from("/market/.agents/packs"),
            remaining: Cell::new(2),
        };
        let err = export_skills(&failing, &request(ExportMode::Build), &sw()).unwrap_err();
        let message = err.to_string();

        assert!(matches!(err, CoreError::SwapFailed(_)), "{message}");
        // The message names what could not be restored and where its bytes
        // survive; the stage is deliberately not deleted.
        assert!(message.contains(".agents"), "{message}");
        assert!(message.contains("backup survives"), "{message}");
        assert!(
            vfs.exists(Path::new(
                "/market/.skillwright-stage/backup/.agents/packs/git-tools/.codex-plugin/plugin.json"
            )),
            "the old entry's bytes are still in the backup"
        );
        // The root that did swap was still rolled back.
        assert!(vfs.exists(Path::new("/market/.claude-plugin/marketplace.json")));
    }

    // --- the repository as its own marketplace ---------------------------

    /// A repository holding a vault, a `.git`, and a file of its own.
    fn repo_vault(vfs: &MemVfs) -> ExportRequest {
        write(
            vfs,
            "/repo/src/marketplaces/house/plugins/tools/skills/review-guide.md",
            &note("review-guide", &[]),
        );
        write(vfs, "/repo/README.md", "# A repository\n");
        write(vfs, "/repo/.git/HEAD", "ref: refs/heads/main\n");
        let mut request = request(ExportMode::Build);
        request.vault_root = PathBuf::from("/repo/src");
        request.out_root = PathBuf::from("/repo");
        request
    }

    #[test]
    fn a_host_that_cannot_see_links_refuses_an_ancestor_of_the_vault() {
        // A wasm host without link methods: a link under the out root could
        // route the swap into the vault, and this host cannot see it.
        let mem = MemVfs::new();
        let request = repo_vault(&mem);
        let vfs = crate::vfs::LinkBlindVfs(mem);
        let message = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(message.contains("unsafe-root"), "{message}");
        assert!(
            !vfs.exists(Path::new("/repo/.claude-plugin")),
            "nothing staged"
        );
    }

    /// A host that sees links, as a host with its own `Vfs` may: the
    /// repository root still builds, and a swapped tree linked into the vault
    /// is refused before anything is staged.
    #[cfg(unix)]
    #[test]
    fn a_host_that_sees_links_builds_at_the_repository_root_and_refuses_a_link_into_the_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let note_path = repo.join("src/marketplaces/house/plugins/tools/skills/review-guide.md");
        std::fs::create_dir_all(note_path.parent().unwrap()).unwrap();
        std::fs::write(&note_path, note("review-guide", &[])).unwrap();
        let mut request = request(ExportMode::Build);
        request.vault_root = repo.join("src");
        request.out_root = repo.clone();

        let vfs = crate::vfs::NativeVfs;
        assert!(export_skills(&vfs, &request, &sw()).unwrap().changed);
        assert!(repo.join(".claude-plugin/marketplace.json").exists());

        // The same layout, with one swapped tree a link into the vault.
        let linked = tmp.path().join("linked");
        let note_path = linked.join("src/marketplaces/house/plugins/tools/skills/review-guide.md");
        std::fs::create_dir_all(note_path.parent().unwrap()).unwrap();
        std::fs::write(&note_path, note("review-guide", &[])).unwrap();
        std::fs::create_dir_all(linked.join("src/skills")).unwrap();
        std::os::unix::fs::symlink(linked.join("src/skills"), linked.join(".claude-plugin"))
            .unwrap();
        request.vault_root = linked.join("src");
        request.out_root = linked.clone();
        let message = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(message.contains("unsafe-root"), "{message}");
        assert_eq!(
            std::fs::read_dir(linked.join("src/skills"))
                .unwrap()
                .count(),
            0,
            "nothing was written into the vault"
        );
    }

    #[test]
    fn the_repository_root_is_a_marketplace_directory_like_any_other() {
        // The whole point: every harness's manifest at the path its harness
        // reads, so a bare repository URL registers with all of them.
        let vfs = MemVfs::new();
        let report = export_skills(&vfs, &repo_vault(&vfs), &sw()).unwrap();
        assert!(report.changed);

        assert!(vfs.exists(Path::new("/repo/.claude-plugin/marketplace.json")));
        assert!(vfs.exists(Path::new("/repo/.agents/plugins/marketplace.json")));
        assert!(vfs.exists(Path::new(
            "/repo/.claude-plugin/packs/tools/skills/review-guide/SKILL.md"
        )));
        assert!(vfs.exists(Path::new(
            "/repo/.agents/packs/tools/skills/review-guide/SKILL.md"
        )));
    }

    #[test]
    fn a_build_at_a_repository_root_touches_nothing_it_did_not_render() {
        // A root owns one top-level entry per harness and nothing else, so
        // the swap can never name `.git`, the vault, or a file of the
        // repository's own — the render produces no path beginning with one.
        let vfs = MemVfs::new();
        export_skills(&vfs, &repo_vault(&vfs), &sw()).unwrap();

        assert_eq!(
            vfs.read_file(Path::new("/repo/README.md")).unwrap(),
            b"# A repository\n"
        );
        assert!(vfs.exists(Path::new("/repo/.git/HEAD")));
        assert!(vfs.exists(Path::new(
            "/repo/src/marketplaces/house/plugins/tools/skills/review-guide.md"
        )));
    }

    #[test]
    fn the_two_harnesses_never_collide_at_one_directory() {
        // Each renders the same skill, and they cannot share the file: the
        // two harnesses spell a skill's tool names differently. Separate
        // trees is what makes one directory possible at all.
        let vfs = MemVfs::new();
        export_skills(&vfs, &repo_vault(&vfs), &sw()).unwrap();
        let claude = "/repo/.claude-plugin/packs/tools/skills/review-guide/SKILL.md";
        let codex = "/repo/.agents/packs/tools/skills/review-guide/SKILL.md";
        assert_ne!(claude, codex);
        assert!(vfs.exists(Path::new(claude)) && vfs.exists(Path::new(codex)));
        // And each manifest points at its own tree.
        let manifest = text_at(&vfs, "/repo/.claude-plugin/marketplace.json");
        assert!(
            manifest.contains("./.claude-plugin/packs/tools"),
            "{manifest}"
        );
        let manifest = text_at(&vfs, "/repo/.agents/plugins/marketplace.json");
        assert!(manifest.contains("./.agents/packs/tools"), "{manifest}");
    }

    #[test]
    fn a_repository_marketplace_is_clean_on_the_check_that_follows() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        export_skills(&vfs, &request, &sw()).unwrap();

        let mut checking = request.clone();
        checking.mode = ExportMode::Check;
        let report = export_skills(&vfs, &checking, &sw()).unwrap();
        assert!(!report.changed, "{:?}", report.units);
        // The repository around it is not drift, and never reported.
        assert!(
            !report
                .units
                .iter()
                .any(|unit| matches!(unit, ExportUnit::Stale { .. })),
            "{:?}",
            report.units
        );
    }

    #[test]
    fn a_pack_removed_from_the_vault_disappears_from_every_tree() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        write(
            &vfs,
            "/repo/src/marketplaces/house/plugins/extra/skills/spare.md",
            &note("spare", &[]),
        );
        export_skills(&vfs, &request, &sw()).unwrap();
        assert!(vfs.exists(Path::new("/repo/.claude-plugin/packs/extra")));

        vfs.remove(Path::new("/repo/src/marketplaces/house/plugins/extra"))
            .unwrap();
        let report = export_skills(&vfs, &request, &sw()).unwrap();
        assert!(!vfs.exists(Path::new("/repo/.claude-plugin/packs/extra")));
        assert!(!vfs.exists(Path::new("/repo/.agents/packs/extra")));
        assert!(!vfs.exists(Path::new("/repo/.cursor-plugin/packs/extra")));
        assert!(!vfs.exists(Path::new("/repo/.pi/packs/extra")));
        // Once per harness, each naming its own path.
        let stale: Vec<&str> = report
            .units
            .iter()
            .filter_map(|unit| match unit {
                ExportUnit::Stale { path, .. } => Some(path.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            stale,
            vec![
                ".claude-plugin/packs/extra",
                ".agents/packs/extra",
                ".cursor-plugin/packs/extra",
                ".pi/packs/extra",
            ]
        );
    }

    #[test]
    fn a_harness_can_be_selected_and_the_others_are_not_rendered() {
        let vfs = MemVfs::new();
        let mut request = repo_vault(&vfs);
        request.harnesses.insert("claude".to_owned());

        export_skills(&vfs, &request, &sw()).unwrap();
        assert!(vfs.exists(Path::new("/repo/.claude-plugin/marketplace.json")));
        assert!(!vfs.exists(Path::new("/repo/.agents")));
    }

    #[test]
    fn a_harness_this_build_cannot_render_is_refused_by_name() {
        // Silently publishing less than was asked for is the failure this
        // crate exists to prevent.
        let vfs = MemVfs::new();
        let mut request = repo_vault(&vfs);
        request.harnesses.insert("windsurf".to_owned());

        let err = export_skills(&vfs, &request, &sw()).unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, CoreError::Refused(_)), "{message}");
        assert!(message.contains("unknown-harness"), "{message}");
        assert!(message.contains("windsurf"), "{message}");
        assert!(!vfs.exists(Path::new("/repo/.claude-plugin")));
    }

    #[test]
    fn a_marketplace_directory_that_is_the_vault_builds_beside_its_notes() {
        let vfs = MemVfs::new();
        let mut request = repo_vault(&vfs);
        request.out_root = PathBuf::from("/repo/src");

        let report = export_skills(&vfs, &request, &sw()).unwrap();
        assert!(report.changed);
        assert!(vfs.exists(Path::new("/repo/src/.claude-plugin/marketplace.json")));
    }

    #[test]
    fn a_marketplace_directory_containing_the_vault_is_allowed() {
        // The ancestor rule would refuse this, and it is the whole case: a
        // repository root contains its own vault and still publishes itself.
        let vfs = MemVfs::new();
        let report = export_skills(&vfs, &repo_vault(&vfs), &sw()).unwrap();
        assert!(report.changed);
    }

    /// The swap moves each tree and the stage aside and removes them, so a
    /// vault inside one would go with it — even a tree an earlier build left
    /// its marker in. A host that cannot see links refuses it by the ancestor
    /// rule instead.
    #[test]
    fn a_vault_inside_a_swapped_tree_is_refused() {
        for (vault, sees_links) in [
            ("/repo/.claude-plugin/packs/vault", true),
            (&format!("/repo/{STAGE_DIR}/vault"), true),
            ("/repo/.claude-plugin/packs/vault", false),
        ] {
            let mem = MemVfs::new();
            let mut request = repo_vault(&mem);
            assert!(export_skills(&mem, &request, &sw()).unwrap().changed);

            write(
                &mem,
                &format!("{vault}/marketplaces/house/plugins/tools/skills/review-guide.md"),
                &note("review-guide", &[]),
            );
            write(&mem, &format!("{vault}/precious.md"), "# Keep me\n");
            request.vault_root = PathBuf::from(vault);
            let vfs: Box<dyn Vfs> = if sees_links {
                Box::new(mem)
            } else {
                Box::new(crate::vfs::LinkBlindVfs(mem))
            };
            let message = export_skills(vfs.as_ref(), &request, &sw())
                .unwrap_err()
                .to_string();
            assert!(message.contains("unsafe-root"), "{vault}: {message}");
            assert!(message.contains(vault), "{vault}: {message}");
            assert!(vfs.exists(&Path::new(vault).join("precious.md")), "{vault}");
        }
    }

    /// The render cannot see a tree it replaces, and the swap deletes it, so
    /// an asset sourced from inside one is refused up front, by name.
    #[test]
    fn an_asset_source_inside_a_swapped_tree_is_refused() {
        let vfs = MemVfs::new();
        let mut request = repo_vault(&vfs);
        assert!(export_skills(&vfs, &request, &sw()).unwrap().changed);

        let source = "/repo/.claude-plugin/packs/launcher";
        write(&vfs, source, "#!/bin/sh\n");
        request.assets = vec![AssetSpec {
            source: PathBuf::from(source),
            dest: "bin/tool".to_owned(),
            plugin: Some("tools".to_owned()),
        }];
        let message = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(message.contains("unsafe-root"), "{message}");
        assert!(message.contains(source), "{message}");
        assert!(vfs.exists(Path::new(source)));
    }

    /// The regression that shaped the swap: output beside the root file
    /// shares `.agents/` and `.pi/` with skills and config people author.
    #[test]
    fn a_hand_added_file_beside_the_packs_survives_a_rebuild() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        export_skills(&vfs, &request, &sw()).unwrap();
        let mine = "/repo/.agents/skills/mine/SKILL.md";
        write(&vfs, mine, "---\nname: mine\n---\n\nBy hand.\n");
        write(&vfs, "/repo/.pi/settings.json", "{}\n");
        write(&vfs, "/repo/.claude-plugin/notes.txt", "mine\n");

        // Nothing reports them, and a rebuild that changes the trees keeps them.
        assert!(
            !export_skills(&vfs, &check_of(&request), &sw())
                .unwrap()
                .changed
        );
        write(
            &vfs,
            "/repo/src/marketplaces/house/plugins/tools/skills/review-guide.md",
            &note_with_body("review-guide", &[], "Revised.\n"),
        );
        let report = export_skills(&vfs, &request, &sw()).unwrap();

        assert!(report.changed);
        assert!(stale_set(&report).is_empty(), "{:?}", stale_set(&report));
        assert_eq!(text_at(&vfs, mine), "---\nname: mine\n---\n\nBy hand.\n");
        assert!(vfs.exists(Path::new("/repo/.pi/settings.json")));
        assert!(vfs.exists(Path::new("/repo/.claude-plugin/notes.txt")));
    }

    /// `request` as a check.
    fn check_of(request: &ExportRequest) -> ExportRequest {
        ExportRequest {
            mode: ExportMode::Check,
            ..request.clone()
        }
    }

    #[test]
    fn packs_this_vault_did_not_build_are_refused() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        let hand = "/repo/.claude-plugin/packs/hand/plugin.json";
        write(&vfs, hand, "{}\n");

        let err = export_skills(&vfs, &request, &sw()).unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, CoreError::Refused(_)), "{message}");
        assert!(message.contains("foreign-tree"), "{message}");
        assert!(message.contains(".claude-plugin/packs"), "{message}");
        assert!(message.contains("build.out"), "{message}");
        // And it stopped before writing anything.
        assert_eq!(vfs.read_file(Path::new(hand)).unwrap(), b"{}\n");
    }

    /// The case the fixture above does not reach, and the one that matters:
    /// a repository that already *is* a marketplace. Its manifest sits at the
    /// path this build renders one to, so every rendered-path test says
    /// "ours" and the swap takes it without a word. Provenance is what tells
    /// them apart.
    #[test]
    fn a_hand_written_marketplace_at_the_same_path_is_refused_not_replaced() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        let manifest = "{\n  \"name\": \"hand\",\n  \"plugins\": []\n}\n";
        write(&vfs, "/repo/.claude-plugin/marketplace.json", manifest);
        write(
            &vfs,
            "/repo/.claude-plugin/packs/hand/skills/by-hand/SKILL.md",
            "---\nname: by-hand\n---\n\nWritten by a person.\n",
        );

        let err = export_skills(&vfs, &request, &sw()).unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, CoreError::Refused(_)), "{message}");
        assert!(message.contains("foreign-tree"), "{message}");
        // Not one byte of it moved.
        assert_eq!(
            vfs.read_file(Path::new("/repo/.claude-plugin/marketplace.json"))
                .unwrap(),
            manifest.as_bytes()
        );
        assert!(vfs.exists(Path::new(
            "/repo/.claude-plugin/packs/hand/skills/by-hand/SKILL.md"
        )));
    }

    /// The residue, pinned so it is a decision rather than a surprise. Only a
    /// `marketplace.json`, with no marker to read, is claimed as ours on the
    /// strength of the path alone, because a commands-only plugin leaves
    /// exactly that for the harness whose plugin format carries no commands.
    /// A hand-written Claude marketplace laid out the documented way looks
    /// identical, and is replaced. See [`DestTree::built_here`].
    #[test]
    fn a_lone_manifest_is_claimed_on_its_path_alone() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        write(
            &vfs,
            "/repo/.claude-plugin/marketplace.json",
            "{\n  \"name\": \"someone-elses\",\n  \"plugins\": []\n}\n",
        );

        // No refusal: the one file present is a path this render produces.
        let report = export_skills(&vfs, &request, &sw()).unwrap();
        assert!(report.changed);
    }

    #[test]
    fn a_harness_that_renders_nothing_loses_only_its_packs() {
        let vfs = MemVfs::new();
        let mut request = repo_vault(&vfs);
        export_skills(&vfs, &request, &sw()).unwrap();
        write(&vfs, "/repo/.pi/settings.json", "{}\n");
        vfs.remove(Path::new("/repo/src/marketplaces/house/plugins/tools"))
            .unwrap();
        write(
            &vfs,
            "/repo/src/marketplaces/house/plugins/gear/commands/run.md",
            "---\nname: run\ndescription: Run.\n---\n\nBody.\n",
        );
        request.harnesses = BTreeSet::from(["pi".to_owned()]);

        export_skills(&vfs, &request, &sw()).unwrap();
        assert!(!vfs.exists(Path::new("/repo/.pi/packs")));
        assert!(vfs.exists(Path::new("/repo/.pi/settings.json")));
    }

    /// A link above an owned path would carry the swap out of the directory.
    #[cfg(unix)]
    #[test]
    fn a_link_above_an_owned_path_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let vault = root.join("vault/plugins/tools/skills");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("guide.md"), note("guide", &[])).unwrap();
        let out = root.join("out");
        let elsewhere = root.join("elsewhere");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, out.join(".agents")).unwrap();

        let mut request = request(ExportMode::Build);
        request.vault_root = root.join("vault");
        request.out_root = out;
        let message = export_skills(&NativeVfs, &request, &sw())
            .unwrap_err()
            .to_string();

        assert!(message.contains("foreign-tree"), "{message}");
        assert!(std::fs::read_dir(&elsewhere).unwrap().next().is_none());
    }

    #[test]
    fn a_tree_that_built_here_before_is_not_mistaken_for_a_foreign_one() {
        // Every rebuild finds its tree occupied. What tells the two cases
        // apart is that a previous build's manifest is still there, at a path
        // this render names — even after every pack has been renamed.
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        export_skills(&vfs, &request, &sw()).unwrap();

        vfs.remove(Path::new("/repo/src/marketplaces/house/plugins/tools"))
            .unwrap();
        write(
            &vfs,
            "/repo/src/marketplaces/house/plugins/gear/skills/review-guide.md",
            &note("review-guide", &[]),
        );

        let report = export_skills(&vfs, &request, &sw()).unwrap();
        assert!(report.changed);
        assert!(vfs.exists(Path::new(
            "/repo/.claude-plugin/packs/gear/.claude-plugin/plugin.json"
        )));
        assert!(!vfs.exists(Path::new("/repo/.claude-plugin/packs/tools")));
    }

    /// Build `/repo`, then rewrite the generated-file marker in every built
    /// file to the one an earlier release of the tool wrote.
    fn built_tree_with_an_earlier_marker(vfs: &MemVfs, request: &ExportRequest) {
        export_skills(vfs, request, &sw()).unwrap();
        for (relative, bytes, _) in files_under(vfs, Path::new("/repo")) {
            if relative.starts_with("src/") {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes)
                .replace(GENERATED_MARKER, "<!-- Generated by tradecraft from ");
            write(vfs, &format!("/repo/{relative}"), &text);
        }
    }

    #[test]
    fn a_tree_with_an_earlier_marker_and_the_same_file_set_is_rebuilt() {
        // No marker counts as ours, but every file is at a path this render
        // produces, so it is still claimed.
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        built_tree_with_an_earlier_marker(&vfs, &request);

        let report = export_skills(&vfs, &request, &sw()).unwrap();
        assert!(report.changed);
    }

    #[test]
    fn packs_with_an_earlier_marker_and_an_extra_file_are_refused() {
        let vfs = MemVfs::new();
        let request = repo_vault(&vfs);
        built_tree_with_an_earlier_marker(&vfs, &request);
        let stray = "/repo/.claude-plugin/packs/tools/stray.txt";
        write(&vfs, stray, "left over\n");

        let err = export_skills(&vfs, &request, &sw()).unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, CoreError::Refused(_)), "{message}");
        assert!(message.contains("foreign-tree"), "{message}");
        assert!(vfs.exists(Path::new(stray)));
    }

    #[test]
    fn the_rail_refuses_a_destination_overlapping_the_vault() {
        // Inside the vault's structure — refused before discovery reads a
        // note. The vault root and a plain folder in it are not here: output
        // `out: .` builds beside the root file. Nor is a directory that merely
        // CONTAINS the vault: that is a repository publishing itself.
        for out in ["/vault/plugins/tools", "/vault/marketplaces/house"] {
            let vfs = example_vault();
            let mut request = request(ExportMode::Build);
            request.out_root = PathBuf::from(out);
            let err = export_skills(&vfs, &request, &sw()).unwrap_err();
            let message = err.to_string();

            assert!(matches!(err, CoreError::Refused(_)), "{out}: {message}");
            assert!(message.contains("unsafe-root"), "{out}: {message}");
            // Nothing was staged anywhere.
            assert!(
                !vfs.exists(Path::new(out).join(".claude-plugin").as_path()),
                "{out}"
            );
            assert!(!vfs.exists(Path::new("/vault/.skillwright-stage")), "{out}");
        }
    }

    /// Two marketplaces, each holding a `core` plugin holding a `review`:
    /// legal in the vault, and one output root cannot hold both.
    fn two_marketplaces() -> MemVfs {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/acme/plugins/core/skills/review.md",
            &note("review", &[]),
        );
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/skills/review.md",
            &note("review", &[]),
        );
        vfs
    }

    /// A vault that names its own marketplace and its own pack.
    fn vault_with_container_notes() -> MemVfs {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/demo-market/demo-market.md",
            "---\nname: demo-market\ndescription: The house marketplace.\n\
             sw-owner:\n  name: kelly/house\nsw-author: house-team\n\
             sw-display_name: The House\n---\n\nProse.\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/tools/tools.md",
            "---\nname: tools\ndescription: Everyday tools.\nsw-version: 2.1.0\nsw-license: MIT\nsw-keywords:\n  - editing\n\
             sw-category: Writing\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/tools/skills/review.md",
            &note("review", &[]),
        );
        vfs
    }

    /// The whole point: a vault that says who owns it publishes that, and a
    /// build flag is not where a marketplace's identity lives.
    #[test]
    fn the_marketplace_note_supplies_the_identity_the_config_used_to() {
        let vfs = vault_with_container_notes();
        build(&vfs);

        let manifest = text_at(&vfs, "/market/.claude-plugin/marketplace.json");
        assert!(manifest.contains(r#""name": "demo-market""#), "{manifest}");
        assert!(manifest.contains(r#""name": "kelly/house""#), "{manifest}");

        // Codex takes the display name; Claude's dialect has no field for it.
        let codex = text_at(&vfs, "/market/.agents/plugins/marketplace.json");
        assert!(codex.contains("The House"), "{codex}");
    }

    /// The vault behind every license scenario: a marketplace note whose
    /// `sw-license` line is the variable, one plugin that says nothing about
    /// its license, and one that licenses itself differently.
    fn vault_with_licenses(market_license: Option<&str>) -> MemVfs {
        let vfs = MemVfs::new();
        let line = market_license
            .map(|spdx| format!("sw-license: {spdx}\n"))
            .unwrap_or_default();
        write(
            &vfs,
            "/vault/marketplaces/demo-market/demo-market.md",
            &format!(
                "---\nname: demo-market\ndescription: The house marketplace.\n\
                 {line}---\n"
            ),
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/tools/skills/review.md",
            &note("review", &[]),
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/vendor/vendor.md",
            "---\nname: vendor\ndescription: Vendored work.\nsw-license: Apache-2.0\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/vendor/skills/other.md",
            &note("other", &[]),
        );
        vfs
    }

    /// The license one pack manifest published.
    fn license_of(vfs: &dyn Vfs, plugin: &str) -> String {
        let manifest = text_at(
            vfs,
            &format!("/market/.claude-plugin/packs/{plugin}/.claude-plugin/plugin.json"),
        );
        serde_json::from_str::<serde_json::Value>(&manifest)
            .unwrap_or_else(|err| panic!("{plugin} manifest is not JSON: {err}\n{manifest}"))["license"]
            .as_str()
            .unwrap_or_else(|| panic!("{plugin} manifest carries no license\n{manifest}"))
            .to_owned()
    }

    /// The point of the rung: a distribution licensed as a whole says so once.
    /// Before it existed, a note that licensed itself still published
    /// `UNLICENSED` into every pack manifest it shipped.
    #[test]
    fn a_plugin_inherits_the_marketplace_note_license() {
        let vfs = vault_with_licenses(Some("MIT"));
        build(&vfs);
        assert_eq!(license_of(&vfs, "tools"), "MIT");
    }

    /// Inheritance is a default, not a policy: one plugin under a differently
    /// licensed marketplace is the case a vendored dependency makes.
    #[test]
    fn a_plugin_licensing_itself_overrides_its_marketplace() {
        let vfs = vault_with_licenses(Some("MIT"));
        build(&vfs);
        assert_eq!(license_of(&vfs, "vendor"), "Apache-2.0");
    }

    /// Nothing anywhere still means the documented default, so adding the rung
    /// did not quietly license an unlicensed distribution.
    #[test]
    fn a_silent_vault_and_a_silent_project_file_stay_unlicensed() {
        let vfs = vault_with_licenses(None);
        build(&vfs);
        assert_eq!(license_of(&vfs, "tools"), "UNLICENSED");
        // The plugin that licensed itself is unaffected by the marketplace's
        // silence — its own note was always the first rung.
        assert_eq!(license_of(&vfs, "vendor"), "Apache-2.0");
    }

    /// A vault with no marketplace note at all still has somewhere to say it.
    #[test]
    fn the_project_file_licenses_a_marketplace_no_note_licenses() {
        let vfs = vault_with_licenses(None);
        let config = MarketplaceConfig {
            license: Some("MIT".to_owned()),
            ..config()
        };
        build_with(&vfs, config);
        assert_eq!(license_of(&vfs, "tools"), "MIT");
    }

    /// The vault is the source of truth the root file falls back to, the
    /// same precedence `owner` and `author` already follow.
    #[test]
    fn the_marketplace_note_beats_the_root_file() {
        let vfs = vault_with_licenses(Some("MIT"));
        let config = MarketplaceConfig {
            license: Some("GPL-3.0-only".to_owned()),
            ..config()
        };
        build_with(&vfs, config);
        assert_eq!(license_of(&vfs, "tools"), "MIT");
    }

    /// The narrowest rung still wins: a per-plugin override is how a project
    /// file relicenses one pack without touching the vault.
    #[test]
    fn a_per_plugin_override_beats_the_marketplace_license() {
        let vfs = vault_with_licenses(Some("MIT"));
        let config = MarketplaceConfig {
            license: Some("MIT".to_owned()),
            plugins: [(
                "tools".to_owned(),
                PluginOverrides {
                    license: Some("BSD-3-Clause".to_owned()),
                    ..PluginOverrides::default()
                },
            )]
            .into_iter()
            .collect(),
            ..config()
        };
        build_with(&vfs, config);
        assert_eq!(license_of(&vfs, "tools"), "BSD-3-Clause");
    }

    /// The note's `description` reached the pack manifests and stopped there,
    /// so a marketplace that described itself published an index that did not
    /// — `claude plugin validate` warns about the absence.
    #[test]
    fn a_marketplace_note_describes_its_own_index() {
        let vfs = vault_with_container_notes();
        build(&vfs);

        // Claude takes a top-level `description`; its documentation prefers
        // that to `metadata.description`, which it keeps for compatibility.
        let claude = text_at(&vfs, "/market/.claude-plugin/marketplace.json");
        assert!(
            claude.contains(r#""description": "The house marketplace.""#),
            "{claude}"
        );

        // Cursor documents no top-level description — `metadata` is the
        // object holding it.
        let cursor = text_at(&vfs, "/market/.cursor-plugin/marketplace.json");
        assert!(
            cursor.contains(r#""metadata": {"#) && cursor.contains("The house marketplace."),
            "{cursor}"
        );

        // Codex documents no description field at all, so it gets none rather
        // than one written to a key invented for the occasion.
        let codex = text_at(&vfs, "/market/.agents/plugins/marketplace.json");
        assert!(!codex.contains("The house marketplace."), "{codex}");
    }

    /// `new marketplace --owner kelly` writes `sw-owner: kelly`, a bare
    /// string, and Claude rejects it outright — `owner: Invalid input`. The
    /// note's spelling is a convenience; the object is the manifest's shape.
    #[test]
    fn a_bare_owner_name_becomes_the_object_both_dialects_require() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/demo-market/demo-market.md",
            "---\nname: demo-market\ndescription: The house marketplace.\n\
             sw-owner: kelly\n---\n",
        );
        write(
            &vfs,
            "/vault/marketplaces/demo-market/plugins/tools/skills/review.md",
            &note("review", &[]),
        );
        build(&vfs);

        for tree in [
            ".claude-plugin/marketplace.json",
            ".cursor-plugin/marketplace.json",
        ] {
            let manifest = text_at(&vfs, &format!("/market/{tree}"));
            assert!(
                manifest.contains("\"owner\": {\n    \"name\": \"kelly\"\n  }"),
                "{tree}: {manifest}"
            );
        }
    }

    /// A plugin note describes its own pack, in the same `description` a skill
    /// note uses for itself — and the generated `Skills: …` list is the
    /// fallback for a pack with no note, not an override of one.
    #[test]
    fn the_plugin_note_supplies_its_manifest_rather_than_the_generated_default() {
        let vfs = vault_with_container_notes();
        build(&vfs);

        let manifest = text_at(
            &vfs,
            "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
        );
        assert!(
            manifest.contains(r#""description": "Everyday tools.""#),
            "{manifest}"
        );
        assert!(!manifest.contains("Skills: review"), "{manifest}");
        assert!(manifest.contains(r#""version": "2.1.0""#), "{manifest}");
        assert!(manifest.contains(r#""license": "MIT""#), "{manifest}");
        assert!(manifest.contains("editing"), "{manifest}");
        // The author falls through the marketplace note, which is the only
        // place it was said — a bare string in the note, promoted to the
        // object Claude's schema requires.
        assert!(manifest.contains(r#""author": {"#), "{manifest}");
        assert!(manifest.contains(r#""name": "house-team""#), "{manifest}");

        let codex = text_at(
            &vfs,
            "/market/.agents/packs/tools/.codex-plugin/plugin.json",
        );
        assert!(codex.contains("Writing"), "{codex}");
    }

    /// An author already shaped as an object passes through untouched,
    /// `email` included — normalization only promotes a bare string.
    #[test]
    fn an_object_author_passes_through_with_its_email() {
        let vfs = example_vault();
        let mut request = request(ExportMode::Build);
        request.config.plugins.insert(
            "git-tools".to_owned(),
            PluginOverrides {
                author: Some(json!({ "name": "Ada", "email": "ada@example.com" })),
                ..Default::default()
            },
        );
        export_skills(&vfs, &request, &sw()).unwrap();

        let plugin = text_at(
            &vfs,
            "/market/.claude-plugin/packs/git-tools/.claude-plugin/plugin.json",
        );
        assert!(plugin.contains(r#""author": {"#), "{plugin}");
        assert!(plugin.contains(r#""name": "Ada""#), "{plugin}");
        assert!(plugin.contains(r#""email": "ada@example.com""#), "{plugin}");
    }

    /// Field by field, not block by block: a request naming one thing and a
    /// note naming another must produce a manifest carrying both.
    #[test]
    fn a_configured_field_survives_beside_one_the_note_supplies() {
        let vfs = vault_with_container_notes();
        let mut request = request(ExportMode::Build);
        request.config.plugins.insert(
            "tools".to_owned(),
            PluginOverrides {
                keywords: Some(vec!["configured".to_owned()]),
                ..PluginOverrides::default()
            },
        );
        export_skills(&vfs, &request, &sw()).unwrap();

        let manifest = text_at(
            &vfs,
            "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
        );
        // The note spoke about keywords too, and outranks the request there.
        assert!(manifest.contains("editing"), "{manifest}");
        assert!(!manifest.contains("configured"), "{manifest}");
        // And the fields only the note gave are still there — the request's
        // block did not blank them.
        assert!(manifest.contains(r#""license": "MIT""#), "{manifest}");
    }

    /// Silence is not an assertion. A vault with no container notes is legal
    /// and the request is its whole identity.
    #[test]
    fn a_vault_with_no_container_notes_still_builds_from_the_request_alone() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/plugins/tools/skills/review.md",
            &note("review", &[]),
        );
        build(&vfs);

        let manifest = text_at(&vfs, "/market/.claude-plugin/marketplace.json");
        assert!(manifest.contains(r#""name": "demo-market""#), "{manifest}");
        let plugin = text_at(
            &vfs,
            "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
        );
        assert!(plugin.contains("Skills: review."), "{plugin}");
    }

    /// The vault names it, so a configured name is beside the point rather
    /// than in conflict with it. Which marketplace to build is
    /// [`ExportRequest::marketplace`]'s job, not this field's. Requiring this
    /// field and refusing a mismatch made every host find the vault's
    /// marketplace itself before calling the core.
    #[test]
    fn the_note_names_the_marketplace_whatever_the_build_was_configured_with() {
        let vfs = vault_with_container_notes();
        let mut request = request(ExportMode::Build);
        request.config.name = Some("something-else".to_owned());

        export_skills(&vfs, &request, &sw()).unwrap();

        let manifest = text_at(&vfs, "/market/.claude-plugin/marketplace.json");
        assert!(manifest.contains(r#""name": "demo-market""#), "{manifest}");
        assert!(!manifest.contains("something-else"), "{manifest}");
    }

    /// A distribution nobody named. The one field with no sensible default,
    /// so it is the one absence that refuses instead of falling back.
    #[test]
    fn a_marketplace_neither_the_vault_nor_the_build_names_is_refused() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/plugins/tools/skills/review.md",
            &note("review", &[]),
        );
        let mut request = request(ExportMode::Build);
        request.config.name = None;

        let err = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("unnamed-marketplace"), "{err}");
        assert!(paths_under(&vfs, Path::new("/market")).is_empty());
    }

    /// A marketplace directory with no note in it. The folder named the level,
    /// and that is enough to name the distribution — a marketplace called
    /// `house` on disk should not need a second place to say so.
    #[test]
    fn a_marketplace_folder_with_no_note_is_named_by_its_folder() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/tools/skills/review.md",
            &note("review", &[]),
        );
        let mut request = request(ExportMode::Build);
        request.config.name = None;

        export_skills(&vfs, &request, &sw()).unwrap();
        let manifest = text_at(&vfs, "/market/.claude-plugin/marketplace.json");
        assert!(manifest.contains(r#""name": "house""#), "{manifest}");
    }

    /// A vault with no marketplace level at all, and a build that names one:
    /// there is no set to select from, so the name is simply what the output
    /// is called rather than a selection that could be wrong.
    #[test]
    fn naming_a_marketplace_a_vault_does_not_hold_names_the_output() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/plugins/tools/skills/review.md",
            &note("review", &[]),
        );
        let mut request = request(ExportMode::Build);
        request.config.name = None;
        request.marketplace = Some("house".to_owned());

        let report = export_skills(&vfs, &request, &sw()).unwrap();
        assert_eq!(
            report.checked, 1,
            "the promoted skill belongs to whichever is built"
        );
        let manifest = text_at(&vfs, "/market/.claude-plugin/marketplace.json");
        assert!(manifest.contains(r#""name": "house""#), "{manifest}");
    }

    /// But a vault that DOES hold marketplaces still refuses a name that is not
    /// one of them: there, the selection can be wrong, and a typo must not
    /// quietly build something else.
    #[test]
    fn naming_a_marketplace_a_populated_vault_lacks_is_still_refused() {
        let vfs = two_marketplaces();
        let request = ExportRequest {
            marketplace: Some("gamma".to_owned()),
            ..request(ExportMode::Build)
        };
        let err = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown-marketplace"), "{err}");
        assert!(err.contains("`acme`") && err.contains("`beta`"), "{err}");
    }

    #[test]
    fn two_marketplaces_are_refused_rather_than_flattened_onto_one_tree() {
        let vfs = two_marketplaces();
        let err = export_skills(&vfs, &request(ExportMode::Build), &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("ambiguous-marketplace"), "{err}");
        assert!(err.contains("`acme`"), "{err}");
        assert!(err.contains("`beta`"), "{err}");
        // The refusal is the rail: nothing was rendered on the way to it.
        assert!(paths_under(&vfs, Path::new("/market")).is_empty());
    }

    #[test]
    fn naming_a_marketplace_builds_that_one_and_leaves_the_other_out() {
        let vfs = two_marketplaces();
        let request = ExportRequest {
            marketplace: Some("acme".to_owned()),
            ..request(ExportMode::Build)
        };
        let report = export_skills(&vfs, &request, &sw()).unwrap();
        assert_eq!(report.checked, 1);
        assert!(
            text_at(
                &vfs,
                "/market/.claude-plugin/packs/core/skills/review/SKILL.md"
            )
            .contains("Use review.")
        );
    }

    #[test]
    fn an_unknown_marketplace_is_refused_naming_the_ones_there_are() {
        let vfs = two_marketplaces();
        let request = ExportRequest {
            marketplace: Some("gamma".to_owned()),
            ..request(ExportMode::Build)
        };
        let err = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown-marketplace"), "{err}");
        assert!(err.contains("it holds `acme` and `beta`"), "{err}");
    }

    /// A marketplace that exists and holds nothing this verb renders is told
    /// so, rather than told it is not there. Commands render (see
    /// `a_commands_only_plugin_still_gets_a_marketplace_entry`), so an agent
    /// — a kind no root has a pack layout for — is what stays genuinely
    /// uncarried.
    #[test]
    fn a_marketplace_holding_no_skill_or_command_is_empty_rather_than_unknown() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/acme/plugins/core/skills/review.md",
            &note("review", &[]),
        );
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/agents/reviewer.md",
            &note("reviewer", &[]),
        );

        let request = ExportRequest {
            marketplace: Some("beta".to_owned()),
            ..request(ExportMode::Build)
        };
        let err = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("empty-marketplace"), "{err}");
        assert!(err.contains("`beta`"), "{err}");
    }

    /// A plugin with no skills at all, only a command, still builds — on the
    /// Claude root, which has a place for one — and still gets a marketplace
    /// index entry, or nothing could ever install it.
    #[test]
    fn a_commands_only_plugin_still_gets_a_marketplace_entry() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/commands/ship.md",
            &note("ship", &[]),
        );

        let request = ExportRequest {
            marketplace: Some("beta".to_owned()),
            ..request(ExportMode::Build)
        };
        let report = export_skills(&vfs, &request, &sw()).unwrap();
        assert_eq!(report.mode, ExportMode::Build);
        assert!(
            vfs.exists(Path::new(
                "/market/.claude-plugin/packs/core/commands/ship.md"
            )),
            "the command file itself"
        );
        assert!(
            vfs.exists(Path::new(
                "/market/.claude-plugin/packs/core/.claude-plugin/plugin.json"
            )),
            "the plugin still gets a manifest"
        );
        let marketplace_manifest = text_at(&vfs, "/market/.claude-plugin/marketplace.json");
        assert!(
            marketplace_manifest.contains("\"core\""),
            "the plugin is listed even with no skills: {marketplace_manifest}"
        );
        // Codex has no place for a command, so the plugin is absent from
        // that root entirely: no directory, no manifest, no listing.
        assert!(
            !vfs.exists(Path::new("/market/.agents/packs/core")),
            "codex has no layout for a commands-only plugin"
        );
    }

    /// The refusal runs after the compile, so a peer the *body* reaches —
    /// not just one seeded by a bundle folder — is refused rather than
    /// silently dropped from the plugin.
    #[test]
    fn a_command_embedding_a_file_from_elsewhere_in_the_vault_is_refused() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/commands/ship.md",
            "---\nname: ship\ndescription: Cuts a release.\n---\n\n             Cut it.\n\n![[team-logo.png]]\n",
        );
        vfs.create_dir_all(Path::new("/vault/assets")).unwrap();
        vfs.write_file(Path::new("/vault/assets/team-logo.png"), b"PNG")
            .unwrap();

        let request = ExportRequest {
            marketplace: Some("beta".to_owned()),
            ..request(ExportMode::Build)
        };
        let err = export_skills(&vfs, &request, &sw()).unwrap_err();

        let message = err.to_string();
        assert!(message.contains("no-renderer"), "{message}");
        assert!(message.contains("team-logo.png"), "{message}");
        assert!(
            !vfs.exists(Path::new(
                "/market/.claude-plugin/packs/core/commands/ship.md"
            )),
            "nothing is committed from a refused build"
        );
    }

    /// A plugin carrying no skills is still a plugin at a root that has a
    /// place for its commands: the diff admits it, so a build reports it and
    /// a following check does not call its own correct output stale.
    #[test]
    fn a_commands_only_plugin_is_reported_and_never_stale() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/commands/ship.md",
            &note("ship", &[]),
        );

        let build = ExportRequest {
            marketplace: Some("beta".to_owned()),
            ..request(ExportMode::Build)
        };
        let report = export_skills(&vfs, &build, &sw()).unwrap();
        assert!(
            report.units.iter().any(|unit| matches!(
                unit,
                ExportUnit::Plugin { root, name, .. } if root == "claude" && name == "core"
            )),
            "the build reports the plugin: {:?}",
            report.units
        );

        let checked = export_skills(
            &vfs,
            &ExportRequest {
                marketplace: Some("beta".to_owned()),
                ..request(ExportMode::Check)
            },
            &sw(),
        )
        .unwrap();
        assert!(!checked.changed, "{:?}", checked.units);
        assert!(
            !checked
                .units
                .iter()
                .any(|unit| matches!(unit, ExportUnit::Stale { .. })),
            "nothing is stale: {:?}",
            checked.units
        );
    }

    /// A hook note, as an author writes one: typed frontmatter and a body
    /// that is the script.
    fn hook_note(name: &str, event: &str, extra: &str, body: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: Use {name}.\nevent: {event}\n{extra}---\n\n{body}\n"
        )
    }

    /// AC-3: a build writes `hooks/hooks.json` under the plugin root, in the
    /// nested shape the harness reads, with the body as the script.
    #[test]
    fn a_plugin_build_writes_its_hooks_json() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/skills/review.md",
            &note("review", &[]),
        );
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/hooks/lint-on-write.md",
            &hook_note(
                "lint-on-write",
                "PreToolUse",
                "matcher: Edit|Write\ntimeout: 5000\n",
                "just lint",
            ),
        );

        let request = ExportRequest {
            marketplace: Some("beta".to_owned()),
            ..request(ExportMode::Build)
        };
        export_skills(&vfs, &request, &sw()).unwrap();

        let hooks = text_at(&vfs, "/market/.claude-plugin/packs/core/hooks/hooks.json");
        let value: serde_json::Value = serde_json::from_str(&hooks).unwrap();
        let group = &value["hooks"]["PreToolUse"][0];
        assert_eq!(group["matcher"], "Edit|Write");
        assert_eq!(group["hooks"][0]["type"], "command");
        assert_eq!(group["hooks"][0]["command"], "just lint");
        assert_eq!(group["hooks"][0]["timeout"], 5);
        let codex = text_at(&vfs, "/market/.agents/packs/core/hooks/hooks.json");
        let codex: serde_json::Value = serde_json::from_str(&codex).unwrap();
        assert_eq!(
            codex["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "just lint"
        );
        let cursor = text_at(&vfs, "/market/.cursor-plugin/packs/core/hooks/hooks.json");
        let cursor: serde_json::Value = serde_json::from_str(&cursor).unwrap();
        assert_eq!(cursor["hooks"]["preToolUse"][0]["command"], "just lint");
        let manifest = text_at(
            &vfs,
            "/market/.cursor-plugin/packs/core/.cursor-plugin/plugin.json",
        );
        assert!(
            manifest.contains("\"hooks\": \"./hooks/hooks.json\""),
            "{manifest}"
        );
        assert!(!vfs.exists(Path::new("/market/.pi/packs/core/hooks/hooks.json")));
    }

    /// Exercises the one-document-per-plugin contract documented on `HarnessTranslator::hooks`.
    #[test]
    fn every_hook_of_a_plugin_lands_in_one_document() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/hooks/lint.md",
            &hook_note("lint", "PreToolUse", "matcher: Edit\n", "just lint"),
        );
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/hooks/report.md",
            &hook_note("report", "Stop", "", "just report"),
        );

        let request = ExportRequest {
            marketplace: Some("beta".to_owned()),
            ..request(ExportMode::Build)
        };
        export_skills(&vfs, &request, &sw()).unwrap();

        let hooks = text_at(&vfs, "/market/.claude-plugin/packs/core/hooks/hooks.json");
        let value: serde_json::Value = serde_json::from_str(&hooks).unwrap();
        assert_eq!(
            value["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "just lint"
        );
        assert_eq!(
            value["hooks"]["Stop"][0]["hooks"][0]["command"],
            "just report"
        );
        // See marketplace_manifest's doc: a hooks-only plugin still gets an index entry.
        let marketplace_manifest = text_at(&vfs, "/market/.claude-plugin/marketplace.json");
        assert!(
            marketplace_manifest.contains("\"core\""),
            "{marketplace_manifest}"
        );
    }

    /// AC-5: an event the root's runtime does not carry stops the build,
    /// naming the event and the target, rather than writing a hooks file
    /// missing one hook.
    #[test]
    fn an_event_the_target_does_not_carry_refuses_the_build() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/hooks/watch.md",
            // Codex's own event, which Claude Code documents no point for;
            // `targets` pins the note to the Claude root so it is not
            // simply left off it for a root that does carry the event.
            &hook_note(
                "watch",
                "Interrupt",
                "sw-targets:\n  - claude\n",
                "just watch",
            ),
        );

        let request = ExportRequest {
            marketplace: Some("beta".to_owned()),
            ..request(ExportMode::Build)
        };
        let err = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("Interrupt"), "{err}");
        assert!(err.contains("claude-code"), "{err}");
        assert!(
            !vfs.exists(Path::new(
                "/market/.claude-plugin/packs/core/hooks/hooks.json"
            )),
            "nothing partial is written"
        );
    }

    /// A note with no `targets` is left off a root that cannot carry its
    /// event when another root does, as `targets` would narrow it.
    #[test]
    fn an_untargeted_note_skips_a_root_that_cannot_carry_it() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/hooks/notify.md",
            &hook_note("notify", "Notification", "", "just notify"),
        );

        let request = ExportRequest {
            marketplace: Some("beta".to_owned()),
            ..request(ExportMode::Build)
        };
        export_skills(&vfs, &request, &sw()).unwrap();
        let claude = "/market/.claude-plugin/packs/core/hooks/hooks.json";
        let text = String::from_utf8(vfs.read_file(Path::new(claude)).unwrap()).unwrap();
        assert!(text.contains("just notify"), "{text}");
        assert!(
            !vfs.exists(Path::new(
                "/market/.cursor-plugin/packs/core/hooks/hooks.json"
            )),
            "cursor has no Notification event, so the note is left off it"
        );
    }

    /// The step-5 rule: a note that states its payload twice is refused
    /// rather than silently preferring one.
    #[test]
    fn a_hook_with_both_a_body_and_a_command_key_is_refused() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/hooks/twice.md",
            &hook_note("twice", "PreToolUse", "command: just lint\n", "just fmt"),
        );

        let request = ExportRequest {
            marketplace: Some("beta".to_owned()),
            ..request(ExportMode::Build)
        };
        let err = export_skills(&vfs, &request, &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("hook-payload-twice"), "{err}");
        assert!(err.contains("twice"), "{err}");
    }

    /// A plugin with both a skill and a command: the Claude root carries
    /// both, the Codex root carries the skill alone — its own plugin format
    /// simply has no place for the command, the same silent narrowing
    /// `targets` already does per root.
    #[test]
    fn a_mixed_plugin_carries_its_command_only_where_the_root_has_a_layout() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/skills/review.md",
            &note("review", &[]),
        );
        write(
            &vfs,
            "/vault/marketplaces/beta/plugins/core/commands/ship.md",
            &note("ship", &[]),
        );

        let request = ExportRequest {
            marketplace: Some("beta".to_owned()),
            ..request(ExportMode::Build)
        };
        export_skills(&vfs, &request, &sw()).unwrap();

        assert!(vfs.exists(Path::new(
            "/market/.claude-plugin/packs/core/skills/review/SKILL.md"
        )));
        assert!(vfs.exists(Path::new(
            "/market/.claude-plugin/packs/core/commands/ship.md"
        )));
        assert!(vfs.exists(Path::new(
            "/market/.agents/packs/core/skills/review/SKILL.md"
        )));
        assert!(
            !vfs.exists(Path::new("/market/.agents/packs/core/commands")),
            "codex's own plugin has no commands/ directory at all"
        );
    }

    /// Promotion, read from the far end: a skill above every marketplace is
    /// shared by all of them, so it renders into each build rather than none.
    #[test]
    fn a_skill_promoted_above_the_marketplaces_renders_into_each_one() {
        let vfs = two_marketplaces();
        write(&vfs, "/vault/skills/tone.md", &note("tone", &[]));

        for market in ["acme", "beta"] {
            let out = PathBuf::from(format!("/market-{market}"));
            let request = ExportRequest {
                marketplace: Some(market.to_owned()),
                out_root: out.clone(),
                ..request(ExportMode::Build)
            };
            let report = export_skills(&vfs, &request, &sw()).unwrap();
            assert_eq!(report.checked, 2, "{market}");
            let paths = paths_under(&vfs, &out);
            // `tone` sits outside `plugins/`, so it lands in the plugin named
            // after the marketplace.
            assert!(
                paths.contains(&".claude-plugin/packs/demo-market/skills/tone/SKILL.md".to_owned()),
                "{market}: {paths:?}"
            );
            assert!(
                paths.contains(&".claude-plugin/packs/core/skills/review/SKILL.md".to_owned()),
                "{market}: {paths:?}"
            );
        }
    }

    /// The one collision discovery cannot see: two legal files in two places,
    /// meeting only because this build renders both into one tree.
    #[test]
    fn a_loose_skill_clashing_with_a_folder_of_the_marketplace_s_name_is_refused_naming_both() {
        let vfs = MemVfs::new();
        write(
            &vfs,
            "/vault/marketplaces/acme/plugins/demo-market/skills/review.md",
            &note("review", &[]),
        );
        write(&vfs, "/vault/skills/review.md", &note("review", &[]));

        let err = export_skills(&vfs, &request(ExportMode::Build), &sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("own-plugin-clash"), "{err}");
        assert!(
            err.contains("marketplaces/acme/plugins/demo-market/skills/review.md"),
            "{err}"
        );
        assert!(err.contains("skills/review.md"), "{err}");
    }

    /// A vault with no marketplaces at all still builds — every skill is
    /// promoted to the top, and the config alone names the marketplace.
    #[test]
    fn a_vault_holding_no_marketplace_builds_into_the_configured_one() {
        let vfs = MemVfs::new();
        write(&vfs, "/vault/skills/tone.md", &note("tone", &[]));

        let report = export_skills(&vfs, &request(ExportMode::Build), &sw()).unwrap();
        assert_eq!(report.checked, 1);
        assert!(
            paths_under(&vfs, Path::new("/market"))
                .contains(&".claude-plugin/packs/demo-market/skills/tone/SKILL.md".to_owned())
        );
    }

    #[test]
    fn an_empty_vault_is_refused_not_built() {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        write(
            &vfs,
            "/vault/marketplaces/house/plugins/draft.md",
            "# Not publishable\n",
        );

        let err = export_skills(&vfs, &request(ExportMode::Build), &sw()).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("empty-marketplace"), "{message}");
        assert!(!vfs.exists(Path::new("/market")));
    }

    #[test]
    fn the_wire_forms_are_kebab_case() {
        let report = ExportReport {
            mode: ExportMode::Check,
            checked: 3,
            changed: true,
            units: vec![
                ExportUnit::Plugin {
                    root: "claude".to_owned(),
                    name: "tools".to_owned(),
                    skills: 2,
                    status: UnitStatus::Drift,
                    version_bump_suggestion: None,
                },
                ExportUnit::Manifest {
                    root: "codex".to_owned(),
                    status: UnitStatus::Ok,
                },
                ExportUnit::Stale {
                    root: "claude".to_owned(),
                    path: "plugins/old".to_owned(),
                    kind: StaleKind::Plugin,
                },
            ],
            warnings: Vec::new(),
        };
        assert_eq!(
            serde_json::to_string(&report).unwrap(),
            r#"{"mode":"check","checked":3,"changed":true,"units":[{"unit":"plugin","root":"claude","name":"tools","skills":2,"status":"drift","version_bump_suggestion":null},{"unit":"manifest","root":"codex","status":"ok"},{"unit":"stale","root":"claude","path":"plugins/old","kind":"plugin"}],"warnings":[]}"#
        );
        // And a request with no mode means build — the write, as with
        // `PublishMode`.
        let request: ExportRequest =
            serde_json::from_str(r#"{"vault_root":"/v","out_root":"/m","config":{"name":"m"}}"#)
                .unwrap();
        assert_eq!(request.mode, ExportMode::Build);
        assert!(request.skills_root.is_none());
    }

    /// A build that quietly omitted an agent would be the failure this crate
    /// is built around. It has no place for one — a pack declares only
    /// `skills/` — so it names it instead, once, with no root.
    #[test]
    fn a_component_no_pack_can_hold_is_named_rather_than_dropped() {
        let vfs = MemVfs::new();
        let base = "/vault/marketplaces/house/plugins/core";
        write(
            &vfs,
            &format!("{base}/skills/review.md"),
            "---\nname: review\ndescription: Reviews.\n---\n\nRead it.\n",
        );
        write(
            &vfs,
            &format!("{base}/agents/triage.md"),
            "---\nname: triage\ndescription: Triages.\n---\n\nSort it.\n",
        );

        let report = check(&vfs);

        let named: Vec<&ExportUnit> = report
            .units
            .iter()
            .filter(|u| matches!(u, ExportUnit::NotCarried { .. }))
            .collect();
        assert_eq!(named.len(), 1, "{:?}", report.units);
        let ExportUnit::NotCarried { root, path, kind } = named[0] else {
            unreachable!()
        };
        assert_eq!(*root, None);
        assert_eq!(path, "marketplaces/house/plugins/core/agents/triage.md");
        assert_eq!(kind, "agent");
        // And the skill still built.
        assert!(
            report
                .units
                .iter()
                .any(|u| matches!(u, ExportUnit::Plugin { .. })),
            "{:?}",
            report.units
        );
    }

    /// A command on a root whose pack format has no place for one is named
    /// on that root, not dropped from it. The root that carries it reports
    /// nothing extra, and a root `targets` narrowed away stays silent.
    #[test]
    fn a_command_a_root_cannot_carry_is_named_on_that_root() {
        let vfs = MemVfs::new();
        let base = "/vault/marketplaces/house/plugins/core";
        write(
            &vfs,
            &format!("{base}/skills/review.md"),
            &note("review", &[]),
        );
        write(
            &vfs,
            &format!("{base}/commands/ship.md"),
            &note("ship", &[]),
        );
        write(
            &vfs,
            &format!("{base}/commands/codex-only.md"),
            &note("codex-only", &["codex"]),
        );

        let report = check(&vfs);

        let named: Vec<(Option<&str>, &str, &str)> = report
            .units
            .iter()
            .filter_map(|u| match u {
                ExportUnit::NotCarried { root, path, kind } => {
                    Some((root.as_deref(), path.as_str(), kind.as_str()))
                }
                _ => None,
            })
            .collect();
        let ship = "marketplaces/house/plugins/core/commands/ship.md";
        let codex_only = "marketplaces/house/plugins/core/commands/codex-only.md";
        assert_eq!(
            named,
            vec![
                (Some("codex"), codex_only, "command"),
                (Some("codex"), ship, "command"),
                (Some("cursor"), ship, "command"),
                (Some("pi"), ship, "command"),
            ],
            "{:?}",
            report.units
        );
    }

    /// A hook on the one root with no hooks file (pi) is named there, and
    /// on no root that carries it.
    #[test]
    fn a_hook_a_root_cannot_carry_is_named_on_that_root() {
        let vfs = MemVfs::new();
        let base = "/vault/marketplaces/beta/plugins/core";
        write(
            &vfs,
            &format!("{base}/skills/review.md"),
            &note("review", &[]),
        );
        write(
            &vfs,
            &format!("{base}/hooks/lint-on-write.md"),
            &hook_note(
                "lint-on-write",
                "PreToolUse",
                "matcher: Edit\n",
                "just lint",
            ),
        );

        let report = export_skills(
            &vfs,
            &ExportRequest {
                marketplace: Some("beta".to_owned()),
                ..request(ExportMode::Check)
            },
            &sw(),
        )
        .unwrap();

        let named: Vec<(Option<&str>, &str, &str)> = report
            .units
            .iter()
            .filter_map(|u| match u {
                ExportUnit::NotCarried { root, path, kind } => {
                    Some((root.as_deref(), path.as_str(), kind.as_str()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            named,
            vec![(
                Some("pi"),
                "marketplaces/beta/plugins/core/hooks/lint-on-write.md",
                "hook"
            )],
            "{:?}",
            report.units
        );
    }

    /// The committed fixture, end to end on a real filesystem: a
    /// bundled skill with an embed, built into every root and clean on the
    /// check that follows.
    #[test]
    fn the_committed_fixture_builds_on_a_real_filesystem() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent-skills-vault");
        let dir = tempfile::tempdir().unwrap();
        let request = ExportRequest {
            vault_root: fixture,
            skills_root: None,
            marketplace: None,
            out_root: dir.path().join("market"),
            harnesses: BTreeSet::new(),
            mode: ExportMode::Build,
            config: config(),
            assets: Vec::new(),
            hooks: Vec::new(),
        };

        let report = export_skills(&NativeVfs, &request, &sw()).unwrap();
        assert_eq!(report.checked, 1);
        assert!(report.changed);

        let skill = dir
            .path()
            .join("market/.claude-plugin/packs/tools/skills/workflow-checklist");
        let text = std::fs::read_to_string(skill.join("SKILL.md")).unwrap();
        assert!(text.contains("name: workflow-checklist"), "{text}");
        assert!(
            text.contains(
                "marketplaces/house/plugins/tools/skills/workflow-checklist/workflow-checklist.md; \
                 do not edit."
            ),
            "{text}"
        );
        // The embed inlined its body; the bundle file still travels.
        assert!(text.contains("Understand the requested outcome."), "{text}");
        assert!(!text.contains("![["), "{text}");
        assert!(skill.join("references/steps.md").exists());
        assert!(
            dir.path()
                .join("market/.agents/packs/tools/skills/workflow-checklist/SKILL.md")
                .exists()
        );

        let check = ExportRequest {
            mode: ExportMode::Check,
            ..request
        };
        let report = export_skills(&NativeVfs, &check, &sw()).unwrap();
        assert!(!report.changed, "{report:?}");
    }
}
