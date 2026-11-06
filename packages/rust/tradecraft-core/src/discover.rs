//! Discovery — every publishable note under a skills root, found and vetted.
//!
//! The scan every marketplace verb stands on:
//! `build` renders what this returns, `check` compares it, `status` lists it.
//! The rules here are the definition of *publishable*, so a note this module
//! refuses is refused everywhere at once rather than differently per verb.
//!
//! # This is not [`crate::scan`]
//!
//! The corpus scan walks arbitrary registered roots looking for **published**
//! `SKILL.md` files, never fails, and lists broken skills so a user can find
//! them. This walks one curated tree of **vault notes**, and it refuses:
//! a skills root feeds a publish, and a scan that shrugged past a duplicate
//! name or a symlink would surface the problem as a wrong marketplace instead
//! of a sentence naming the note.
//!
//! # The shapes
//!
//! Where a note sits decides what it is. A kind folder ([`KIND_DIRS`]) names
//! what it holds, and each thing it holds is a note or a folder with its note
//! inside:
//!
//! | Path | Read as |
//! |---|---|
//! | `<kind-dir>/<name>.md` | a component of that kind |
//! | `<kind-dir>/<name>/<name>.md` | the same, carrying the files beside it |
//! | `skills/<name>/SKILL.md` | a skill carrying files, named by its folder |
//!
//! Containers repeat the pattern one level down, so
//! `marketplaces/<m>/plugins/<p>/skills/<s>.md` is the full shape. A note
//! that is not under a kind folder is a resource or the author's own writing.
//!
//! The folder is the answer: no frontmatter key names a note's kind or its
//! plugin. Every component reports which layer answered.
//!
//! # What *declared* means
//!
//! A component declares `description`. That key alone is the opt-in: a note
//! without it — prose, a bundle resource, a draft — is skipped without
//! comment, and that silence is what lets resources live beside their entry
//! note. `name` is optional and defaults to the filename, slugified, which is
//! the answer [`crate::vault::read_vault_skill`] already gives a single-note
//! publish.
//!
//! `name` without `description` also opts in: the note is admitted with
//! [`MISSING_DESCRIPTION`] and named in [`DiscoveryReport::warnings`].
//! Defaulting the description for a note with neither key would make every
//! note in the tree a component. A container's own note (`<p>/<p>.md`) is the
//! exception: the folder already makes the container, the note is optional,
//! and it is read whatever keys it holds, with an empty description and no
//! warning. A note that declares a description but sits
//! at no shape is refused, and told where a component can live — dropping it
//! silently is the failure this reader exists to avoid.
//!
//! # Every kind, not only skills
//!
//! The scan returns plugins, commands and agents alongside skills, because a
//! workbench has to list a command and say what it is. The marketplace render
//! is unaffected: [`crate::export`] filters to [`EntityKind::Skill`] before it
//! renders; [`crate::publish`] takes each kind by its own route.
//!
//! # Validation
//!
//! The skills-spec name rule (≤64, `^[a-z0-9]+(-[a-z0-9]+)*$`) for component
//! and plugin names, the 1024-character description cap, the 500-character
//! compatibility cap, the `<name>/<name>.md` bundle rule, bundle overlap
//! refused, symlinks refused.
//! Beyond those:
//!
//! * **A declared `name` need not match the filename.** The filename decides
//!   the name when frontmatter declares none; when frontmatter declares one, it
//!   wins — the same answer [`crate::publish`] gives. Two components still
//!   cannot publish onto one directory: that is the uniqueness key below.
//!
//! * **Names are unique per plugin and kind**, not across the whole scan: a
//!   plugin may hold both a `review` command and a `review` skill, which are
//!   two directories. What still cannot happen is two components publishing
//!   onto one.
//!
//! * **Targets validate against [`KNOWN_TARGETS`]** — the names this
//!   build publishes to — and the key is `targets` read through
//!   [`VaultFrontmatter`], not a bare `skill-targets`. With that key comes the
//!   core's contract: absent means every destination, an empty list means
//!   nowhere, and a scalar is refused rather than comma-split — the shape rules
//!   [`crate::frontmatter`] already enforces on every read.
//! * **[`slugify`] folds ASCII only**, with no NFKD normalisation: `Café.md`
//!   cannot carry `name: cafe`, and the fix is to name the file in the name's
//!   own alphabet. Unicode normalisation tables are a dependency this crate
//!   does not otherwise need.
//!
//! # Deterministic, like every report in this crate
//!
//! Directory entries are sorted before descent, results are sorted by name,
//! and nothing reads a clock or iterates a hash map — two runs over one tree
//! return byte-identical results, which is what lets `check` compare a
//! marketplace against a fresh discovery at all.
//!
//! # Symlinks are refused, as far as the host can see them
//!
//! A link inside a skills source reads as inside the tree and resolves
//! outside it; a bundle copied file-by-file would silently flatten it into
//! whatever it pointed at. The walk asks [`Vfs::is_symlink`] about every
//! entry and refuses `unsafe-path` on the first hit. On a host whose `Vfs`
//! cannot see links — [`crate::MemVfs`], the wasm bridge over a host object
//! without link methods — the method's
//! default answers `false` and the refusal cannot fire; that is the honest
//! floor, the same one [`Vfs::canonicalize`]'s lexical default sets for the
//! publish guard.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::CoreError;
use crate::frontmatter::{
    ContainerIdentity, EntityKind, KindHints, KindSource, VaultFrontmatter, resolve_kind,
};
use crate::model::member_block_keys;
use crate::namespace::Namespace;
use crate::publish::KNOWN_TARGETS;
use crate::skill::{forward_slashed, read_utf8, split_frontmatter};
use crate::vault::frontmatter_map;
use crate::vfs::Vfs;

/// One component, as discovery hands it to the verbs.
///
/// `targets` keeps the three-outcome `targets` reading instead of being
/// expanded to an explicit all-targets list, so the layer that knows what
/// "every destination" means today is the one that decides it.
// No `Eq`: `identity` carries JSON for the two manifest fields
// whose dialects allow an object, and `serde_json::Value` is only `PartialEq`
// because it can hold a float. Nothing keys a map or a set on a whole
// component — uniqueness is keyed on `(marketplace, plugin, kind, name)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredComponent {
    /// The frontmatter `name`, already validated against the Agent Skills spec.
    pub name: String,
    /// The frontmatter `description`, ≤1024 characters, or
    /// [`MISSING_DESCRIPTION`] when the note wrote none. Empty on a container
    /// note with neither `name` nor `description`.
    pub description: String,
    /// `description` is not the note's own.
    #[serde(default)]
    pub description_filled: bool,
    /// What this component is, resolved across the layers that can decide it.
    pub kind: EntityKind,
    /// Which layer produced [`Self::kind`], straight from
    /// [`crate::frontmatter::resolve_kind`].
    pub kind_source: KindSource,
    /// What the structure said this was, before frontmatter was read. `None`
    /// where structure said nothing — a component at the root has no folder
    /// above it naming a kind.
    ///
    /// Kept beside the resolved value rather than discarded so a caller can
    /// tell "the folder and the note agree" from "the note overruled the
    /// folder".
    pub folder_kind: Option<EntityKind>,
    /// `targets` exactly as [`VaultFrontmatter::targets`] read it, every
    /// name checked against [`KNOWN_TARGETS`]. `None` is absent — every
    /// destination; `Some(vec![])` is nowhere.
    pub targets: Option<Vec<String>>,
    /// The plugin holding this component. `None` is a bare component, sitting
    /// at the root and belonging to no plugin.
    ///
    /// The containing folder decides it. Where a marketplace needs every
    /// skill under *some* plugin directory, [`crate::export`] supplies the
    /// plugin named after the marketplace: a publication concern, not a fact
    /// about the vault.
    pub plugin: Option<String>,
    /// The marketplace this belongs to — the directory holding the `plugins/`
    /// folder it sits under. `None` for a marketplace's own note, and for a
    /// component with no marketplace folder above it.
    ///
    /// Structural, with no frontmatter counterpart.
    pub marketplace: Option<String>,
    /// Absolute path of the entry note.
    pub entry_path: PathBuf,
    /// The directory this component owns — everything under it ships with a
    /// skill. `None` for a lone note, which travels alone.
    pub bundle_root: Option<PathBuf>,
    /// Files this component owns, relative to [`Self::bundle_root`] and
    /// forward-slashed, sorted.
    ///
    /// Ownership is *deepest* directory wins, so a plugin holding
    /// `skills/code-review/scripts/check.py` does not claim it —
    /// `code-review` does. Without that rule a plugin reads as a flat dump of
    /// its own subtree.
    pub resources: Vec<String>,
    /// The entry's path relative to the vault root, forward-slashed — the
    /// spelling reports and provenance markers print.
    pub vault_relative_path: String,
    /// A hook note's routing and payload, as a reader of the tree needs them:
    /// `None` on every other kind.
    ///
    /// Carried here because a hook's file name says nothing about what fires
    /// it — a lister that only had the name would have to open each note to
    /// tell one hook from another.
    #[serde(default)]
    pub hook: Option<DiscoveredHook>,
    /// The manifest fields this note publishes under, read from its own
    /// frontmatter.
    ///
    /// Carried for every kind and consulted for two: a marketplace note names
    /// the distribution, a plugin note names the pack. A skill's is silent in
    /// every vault worth having, because a skill's manifest entry is derived
    /// from the skill and a second spelling of it could only disagree.
    ///
    /// Surfaced here rather than left in the note because the scan is the only
    /// pass that reads a container note at all — [`crate::export`] renders
    /// from what discovery hands it and never opens a file of its own.
    pub identity: ContainerIdentity,
    /// The note's harness blocks (`claude:`, `codex:`, `cursor:`, `pi:`),
    /// keyed by harness. A container's are merged over the manifests it
    /// generates and inherited by its members.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub blocks: Map<String, Value>,
    /// Keys the key table does not list for this kind, sorted. The scan
    /// warns about each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_keys: Vec<String>,
}

impl DiscoveredComponent {
    /// The folder a container is known by: a plugin's entry note is named for
    /// it, and a marketplace's sits in it as the root file. A marketplace note's
    /// `name` is only what it publishes under.
    pub(crate) fn folder(&self) -> &str {
        let named = if self.kind == EntityKind::Marketplace {
            self.entry_path.parent().and_then(Path::file_name)
        } else {
            self.entry_path.file_stem()
        };
        named.and_then(|name| name.to_str()).unwrap_or_default()
    }
}

/// What happens to one unknown key, as a warning says it. A key the namespace
/// claims and a field of the `adopted` block are consumed on the read.
fn unknown_key_fate(kind: EntityKind, key: &str, ns: &Namespace) -> &'static str {
    match kind {
        EntityKind::Plugin | EntityKind::Marketplace | EntityKind::Hook => "ignored",
        _ if ns.is_ours(key) || key.starts_with(&format!("{}.", ns.key("adopted"))) => "dropped",
        EntityKind::Skill => "published under `metadata:`",
        EntityKind::Command => "carried as written",
        EntityKind::Agent => "carried as written when the agent publishes on its own",
    }
}

/// The warning for a note's unknown keys, or `None` when it has none.
pub(crate) fn unknown_key_warning(
    path: &str,
    kind: EntityKind,
    unknown: &[String],
    ns: &Namespace,
) -> Option<DiscoveryWarning> {
    if unknown.is_empty() {
        return None;
    }
    let keys = quoted(&unknown.iter().map(String::as_str).collect::<Vec<_>>());
    let mut fates: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for key in unknown {
        fates
            .entry(unknown_key_fate(kind, key, ns))
            .or_default()
            .push(key);
    }
    let fate = if fates.len() == 1 {
        fates.into_keys().collect::<String>()
    } else {
        fates
            .iter()
            .map(|(fate, keys)| format!("{} {fate}", quoted(keys)))
            .collect::<Vec<_>>()
            .join("; ")
    };
    let mut hint = if ns.is_bare() {
        String::new()
    } else {
        let respelled: Vec<String> = unknown
            .iter()
            .filter_map(|key| {
                let field = ns.field(key).replace('-', "_");
                let bare = key == "adopted" || crate::harness::profile_named(key).is_some();
                let hyphen = field != ns.field(key) && crate::keys::is_skillwright_key(&field);
                (bare || hyphen).then(|| format!("`{key}` is `{}`", ns.key(&field)))
            })
            .collect();
        let examples = if respelled.is_empty() {
            String::new()
        } else {
            format!(": {}", respelled.join(", "))
        };
        format!(
            ". Under namespace `{}`, skillwright's keys are spelled `{}`{examples}",
            ns.stem(),
            ns.key("<key>")
        )
    };
    // The one spec key that lands here: it is listed, but the value is not one
    // a harness reads.
    if unknown.iter().any(|key| key == "allowed-tools") {
        hint.push_str(". `allowed-tools` takes a list, or a space-separated string");
    }
    let subject = if unknown.len() == 1 {
        "is not a key"
    } else {
        "are not keys"
    };
    let article = if kind == EntityKind::Agent { "an" } else { "a" };
    Some(DiscoveryWarning {
        path: path.to_owned(),
        reason: UNKNOWN_KEY_REASON.to_owned(),
        detail: format!(
            "{keys} {subject} skillwright reads on {article} {}; {fate}{hint}",
            kind.as_str()
        ),
    })
}

/// A note's unknown keys, the keys in a container's blocks that nothing reads
/// included.
pub(crate) fn unknown_keys(frontmatter: &VaultFrontmatter, kind: EntityKind) -> Vec<String> {
    let mut unknown = frontmatter.unknown.clone();
    if kind.holds_components() {
        unknown.extend(unknown_container_block_keys(&frontmatter.overrides, kind));
    }
    unknown
}

/// The keys in a container note's harness blocks that no file the block
/// reaches reads, as `<harness>.<key>`. A plugin's block reaches its pack
/// manifest, its marketplace entry and, for a key neither reads, its members.
/// A marketplace's reaches its marketplace manifest, and the rest passes down
/// to its plugins unless it says which plugin this is.
pub(crate) fn unknown_container_block_keys(
    blocks: &Map<String, Value>,
    kind: EntityKind,
) -> Vec<String> {
    let mut unknown = Vec::new();
    for (harness, block) in blocks {
        let Some(profile) = crate::harness::profile_named(harness) else {
            continue;
        };
        let Some(format) = profile.plugin.as_ref() else {
            continue;
        };
        let members = member_block_keys(EntityKind::Skill, harness);
        for key in block.as_object().into_iter().flat_map(|b| b.keys()) {
            let name = key.as_str();
            let on_a_plugin = format.manifest_fields.contains(&name)
                || format.entry_fields.contains(&name)
                || (harness == "pi" && key == crate::harness::manifest::NAMING_KEY)
                || members.contains(key);
            let known = if kind == EntityKind::Marketplace {
                format.marketplace_fields.contains(&name)
                    || key == crate::harness::manifest::PACKS_KEY
                    || (on_a_plugin && !crate::keys::BLOCK_IDENTITY_KEYS.contains(&name))
            } else {
                on_a_plugin
            };
            if !known {
                unknown.push(format!("{harness}.{key}"));
            }
        }
    }
    unknown
}

/// The warning reason for a note that declares a description where it cannot
/// ship.
pub const NEVER_SHIPS_REASON: &str = "never-ships";

/// The reason a note under a kind folder that declares no component is named:
/// a typo in `description` must not make a note vanish.
pub const NO_COMPONENT_KEYS_REASON: &str = "no-component-keys";

/// The warning reason for a key the key table does not list.
pub const UNKNOWN_KEY_REASON: &str = "unknown-key";

/// What a hook note routes on and what it runs, summarised for a listing.
///
/// A summary, not the projection: [`crate::projection::to_adapt_hook`] stays
/// the authority on what a hook renders to and on every refusal. This reads
/// the same keys with none of the refusals, because a tree has to draw a
/// half-written hook too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredHook {
    /// `event` — what fires the hook. `None` where the note names none, which
    /// the projection refuses and a listing still has to show.
    pub event: Option<String>,
    /// `matcher` — pipe-delimited tool names. `None` matches every tool.
    pub matcher: Option<String>,
    /// `type`, defaulted to `command` exactly as the projection defaults it.
    /// An unrecognised spelling is carried verbatim; naming it a typo is the
    /// projection's job.
    pub hook_type: String,
    /// The `command`, `prompt` or `url` this hook runs, whichever its type
    /// licenses — the key when the note writes one, otherwise the body's
    /// first non-empty line, which is where a `command` or `prompt` hook may
    /// instead put it. `None` for `mcp_tool` and `agent`, which carry their
    /// payload elsewhere.
    pub payload: Option<String>,
}

/// What one scan found, and what it could not read.
///
/// Two lists rather than a `Result`, because a vault is a shared tree and a
/// note nobody else owns must not decide whether everybody else's publish
/// runs. Every caller reads both: the components are the roster, and
/// [`Self::skipped`] is the part of the vault this answer does not cover.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryReport {
    /// Every publishable note, sorted by name.
    pub components: Vec<DiscoveredComponent>,
    /// Every note the scan could not read, in path order.
    pub skipped: Vec<SkippedNote>,
    /// Notes admitted with something filled in for them, in path order.
    #[serde(default)]
    pub warnings: Vec<DiscoveryWarning>,
}

/// The description a component gets when its note names itself but writes no
/// `description`. [`DiscoveryReport::warnings`] names every note it stands in
/// for; `build.require_descriptions` refuses the build instead.
pub const MISSING_DESCRIPTION: &str = "No description provided.";

/// The warning reason naming a note published with [`MISSING_DESCRIPTION`].
pub const MISSING_DESCRIPTION_REASON: &str = "missing-description";

/// One note the scan admitted, with what it filled in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryWarning {
    /// Vault-relative and forward-slashed, as [`SkippedNote::path`].
    pub path: String,
    /// A closed-set token: `missing-description` from the scan;
    /// [`crate::export`] adds `missing-version` for a plugin.
    pub reason: String,
    /// Extra detail for a person to read.
    pub detail: String,
}

/// One note the scan could not read, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedNote {
    /// The note's path relative to the vault root, forward-slashed — the same
    /// spelling every other report prints.
    pub path: String,
    /// A closed-set reason token. The refusals that already carry one keep it
    /// (`unknown-target`, `unknown-kind`); everything else — frontmatter that
    /// will not parse, a name the Agent Skills spec does not admit, an unreadable file —
    /// is `unreadable-note`, the vocabulary's entry for "this note does not
    /// parse".
    pub reason: String,
    /// The sentence naming what is wrong, as the error stated it.
    pub detail: String,
    /// The plugin folder holding this note.
    ///
    /// A pack publish checks its own members against this: the publish spec
    /// forbids installing a subset silently. `None` where no plugin folder
    /// holds it: the note joins no pack, and no pack is blocked.
    pub plugin: Option<String>,
}

impl SkippedNote {
    fn new(vault_root: &Path, entry_path: &Path, plugin: Option<String>, err: &CoreError) -> Self {
        let detail = err.to_string();
        // `CoreError::refused` puts the token first, `<token>: <prose>`. Every
        // other error is a content problem with no token of its own.
        // Some rails carry prose alone, so the prefix only counts as a token
        // when it is shaped like one.
        let reason = match err {
            CoreError::Refused(message) => message
                .split_once(": ")
                .map(|(token, _)| token)
                .filter(|token| {
                    !token.is_empty()
                        && token
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                })
                .unwrap_or("unreadable-note")
                .to_owned(),
            _ => "unreadable-note".to_owned(),
        };
        Self {
            path: entry_path
                .strip_prefix(vault_root)
                .map(forward_slashed)
                .unwrap_or_else(|_| entry_path.display().to_string()),
            reason,
            detail,
            plugin,
        }
    }
}

/// Walks `content_root` (the vault root when `None`) and returns every
/// publishable note, sorted by name.
///
/// `ns` is the vault's frontmatter namespace, the same value every other
/// reader takes — a note whose `targets` this scan validated must be the
/// note whose `targets` the publish reads, or the two disagree about where
/// it goes.
///
/// The content root must lie inside the vault. A root outside it has no
/// vault-relative spelling for its notes, and a provenance marker naming `../`
/// paths would point at files the vault does not own.
///
/// # A bad note stops itself, not the scan
///
/// Everything wrong with *one* note — an illegal name, an over-long
/// description, a `targets` this build does
/// not know, a declared note at no recognised shape, frontmatter that will
/// not parse — lands in [`DiscoveryReport::skipped`] and the walk carries on.
/// One person's half-written draft must not block everybody else's publish.
/// A `name` with no `description` is not wrong enough to skip: the note is
/// admitted with [`MISSING_DESCRIPTION`] and named in
/// [`DiscoveryReport::warnings`].
///
/// What stays run-level is what is *not* a fact about one note: a duplicate
/// name and a bundle overlap are two-note diagnoses that have to name both
/// parties, and a symlink is a safety rail that fires before a note is read.
///
/// # Errors
///
/// * [`CoreError::RootNotFound`] — the content root does not exist.
/// * `refused: unsafe-path` — a symlink anywhere in the source.
/// * [`CoreError::Io`] — a folder in the source that cannot be listed.
/// * `refused: path-collision` — a duplicate skill name, or a bundle that
///   contains another skill's entry.
pub fn discover_components(
    vfs: &dyn Vfs,
    vault_root: &Path,
    content_root: Option<&Path>,
    ns: &Namespace,
) -> Result<DiscoveryReport, CoreError> {
    let source = load_source(vfs, vault_root, content_root)?;
    discover_in(vfs, vault_root, &source, ns)
}

/// The skills source at `content_root` (the vault root when `None`): its
/// root, and every file under it in walk order.
///
/// The walk is markdown-vault-fs's. It keeps dot-named files (a bundle may
/// ship one, a link may name one), never enters `.obsidian`, `.trash` or
/// `node_modules`, and walks only the content root. A symlink anywhere in it,
/// or a folder that cannot be listed, fails the walk: a skills source feeds a
/// publish, and an unreadable corner of it must not silently under-publish.
/// The vault's excluded files are still listed; discovery leaves them out.
#[derive(Debug, Clone)]
pub(crate) struct Source {
    pub(crate) root: PathBuf,
    pub(crate) files: Vec<PathBuf>,
}

/// Walks the skills source. See [`Source`].
///
/// # Errors
///
/// * [`CoreError::RootNotFound`] — the content root does not exist.
/// * `refused: unsafe-path` — a symlink anywhere in the source.
/// * [`CoreError::Io`] — a folder that cannot be listed.
pub(crate) fn load_source(
    vfs: &dyn Vfs,
    vault_root: &Path,
    content_root: Option<&Path>,
) -> Result<Source, CoreError> {
    let root = content_root.unwrap_or(vault_root).to_path_buf();
    if !vfs.exists(&root) {
        return Err(CoreError::RootNotFound(root));
    }
    let files = crate::walk::vault_files_refusing_symlinks(
        vfs,
        &root,
        crate::compile::EXCLUDED_VAULT_DIRECTORIES,
        |path| {
            CoreError::refused(
                "unsafe-path",
                format!(
                    "{}: a symlink in a skills source — it reads as inside the tree and \
                     resolves outside it; replace it with the real file or directory",
                    path.display()
                ),
            )
        },
    )?;
    Ok(Source { root, files })
}

/// [`discover_components`] over a source [`load_source`] already walked.
///
/// # Errors
///
/// `refused: path-collision`, as [`discover_components`].
pub(crate) fn discover_in(
    vfs: &dyn Vfs,
    vault_root: &Path,
    source: &Source,
    ns: &Namespace,
) -> Result<DiscoveryReport, CoreError> {
    let root = &source.root;
    let files = source_files(vfs, vault_root, source);

    // Structure first, over the whole file set: a note's shape depends on what
    // sits beside it, so no single path can be classified on its own.
    let patterns = crate::patterns::Patterns::read(vfs, vault_root);
    let placed = place(root, &files, &patterns);

    let mut discovered = Vec::new();
    let mut skipped = Vec::new();
    let mut warnings = Vec::new();
    for entry_path in &files {
        let Some(placement) = placed.get(entry_path) else {
            // Outside every marketplace this is simply the author's own vault,
            // and a note there is not a malformed component however it is
            // shaped. Only inside a marketplace is a declared note at an
            // unrecognised path a mistake worth naming.
            let reads_as = conventional(root, entry_path, &patterns);
            if under_a_kind_folder(root, &reads_as) {
                let relative = |path: &Path| {
                    path.strip_prefix(root)
                        .map(forward_slashed)
                        .unwrap_or_else(|_| path.display().to_string())
                };
                if let Some(market) = old_marketplace_note(&relative(entry_path)) {
                    let err = CoreError::malformed(
                        entry_path,
                        format!(
                            "a marketplace's note is `marketplaces/{market}/skillwright.md` now: \
                             move this file there"
                        ),
                    );
                    skipped.push(SkippedNote::new(vault_root, entry_path, None, &err));
                } else if patterns.never_ships(&relative(entry_path))
                    || patterns.in_a_named_never_ship_folder(&relative(entry_path))
                {
                    if declares_description(vfs, entry_path, ns).unwrap_or(false) {
                        warnings.push(DiscoveryWarning {
                            path: relative(entry_path),
                            reason: NEVER_SHIPS_REASON.to_owned(),
                            detail: "this note declares a description and will not be published, \
                                     because a folder that never ships holds it"
                                .to_owned(),
                        });
                    }
                } else if in_a_skill_folder(entry_path, &placed, root) {
                    // A file of that skill: it ships with it, whatever it declares.
                } else if let Err(err) = refuse_stray(vfs, root, entry_path, &reads_as, ns) {
                    // A stray sits at no recognised shape, so it is a member
                    // of nothing and blocks no pack.
                    skipped.push(SkippedNote::new(vault_root, entry_path, None, &err));
                }
            }
            continue;
        };
        match read_source(vfs, vault_root, entry_path, placement, &placed, &files, ns) {
            Ok(Some(component)) => {
                // An empty description is a container note that names and
                // describes nothing, which is valid: it is not a filler.
                if component.description_filled && !component.description.is_empty() {
                    // Only these two kinds can take a configured description.
                    let unless =
                        if matches!(component.kind, EntityKind::Plugin | EntityKind::Marketplace) {
                            " unless the build configures one"
                        } else {
                            ""
                        };
                    warnings.push(DiscoveryWarning {
                        path: component.vault_relative_path.clone(),
                        reason: MISSING_DESCRIPTION_REASON.to_owned(),
                        detail: format!(
                            "`{}` has a `name` and no `description`; published with \
                             \"{MISSING_DESCRIPTION}\"{unless}",
                            component.name
                        ),
                    });
                }
                if let Some(warning) = unknown_key_warning(
                    &component.vault_relative_path,
                    component.kind,
                    &component.unknown_keys,
                    ns,
                ) {
                    warnings.push(warning);
                }
                discovered.push(component);
            }
            Ok(None) => {
                if placement.kind.is_some_and(|kind| !kind.holds_components())
                    && has_frontmatter_keys(vfs, entry_path)
                {
                    warnings.push(DiscoveryWarning {
                        path: entry_path
                            .strip_prefix(root)
                            .map(forward_slashed)
                            .unwrap_or_else(|_| entry_path.display().to_string()),
                        reason: NO_COMPONENT_KEYS_REASON.to_owned(),
                        detail: "this note has frontmatter but no `name` or `description`, so \
                                 it is not a component and will not ship"
                            .to_owned(),
                    });
                }
            }
            Err(err) => {
                // The folder names the pack, so a pack never ships without
                // a member it could not read.
                let plugin = placement.plugin.clone();
                skipped.push(SkippedNote::new(vault_root, entry_path, plugin, &err));
            }
        }
    }

    // Duplicates before overlaps, both across the whole scan: each is a
    // two-note diagnosis, and it has to name both notes — which is why these
    // run after collection rather than inside the walk.
    //
    // Uniqueness is per (marketplace, plugin, kind, name), not per name: a
    // plugin's `review` command and its `review` skill are two directories,
    // and two plugins may each hold a `review`. What still cannot happen is
    // two components publishing onto one directory, which is exactly this key.
    type PublishKey<'a> = (Option<&'a str>, Option<&'a str>, EntityKind, &'a str);
    let mut by_key: BTreeMap<PublishKey, &DiscoveredComponent> = BTreeMap::new();
    for component in &discovered {
        let key = (
            component.marketplace.as_deref(),
            component.plugin.as_deref(),
            component.kind,
            component.name.as_str(),
        );
        if let Some(first) = by_key.get(&key) {
            return Err(CoreError::refused(
                "path-collision",
                format!(
                    "duplicate {} name `{}` in {}: {}, {} — two components cannot publish onto \
                     one directory",
                    component.kind.as_str(),
                    component.name,
                    component
                        .plugin
                        .as_deref()
                        .map_or_else(|| "no plugin".to_owned(), |p| format!("plugin `{p}`")),
                    first.vault_relative_path,
                    component.vault_relative_path
                ),
            ));
        }
        by_key.insert(key, component);
    }
    for outer in &discovered {
        // A container's directory holds the level below it by design, so that
        // is not an overlap. The invariant this protects is a *shipping* one:
        // everything under a skill's own directory travels with it, so an
        // entry inside another entry's would publish twice.
        if outer.kind.holds_components() {
            continue;
        }
        let Some(bundle) = outer.bundle_root.as_deref() else {
            continue;
        };
        for inner in &discovered {
            if inner.entry_path != outer.entry_path && inner.entry_path.starts_with(bundle) {
                return Err(CoreError::refused(
                    "path-collision",
                    format!(
                        "component bundles cannot overlap: {} contains {} — everything under a \
                         bundle ships with it, so an inner component would publish twice",
                        outer.vault_relative_path, inner.vault_relative_path
                    ),
                ));
            }
        }
    }

    // Keys are unique by the check above, so this order is total.
    discovered.sort_by(|a, b| {
        (&a.name, a.kind.as_str(), &a.plugin).cmp(&(&b.name, b.kind.as_str(), &b.plugin))
    });
    // `skipped` and `warnings` are already in path order: `walk` sorts every
    // directory before it descends, and nothing has reordered the file list
    // since.
    Ok(DiscoveryReport {
        components: discovered,
        skipped,
        warnings,
    })
}

/// A note at no recognised shape, which declares itself a component anyway.
///
/// Silence is right for the ordinary case: resources live beside their entry
/// and declare nothing, and a scan that commented on every one of them would
/// be unusable. But a note carrying a `description` is a note somebody
/// authored to be published, and dropping it without a word is the failure
/// mode this whole reader exists to avoid. Name it, and say where it would
/// have to sit.
fn refuse_stray(
    vfs: &dyn Vfs,
    skills_root: &Path,
    entry_path: &Path,
    reads_as: &Path,
    ns: &Namespace,
) -> Result<(), CoreError> {
    if !declares_description(vfs, entry_path, ns)? {
        return Ok(());
    }

    let dirs: Vec<&str> = KIND_DIRS.iter().map(|(dir, _)| *dir).collect();
    let relative = entry_path
        .strip_prefix(skills_root)
        .map(forward_slashed)
        .unwrap_or_else(|_| entry_path.display().to_string());
    let conventional = reads_as
        .strip_prefix(skills_root)
        .map(forward_slashed)
        .unwrap_or_else(|_| reads_as.display().to_string());
    let pattern = if conventional == relative {
        String::new()
    } else {
        format!(" (`build.patterns` reads it as `{conventional}`)")
    };
    Err(CoreError::malformed(
        entry_path,
        format!(
            "`{relative}`{pattern} declares a `description` but sits where no component can \
             be read. A component is `<name>.md` or `<name>/<name>.md` directly inside a kind \
             folder (`{}`), and a skill may use `<name>/SKILL.md`. A kind folder sits at the \
             root or inside whatever the folder above it holds — \
             `marketplaces/<name>/plugins/<name>/skills/<name>.md` is the full shape",
            dirs.join("`, `")
        ),
    ))
}

/// The folder a plugin puts each kind in.
///
/// This map IS the structural rule. A plugin's own layout says what each of
/// its folders holds, so nothing inside has to declare a kind — and a reader
/// that knows the map can name a component the author never annotated.
///
/// Order is the order a tree lists them, which is the order
/// [`EntityKind::KNOWN`] does not use: `KNOWN` sorts alphabetically for
/// refusal messages, this is the layout.
pub const KIND_DIRS: &[(&str, EntityKind)] = &[
    ("marketplaces", EntityKind::Marketplace),
    ("plugins", EntityKind::Plugin),
    ("skills", EntityKind::Skill),
    ("commands", EntityKind::Command),
    ("agents", EntityKind::Agent),
    ("hooks", EntityKind::Hook),
];

/// What the folders say about one note, without scanning the whole vault.
///
/// Every field is `None` where structure said nothing, and none of them is a
/// default. A caller resolves the kind through
/// [`crate::frontmatter::resolve_kind`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderPlacement {
    /// The kind folder above the note. `None` means none is, so the default
    /// decides. Never a claim that it is a skill.
    pub kind: Option<EntityKind>,
    /// The marketplace whose tree this sits in. `None` for a component
    /// promoted above every marketplace, and for a marketplace's own note.
    pub marketplace: Option<String>,
    /// The plugin directory holding it.
    pub plugin: Option<String>,
}

/// What structure says one note is, without scanning the whole vault.
///
/// The same rule [`discover_components`] applies, for a caller holding a
/// single path — [`crate::publish`] is one, because a publish is handed one
/// note and must not guess `skill` for a note the folders already named
/// something else; a workbench reading one note for a detail view is another,
/// and it needs the two levels above as well or its detail screen contradicts
/// its own tree.
///
/// It *is* that rule, not a second reading of it: the kind folder above a note
/// names the note, so `classify` answers this from the path alone.
pub fn folder_placement(content_root: &Path, entry: &Path) -> FolderPlacement {
    folder_placement_with(&crate::patterns::Patterns::default(), content_root, entry)
}

/// [`folder_placement`], reading the tree through the root file's patterns.
pub fn folder_placement_with(
    patterns: &crate::patterns::Patterns,
    content_root: &Path,
    entry: &Path,
) -> FolderPlacement {
    let path = conventional(content_root, entry, patterns);
    let Ok(rest) = path.strip_prefix(content_root) else {
        return FolderPlacement::default();
    };
    let parts: Vec<String> = rest
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    match classify(content_root, &parts) {
        None => FolderPlacement::default(),
        Some(placement) => FolderPlacement {
            kind: placement.kind,
            marketplace: placement.marketplace,
            plugin: placement.plugin,
        },
    }
}

/// Just the kind, for the callers that rank only that.
///
/// Expressed through [`folder_placement`] rather than beside it: two functions
/// reading the same structure are two chances to read it differently.
pub fn folder_kind(content_root: &Path, entry: &Path) -> Option<EntityKind> {
    folder_placement(content_root, entry).kind
}

/// Does this path pass through a kind folder at all?
///
/// The scan's standing to complain: inside one, a declared note at an
/// unrecognised path is a mistake worth naming; outside one the vault is
/// simply the author's.
pub(crate) fn under_a_kind_folder(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok_and(|rest| {
        rest.iter()
            .any(|part| kind_dir(&part.to_string_lossy()).is_some())
    })
}

/// Whether `path` sits in the folder of a skill whose entry note is placed.
fn in_a_skill_folder(path: &Path, placed: &BTreeMap<PathBuf, Placement>, root: &Path) -> bool {
    placed.iter().any(|(entry, placement)| {
        let bundled = entry
            .strip_prefix(root)
            .is_ok_and(|rel| is_bundle_entry(&forward_slashed(rel)).unwrap_or(false));
        placement.kind == Some(EntityKind::Skill)
            && bundled
            && entry
                .parent()
                .is_some_and(|folder| path.starts_with(folder))
    })
}

/// The marketplace folder of the old note form `marketplaces/<m>/<m>.md`.
fn old_marketplace_note(rel: &str) -> Option<&str> {
    let (market, file) = rel.strip_prefix("marketplaces/")?.split_once('/')?;
    (md_stem(file) == Some(market)).then_some(market)
}

/// Whether the note at `path` has a frontmatter block holding any key.
fn has_frontmatter_keys(vfs: &dyn Vfs, path: &Path) -> bool {
    read_utf8(vfs, path).is_ok_and(|source| {
        let (frontmatter, _body) = split_frontmatter(&source);
        frontmatter_map(frontmatter, path).is_ok_and(|fields| !fields.is_empty())
    })
}

/// Whether the note at `path` is Markdown and declares a `description`.
fn declares_description(vfs: &dyn Vfs, path: &Path, ns: &Namespace) -> Result<bool, CoreError> {
    let markdown = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"));
    if !markdown {
        return Ok(false);
    }
    let source = read_utf8(vfs, path)?;
    let (frontmatter, _body) = split_frontmatter(&source);
    let mut fields = frontmatter_map(frontmatter, path)?;
    // Drained for its refusals: a removed key on a stray note is still worth
    // naming, and it names itself better than the shape does.
    VaultFrontmatter::take_from(&mut fields, path, ns, EntityKind::Skill)?;
    Ok(trimmed_string(&fields, "description").is_some())
}

/// The kind a folder name declares, or `None` for any other folder.
pub(crate) fn kind_dir(name: &str) -> Option<EntityKind> {
    KIND_DIRS
        .iter()
        .find(|(dir, _)| *dir == name)
        .map(|(_, kind)| *kind)
}

/// Where structure puts a note, before any frontmatter is read.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Placement {
    name: String,
    /// What structure says this is, or `None` where structure says nothing.
    kind: Option<EntityKind>,
    /// The marketplace this sits in. `None` for a marketplace itself, and for
    /// a component whose kind folders start at the content root.
    marketplace: Option<String>,
    plugin: Option<String>,
    /// The directory this component owns, or `None` for a lone file.
    dir: Option<PathBuf>,
    /// Whether a container can sit here: at the vault root, or a plugin at a
    /// marketplace's root. A note deeper down only has a container's shape.
    container_level: bool,
}

/// Reads the paths as components, using structure alone.
///
/// The shapes are the module docs' table; everything else is a resource
/// belonging to whichever component directory contains it. The plugin's own
/// note is the `<name>/<name>.md` shape; what separates it from a bare
/// component is only whether a kind folder sits beside it.
///
/// # The rule cannot name a bare component's kind
///
/// Nothing sits above `<name>.md` at the root to say what it is, so
/// [`Placement::kind`] is `None` and the default decides. A directory at the root holding files but no kind
/// folder has the same problem, and it is the rule's weak point: a plugin
/// with no components yet is shaped exactly like a bare component that
/// carries resources.
fn place(
    root: &Path,
    files: &[PathBuf],
    patterns: &crate::patterns::Patterns,
) -> BTreeMap<PathBuf, Placement> {
    let mut placed = BTreeMap::new();
    for path in files {
        let conventional = conventional(root, path, patterns);
        let Ok(rest) = conventional.strip_prefix(root) else {
            continue;
        };
        let parts: Vec<String> = rest
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if let Some(mut placement) = classify(root, &parts) {
            // The folder it owns is the real one, whatever a pattern calls it.
            if placement.dir.is_some() {
                placement.dir = path.parent().map(Path::to_path_buf);
            }
            placed.insert(path.clone(), placement);
        }
    }
    placed
}

/// `path` as the conventional path it plays the part of under `root`.
fn conventional(root: &Path, path: &Path, patterns: &crate::patterns::Patterns) -> PathBuf {
    path.strip_prefix(root).map_or_else(
        |_| path.to_path_buf(),
        |rest| root.join(patterns.canonical(&forward_slashed(rest))),
    )
}

/// One path, read as the alternation the tree is.
///
/// Every level is spelled the same way — a kind folder naming what it holds,
/// then the thing it holds — so this is a loop rather than one arm per depth,
/// and a level added to [`KIND_DIRS`] needs no code here at all:
///
/// ```text
/// marketplaces/acme/plugins/core/skills/review/review.md
///  \_________/ \__/ \_____/ \__/ \____/ \____/ \_______/
///     kind      name   kind  name  kind   name    entry
/// ```
///
/// A kind folder names its contents, so a container's kind is *read* rather
/// than inferred from what it holds — which is what makes promotion work.
/// `skills/` at the root, inside a marketplace, or inside a plugin is one
/// folder meaning one thing; only how widely the skill is shared changes.
///
/// Two terminal shapes, the two every level already used:
///
/// * `<kind>/<name>.md` — a component that travels alone.
/// * `<kind>/<name>/<name>.md` — a component that owns its directory, and so
///   carries what is inside it. For a plugin, what is inside it is the next
///   level down. A skill's entry may also be `<name>/SKILL.md`, named by its
///   folder.
/// * `marketplaces/<name>/skillwright.md` — a marketplace, with its plugins
///   beneath it. This is the only form a marketplace has.
///
/// `None` for anything else, which is most of a vault.
fn classify(root: &Path, parts: &[String]) -> Option<Placement> {
    // The containers passed through on the way in, so a component can name the
    // plugin and the marketplace it belongs to however deep it sits.
    let mut chain: Vec<(EntityKind, String)> = Vec::new();
    let mut dir = root.to_path_buf();
    let mut i = 0;

    while i + 1 < parts.len() {
        let kind = kind_dir(&parts[i])?;
        let name = &parts[i + 1];

        // `<kind>/<name>.md` — the entry travels alone. A marketplace never
        // does: its note is the root file in its folder.
        if i + 2 == parts.len() {
            if kind == EntityKind::Marketplace {
                return None;
            }
            return Some(placement(md_stem(name)?.to_owned(), kind, &chain, None));
        }
        // `<kind>/<name>/<name>.md` — the entry owns the directory it is in.
        if i + 3 == parts.len() && is_entry(&parts[i + 2], name, kind) {
            let owned = dir.join(&parts[i]).join(name);
            return Some(placement(name.clone(), kind, &chain, Some(owned)));
        }

        // Neither: descend through this container into the next kind folder.
        chain.push((kind, name.clone()));
        dir = dir.join(&parts[i]).join(name);
        i += 2;
    }
    None
}

/// The entry file a skill may use in place of `<name>.md`, named by its folder.
pub const SKILL_ENTRY: &str = "SKILL.md";

/// Is `file` the entry note of the folder `name`, for a component of `kind`?
pub(crate) fn is_entry(file: &str, name: &str, kind: EntityKind) -> bool {
    if kind == EntityKind::Marketplace {
        return file == crate::settings::ROOT_FILE;
    }
    md_stem(file) == Some(name) || (kind == EntityKind::Skill && is_skill_entry(file))
}

/// `SKILL.md`, case-insensitively, as `md_stem` reads every other note.
pub fn is_skill_entry(file: &str) -> bool {
    file.eq_ignore_ascii_case(SKILL_ENTRY)
}

/// Whether the content-relative, `/`-separated `rel` is the entry note of a
/// component that owns its directory, by the same shape rule the scan reads
/// (`classify`). A note directly inside a kind folder travels alone, whatever
/// it is named. `None` for a path the rule gives no shape at all.
pub(crate) fn is_bundle_entry(rel: &str) -> Option<bool> {
    let parts: Vec<String> = rel.split('/').map(str::to_owned).collect();
    classify(Path::new(""), &parts).map(|placement| placement.dir.is_some())
}

/// The stem of a `.md` filename, case-insensitively. `README.MD` is a note
/// like any other.
pub(crate) fn md_stem(name: &str) -> Option<&str> {
    name.strip_suffix(".md").or_else(|| {
        name.rfind('.')
            .filter(|i| name[i + 1..].eq_ignore_ascii_case("md"))
            .map(|i| &name[..i])
    })
}

/// A classified entry, with its ancestry read off the chain it sits under.
fn placement(
    name: String,
    kind: EntityKind,
    chain: &[(EntityKind, String)],
    dir: Option<PathBuf>,
) -> Placement {
    let nearest = |want: EntityKind| {
        chain
            .iter()
            .rev()
            .find(|(k, _)| *k == want)
            .map(|(_, n)| n.clone())
    };
    Placement {
        name,
        kind: Some(kind),
        // The NEAREST enclosing one of each: a skill promoted to its
        // marketplace names that marketplace and no plugin; one inside a
        // plugin names both.
        marketplace: nearest(EntityKind::Marketplace),
        plugin: nearest(EntityKind::Plugin),
        dir,
        container_level: match chain {
            [] => true,
            [(EntityKind::Marketplace, _)] => kind == EntityKind::Plugin,
            _ => false,
        },
    }
}

/// Files each component owns, deepest directory wins.
///
/// A path belongs to the innermost component directory containing it, and no
/// entry note is ever a resource. Without the depth rule a plugin claims every
/// file beneath it, including the notes and resources of the components it
/// holds.
fn owned_resources(
    files: &[PathBuf],
    placed: &BTreeMap<PathBuf, Placement>,
    dir: &Path,
) -> Vec<String> {
    let mut deeper: Vec<&Path> = placed
        .values()
        .filter_map(|p| p.dir.as_deref())
        .filter(|d| *d != dir && d.starts_with(dir))
        .collect();
    deeper.sort_by_key(|d| std::cmp::Reverse(d.as_os_str().len()));

    let mut out: Vec<String> = files
        .iter()
        .filter(|path| path.starts_with(dir))
        .filter(|path| !placed.contains_key(*path))
        .filter(|path| !deeper.iter().any(|d| path.starts_with(d)))
        .filter_map(|path| path.strip_prefix(dir).ok().map(forward_slashed))
        .collect();
    out.sort();
    out
}

/// Every file in the source, in walk order, without the vault's excluded
/// files (Obsidian's "Excluded files", see [`crate::walk::excluded_files`]),
/// so an excluded file is neither placed nor published.
fn source_files(vfs: &dyn Vfs, vault_root: &Path, source: &Source) -> Vec<PathBuf> {
    let excluded = crate::walk::excluded_files(vfs, vault_root);
    let patterns = crate::patterns::Patterns::read(vfs, vault_root);
    source
        .files
        .iter()
        // A path outside the vault (a content root elsewhere) has no
        // vault-relative spelling for the list to match, so it stays.
        .filter(|path| {
            path.strip_prefix(vault_root).map_or(true, |relative| {
                let relative = forward_slashed(relative);
                !excluded(&relative) && !patterns.excluded(&relative)
            })
        })
        .cloned()
        .collect()
}

/// Reads one note at a recognised shape and decides: a component (`Some`),
/// not a candidate (`None`), or wrong enough to stop the scan (`Err`).
fn read_source(
    vfs: &dyn Vfs,
    vault_root: &Path,
    entry_path: &Path,
    placement: &Placement,
    placed: &BTreeMap<PathBuf, Placement>,
    files: &[PathBuf],
    ns: &Namespace,
) -> Result<Option<DiscoveredComponent>, CoreError> {
    let source = read_utf8(vfs, entry_path)?;
    let (frontmatter, body) = split_frontmatter(&source);
    let mut fields = frontmatter_map(frontmatter, entry_path)?;
    // The folder decides the kind, through the one ranking `resolve_kind`
    // holds.
    let resolved = resolve_kind(KindHints {
        requested: None,
        folder: placement.kind,
    });
    // A marketplace's note is a root file, whose keys are always bare, and
    // whose `build:` and `workspace:` are not fields.
    let bare = Namespace::bare();
    let ns = if resolved.kind == EntityKind::Marketplace {
        fields.remove("build");
        fields.remove("workspace");
        &bare
    } else {
        ns
    };
    let ours = VaultFrontmatter::take_from(&mut fields, entry_path, ns, resolved.kind)?;
    let unknown_keys = unknown_keys(&ours, resolved.kind);

    let name = trimmed_string(&fields, "name");
    let description = trimmed_string(&fields, "description");
    let (name, description, description_filled) = match (name, description) {
        // `description` is the opt-in, and `name` follows the filename when
        // the note does not declare one — the same answer
        // `read_vault_skill` gives a single-note publish, from the same
        // function.
        (name, Some(description)) => (
            name.unwrap_or_else(|| default_name(entry_path)),
            description,
            false,
        ),
        // A container's own note is optional and holds whichever of its keys
        // it needs: the folder names it, and it need not describe itself.
        (None, None)
            if resolved.kind.holds_components()
                && placement.dir.is_some()
                && placement.container_level =>
        {
            (default_name(entry_path), String::new(), true)
        }
        // Neither: an ordinary note sharing the tree. The common case, and
        // the silence that lets bundle resources live beside their entry.
        (None, None) => return Ok(None),
        // A `name` alone still opts in. The caller reports the filler.
        (Some(name), None) => (name, MISSING_DESCRIPTION.to_owned(), true),
    };

    validate_name(&name, "component name", entry_path)?;
    // A plugin is known by its folder in every manifest and every call, so a
    // `name` that differs could not take effect.
    if resolved.kind == EntityKind::Plugin
        && placement.container_level
        && trimmed_string(&fields, "name").is_some_and(|declared| declared != placement.name)
    {
        return Err(CoreError::malformed(
            entry_path,
            format!(
                "`name: {name}` differs from its folder `{}`. A plugin publishes under its \
                 folder: remove `name`, or rename the folder",
                placement.name
            ),
        ));
    }
    if let Some(refusal) = description_refusal(&description) {
        return Err(CoreError::malformed(entry_path, refusal));
    }

    // The Agent Skills spec's other hard cap, refused in the same shape and in the same
    // place: at the read, where the author can act on it, rather than in the
    // renderer, whose only response would be to emit something wrong.
    if let Some(compatibility) = trimmed_string(&fields, "compatibility")
        && let Some(refusal) = compatibility_refusal(&compatibility)
    {
        return Err(CoreError::malformed(entry_path, refusal));
    }

    // The shape already decided what this note owns; `place` refused every
    // other arrangement before the note was opened.
    let bundle_root = placement.dir.clone();
    let resources = bundle_root
        .as_deref()
        .map(|dir| owned_resources(files, placed, dir))
        .unwrap_or_default();

    // The folder puts a component in its plugin. The plugin becomes a
    // marketplace directory name, so a bad one fails here, on the note,
    // rather than at the far end of a publish.
    let plugin = placement.plugin.clone();
    if let Some(plugin) = plugin.as_deref() {
        validate_name(plugin, "plugin name", entry_path)?;
    }

    if let Some(declared) = ours.targets.as_deref() {
        let unknown: Vec<&str> = declared
            .iter()
            .map(String::as_str)
            .filter(|name| !KNOWN_TARGETS.contains(name))
            .collect();
        if !unknown.is_empty() {
            return Err(CoreError::refused(
                "unknown-target",
                format!(
                    "{}: `{}` names {}, which this build does not know (known: {})",
                    entry_path.display(),
                    ns.key("targets"),
                    quoted(&unknown),
                    KNOWN_TARGETS.join(", ")
                ),
            ));
        }
    }

    let vault_relative_path = entry_path
        .strip_prefix(vault_root)
        .map(forward_slashed)
        .map_err(|_| CoreError::PathEscapesRoot(entry_path.to_path_buf()))?;

    Ok(Some(DiscoveredComponent {
        name,
        description,
        description_filled,
        kind: resolved.kind,
        kind_source: resolved.source,
        folder_kind: placement.kind,
        targets: ours.targets,
        plugin,
        marketplace: placement.marketplace.clone(),
        entry_path: entry_path.to_path_buf(),
        bundle_root,
        resources,
        vault_relative_path,
        identity: ours.identity,
        hook: (resolved.kind == EntityKind::Hook).then(|| read_hook(&fields, body)),
        unknown_keys,
        blocks: ours.overrides,
    }))
}

/// The notes of the containers `entry` sits in, outermost first, as the paths
/// they would have: `plugins/p/skills/s.md` passes through `plugins/p/p.md`. A
/// container's own note is listed as its own container.
pub(crate) fn container_notes(root: &Path, entry: &Path) -> Vec<PathBuf> {
    let Ok(rest) = entry.strip_prefix(root) else {
        return Vec::new();
    };
    let parts: Vec<String> = rest
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let mut notes = Vec::new();
    let mut dir = root.to_path_buf();
    for level in parts.windows(2).step_by(2) {
        let [folder, name] = level else { continue };
        let Some(kind) = kind_dir(folder) else {
            break;
        };
        dir = dir.join(folder).join(name);
        if kind == EntityKind::Marketplace {
            notes.push(dir.join(crate::settings::ROOT_FILE));
        } else if kind.holds_components() {
            notes.push(dir.join(format!("{name}.md")));
        }
    }
    notes
}

/// [`container_notes`] through the root file's patterns: the path of a plugin
/// note is the plugin's real folder, whatever a pattern calls it.
pub(crate) fn container_notes_with(
    patterns: &crate::patterns::Patterns,
    root: &Path,
    entry: &Path,
) -> Vec<PathBuf> {
    let Ok(rest) = entry.strip_prefix(root) else {
        return Vec::new();
    };
    let relative = forward_slashed(rest);
    let folder = patterns.plugin_folder(&relative);
    container_notes(root, &conventional(root, entry, patterns))
        .into_iter()
        .map(|note| {
            let in_plugins = note
                .parent()
                .and_then(Path::parent)
                .is_some_and(|above| above.ends_with("plugins"));
            match (folder, note.file_name()) {
                (Some(folder), Some(name)) if in_plugins => root.join(folder).join(name),
                _ => note,
            }
        })
        .collect()
}

/// Reads one container's note the way a scan reads it. `None` when it does not
/// exist: a container's note is optional.
pub(crate) fn read_container(
    vfs: &dyn Vfs,
    root: &Path,
    entry: &Path,
    ns: &Namespace,
) -> Result<Option<DiscoveredComponent>, CoreError> {
    if !vfs.exists(entry) {
        return Ok(None);
    }
    let patterns = crate::patterns::Patterns::read(vfs, root);
    let conventional = conventional(root, entry, &patterns);
    let Ok(rest) = conventional.strip_prefix(root) else {
        return Ok(None);
    };
    let parts: Vec<String> = rest
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let Some(mut placement) = classify(root, &parts) else {
        return Ok(None);
    };
    if placement.dir.is_some() {
        placement.dir = entry.parent().map(Path::to_path_buf);
    }
    read_source(vfs, root, entry, &placement, &BTreeMap::new(), &[], ns)
}

/// One hook note's routing and payload off its own frontmatter and body.
fn read_hook(fields: &Map<String, Value>, body: &str) -> DiscoveredHook {
    let hook_type = trimmed_string(fields, "type").unwrap_or_else(|| DEFAULT_HOOK_TYPE.to_owned());
    // The body is the payload for the two types that have one to write, and
    // its first line is as much of a script as a row can show.
    let from_body = || {
        body.lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_owned)
    };
    let payload = match hook_type.as_str() {
        "command" => trimmed_string(fields, "command").or_else(from_body),
        "prompt" => trimmed_string(fields, "prompt").or_else(from_body),
        "http" => trimmed_string(fields, "url"),
        _ => None,
    };
    DiscoveredHook {
        event: trimmed_string(fields, "event"),
        matcher: trimmed_string(fields, "matcher"),
        hook_type,
        payload,
    }
}

/// What a hook note's absent `type` means, matching the projection's default.
const DEFAULT_HOOK_TYPE: &str = "command";

/// A field as discovery reads it: a non-empty string, trimmed. Any other
/// shape is absence, consistent with
/// [`crate::vault`]'s rule that a wrong-shaped modeled key falls back rather
/// than being consumed.
fn trimmed_string(fields: &Map<String, Value>, key: &str) -> Option<String> {
    fields
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Maximum length of a component or plugin `name`, per the Agent Skills
/// spec. Matches `agent_adapt::naming::SKILL_NAME_MAX_LEN`; the two must
/// agree, since a name this scan admits is one `slugify_skill_name` must
/// also be able to produce.
pub const NAME_MAX_LEN: usize = 64;

/// Maximum length of a `description`, per the Agent Skills spec.
pub const DESCRIPTION_MAX_LEN: usize = 1024;

/// Maximum length of a `compatibility` note, per the Agent Skills spec.
pub const COMPATIBILITY_MAX_LEN: usize = 500;

/// Why `value` is not a `what` the Agent Skills spec admits — the name rule
/// `^[a-z0-9]+(-[a-z0-9]+)*$`, at most [`NAME_MAX_LEN`] characters, spelled
/// as segment checks rather than a regex dependency — or `None` when it is.
///
/// Separate from `validate_name` and public, because a caller composing a
/// note that does not exist yet has no path to name in a [`CoreError`]: it
/// wants the prose to put beside a field. The scan wants the error. Both
/// read the rule from here, so the two cannot drift.
pub fn name_refusal(value: &str, what: &str) -> Option<String> {
    let legal = !value.is_empty()
        && value.len() <= NAME_MAX_LEN
        && value.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        });
    (!legal).then(|| {
        format!(
            "`{value}` is not a {what} the Agent Skills spec admits: 1–{NAME_MAX_LEN} characters of \
             `a-z`, `0-9`, and `-`, with no leading, trailing, or doubled hyphen"
        )
    })
}

/// Why `value` is not a `description` the Agent Skills spec admits, or `None`.
/// Characters, not bytes — the Agent Skills spec's cap counts characters.
pub fn description_refusal(value: &str) -> Option<String> {
    let described = value.chars().count();
    (described > DESCRIPTION_MAX_LEN).then(|| {
        format!("the description is {described} characters; the Agent Skills spec caps it at {DESCRIPTION_MAX_LEN}")
    })
}

/// Why `value` is not a `compatibility` note the Agent Skills spec admits, or
/// `None`. Counted like [`description_refusal`]'s.
pub fn compatibility_refusal(value: &str) -> Option<String> {
    let stated = value.chars().count();
    (stated > COMPATIBILITY_MAX_LEN).then(|| {
        format!("the compatibility is {stated} characters; the Agent Skills spec caps it at {COMPATIBILITY_MAX_LEN}")
    })
}

/// [`name_refusal`] as the scan's own error, against the note carrying the
/// name.
///
/// `pub(crate)` for [`crate::adopt`]: what adoption writes must satisfy this
/// scan, so it validates with the same rule rather than a copy that drifts.
pub(crate) fn validate_name(value: &str, what: &str, path: &Path) -> Result<(), CoreError> {
    match name_refusal(value, what) {
        Some(message) => Err(CoreError::malformed(path, message)),
        None => Ok(()),
    }
}

/// The name a note publishes under when its frontmatter declares none: the
/// filename, slugified — or for a `SKILL.md`, its folder's name.
///
/// One derivation for both readers. [`crate::vault::read_vault_skill`] calls
/// it for a single-note publish and `read_source` calls it for a scan, so
/// the two cannot answer differently about the same file. Slugified rather
/// than raw, because `validate_name` runs on the result: a stem with a
/// capital or a space would otherwise default straight into a refusal.
pub fn default_name(path: &Path) -> String {
    // A `SKILL.md` entry and a root file are named by the folder they own.
    let named = match path.file_name().and_then(|f| f.to_str()) {
        Some(file) if is_skill_entry(file) || file == crate::settings::ROOT_FILE => {
            path.parent().and_then(|p| p.file_name())
        }
        _ => path.file_stem(),
    };
    slugify(named.and_then(|s| s.to_str()).unwrap_or_default())
}

/// The slug, ASCII half: lowercase, every run of anything else one
/// `-`, trimmed. `Workflow Checklist` → `workflow-checklist`. See the module
/// doc for the deliberate NFKD divergence.
///
/// Public because deriving a name is not only adoption's problem. Any caller
/// that *authors* a note — [`crate::adopt`], an editor composing a new one —
/// has to produce the name and the filename this scan will later check
/// against each other, and the only way that check cannot drift is for one
/// function to answer it for everybody.
pub fn slugify(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut pending_separator = false;
    for c in value.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if pending_separator && !out.is_empty() {
                out.push('-');
            }
            pending_separator = false;
            out.push(c);
        } else {
            pending_separator = true;
        }
    }
    out
}

fn quoted(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::MemVfs;
    // Only the tests that need a real filesystem (unix: symlinks) use it.
    #[cfg(unix)]
    use crate::vfs::NativeVfs;

    fn sw() -> Namespace {
        Namespace::sw()
    }

    /// An empty vault holding one marketplace directory and nothing else.
    fn vault() -> MemVfs {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/vault/marketplaces/house/plugins"))
            .unwrap();
        vfs
    }

    fn write(vfs: &dyn Vfs, rel: &str, content: &str) {
        let path = Path::new("/vault").join(rel);
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        vfs.write_file(&path, content.as_bytes()).unwrap();
    }

    /// Source frontmatter using the `targets` key. An empty
    /// target list here means "omit the key".
    fn source(name: &str, targets: &[&str]) -> String {
        let mut out =
            format!("---\nname: {name}\ndescription: Use this skill to test {name} exports.\n");
        if !targets.is_empty() {
            out.push_str("sw-targets:\n");
            for target in targets {
                out.push_str(&format!("  - {target}\n"));
            }
        }
        out.push_str("---\n\n# Body\n");
        out
    }

    fn scan(vfs: &dyn Vfs) -> Result<DiscoveryReport, CoreError> {
        discover_components(vfs, Path::new("/vault"), None, &sw())
    }

    /// The per-note assertions' shorthand: a scan that skipped anything reads
    /// as a failure here, carrying the skipped note's own sentence, so a case
    /// asserting a note is refused says so at the grain it cares about. The
    /// isolation itself is asserted through [`scan`].
    fn discover(vfs: &dyn Vfs) -> Result<Vec<DiscoveredComponent>, CoreError> {
        let report = scan(vfs)?;
        match report.skipped.first() {
            Some(skipped) => Err(CoreError::Refused(format!(
                "{}: {}",
                skipped.reason, skipped.detail
            ))),
            None => Ok(report.components),
        }
    }

    /// A hook's row has to say what fires it and what it runs; the file name
    /// says neither.
    #[test]
    fn a_hook_note_carries_its_event_matcher_and_command() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/review/hooks/lint-on-write.md",
            "---\nname: lint-on-write\ndescription: Lints what was just written.\nevent: PostToolUse\nmatcher: Edit|Write\ncommand: just lint\n---\n",
        );

        let found = discover(&vfs).unwrap();

        let hook = found[0].hook.as_ref().unwrap();
        assert_eq!(hook.event.as_deref(), Some("PostToolUse"));
        assert_eq!(hook.matcher.as_deref(), Some("Edit|Write"));
        assert_eq!(hook.hook_type, "command");
        assert_eq!(hook.payload.as_deref(), Some("just lint"));
    }

    /// The body is where a `command` hook may write its script instead, and
    /// its first line is as much of one as a row can show.
    #[test]
    fn a_hook_with_no_command_key_reads_its_payload_off_the_body() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/review/hooks/format.md",
            "---\nname: format\ndescription: Formats the file.\nevent: PostToolUse\n---\n\nbun run format\n",
        );

        let hook = discover(&vfs).unwrap()[0].hook.clone().unwrap();

        assert_eq!(hook.payload.as_deref(), Some("bun run format"));
    }

    /// The projection refuses an event-less hook; discovery still has to
    /// report it, because a tree that dropped it would hide the mistake.
    #[test]
    fn a_hook_missing_its_event_is_still_discovered() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/review/hooks/nothing.md",
            "---\nname: nothing\ndescription: Fires on nothing.\ncommand: just lint\n---\n",
        );

        let hook = discover(&vfs).unwrap()[0].hook.clone().unwrap();

        assert!(hook.event.is_none());
        assert_eq!(hook.payload.as_deref(), Some("just lint"));
    }

    /// Only hooks carry the summary — every other kind reads as it did.
    #[test]
    fn a_skill_carries_no_hook_summary() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/tools/skills/review.md",
            &source("review", &[]),
        );

        assert!(discover(&vfs).unwrap()[0].hook.is_none());
    }

    /// The committed fixture vault (`tests/fixtures/agent-skills-vault`).
    #[test]
    fn the_committed_fixture_discovers_as_one_bundled_skill() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/tools/skills/workflow-checklist/workflow-checklist.md",
            "---\nname: workflow-checklist\ndescription: A fixture skill for testing bundle export.\nsw-targets:\n  - codex\n  - claude\n---\n\n# Workflow Checklist\n\n![[references/steps]]\n",
        );
        write(
            &vfs,
            "marketplaces/house/plugins/tools/skills/workflow-checklist/references/steps.md",
            "# Steps\n\n1. Understand the requested outcome.\n2. Validate the result.\n",
        );

        let found = discover(&vfs).unwrap();
        assert_eq!(found.len(), 1, "the resource note must not publish");
        let skill = &found[0];
        assert_eq!(skill.name, "workflow-checklist");
        assert_eq!(
            skill.description,
            "A fixture skill for testing bundle export."
        );
        assert_eq!(
            skill.targets,
            Some(vec!["codex".to_owned(), "claude".to_owned()])
        );
        // Inside `skills/`, so the folder names the kind and the note says
        // nothing; inside `plugins/tools/`, so the folder names the plugin.
        assert_eq!(skill.kind, EntityKind::Skill);
        assert_eq!(skill.kind_source, KindSource::Directory);
        assert_eq!(skill.folder_kind, Some(EntityKind::Skill));
        assert_eq!(skill.plugin.as_deref(), Some("tools"));
        assert_eq!(skill.marketplace.as_deref(), Some("house"));
        assert_eq!(skill.resources, vec!["references/steps.md".to_owned()]);
        assert_eq!(
            skill.entry_path,
            PathBuf::from(
                "/vault/marketplaces/house/plugins/tools/skills/workflow-checklist/workflow-checklist.md"
            )
        );
        assert_eq!(
            skill.bundle_root,
            Some(PathBuf::from(
                "/vault/marketplaces/house/plugins/tools/skills/workflow-checklist"
            ))
        );
        assert_eq!(
            skill.vault_relative_path,
            "marketplaces/house/plugins/tools/skills/workflow-checklist/workflow-checklist.md"
        );
    }

    #[test]
    fn a_note_the_vault_excludes_is_not_discovered() {
        // Obsidian's "Excluded files": a folder prefix and a regex. Neither
        // note is placed, so neither is published; the list hides nothing
        // else.
        let vfs = vault();
        write(
            &vfs,
            ".obsidian/app.json",
            r#"{"userIgnoreFilters": ["marketplaces/house/skills/drafts/", "/wip\\.md$/"]}"#,
        );
        write(
            &vfs,
            "marketplaces/house/skills/kept.md",
            &source("kept", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/skills/drafts/draft.md",
            &source("draft", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/skills/wip.md",
            &source("wip", &[]),
        );

        let names: Vec<String> = discover(&vfs)
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, vec!["kept"]);
    }

    /// The five shapes, in one vault, read as one roster.
    /// Promotion: the same `skills/` folder at three levels, and the level it
    /// sits at is how widely the skill is shared. Peers are left alone.
    #[test]
    fn a_kind_folder_means_the_same_thing_at_every_level_it_sits_at() {
        let vfs = vault();
        // Shared by everything: outside any marketplace.
        write(&vfs, "skills/house-style.md", &source("house-style", &[]));
        // Shared by one marketplace's plugins.
        write(
            &vfs,
            "marketplaces/acme/skills/tone.md",
            &source("tone", &[]),
        );
        // Used by one plugin.
        write(
            &vfs,
            "marketplaces/acme/plugins/core/skills/review.md",
            &source("review", &[]),
        );
        // A peer directory: no kind folder anywhere in its path, so the notes
        // in it are the author's own and are neither scanned nor refused.
        write(&vfs, "references/notes.md", &source("notes", &[]));

        let found = discover(&vfs).unwrap();
        let rows: Vec<(&str, Option<&str>, Option<&str>)> = found
            .iter()
            .map(|c| {
                (
                    c.name.as_str(),
                    c.marketplace.as_deref(),
                    c.plugin.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("house-style", None, None),
                ("review", Some("acme"), Some("core")),
                ("tone", Some("acme"), None),
            ],
            "one folder, three scopes — and the peer note is not a component"
        );
        assert!(found.iter().all(|c| c.kind == EntityKind::Skill));
    }

    #[test]
    fn structure_names_the_kind_and_the_plugin() {
        let vfs = vault();
        // A plugin: a root directory holding a kind folder.
        write(
            &vfs,
            "marketplaces/house/plugins/core/core.md",
            &source("core", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/plugins/core/README.md",
            "Notes for maintainers.\n",
        );
        // Its skill, carrying a file.
        write(
            &vfs,
            "marketplaces/house/plugins/core/skills/code-review/code-review.md",
            &source("code-review", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/plugins/core/skills/code-review/check.py",
            "#!/x\n",
        );
        // Its skill, travelling alone.
        write(
            &vfs,
            "marketplaces/house/plugins/core/skills/release-notes.md",
            &source("release-notes", &["claude"]),
        );
        // A kind this build cannot publish, named by its folder alone.
        write(
            &vfs,
            "marketplaces/house/plugins/core/commands/review.md",
            &source("review", &[]),
        );
        // A skill PROMOTED to the marketplace: shared by every plugin under
        // it, and belonging to none of them. The same `skills/` folder,
        // one level up.
        write(
            &vfs,
            "marketplaces/house/skills/spike-notes.md",
            &source("spike-notes", &[]),
        );

        let found = discover(&vfs).unwrap();
        let rows: Vec<(&str, EntityKind, Option<&str>, KindSource)> = found
            .iter()
            .map(|c| (c.name.as_str(), c.kind, c.plugin.as_deref(), c.kind_source))
            .collect();
        assert_eq!(
            rows,
            vec![
                (
                    "code-review",
                    EntityKind::Skill,
                    Some("core"),
                    KindSource::Directory
                ),
                ("core", EntityKind::Plugin, None, KindSource::Directory),
                (
                    "release-notes",
                    EntityKind::Skill,
                    Some("core"),
                    KindSource::Directory
                ),
                (
                    "review",
                    EntityKind::Command,
                    Some("core"),
                    KindSource::Directory
                ),
                // Promoted: named by `skills/` like the rest, in no plugin.
                (
                    "spike-notes",
                    EntityKind::Skill,
                    None,
                    KindSource::Directory
                ),
            ]
        );
        assert!(
            found
                .iter()
                .all(|c| c.marketplace.as_deref() == Some("house")),
            "every one of them is in the same marketplace, at three different levels"
        );

        // Ownership is deepest-directory-wins: the plugin keeps its own file
        // and claims nothing belonging to the components beneath it.
        let by = |name: &str| found.iter().find(|c| c.name == name).unwrap();
        assert_eq!(by("core").resources, vec!["README.md".to_owned()]);
        assert_eq!(by("code-review").resources, vec!["check.py".to_owned()]);
        assert!(by("release-notes").resources.is_empty());
    }
    /// The kind folder above a note names it, at whatever level it sits.
    #[test]
    fn the_kind_folder_above_a_note_names_it() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/onboarding/onboarding.md",
            &source("onboarding", &[]),
        );
        let found = discover(&vfs).unwrap();
        assert_eq!(
            found[0].kind,
            EntityKind::Plugin,
            "`plugins/` names what it holds"
        );
        assert_eq!(found[0].folder_kind, Some(EntityKind::Plugin));
        assert_eq!(found[0].kind_source, KindSource::Directory);
        assert_eq!(found[0].marketplace.as_deref(), Some("house"));
    }

    /// `hooks/` names what it holds, and the note says which layer answered.
    #[test]
    fn a_hooks_folder_names_a_hook_and_the_note_need_not() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/core/hooks/lint-on-write.md",
            "---\nname: lint-on-write\ndescription: d\nevent: PreToolUse\n---\njust lint\n",
        );
        let found = discover(&vfs).unwrap();
        let hook = found.iter().find(|c| c.name == "lint-on-write").unwrap();
        assert_eq!(hook.kind, EntityKind::Hook);
        assert_eq!(hook.kind_source, KindSource::Directory);
        assert_eq!(hook.plugin.as_deref(), Some("core"));

        // The same note, declaring what it already is: one kind, a different
        // layer, and the report says which.
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/core/hooks/lint-on-write.md",
            "---\nname: lint-on-write\ndescription: d\nevent: PreToolUse\n---\njust lint\n",
        );
        let found = discover(&vfs).unwrap();
        let hook = found.iter().find(|c| c.name == "lint-on-write").unwrap();
        assert_eq!(hook.kind, EntityKind::Hook);
        assert_eq!(hook.kind_source, KindSource::Directory);
    }

    /// A note that declares itself but sits where no shape can read it is
    /// named, not dropped. Silence is for resources, which declare nothing.
    #[test]
    fn a_declared_note_at_an_unreadable_path_is_refused_with_the_shapes() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/core/core.md",
            &source("core", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/plugins/core/skills/a/a.md",
            &source("a", &[]),
        );
        // Inside a plugin, in a folder that is no kind folder.
        write(
            &vfs,
            "marketplaces/house/plugins/core/extra/stray.md",
            &source("stray", &[]),
        );

        let err = discover(&vfs).unwrap_err().to_string();
        assert!(err.contains("stray.md"), "{err}");
        assert!(
            err.contains("marketplaces/<name>/plugins/<name>/skills/"),
            "{err}"
        );

        // The same file, declaring nothing, is an ordinary resource.
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/core/core.md",
            &source("core", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/plugins/core/skills/a/a.md",
            &source("a", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/plugins/core/skills/a/extra/note.md",
            "# prose\n",
        );
        let found = discover(&vfs).unwrap();
        let a = found.iter().find(|c| c.name == "a").unwrap();
        assert_eq!(a.resources, vec!["extra/note.md".to_owned()]);
    }

    /// The stray rule follows the opt-in, so a `description` alone is enough
    /// to be named. A resource still declares neither key and stays silent —
    /// which is what keeps the widened opt-in from commenting on every file
    /// beside an entry note.
    #[test]
    fn a_stray_carrying_a_description_alone_is_refused_with_the_shapes() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/core/skills/a/a.md",
            &source("a", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/plugins/core/extra/stray.md",
            "---\ndescription: half a draft\n---\nb\n",
        );

        let err = discover(&vfs).unwrap_err().to_string();
        assert!(err.contains("stray.md"), "{err}");
        assert!(err.contains("no component can be read"), "{err}");
    }

    #[test]
    fn notes_that_declare_nothing_are_skipped_not_refused() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/notes.md",
            "# Just prose, no frontmatter\n",
        );
        write(
            &vfs,
            "marketplaces/house/skills/draft.md",
            "---\ntags:\n  - idea\n---\nA draft.\n",
        );
        // Not markdown, not an entry.
        write(&vfs, "marketplaces/house/plugins/tool.sh", "#!/bin/sh\n");

        assert!(discover(&vfs).unwrap().is_empty());
    }

    #[test]
    fn a_description_alone_is_a_component_named_after_its_file() {
        // The opt-in is `description`. The name follows the filename, which is
        // the answer a single-note publish of the same note gives.
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/Release Notes.md",
            "---\ndescription: d\n---\nb\n",
        );
        let found = discover(&vfs).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "release-notes");
    }

    #[test]
    fn a_name_with_no_description_is_admitted_with_the_filler_and_a_warning() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/mute.md",
            "---\nname: mute\n---\nb\n",
        );
        // Whitespace is absence.
        write(
            &vfs,
            "marketplaces/house/skills/blank.md",
            "---\nname: blank\ndescription: \"  \"\n---\nb\n",
        );
        let report = scan(&vfs).unwrap();
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        let filled: Vec<(&str, &str, bool)> = report
            .components
            .iter()
            .map(|c| {
                (
                    c.name.as_str(),
                    c.description.as_str(),
                    c.description_filled,
                )
            })
            .collect();
        assert_eq!(
            filled,
            [
                ("blank", MISSING_DESCRIPTION, true),
                ("mute", MISSING_DESCRIPTION, true)
            ]
        );
        let warned: Vec<(&str, &str)> = report
            .warnings
            .iter()
            .map(|w| (w.path.as_str(), w.reason.as_str()))
            .collect();
        assert_eq!(
            warned,
            [
                ("marketplaces/house/skills/blank.md", "missing-description"),
                ("marketplaces/house/skills/mute.md", "missing-description")
            ]
        );
    }

    #[test]
    fn a_described_note_carries_no_warning() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/said.md",
            "---\nname: said\ndescription: d\n---\nb\n",
        );
        let report = scan(&vfs).unwrap();
        assert!(!report.components[0].description_filled);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn the_skills_spec_name_rule_is_enforced() {
        for bad in ["Upper", "-lead", "trail-", "no--doubles", "under_score"] {
            let vfs = vault();
            write(
                &vfs,
                "marketplaces/house/skills/x.md",
                &format!("---\nname: {bad}\ndescription: d\n---\nb\n"),
            );
            let err = discover(&vfs).unwrap_err().to_string();
            assert!(err.contains("Agent Skills spec"), "{bad}: {err}");
            assert!(
                err.contains("/vault/marketplaces/house/skills/x.md"),
                "{bad}: {err}"
            );
        }

        // 64 is the cap, not one under it.
        let cap = "a".repeat(64);
        let vfs = vault();
        write(
            &vfs,
            &format!("marketplaces/house/skills/{cap}.md"),
            &format!("---\nname: {cap}\ndescription: d\n---\nb\n"),
        );
        assert_eq!(discover(&vfs).unwrap().len(), 1);

        let over = "a".repeat(65);
        let vfs = vault();
        write(
            &vfs,
            &format!("marketplaces/house/skills/{over}.md"),
            &format!("---\nname: {over}\ndescription: d\n---\nb\n"),
        );
        assert!(discover(&vfs).is_err());
    }

    #[test]
    fn the_description_cap_is_1024_characters() {
        let cap = "d".repeat(1024);
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/talk.md",
            &format!("---\nname: talk\ndescription: {cap}\n---\nb\n"),
        );
        assert_eq!(discover(&vfs).unwrap().len(), 1);

        let over = "d".repeat(1025);
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/talk.md",
            &format!("---\nname: talk\ndescription: {over}\n---\nb\n"),
        );
        let err = discover(&vfs).unwrap_err().to_string();
        assert!(err.contains("1024"), "{err}");
    }

    #[test]
    fn the_compatibility_cap_is_500_characters() {
        let cap = "c".repeat(500);
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/talk.md",
            &format!("---\nname: talk\ndescription: d\ncompatibility: {cap}\n---\nb\n"),
        );
        assert_eq!(discover(&vfs).unwrap().len(), 1);

        let over = "c".repeat(501);
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/talk.md",
            &format!("---\nname: talk\ndescription: d\ncompatibility: {over}\n---\nb\n"),
        );
        let err = discover(&vfs).unwrap_err().to_string();
        assert!(err.contains("compatibility"), "{err}");
        assert!(err.contains("501"), "{err}");
        assert!(err.contains("500"), "{err}");
    }

    #[test]
    fn a_declared_name_need_not_match_the_filename() {
        // The slug is what lets an Obsidian-titled note publish under its
        // kebab-case name without renaming the file.
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/Workflow Checklist.md",
            "---\nname: workflow-checklist\ndescription: d\n---\nb\n",
        );
        assert_eq!(discover(&vfs).unwrap()[0].name, "workflow-checklist");

        // A declared name unrelated to the stem is the note's own answer, and
        // it is the answer a publish of that note has always given.
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/other.md",
            "---\nname: different\ndescription: d\n---\nb\n",
        );
        assert_eq!(discover(&vfs).unwrap()[0].name, "different");
    }

    #[test]
    fn one_unreadable_note_does_not_stop_the_scan() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/good-one.md",
            &source("good-one", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/skills/good-two.md",
            &source("good-two", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/skills/draft.md",
            "---\nname: Not A Legal Name\ndescription: d\n---\nb\n",
        );

        let report = scan(&vfs).unwrap();

        let names: Vec<&str> = report.components.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["good-one", "good-two"]);
        assert_eq!(report.skipped.len(), 1);
        let skipped = &report.skipped[0];
        assert_eq!(skipped.path, "marketplaces/house/skills/draft.md");
        assert_eq!(skipped.reason, "unreadable-note");
        assert!(skipped.detail.contains("Agent Skills spec"), "{skipped:?}");
    }

    #[test]
    fn a_skipped_note_carries_the_token_its_refusal_already_had() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/aimed.md",
            "---\nname: aimed\ndescription: d\nsw-targets: [nowhere]\n---\nb\n",
        );

        let report = scan(&vfs).unwrap();

        assert!(report.components.is_empty());
        assert_eq!(report.skipped[0].reason, "unknown-target");
    }

    #[test]
    fn a_skipped_note_reports_the_pack_it_belongs_to() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/tools/skills/draft.md",
            "---\nname: Not A Legal Name\ndescription: d\n---\nb\n",
        );

        let report = scan(&vfs).unwrap();

        assert_eq!(report.skipped[0].plugin.as_deref(), Some("tools"));
    }

    #[test]
    fn a_duplicate_name_is_refused_naming_both_notes() {
        // Legal on its own, either way: a lone `foo.md` and a `foo/foo.md`
        // that carries files both spell the name right. In one `skills/`
        // folder they claim one published directory.
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/tools/skills/foo.md",
            &source("foo", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/plugins/tools/skills/foo/foo.md",
            &source("foo", &[]),
        );

        let err = discover(&vfs).unwrap_err().to_string();
        assert!(err.contains("path-collision"), "{err}");
        assert!(err.contains("skills/foo/foo.md"), "{err}");
        assert!(err.contains("skills/foo.md"), "{err}");
    }

    /// The shape rule reaches this case before the overlap check does.
    ///
    /// A component nested inside another's bundle has no shape that reads, so
    /// it is refused for sitting nowhere rather than for overlapping. The
    /// overlap check survives as a guard on `bundle_root`'s meaning — what
    /// ships with a skill must not contain another entry — and the two
    /// refusals say the same thing to the author either way.
    #[test]
    fn a_component_inside_another_components_bundle_is_refused() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/outer/outer.md",
            &source("outer", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/plugins/outer/inner/inner.md",
            &source("inner", &[]),
        );

        let err = discover(&vfs).unwrap_err().to_string();
        assert!(
            err.contains("marketplaces/house/plugins/outer/inner/inner.md"),
            "{err}"
        );
        assert!(err.contains("no component can be read"), "{err}");
    }

    /// Outside a kind folder, a `SKILL.md` or a `<name>/<name>.md` is the
    /// author's own writing, not a component.
    #[test]
    fn an_entry_shape_outside_a_kind_folder_is_not_a_component() {
        let vfs = vault();
        write(&vfs, "notes/notes.md", &source("notes", &[]));
        write(&vfs, "drafts/idea/SKILL.md", &source("idea", &[]));
        let report = scan(&vfs).unwrap();
        assert!(report.components.is_empty(), "{:?}", report.components);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    }

    /// A `SKILL.md` entry takes its folder's name when it declares none, and
    /// owns its folder like `<name>/<name>.md` does.
    #[test]
    fn a_skill_md_entry_is_named_by_its_folder() {
        let vfs = vault();
        write(
            &vfs,
            "skills/code-review/SKILL.md",
            "---\ndescription: Reviews code.\n---\n",
        );
        write(&vfs, "skills/code-review/check.py", "#!/x\n");
        // Only a skill takes `SKILL.md`: a plugin's entry is `<name>/<name>.md`.
        write(&vfs, "plugins/tools/SKILL.md", &source("tools", &[]));

        let report = scan(&vfs).unwrap();
        let rows: Vec<(&str, EntityKind)> = report
            .components
            .iter()
            .map(|c| (c.name.as_str(), c.kind))
            .collect();
        assert_eq!(rows, [("code-review", EntityKind::Skill)]);
        assert_eq!(report.components[0].resources, ["check.py"]);
        // It declares itself, so it is refused as a stray rather than dropped.
        let skipped: Vec<&str> = report.skipped.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(skipped, ["plugins/tools/SKILL.md"]);
    }

    #[test]
    fn targets_validate_against_the_cores_known_set() {
        // The decision's named divergence: the whole known set, not `codex`/`claude`.
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/wide.md",
            &source("wide", &["antigravity", "portable", "amp"]),
        );
        assert_eq!(
            discover(&vfs).unwrap()[0].targets,
            Some(vec![
                "antigravity".to_owned(),
                "portable".to_owned(),
                "amp".to_owned()
            ])
        );

        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/typo.md",
            &source("typo", &["emacs"]),
        );
        let err = discover(&vfs).unwrap_err().to_string();
        assert!(err.contains("unknown-target"), "{err}");
        assert!(err.contains("emacs"), "{err}");
        assert!(
            err.contains("/vault/marketplaces/house/skills/typo.md"),
            "{err}"
        );
        // The fix is on screen.
        assert!(err.contains("claude-code"), "{err}");
    }

    #[test]
    fn targets_keep_the_cores_three_outcomes() {
        // Absent means every destination and empty means nowhere — the core's
        // `targets` contract. A scalar is refused, never comma-split.
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/keen.md",
            &source("keen", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/skills/mute.md",
            "---\nname: mute\ndescription: d\nsw-targets: []\n---\nb\n",
        );
        let found = discover(&vfs).unwrap();
        assert_eq!(found[0].targets, None);
        assert_eq!(found[1].targets, Some(vec![]));

        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/scalar.md",
            "---\nname: scalar\ndescription: d\nsw-targets: codex, claude\n---\nb\n",
        );
        let err = discover(&vfs).unwrap_err().to_string();
        assert!(err.contains("must be a list"), "{err}");
    }

    #[test]
    fn the_plugin_name_the_folder_gives_is_validated() {
        // A directory name that cannot be a marketplace directory fails on the
        // first component under it, not at the far end of a publish.
        // No plugin note here: its own `name` would fail the component rule
        // first, and what is under test is the folder as a *source* of the
        // plugin fact.
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/plugins/Bad Name/skills/y.md",
            &source("y", &[]),
        );
        let err = discover(&vfs).unwrap_err().to_string();
        assert!(err.contains("plugin name"), "{err}");
        assert!(err.contains("Bad Name"), "{err}");
    }

    #[test]
    fn results_are_sorted_by_name_whatever_the_walk_found_first() {
        let vfs = vault();
        write(
            &vfs,
            "marketplaces/house/skills/zulu.md",
            &source("zulu", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/plugins/alpha/alpha.md",
            &source("alpha", &[]),
        );
        write(
            &vfs,
            "marketplaces/house/skills/mike.md",
            &source("mike", &[]),
        );

        let names: Vec<String> = discover(&vfs)
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, vec!["alpha", "mike", "zulu"]);
    }

    /// The path-only reading, for the callers holding one note and no scan.
    #[test]
    fn folder_placement_names_every_level_a_path_passes_through() {
        let root = Path::new("/vault");
        let deep = folder_placement(
            root,
            Path::new("/vault/marketplaces/house/plugins/core/skills/review/review.md"),
        );
        assert_eq!(deep.kind, Some(EntityKind::Skill));
        assert_eq!(deep.marketplace.as_deref(), Some("house"));
        assert_eq!(deep.plugin.as_deref(), Some("core"));

        // A command, which the note itself never declared.
        let command = folder_placement(root, Path::new("/vault/plugins/core/commands/ship.md"));
        assert_eq!(command.kind, Some(EntityKind::Command));
        assert_eq!(command.marketplace, None, "no marketplace level above it");
        assert_eq!(command.plugin.as_deref(), Some("core"));

        // A marketplace's own note belongs to no marketplace — it is one.
        let market = folder_placement(root, Path::new("/vault/marketplaces/house/skillwright.md"));
        assert_eq!(market.kind, Some(EntityKind::Marketplace));
        assert_eq!(market.marketplace, None);

        // Outside any kind folder, structure says nothing at all — which is
        // not the same as saying `skill`.
        assert_eq!(
            folder_placement(root, Path::new("/vault/elsewhere/note.md")),
            FolderPlacement::default()
        );
        // And a path that is not under the root at all is the same silence
        // rather than a panic.
        assert_eq!(
            folder_placement(root, Path::new("/somewhere/else.md")),
            FolderPlacement::default()
        );
    }

    /// Where the scan starts, and what a component belongs to when the
    /// levels above it are missing.
    #[test]
    fn the_root_is_the_vault_and_a_missing_configured_root_is_an_error() {
        let vfs = vault();
        // Promoted out of every plugin, still inside its marketplace: the
        // kind folder names it, and the levels above it name the rest.
        write(
            &vfs,
            "marketplaces/house/skills/demo.md",
            &source("demo", &[]),
        );
        // A peer directory: it holds no kind folder, so nothing in it is
        // scanned or refused. This is where shared notes live.
        write(&vfs, "elsewhere/hidden.md", &source("hidden", &[]));

        let found = discover(&vfs).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "demo");
        assert_eq!(found[0].marketplace.as_deref(), Some("house"));
        assert_eq!(found[0].plugin, None, "promoted above every plugin");

        // The same rule with the top two levels absent: a vault whose root
        // holds `plugins/` is one unnamed marketplace.
        let bare = MemVfs::new();
        write(&bare, "plugins/core/skills/demo.md", &source("demo", &[]));
        let found = discover(&bare).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].marketplace, None);
        assert_eq!(found[0].plugin.as_deref(), Some("core"));

        // A configured root replaces the vault as the place the scan starts,
        // and vault-relative paths are still spelled from the vault root.
        write(
            &vfs,
            "elsewhere/plugins/hidden/skills/note.md",
            &source("note", &[]),
        );
        let found = discover_components(
            &vfs,
            Path::new("/vault"),
            Some(Path::new("/vault/elsewhere")),
            &sw(),
        )
        .unwrap()
        .components;
        assert_eq!(found[0].name, "note");
        assert_eq!(found[0].plugin.as_deref(), Some("hidden"));
        assert_eq!(
            found[0].vault_relative_path,
            "elsewhere/plugins/hidden/skills/note.md"
        );

        let err = discover_components(
            &vfs,
            Path::new("/vault"),
            Some(Path::new("/vault/absent")),
            &sw(),
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::RootNotFound(_)), "{err}");
    }

    /// The one discovery rule [`MemVfs`] cannot exercise: it has no links.
    #[cfg(unix)]
    #[test]
    fn a_symlink_in_the_skills_source_is_refused_on_a_real_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        std::fs::create_dir_all(plugins.join("demo")).unwrap();
        std::fs::write(
            plugins.join("demo/demo.md"),
            "---\nname: demo\ndescription: d\n---\nb\n",
        )
        .unwrap();

        // Sound before any link arrives.
        let found = discover_components(&NativeVfs, dir.path(), None, &Namespace::sw())
            .unwrap()
            .components;
        assert_eq!(found.len(), 1);

        // A linked directory is refused by name...
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, plugins.join("linked")).unwrap();
        let err = discover_components(&NativeVfs, dir.path(), None, &Namespace::sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("unsafe-path"), "{err}");
        assert!(err.contains("linked"), "{err}");
        std::fs::remove_file(plugins.join("linked")).unwrap();

        // ...and so is a linked file, wherever in the tree it sits.
        std::fs::write(dir.path().join("real.md"), "outside the source\n").unwrap();
        std::os::unix::fs::symlink(dir.path().join("real.md"), plugins.join("demo/stray.md"))
            .unwrap();
        let err = discover_components(&NativeVfs, dir.path(), None, &Namespace::sw())
            .unwrap_err()
            .to_string();
        assert!(err.contains("unsafe-path"), "{err}");
        assert!(err.contains("stray.md"), "{err}");
    }

    /// `.trash` holds what `delete` moved out of the tree. The walk must not
    /// descend into it — a trashed note's old shape no longer resolves to a
    /// valid placement and would otherwise surface as an unreadable stray.
    #[test]
    fn a_trash_directory_is_never_walked() {
        let vfs = vault();
        write(
            &vfs,
            ".trash/marketplaces/house/plugins/review/skills/old/old.md",
            &source("old", &[]),
        );

        let found = discover(&vfs).unwrap();

        assert!(found.is_empty(), "{found:?}");
    }

    /// Tooling folders are never walked, wherever they sit: a declared note
    /// under one is not a stray, and a bundle does not own their files. A
    /// dot-named file is still walked: a bundle may ship one.
    #[test]
    fn tooling_folders_are_never_walked_and_dot_files_are() {
        let vfs = vault();
        write(&vfs, "plugins/demo/demo.md", &source("demo", &[]));
        write(&vfs, "plugins/demo/.env.example", "KEY=\n");
        write(&vfs, "plugins/demo/node_modules/pkg/index.js", "x\n");
        write(
            &vfs,
            "plugins/demo/node_modules/skills/vendored.md",
            &source("vendored", &[]),
        );
        write(
            &vfs,
            ".obsidian/plugins/tool/skills/tool.md",
            &source("tool", &[]),
        );

        let found = discover(&vfs).unwrap();

        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].resources, vec![".env.example".to_owned()]);
    }

    /// A folder the walk cannot list fails the scan rather than shrinking it.
    #[cfg(unix)]
    #[test]
    fn a_folder_that_cannot_be_listed_fails_the_scan() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("plugins/demo/locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::write(
            dir.path().join("plugins/demo/demo.md"),
            "---\nname: demo\ndescription: d\n---\nb\n",
        )
        .unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root lists any folder, so there is nothing to test.
        let listable = std::fs::read_dir(&locked).is_ok();
        let result = discover_components(&NativeVfs, dir.path(), None, &Namespace::sw());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        if listable {
            return;
        }
        let err = result.unwrap_err();
        assert!(matches!(err, CoreError::Io { .. }), "{err}");
        assert!(err.to_string().contains("locked"), "{err}");
    }
}
