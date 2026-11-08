//! The cascade: what a note takes from the containers above it.
//!
//! Every path that reads a note goes through here, so a note gives the same
//! answer whether a build, a pack publish or a single-note publish reads it.
//! The order, nearest first:
//!
//! 1. the note's own frontmatter;
//! 2. its plugin note;
//! 3. the `skillwright.md` of the marketplace folder it sits in, if any;
//! 4. the request's config, which is the vault's `skillwright.md` with the
//!    workspace file already folded in
//!    ([`crate::settings::RootFile::inherit`]).
//!
//! Which keys come down is the key table's answer ([`crate::keys::inherits`]).
//! Names, descriptions and versions never do.
//!
//! A container is known by its folder, as its members are. A marketplace
//! note's `name` is only what it publishes under.
//!
//! A note under no marketplace folder takes the one marketplace the vault
//! holds, as a build does. In a vault of several there is no single answer, so
//! it stays on the request's config.
//!
//! # A harness block key goes where the harness reads it
//!
//! For one key on a member, the order is: the note's own harness block, the
//! note's own frontmatter, an inherited block (nearest container first), then
//! what skillwright generates. `metadata` follows that order key by key.
//!
//! Blocks inherit key by key, with these limits:
//!
//! * A key the member's kind does not accept stays with the container.
//! * A key the container's own manifests read stays with the container.
//!   `description`, `name`, `license` and Claude's `metadata` sit on both
//!   lists; on a container they describe the pack. A note's own block can
//!   still set them.
//! * A plugin takes neither the marketplace manifest's top-level keys nor
//!   [`crate::keys::BLOCK_IDENTITY_KEYS`] from a marketplace block.
//!
//! A block key that reaches no file warns.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::map::Entry;
use serde_json::{Map, Value};

use crate::discover::{DiscoveredComponent, DiscoveryReport, read_container};
use crate::error::CoreError;
use crate::export::MarketplaceConfig;
use crate::frontmatter::EntityKind;
use crate::harness::manifest::{inherited, plugin_block};
use crate::keys;
use crate::namespace::Namespace;
use crate::publish::accepted_keys;
use crate::vault::VaultNote;

/// The containers above some notes, folded into one config.
#[derive(Debug, Clone)]
pub struct Cascade {
    /// The request's config with the marketplace note over it, and each plugin
    /// note in [`MarketplaceConfig::plugins`].
    pub config: MarketplaceConfig,
    /// The vault-relative paths of the silent notes whose description the
    /// request supplied.
    pub configured: BTreeSet<String>,
    /// The marketplace folded in, which a note under no marketplace folder
    /// takes as its own.
    market: Option<String>,
    /// The request's own `targets`, for a component of another marketplace.
    requested_targets: Option<Vec<String>>,
}

impl Cascade {
    /// Folds the marketplace `market` and the plugin notes under it, out of a
    /// scan or the notes [`Self::above`] read, over `requested`.
    pub fn over(
        containers: &[DiscoveredComponent],
        market: Option<&str>,
        requested: &MarketplaceConfig,
    ) -> Self {
        let mut config = requested.clone();
        let mut configured = BTreeSet::new();

        // A marketplace's own note belongs to no marketplace, so its folder
        // finds it — see `DiscoveredComponent::marketplace`.
        let note = market.and_then(|name| {
            containers
                .iter()
                .find(|c| c.kind == EntityKind::Marketplace && c.folder() == name)
        });
        if let Some(note) = note {
            config.name = Some(note.name.clone());
            // A marketplace describes itself where a skill describes itself.
            if take_description(&mut config.description, note) {
                configured.insert(note.vault_relative_path.clone());
            }
            take(&mut config.display_name, note.identity.display_name.clone());
            take(&mut config.owner, note.identity.owner.clone());
            take(&mut config.author, note.identity.author.clone());
            take(&mut config.license, note.identity.license.clone());
            take(&mut config.version, note.identity.version.clone());
            take(&mut config.targets, note.targets.clone());
            take(&mut config.repository, note.identity.repository.clone());
            take(&mut config.homepage, note.identity.homepage.clone());
            merge_blocks(&mut config.blocks, &note.blocks);
        }
        // Failing a note, the marketplace's own name is what it is called: the
        // folder named the level, and a distribution named `house` on disk
        // should not need a second place to say so.
        if config.name.is_none() {
            config.name = market.map(str::to_owned);
        }
        // `version` is not inherited: a marketplace folder's version is its
        // own note's, never the root file's above it.
        if market.is_some_and(|m| marketplaces_in(containers).contains(m)) {
            config.version = note.and_then(|n| n.identity.version.clone());
        }

        // Each plugin note fills its own override block. A block the request
        // already spelled is left alone only field by field: a vault naming its
        // pack's license and a request naming its version must produce a
        // manifest carrying both, not whichever side spoke last.
        for note in containers
            .iter()
            .filter(|c| c.kind == EntityKind::Plugin)
            .filter(|c| belongs_to(c, market))
        {
            let overrides = config.plugins.entry(note.folder().to_owned()).or_default();
            // A plugin note describes its pack in the same place a skill note
            // describes itself.
            if take_description(&mut overrides.description, note) {
                configured.insert(note.vault_relative_path.clone());
            }
            take(&mut overrides.version, note.identity.version.clone());
            take(&mut overrides.author, note.identity.author.clone());
            take(&mut overrides.license, note.identity.license.clone());
            take(&mut overrides.keywords, note.identity.keywords.clone());
            take(&mut overrides.category, note.identity.category.clone());
            take(
                &mut overrides.display_name,
                note.identity.display_name.clone(),
            );
            take(&mut overrides.targets, note.targets.clone());
            take(&mut overrides.repository, note.identity.repository.clone());
            take(&mut overrides.homepage, note.identity.homepage.clone());
            merge_blocks(&mut overrides.blocks, &note.blocks);
        }

        Self {
            config,
            configured,
            market: market.map(str::to_owned),
            requested_targets: requested.targets.clone(),
        }
    }

    /// The containers above the one note at `note_path`, read from their own
    /// paths with no scan: a publish of one note must not fail on a problem
    /// elsewhere in the vault. With no `vault_root` there is no folder to read
    /// a container from, so `requested` is all there is.
    ///
    /// # Errors
    ///
    /// What reading a container note refuses. A note whose plugin note cannot
    /// be read cannot know where it may go.
    pub fn above(
        vfs: &dyn crate::vfs::Vfs,
        vault_root: Option<&Path>,
        note_path: &Path,
        requested: &MarketplaceConfig,
        ns: &Namespace,
    ) -> Result<Self, CoreError> {
        let Some(root) = vault_root else {
            return Ok(Self::over(&[], None, requested));
        };
        let patterns = crate::patterns::Patterns::read(vfs, root);
        let mut paths = crate::discover::container_notes_with(&patterns, root, note_path);
        let placed = crate::discover::folder_placement_with(&patterns, root, note_path).marketplace;
        let market = match placed {
            Some(market) => Some(market),
            None => only_marketplace(vfs, root, ns)?.inspect(|name| {
                paths.push(marketplace_note(root, name));
            }),
        };
        let mut containers = Vec::new();
        for path in paths {
            containers.extend(read_container(vfs, root, &path, ns)?);
        }
        Ok(Self::over(&containers, market.as_deref(), requested))
    }

    /// Gives every scanned component with no `targets` of its own what its
    /// containers say.
    pub fn resolve_targets(&self, components: &mut [DiscoveredComponent]) {
        let from_plugins: BTreeMap<(Option<String>, String), Vec<String>> = components
            .iter()
            .filter(|c| c.kind == EntityKind::Plugin)
            .filter_map(|c| {
                c.targets
                    .clone()
                    .map(|targets| ((c.marketplace.clone(), c.folder().to_owned()), targets))
            })
            .collect();
        let from_markets: BTreeMap<String, Vec<String>> = components
            .iter()
            .filter(|c| c.kind == EntityKind::Marketplace)
            .filter_map(|c| {
                c.targets
                    .clone()
                    .map(|targets| (c.folder().to_owned(), targets))
            })
            .collect();
        // `targets` inherits on every kind (the key table), so only a
        // marketplace's own note and a note that sets it are left alone.
        for note in components.iter_mut() {
            if note.kind == EntityKind::Marketplace || note.targets.is_some() {
                continue;
            }
            let plugin = note
                .plugin
                .as_ref()
                .and_then(|plugin| from_plugins.get(&(note.marketplace.clone(), plugin.clone())));
            let own = note.marketplace.as_deref().or(self.market.as_deref());
            let market = if own == self.market.as_deref() {
                self.config.targets.as_ref()
            } else {
                own.and_then(|m| from_markets.get(m))
                    .or(self.requested_targets.as_ref())
            };
            note.targets = plugin.or(market).cloned();
        }
    }

    /// Gives one read note what it leaves out: `targets`, a skill's `license`,
    /// and each harness block key by key. `plugin` is the plugin holding it.
    pub fn apply(
        &self,
        note: &mut VaultNote,
        kind: EntityKind,
        plugin: Option<&str>,
        ns: &Namespace,
    ) {
        let plugin = plugin.unwrap_or_default();
        if note.frontmatter.targets.is_none() {
            note.frontmatter.targets = inherited(
                &self.config,
                plugin,
                "targets",
                |own| &own.targets,
                |market| &market.targets,
            );
        }
        // A container's own `license` and blocks are the manifest's, read from
        // the config.
        if kind.holds_components() {
            return;
        }
        if note.projection.content.license.is_none() && keys::inherits(kind, "license") {
            note.projection.content.license = inherited(
                &self.config,
                plugin,
                "license",
                |own| &own.license,
                |market| &market.license,
            );
        }
        let own_metadata = crate::projection::skill_metadata(&note.projection.content, ns);
        for harness in keys::HARNESS_BLOCKS
            .iter()
            .filter(|harness| keys::inherits(kind, harness))
        {
            let reaching = member_block_keys(kind, harness);
            for (key, value) in plugin_block(&self.config, plugin, harness) {
                let merges = key == "metadata" && value.is_object();
                if !reaching.contains(&key) || (!merges && note.frontmatter.own_keys.contains(&key))
                {
                    continue;
                }
                let block = note
                    .frontmatter
                    .blocks
                    .entry((*harness).to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                let Some(block) = block.as_object_mut() else {
                    continue;
                };
                if merges {
                    merge_metadata(block, value, &own_metadata);
                } else {
                    block.entry(key).or_insert(value);
                }
            }
        }
    }
}

/// Adds an inherited `metadata` block to a note's block, key by key, under the
/// keys the block and the note's own metadata already set.
fn merge_metadata(block: &mut Map<String, Value>, inherited: Value, own: &Map<String, Value>) {
    let Value::Object(inherited) = inherited else {
        return;
    };
    let kept = inherited
        .into_iter()
        .filter(|(key, _)| !own.contains_key(key));
    match block.entry("metadata") {
        Entry::Vacant(slot) => {
            slot.insert(Value::Object(kept.collect()));
        }
        // Anything but a mapping is refused by `publish::overrides_for`.
        Entry::Occupied(mut slot) => {
            if let Some(into) = slot.get_mut().as_object_mut() {
                for (key, value) in kept {
                    into.entry(key).or_insert(value);
                }
            }
        }
    }
}

/// The keys of `harness`'s block a note of `kind` takes from a container: the
/// keys its kind accepts, less those a container's own manifests read and
/// those the harness reads on a subagent alone.
///
/// Read from a table built once for every kind and every harness with a
/// plugin format; any other name takes no key.
pub(crate) fn member_block_keys(kind: EntityKind, harness: &str) -> &'static BTreeSet<String> {
    static TABLE: OnceLock<BTreeMap<&'static str, BTreeMap<EntityKind, BTreeSet<String>>>> =
        OnceLock::new();
    static NONE: BTreeSet<String> = BTreeSet::new();
    let table = TABLE.get_or_init(|| {
        crate::harness::with_plugin_format()
            .map(|profile| {
                let by_kind = EntityKind::ALL
                    .into_iter()
                    .map(|kind| (kind, compute_member_block_keys(kind, profile)))
                    .collect();
                (profile.name, by_kind)
            })
            .collect()
    });
    table
        .get(harness)
        .and_then(|by_kind| by_kind.get(&kind))
        .unwrap_or(&NONE)
}

/// [`member_block_keys`] worked out for one harness with a plugin format,
/// for the table.
fn compute_member_block_keys(
    kind: EntityKind,
    profile: &crate::harness::HarnessProfile,
) -> BTreeSet<String> {
    let format = profile.format();
    let agent_only = profile.agent_override_keys();
    let mut keys = accepted_keys(kind, profile.name);
    keys.retain(|key| {
        !agent_only.contains(key)
            && !format.manifest_fields.contains(&key.as_str())
            && !format.entry_fields.contains(&key.as_str())
    });
    keys
}

/// The path of the note of the marketplace in `folder`.
fn marketplace_note(root: &Path, folder: &str) -> PathBuf {
    root.join(crate::paths::WORKSPACE_DIR)
        .join(folder)
        .join(crate::settings::ROOT_FILE)
}

/// The vault's one marketplace, or `None` when it holds none or several. Counted
/// by scanning `marketplaces/` as a build does, so the two cannot disagree.
///
/// # Errors
///
/// What a build's scan of that folder refuses.
fn only_marketplace(
    vfs: &dyn crate::vfs::Vfs,
    root: &Path,
    ns: &Namespace,
) -> Result<Option<String>, CoreError> {
    let folder = root.join(crate::paths::WORKSPACE_DIR);
    if !vfs.exists(&folder) {
        return Ok(None);
    }
    let files = crate::discover::load_source(vfs, root, Some(&folder))?.files;
    let source = crate::discover::Source {
        root: root.to_path_buf(),
        files,
    };
    let scan = crate::discover::discover_in(vfs, root, &source, ns)?;
    Ok(sole_marketplace(&marketplaces_in(&scan.components)))
}

/// The marketplaces a scan holds, by folder. A marketplace's own note counts:
/// a marketplace need not hold a member of its own.
pub fn marketplaces_in(components: &[DiscoveredComponent]) -> BTreeSet<&str> {
    components
        .iter()
        .filter_map(|c| match c.kind {
            EntityKind::Marketplace => Some(c.folder()),
            _ => c.marketplace.as_deref(),
        })
        .collect()
}

/// The one marketplace of `present`, or `None` when it holds none or several.
pub(super) fn sole_marketplace(present: &BTreeSet<&str>) -> Option<String> {
    match present.len() {
        1 => present.first().map(|name| (*name).to_owned()),
        _ => None,
    }
}

/// Gives every component of a scan what its containers say about `targets`,
/// for a caller reporting on the vault with no build to run. `requested` is
/// the root file's config.
pub fn inherit_targets(report: &mut DiscoveryReport, requested: &MarketplaceConfig) {
    let market = sole_marketplace(&marketplaces_in(&report.components));
    Cascade::over(&report.components, market.as_deref(), requested)
        .resolve_targets(&mut report.components);
}

/// Does this component render into the marketplace being built?
///
/// Its own do. So does everything promoted above every marketplace: a
/// component at the vault level belongs to none of them and is shared by all
/// of them — the promotion rule read from the far end — so it renders into
/// each marketplace that is built, rather than into none.
pub(super) fn belongs_to(component: &DiscoveredComponent, market: Option<&str>) -> bool {
    match (component.marketplace.as_deref(), market) {
        (None, _) => true,
        (Some(own), Some(market)) => own == market,
        (Some(_), None) => false,
    }
}

/// Lays a note's harness blocks over `slot`, key by key: the note wins where
/// both set a key.
fn merge_blocks(slot: &mut BTreeMap<String, Map<String, Value>>, from: &Map<String, Value>) {
    for (harness, block) in from {
        if let Some(block) = block.as_object() {
            let into = slot.entry(harness.clone()).or_default();
            for (key, value) in block {
                into.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Writes `from` into `slot` when the note supplied it, leaving a request-only
/// value in place. The vault outranks the request where both spoke; where the
/// vault is silent the request is all there is.
fn take<T>(slot: &mut Option<T>, from: Option<T>) {
    if let Some(value) = from {
        *slot = Some(value);
    }
}

/// [`take`] for a note's `description`. A note with none carries discovery's
/// [`crate::discover::MISSING_DESCRIPTION`] filler, which is the vault being
/// silent: it yields to the request's value and stands in only where the
/// request is silent too. Returns whether the request's value stood in for
/// the filler. A container note with neither `name` nor `description` is
/// silent altogether and leaves the slot alone.
fn take_description(slot: &mut Option<String>, note: &DiscoveredComponent) -> bool {
    if note.description.is_empty() {
        return false;
    }
    if !note.description_filled || slot.is_none() {
        *slot = Some(note.description.clone());
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A harness with no plugin format has no container block to take keys
    /// from, so it gives a member none, whatever its kind.
    #[test]
    fn a_harness_with_no_plugin_format_gives_a_member_no_block_keys() {
        let without: Vec<&str> = crate::harness::PROFILES
            .iter()
            .filter(|profile| profile.plugin.is_none())
            .map(|profile| profile.name)
            .collect();
        assert!(!without.is_empty());
        for harness in without {
            for kind in EntityKind::ALL {
                assert!(
                    member_block_keys(kind, harness).is_empty(),
                    "{harness} {kind:?}"
                );
            }
        }
        assert!(member_block_keys(EntityKind::Skill, "no-such-harness").is_empty());
        assert!(!member_block_keys(EntityKind::Skill, "claude").is_empty());
    }
}
