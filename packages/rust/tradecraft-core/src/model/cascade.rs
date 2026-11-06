//! The cascade: what a note takes from the containers above it.
//!
//! Every path that reads a note goes through here, so a note gives the same
//! answer whether a build, a pack publish or a single-note publish reads it.
//! The order, nearest first:
//!
//! 1. the note's own frontmatter;
//! 2. its plugin note;
//! 3. its marketplace note;
//! 4. the request's config, which is `skillwright.md` with the workspace file
//!    already folded in ([`crate::settings::RootFile::inherit`]).
//!
//! Which keys come down is the key table's answer ([`crate::keys::inherits`]).
//! Names, descriptions and versions never do.
//!
//! # A harness block key goes where the harness reads it
//!
//! Blocks inherit key by key, with two limits:
//!
//! * A key the container's own manifests read stays with the container and
//!   never reaches a skill, command or agent. `description`, `name`, `license`
//!   and Claude's `metadata` sit on both lists; on a container they describe
//!   the pack. A note's own block can still set them.
//! * A plugin takes neither the marketplace manifest's top-level keys nor
//!   [`crate::keys::BLOCK_IDENTITY_KEYS`] from a marketplace block.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

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

        // A marketplace's own note is the one component that belongs to no
        // marketplace while naming one — see `DiscoveredComponent::marketplace`.
        let note = market.and_then(|name| {
            containers
                .iter()
                .find(|c| c.kind == EntityKind::Marketplace && c.name == name)
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

        // Each plugin note fills its own override block. A block the request
        // already spelled is left alone only field by field: a vault naming its
        // pack's license and a request naming its version must produce a
        // manifest carrying both, not whichever side spoke last.
        for note in containers
            .iter()
            .filter(|c| c.kind == EntityKind::Plugin)
            .filter(|c| belongs_to(c, market))
        {
            let overrides = config.plugins.entry(note.name.clone()).or_default();
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
        let mut containers = Vec::new();
        for path in crate::discover::container_notes(root, note_path) {
            containers.extend(read_container(vfs, root, &path, ns)?);
        }
        let market = crate::discover::folder_placement(root, note_path).marketplace;
        Ok(Self::over(&containers, market.as_deref(), requested))
    }

    /// Gives every scanned component with no `targets` of its own what its
    /// containers say. `built` is the marketplace a vault-level component is
    /// being built into, which has none of its own.
    pub fn resolve_targets(&self, components: &mut [DiscoveredComponent], built: Option<&str>) {
        let from_plugins: BTreeMap<(Option<String>, String), Vec<String>> = components
            .iter()
            .filter(|c| c.kind == EntityKind::Plugin)
            .filter_map(|c| {
                c.targets
                    .clone()
                    .map(|targets| ((c.marketplace.clone(), c.name.clone()), targets))
            })
            .collect();
        let from_markets: BTreeMap<String, Vec<String>> = components
            .iter()
            .filter(|c| c.kind == EntityKind::Marketplace)
            .filter_map(|c| c.targets.clone().map(|targets| (c.name.clone(), targets)))
            .collect();
        for note in components.iter_mut() {
            if note.kind == EntityKind::Marketplace
                || note.targets.is_some()
                || !keys::inherits(note.kind, "targets")
            {
                continue;
            }
            let plugin = note
                .plugin
                .as_ref()
                .and_then(|plugin| from_plugins.get(&(note.marketplace.clone(), plugin.clone())));
            let market = note
                .marketplace
                .as_deref()
                .or(built)
                .and_then(|m| from_markets.get(m));
            note.targets = plugin
                .or(market)
                .or(self.requested_targets.as_ref())
                .cloned();
        }
    }

    /// Gives one read note what it leaves out: `targets`, a skill's `license`,
    /// and each harness block key by key. `plugin` is the plugin holding it.
    pub fn apply(&self, note: &mut VaultNote, kind: EntityKind, plugin: Option<&str>) {
        let plugin = plugin.unwrap_or_default();
        if note.frontmatter.targets.is_none() && keys::inherits(kind, "targets") {
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
        for harness in keys::HARNESS_BLOCKS
            .iter()
            .filter(|harness| keys::inherits(kind, harness))
        {
            let accepted = accepted_keys(kind, harness);
            let format = crate::harness::profile_named(harness).map(|p| p.format());
            let on_a_manifest = |key: &str| {
                format.is_some_and(|f| {
                    f.manifest_fields.contains(&key)
                        || f.entry_fields.contains(&key)
                        || f.marketplace_fields.contains(&key)
                })
            };
            for (key, value) in plugin_block(&self.config, plugin, harness) {
                if !accepted.contains(&key) || on_a_manifest(&key) {
                    continue;
                }
                let block = note
                    .frontmatter
                    .overrides
                    .entry((*harness).to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(block) = block.as_object_mut() {
                    block.entry(key).or_insert(value);
                }
            }
        }
    }
}

/// Gives every component of a scan what its containers say about `targets`,
/// for a caller reporting on the vault with no build to run. `requested` is
/// the root file's config.
pub fn inherit_targets(report: &mut DiscoveryReport, requested: &MarketplaceConfig) {
    Cascade::over(&[], None, requested).resolve_targets(&mut report.components, None);
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
