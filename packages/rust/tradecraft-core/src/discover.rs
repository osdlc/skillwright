//! Discovery: every note in a vault, found by its position and vetted.
//!
//! The scan every marketplace verb stands on: `build` renders what this
//! returns and `check` compares it. The rules here are the definition of a
//! component, so a note this module refuses is refused everywhere at once.
//!
//! # Position decides
//!
//! [`crate::layout::read_layout`] checks the folders first, and refuses a
//! vault not in the layout before any note is read. Then each note is what
//! its position says ([`crate::layout::classify`]):
//!
//! | Path | Read as |
//! |---|---|
//! | `marketplaces/<m>/<m>.md` | a marketplace |
//! | `marketplaces/<m>/plugins/<p>/<p>.md` | a plugin |
//! | `…/plugins/<p>/<kind>/<c>/<c>.md` | a skill, agent or command |
//!
//! Frontmatter never decides kind, plugin or marketplace. Every other file is
//! a resource of the folder that holds it.
//!
//! Every note at a component position is a component. One with no
//! `description` is listed with a [`MISSING_DESCRIPTION_REASON`] entry in
//! [`DiscoveryReport::warnings`], and a build refuses it.
//!
//! # Validation
//!
//! The Agent Skills name rule for every folder name (checked by the layout),
//! the 1024-character description cap, the 500-character compatibility cap,
//! names unique within a plugin across all kinds, symlinks refused.
//!
//! # Deterministic, like every report in this crate
//!
//! Directory entries are sorted before descent, results are sorted by name,
//! and nothing reads a clock or iterates a hash map, so two runs over one tree
//! return byte-identical results.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::CoreError;
use crate::frontmatter::{
    ContainerIdentity, EntityKind, KindHints, KindSource, VaultFrontmatter, resolve_kind,
};
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
// component — uniqueness is keyed on `(marketplace, plugin, name)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DiscoveredComponent {
    /// The frontmatter `name`, already validated against the Agent Skills spec.
    pub name: String,
    /// The frontmatter `description`, ≤1024 characters. Empty when the note
    /// wrote none. A build refuses a skill, command or agent with none
    /// ([`Self::lacks_description`]); a container need not have one.
    pub description: String,
    /// What this component is, from its position.
    pub kind: EntityKind,
    /// Which layer produced [`Self::kind`], straight from
    /// [`crate::frontmatter::resolve_kind`].
    pub kind_source: KindSource,
    /// What the position said this was. Always the same as [`Self::kind`].
    pub folder_kind: Option<EntityKind>,
    /// `targets` exactly as [`VaultFrontmatter::targets`] read it, every
    /// name checked against [`KNOWN_TARGETS`]. `None` is absent — every
    /// destination; `Some(vec![])` is nowhere.
    pub targets: Option<Vec<String>>,
    /// The plugin folder holding this note. `None` for a marketplace's note.
    pub plugin: Option<String>,
    /// The marketplace folder this sits under. `None` for a marketplace's
    /// own note.
    pub marketplace: Option<String>,
    /// Absolute path of the entry note.
    pub entry_path: PathBuf,
    /// The folder this note owns, the one named for it.
    pub bundle_root: Option<PathBuf>,
    /// Files this note owns, relative to [`Self::bundle_root`] and
    /// forward-slashed, sorted. A plugin does not own its components' files.
    pub resources: Vec<String>,
    /// The entry's path relative to the vault root, forward-slashed — the
    /// spelling reports and provenance markers print.
    pub vault_relative_path: String,
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
    /// A member with a `name` and no `description`, which a build and a
    /// publish refuse.
    pub fn lacks_description(&self) -> bool {
        self.description.is_empty() && needs_description(self.kind)
    }

    /// Why a member's name, taken from its file name because it states none,
    /// is not one the Agent Skills spec admits, or `None` when it is. The
    /// scan refuses a stated name that breaks the rule, so only a derived one
    /// reaches here.
    pub fn invalid_name(&self) -> Option<String> {
        if !needs_description(self.kind) || name_refusal(&self.name, "name").is_none() {
            return None;
        }
        // The slug holds only `a-z`, `0-9` and single inner hyphens, so it
        // breaks the rule only by being empty or too long.
        Some(if self.name.is_empty() {
            "its file name has no ASCII letter or digit, so the name it gives is empty".to_owned()
        } else {
            format!(
                "the name its file name gives, `{}`, is {} characters, over the {NAME_MAX_LEN} \
                 the Agent Skills spec allows",
                self.name,
                self.name.len()
            )
        })
    }

    /// The folder this note is named for. A marketplace note's `name` is only
    /// what it publishes under.
    pub fn folder(&self) -> &str {
        self.entry_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
    }
}

/// What happens to one unknown key, as a warning says it. A key the namespace
/// claims is consumed on the read.
fn unknown_key_fate(kind: EntityKind, key: &str, ns: &Namespace) -> &'static str {
    let in_a_block = key
        .split_once('.')
        .is_some_and(|(block, _)| crate::publish::canonical_target(block).is_some());
    match kind {
        EntityKind::Plugin | EntityKind::Marketplace | EntityKind::Hook => "ignored",
        _ if in_a_block => "ignored",
        _ if ns.is_ours(key) => "dropped",
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
    let hint = if ns.is_bare() {
        String::new()
    } else {
        let respelled: Vec<String> = unknown
            .iter()
            .filter_map(|key| {
                let field = ns.field(key).replace('-', "_");
                let bare = crate::harness::profile_named(key).is_some();
                let hyphen = field != ns.field(key)
                    && EntityKind::ALL
                        .iter()
                        .any(|kind| crate::keys::is_owned(*kind, &field));
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
    unknown.extend(unknown_block_keys(&frontmatter.blocks, kind));
    unknown
}

/// The keys in a note's harness blocks that no harness file reads, as
/// `<harness>.<key>`. None on an agent or a hook, whose blocks are refused or
/// ignored whole.
pub(crate) fn unknown_block_keys(blocks: &Map<String, Value>, kind: EntityKind) -> Vec<String> {
    if kind.holds_components() {
        unknown_container_block_keys(blocks, kind)
    } else if matches!(kind, EntityKind::Skill | EntityKind::Command) {
        unknown_component_block_keys(blocks)
    } else {
        Vec::new()
    }
}

/// Refuses a value of the wrong kind in the root file's own blocks. A file
/// that cannot be parsed passes on purpose: the run that reads its settings
/// says what is wrong with it.
///
/// # Errors
///
/// `refused: malformed`.
fn check_root_blocks(vfs: &dyn Vfs, vault_root: &Path) -> Result<(), CoreError> {
    let file = vault_root.join(crate::settings::ROOT_FILE);
    let Ok(text) = read_utf8(vfs, &file) else {
        return Ok(());
    };
    let Ok(root) = crate::settings::parse_root_file(&text) else {
        return Ok(());
    };
    crate::harness::manifest::check_block_values(&root.to_config())
}

/// Refuses a value of the wrong kind on a component note: a bare spec key, a
/// key in a skill's or command's block, or a key in a container's block. The
/// checks a build makes when it reads and loads the note.
///
/// # Errors
///
/// `refused: malformed`.
fn check_values(
    fields: &mut Map<String, Value>,
    ours: &VaultFrontmatter,
    path: &Path,
    kind: EntityKind,
    folder: &str,
    ns: &Namespace,
) -> Result<(), CoreError> {
    crate::vault::check_spec_kinds(fields, path, kind)?;
    match kind {
        EntityKind::Skill | EntityKind::Command => {
            crate::publish::overrides_for(ours, path, kind, ns)?;
        }
        EntityKind::Plugin | EntityKind::Marketplace => {
            let name = match kind {
                EntityKind::Plugin => Some(folder),
                _ => fields.get("name").and_then(Value::as_str),
            };
            let owner = crate::harness::manifest::container_owner(kind, name);
            crate::harness::manifest::check_container_blocks(&owner, kind, &ours.blocks)?;
        }
        EntityKind::Agent | EntityKind::Hook => {}
    }
    Ok(())
}

/// The keys in a skill's or command's harness blocks that harness does not
/// read, as `<harness>.<key>`. They warn, and the build drops them.
pub(crate) fn unknown_component_block_keys(blocks: &Map<String, Value>) -> Vec<String> {
    let mut unknown = Vec::new();
    for (declared, block) in blocks {
        let Some(accepted) = crate::harness::profile_for(declared)
            .and_then(|profile| profile.block_keys(EntityKind::Skill))
        else {
            continue;
        };
        for key in block.as_object().into_iter().flat_map(|b| b.keys()) {
            if !accepted.contains(key) {
                unknown.push(format!("{declared}.{key}"));
            }
        }
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
        let Some(accepted) =
            crate::harness::profile_named(harness).and_then(|profile| profile.block_keys(kind))
        else {
            continue;
        };
        for key in block.as_object().into_iter().flat_map(|b| b.keys()) {
            if !accepted.contains(key) {
                unknown.push(format!("{harness}.{key}"));
            }
        }
    }
    unknown
}

/// The warning reason for a key the key table does not list.
pub const UNKNOWN_KEY_REASON: &str = "unknown-key";

/// What one scan found, and what it could not read.
///
/// Two lists rather than a `Result`, because a vault is a shared tree and a
/// note nobody else owns must not decide whether everybody else's publish
/// runs. Every caller reads both: the components are the roster, and
/// [`Self::skipped`] is the part of the vault this answer does not cover.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DiscoveryReport {
    /// Every publishable note, sorted by name.
    pub components: Vec<DiscoveredComponent>,
    /// Every note the scan could not read, in path order.
    pub skipped: Vec<SkippedNote>,
    /// What the scan has to say about notes it admitted, in path order. An
    /// entry [`DiscoveryWarning::is_error`] names is one a build refuses.
    #[serde(default)]
    pub warnings: Vec<DiscoveryWarning>,
}

/// The reason naming a member note with a `name` and no `description`. A
/// build and a publish refuse such a note; a scan lists it with this entry in
/// [`DiscoveryReport::warnings`].
pub const MISSING_DESCRIPTION_REASON: &str = "missing-description";

/// The reason naming a member note that states no `name` and whose file name
/// gives one the Agent Skills spec does not admit: empty, or too long. A
/// build and a publish refuse such a note; a scan lists it with this entry in
/// [`DiscoveryReport::warnings`].
pub const INVALID_NAME_REASON: &str = "invalid-name";

/// One note the scan admitted, and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DiscoveryWarning {
    /// Vault-relative and forward-slashed, as [`SkippedNote::path`].
    pub path: String,
    /// A closed-set token: `missing-description` from the scan;
    /// [`crate::export`] adds `missing-version` for a plugin.
    pub reason: String,
    /// Extra detail for a person to read.
    pub detail: String,
}

impl DiscoveryWarning {
    /// A warning about the note at `path`, with its `reason` token and the
    /// `detail` a person reads.
    pub fn new(
        path: impl Into<String>,
        reason: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
            detail: detail.into(),
        }
    }

    /// Whether a build refuses the note this names, rather than ships it: a
    /// member with no description or an invalid derived name, or a path
    /// collision.
    pub fn is_error(&self) -> bool {
        use crate::publish::ReasonToken;
        matches!(
            ReasonToken::parse(&self.reason),
            Some(
                ReasonToken::MissingDescription
                    | ReasonToken::InvalidName
                    | ReasonToken::PathCollision
            )
        )
    }
}

/// One note the scan could not read, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
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
    /// The sentence naming what is wrong, after the reason token.
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
        // `CoreError::refused` puts the token first, `<token>: <prose>`, and the
        // token is the reason, said once. Every other error is a content
        // problem with no token of its own.
        let (reason, detail) = match err {
            CoreError::Refused(message) => match CoreError::split_refusal(message) {
                Some((token, prose)) => (token.to_owned(), prose.to_owned()),
                None => ("unreadable-note".to_owned(), err.to_string()),
            },
            _ => ("unreadable-note".to_owned(), err.to_string()),
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

    /// The reason and the detail as one sentence, for a report that lists
    /// skipped notes under a single token of its own.
    pub fn sentence(&self) -> String {
        if self.reason == "unreadable-note" {
            self.detail.clone()
        } else {
            format!("{}: {}", self.reason, self.detail)
        }
    }
}

/// Every note of the vault at `vault_root`, sorted by name.
///
/// `ns` is the vault's frontmatter namespace, the same value every other
/// reader takes.
///
/// # A bad note stops itself, not the scan
///
/// Everything wrong with *one* note (an over-long description, a `targets`
/// this build does not know, frontmatter that will not parse) lands in
/// [`DiscoveryReport::skipped`] and the walk carries on. A note with no
/// `description` is listed, and named in [`DiscoveryReport::warnings`] with
/// [`MISSING_DESCRIPTION_REASON`], which a build refuses.
///
/// # Errors
///
/// * `refused: layout` — the vault is not in the layout (spec section 11).
/// * `refused: unsafe-path` — a symlink under `marketplaces/`.
/// * [`CoreError::Io`] — a folder that cannot be listed.
/// * `refused: path-collision` — two components of one name in one plugin.
pub fn discover_components(
    vfs: &dyn Vfs,
    vault_root: &Path,
    ns: &Namespace,
) -> Result<DiscoveryReport, CoreError> {
    let layout = crate::layout::read_layout(vfs, vault_root)?;
    discover_in(vfs, &layout, ns)
}

/// [`discover_components`] for a roster: a duplicate name is not an error.
/// Every component stays, and each note involved gets a `path-collision`
/// entry in [`DiscoveryReport::warnings`] naming the other party. A host that
/// builds from the result must refuse on those warnings.
///
/// # Errors
///
/// As [`discover_components`], less `path-collision`.
pub fn discover_roster(
    vfs: &dyn Vfs,
    vault_root: &Path,
    ns: &Namespace,
) -> Result<DiscoveryReport, CoreError> {
    let layout = crate::layout::read_layout(vfs, vault_root)?;
    discover_with(vfs, &layout, ns, true)
}

/// [`discover_components`] over a layout already read.
///
/// # Errors
///
/// `refused: path-collision`, as [`discover_components`].
pub(crate) fn discover_in(
    vfs: &dyn Vfs,
    layout: &crate::layout::Layout,
    ns: &Namespace,
) -> Result<DiscoveryReport, CoreError> {
    discover_with(vfs, layout, ns, false)
}

/// [`discover_in`], with `roster` turning each path collision into warnings
/// instead of a refusal.
fn discover_with(
    vfs: &dyn Vfs,
    layout: &crate::layout::Layout,
    ns: &Namespace,
    roster: bool,
) -> Result<DiscoveryReport, CoreError> {
    let vault_root = layout.root.as_path();
    let files = &layout.files;
    let placed: BTreeMap<PathBuf, crate::layout::Position> = files
        .iter()
        .filter_map(|path| {
            let rel = forward_slashed(path.strip_prefix(vault_root).ok()?);
            Some((path.clone(), crate::layout::classify(&rel)?))
        })
        .collect();

    let mut discovered = Vec::new();
    let mut skipped = Vec::new();
    let mut warnings = Vec::new();
    // The root file is configuration, not a scanned note, but a build checks
    // its blocks too, so the scan does.
    if let Err(err) = check_root_blocks(vfs, vault_root) {
        let root_file = vault_root.join(crate::settings::ROOT_FILE);
        skipped.push(SkippedNote::new(vault_root, &root_file, None, &err));
    }
    for (entry_path, position) in &placed {
        match read_source(vfs, vault_root, entry_path, position, &placed, files, ns) {
            Ok(component) => {
                if component.lacks_description() {
                    warnings.push(missing_description(&component));
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
            Err(err) => {
                let plugin = position.plugin().map(str::to_owned);
                skipped.push(SkippedNote::new(vault_root, entry_path, plugin, &err));
            }
        }
    }

    // Names are unique within a plugin across all kinds (spec section 3 rule
    // 7). A two-note diagnosis, so it runs after collection and names both.
    type PublishKey<'a> = (Option<&'a str>, Option<&'a str>, &'a str);
    let mut by_key: BTreeMap<PublishKey, Vec<&DiscoveredComponent>> = BTreeMap::new();
    for component in discovered
        .iter()
        .filter(|c| !c.kind.holds_components() && c.invalid_name().is_none())
    {
        let key = (
            component.marketplace.as_deref(),
            component.plugin.as_deref(),
            component.name.as_str(),
        );
        by_key.entry(key).or_default().push(component);
    }
    // Every note of a duplicate gets one warning, naming all of them.
    for ((_, plugin, name), group) in by_key.iter().filter(|(_, g)| g.len() > 1) {
        let paths: Vec<String> = group
            .iter()
            .map(|c| format!("{} {}", c.kind.as_str(), c.vault_relative_path))
            .collect();
        let detail = format!(
            "duplicate name `{name}` in plugin `{}`: {}. Names are unique within a plugin, \
             across skills, agents and commands",
            plugin.unwrap_or_default(),
            paths.join(", ")
        );
        if !roster {
            return Err(CoreError::refused("path-collision", detail));
        }
        for component in group {
            warnings.push(collision_warning(&component.vault_relative_path, &detail));
        }
    }
    warnings.sort_by(|a, b| a.path.cmp(&b.path));
    // Keys are unique by the check above, so this order is total (a roster
    // keeps colliding keys, and then sorts by path too).
    discovered.sort_by(|a, b| {
        (&a.name, a.kind.as_str(), &a.plugin, &a.vault_relative_path).cmp(&(
            &b.name,
            b.kind.as_str(),
            &b.plugin,
            &b.vault_relative_path,
        ))
    });
    // `skipped` is in path order: `placed` is a sorted map.
    Ok(DiscoveryReport {
        components: discovered,
        skipped,
        warnings,
    })
}

/// Whether a note of `kind` must describe itself to publish. A container
/// need not.
pub(crate) fn needs_description(kind: EntityKind) -> bool {
    !kind.holds_components() && kind != EntityKind::Hook
}

/// The entry a scan carries for a member note with a `name` and no
/// `description`.
fn missing_description(component: &DiscoveredComponent) -> DiscoveryWarning {
    DiscoveryWarning {
        path: component.vault_relative_path.clone(),
        reason: MISSING_DESCRIPTION_REASON.to_owned(),
        detail: format!(
            "`{}` has a `name` and no `description`; a build refuses it",
            component.name
        ),
    }
}

/// Refuses with [`MISSING_DESCRIPTION_REASON`] when `paths` names any note.
/// A harness reads a component's description to decide when to load it, so
/// a member without one is not published.
///
/// # Errors
///
/// `refused: missing-description`, naming every path.
pub fn refuse_undescribed<'a>(paths: impl IntoIterator<Item = &'a str>) -> Result<(), CoreError> {
    let paths: Vec<&str> = paths.into_iter().collect();
    if paths.is_empty() {
        return Ok(());
    }
    Err(CoreError::refused(
        MISSING_DESCRIPTION_REASON,
        format!(
            "{} note(s) have a `name` and no `description`: {}. A harness reads the \
             description to decide when to load a component. Nothing was written",
            paths.len(),
            paths.join(", ")
        ),
    ))
}

/// Refuses with [`INVALID_NAME_REASON`] when `components` holds a member
/// whose derived name is not one the Agent Skills spec admits
/// ([`DiscoveredComponent::invalid_name`]). The name is not shortened or
/// transliterated: the author sets `name:`.
///
/// # Errors
///
/// `refused: invalid-name`, naming every note and why.
pub fn refuse_invalid_names<'a>(
    components: impl IntoIterator<Item = &'a DiscoveredComponent>,
) -> Result<(), CoreError> {
    let notes: Vec<String> = components
        .into_iter()
        .filter_map(|c| {
            c.invalid_name()
                .map(|why| format!("{} ({why})", c.vault_relative_path))
        })
        .collect();
    if notes.is_empty() {
        return Ok(());
    }
    Err(CoreError::refused(
        INVALID_NAME_REASON,
        format!(
            "{} note(s) state no `name`, and the name their file name gives is not one the \
             Agent Skills spec admits: {}. Set `name:` in each note's frontmatter. Nothing was \
             written",
            notes.len(),
            notes.join("; ")
        ),
    ))
}

/// The warning a roster carries for one note in a path collision.
fn collision_warning(path: &str, detail: &str) -> DiscoveryWarning {
    DiscoveryWarning {
        path: path.to_owned(),
        reason: "path-collision".to_owned(),
        detail: detail.to_owned(),
    }
}

/// Reads the note at `position` into a component, or refuses it.
fn read_source(
    vfs: &dyn Vfs,
    vault_root: &Path,
    entry_path: &Path,
    position: &crate::layout::Position,
    placed: &BTreeMap<PathBuf, crate::layout::Position>,
    files: &[PathBuf],
    ns: &Namespace,
) -> Result<DiscoveredComponent, CoreError> {
    let source = read_utf8(vfs, entry_path)?;
    let (frontmatter, _body) = split_frontmatter(&source);
    let mut fields = frontmatter_map(frontmatter, entry_path)?;
    let kind = position.kind();
    let resolved = resolve_kind(KindHints {
        requested: None,
        folder: Some(kind),
    });
    // A marketplace's note takes bare keys, as the root file does.
    let bare = Namespace::bare();
    let ns = if kind == EntityKind::Marketplace {
        &bare
    } else {
        ns
    };
    let ours = VaultFrontmatter::take_from(&mut fields, entry_path, ns, kind)?;
    let unknown_keys = unknown_keys(&ours, kind);
    crate::vault::check_spec_kinds(&mut fields, entry_path, kind)?;
    let declared = trimmed_string(&fields, "name");
    let name = declared
        .clone()
        .unwrap_or_else(|| position.name().to_owned());
    let description = trimmed_string(&fields, "description").unwrap_or_default();

    // The value checks a build makes on this note, so the scan refuses what a
    // build refuses.
    check_values(&mut fields, &ours, entry_path, kind, position.name(), ns)?;
    if declared.is_some() && needs_description(kind) {
        validate_name(&name, "component name", entry_path)?;
    }
    // A plugin is known by its folder in every manifest and every call, so a
    // `name` that differs could not take effect.
    if kind == EntityKind::Plugin && declared.as_deref().is_some_and(|d| d != position.name()) {
        return Err(CoreError::malformed(
            entry_path,
            format!(
                "`name: {name}` differs from its folder `{}`. A plugin publishes under its \
                 folder: remove `name`, or rename the folder",
                position.name()
            ),
        ));
    }
    if let Some(refusal) = description_refusal(&description) {
        return Err(CoreError::malformed(entry_path, refusal));
    }
    if let Some(compatibility) = trimmed_string(&fields, "compatibility")
        && let Some(refusal) = compatibility_refusal(&compatibility)
    {
        return Err(CoreError::malformed(entry_path, refusal));
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

    let bundle_root = entry_path.parent().map(Path::to_path_buf);
    let resources = bundle_root
        .as_deref()
        .map(|dir| owned_resources(files, placed, dir))
        .unwrap_or_default();
    let vault_relative_path = entry_path
        .strip_prefix(vault_root)
        .map(forward_slashed)
        .map_err(|_| CoreError::PathEscapesRoot(entry_path.to_path_buf()))?;

    Ok(DiscoveredComponent {
        name,
        description,
        kind: resolved.kind,
        kind_source: resolved.source,
        folder_kind: Some(kind),
        targets: ours.targets,
        plugin: position.plugin().map(str::to_owned),
        marketplace: (kind != EntityKind::Marketplace).then(|| position.marketplace().to_owned()),
        entry_path: entry_path.to_path_buf(),
        bundle_root,
        resources,
        vault_relative_path,
        identity: ours.identity,
        unknown_keys,
        blocks: ours.blocks,
    })
}

/// The files a note's folder holds, relative to it: every file under `dir`
/// except notes and the files of a note in a deeper folder, so a plugin does
/// not own its components' files and a marketplace does not own its plugins'.
fn owned_resources(
    files: &[PathBuf],
    placed: &BTreeMap<PathBuf, crate::layout::Position>,
    dir: &Path,
) -> Vec<String> {
    let deeper: Vec<&Path> = placed
        .keys()
        .filter_map(|note| note.parent())
        .filter(|d| *d != dir && d.starts_with(dir))
        .collect();
    let mut out: Vec<String> = files
        .iter()
        .filter(|path| path.starts_with(dir) && !placed.contains_key(*path))
        .filter(|path| !deeper.iter().any(|d| path.starts_with(d)))
        .filter_map(|path| path.strip_prefix(dir).ok().map(forward_slashed))
        .collect();
    out.sort();
    out
}

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

/// Why a name with nothing sluggable in it is refused, stated once. `what` is
/// the level being named — a component name, a plugin name, a note name — so
/// the refusal says which one had nothing to slug.
pub fn nameless(what: &str) -> String {
    format!("a {what} needs at least one letter or digit, so a filename can be slugged from it")
}

/// Why `slug` cannot be a `what`: nothing sluggable in the name it came from,
/// or a slug the skills-spec grammar refuses. `None` means it is usable.
///
/// A rename composes no note, so the cap is checked here too — otherwise an
/// over-long name refuses the vault's next scan instead of the rename.
pub fn slug_refusal(slug: &str, what: &str) -> Option<String> {
    if slug.is_empty() {
        return Some(nameless(what));
    }
    name_refusal(slug, what)
}

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
pub(crate) fn validate_name(value: &str, what: &str, path: &Path) -> Result<(), CoreError> {
    match name_refusal(value, what) {
        Some(message) => Err(CoreError::malformed(path, message)),
        None => Ok(()),
    }
}

/// The name a note publishes under when its frontmatter declares none: the
/// file name, slugified. A note is `<name>/<name>.md`, so this is its folder.
pub fn default_name(path: &Path) -> String {
    slugify(
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default(),
    )
}

/// The slug, ASCII half: lowercase, every run of anything else one
/// `-`, trimmed. `Workflow Checklist` → `workflow-checklist`. See the module
/// doc for the deliberate NFKD divergence.
///
/// Public because any caller that authors a note, such as `init`, has to
/// produce the name and the filename this scan will later check against each
/// other, and the only way that check cannot drift is for one function to
/// answer it for everybody.
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

    const SKILL: &str = "---\ndescription: Use it.\n---\nBody.\n";

    /// A vault holding `files`, with the root file and every container note
    /// the paths pass through.
    fn vault(files: &[(&str, &str)]) -> MemVfs {
        let vfs = MemVfs::new();
        let put = |path: &str, text: &str| {
            let path = Path::new("/v").join(path);
            vfs.create_dir_all(path.parent().unwrap()).unwrap();
            vfs.write_file(&path, text.as_bytes()).unwrap();
        };
        put("skillwright.md", "---\n---\n");
        for (path, _) in files {
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() > 2 {
                put(&format!("marketplaces/{0}/{0}.md", parts[1]), "---\n---\n");
            }
            if parts.len() > 4 {
                put(
                    &format!("marketplaces/{}/plugins/{1}/{1}.md", parts[1], parts[3]),
                    "---\n---\n",
                );
            }
        }
        for (path, text) in files {
            put(path, text);
        }
        vfs
    }

    fn scan(vfs: &MemVfs) -> Result<DiscoveryReport, CoreError> {
        discover_components(vfs, Path::new("/v"), &Namespace::bare())
    }

    #[test]
    fn position_alone_decides_kind_plugin_and_marketplace() {
        let vfs = vault(&[
            (
                "marketplaces/acme/plugins/tools/skills/review/review.md",
                SKILL,
            ),
            (
                "marketplaces/acme/plugins/tools/agents/helper/helper.md",
                SKILL,
            ),
            (
                "marketplaces/acme/plugins/tools/commands/ship/ship.md",
                SKILL,
            ),
        ]);
        let report = scan(&vfs).unwrap();
        let found: Vec<(&str, EntityKind, Option<&str>, Option<&str>)> = report
            .components
            .iter()
            .map(|c| {
                (
                    c.name.as_str(),
                    c.kind,
                    c.plugin.as_deref(),
                    c.marketplace.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                ("acme", EntityKind::Marketplace, None, None),
                ("helper", EntityKind::Agent, Some("tools"), Some("acme")),
                ("review", EntityKind::Skill, Some("tools"), Some("acme")),
                ("ship", EntityKind::Command, Some("tools"), Some("acme")),
                ("tools", EntityKind::Plugin, Some("tools"), Some("acme")),
            ]
        );
    }

    /// Every note at a component position is a component: one with no
    /// `description` is listed, and named so a build refuses it.
    #[test]
    fn a_note_with_no_description_is_a_component_a_build_refuses() {
        let vfs = vault(&[(
            "marketplaces/acme/plugins/tools/skills/draft/draft.md",
            "Just prose.\n",
        )]);
        let report = scan(&vfs).unwrap();
        let draft = report
            .components
            .iter()
            .find(|c| c.name == "draft")
            .unwrap();
        assert!(draft.lacks_description());
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.reason == MISSING_DESCRIPTION_REASON && w.is_error())
        );
    }

    /// Section 3 rule 7: names are unique within a plugin across kinds, and
    /// free across plugins.
    #[test]
    fn a_skill_and_a_command_of_one_name_in_one_plugin_are_refused_naming_both() {
        let vfs = vault(&[
            (
                "marketplaces/acme/plugins/tools/skills/deploy/deploy.md",
                SKILL,
            ),
            (
                "marketplaces/acme/plugins/tools/commands/deploy/deploy.md",
                SKILL,
            ),
        ]);
        let err = scan(&vfs).unwrap_err().to_string();
        assert!(err.contains("path-collision"), "{err}");
        assert!(err.contains("skills/deploy/deploy.md"), "{err}");
        assert!(err.contains("commands/deploy/deploy.md"), "{err}");

        let vfs = vault(&[
            (
                "marketplaces/acme/plugins/tools/skills/deploy/deploy.md",
                SKILL,
            ),
            (
                "marketplaces/acme/plugins/ops/commands/deploy/deploy.md",
                SKILL,
            ),
        ]);
        assert!(scan(&vfs).is_ok());
    }

    /// Section 3 rule 10: one skill copied into two marketplaces is two
    /// components.
    #[test]
    fn one_skill_copied_into_two_marketplaces_is_two_components() {
        let vfs = vault(&[
            (
                "marketplaces/acme/plugins/tools/skills/review/review.md",
                SKILL,
            ),
            (
                "marketplaces/beta/plugins/tools/skills/review/review.md",
                SKILL,
            ),
        ]);
        let report = scan(&vfs).unwrap();
        let copies: Vec<Option<&str>> = report
            .components
            .iter()
            .filter(|c| c.name == "review")
            .map(|c| c.marketplace.as_deref())
            .collect();
        assert_eq!(copies, [Some("acme"), Some("beta")]);
    }

    /// A plugin owns its shared folders, not its components' files, and a
    /// dot-named file is never read.
    #[test]
    fn a_plugin_owns_its_shared_files_and_a_skill_its_own() {
        let vfs = vault(&[
            (
                "marketplaces/acme/plugins/tools/skills/review/review.md",
                SKILL,
            ),
            (
                "marketplaces/acme/plugins/tools/skills/review/scripts/lint.sh",
                "x",
            ),
            ("marketplaces/acme/plugins/tools/scripts/deploy.sh", "x"),
            (
                "marketplaces/acme/plugins/tools/assets/.eslintrc.json",
                "{}",
            ),
        ]);
        let report = scan(&vfs).unwrap();
        let of = |name: &str| {
            report
                .components
                .iter()
                .find(|c| c.name == name && c.kind != EntityKind::Marketplace)
                .unwrap()
                .resources
                .clone()
        };
        assert_eq!(of("tools"), ["scripts/deploy.sh"]);
        assert_eq!(of("review"), ["scripts/lint.sh"]);
    }

    #[test]
    fn a_bad_note_is_skipped_and_the_scan_goes_on() {
        let long = format!("---\ndescription: {}\n---\n", "x".repeat(1025));
        let vfs = vault(&[
            (
                "marketplaces/acme/plugins/tools/skills/long/long.md",
                long.as_str(),
            ),
            (
                "marketplaces/acme/plugins/tools/skills/aim/aim.md",
                "---\ndescription: d\ntargets: [nowhere]\n---\n",
            ),
            ("marketplaces/acme/plugins/tools/skills/ok/ok.md", SKILL),
        ]);
        let report = scan(&vfs).unwrap();
        let skipped: Vec<(&str, &str)> = report
            .skipped
            .iter()
            .map(|s| (s.path.as_str(), s.reason.as_str()))
            .collect();
        assert_eq!(
            skipped,
            [
                (
                    "marketplaces/acme/plugins/tools/skills/aim/aim.md",
                    "unknown-target"
                ),
                (
                    "marketplaces/acme/plugins/tools/skills/long/long.md",
                    "unreadable-note"
                ),
            ]
        );
        assert!(report.components.iter().any(|c| c.name == "ok"));
    }

    #[test]
    fn a_plugin_note_naming_another_folder_is_refused() {
        let vfs = vault(&[(
            "marketplaces/acme/plugins/tools/tools.md",
            "---\nname: other\n---\n",
        )]);
        let report = scan(&vfs).unwrap();
        assert_eq!(report.skipped.len(), 1, "{:?}", report.skipped);
        assert!(report.skipped[0].detail.contains("differs from its folder"));
    }

    #[test]
    fn slugify_folds_ascii_and_default_name_reads_the_file_stem() {
        assert_eq!(slugify("Workflow Checklist"), "workflow-checklist");
        assert_eq!(
            default_name(Path::new("/v/skills/review/review.md")),
            "review"
        );
    }

    #[test]
    fn the_name_rule_admits_digits_and_refuses_doubled_hyphens_and_length() {
        assert!(name_refusal("v2-tools", "name").is_none());
        assert!(name_refusal("a--b", "name").is_some());
        assert!(name_refusal(&"a".repeat(64), "name").is_none());
        assert!(name_refusal(&"a".repeat(65), "name").is_some());
    }
}
