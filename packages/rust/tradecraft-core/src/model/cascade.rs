//! The cascade: what a note takes from the containers above it.
//!
//! Every path that reads a note goes through here. The order, nearest first:
//!
//! 1. the note's own frontmatter;
//! 2. its plugin note;
//! 3. its marketplace note, `marketplaces/<m>/<m>.md`;
//! 4. the request's config, the shared defaults of the vault's
//!    `skillwright.md`.
//!
//! Which keys come down is the key table's answer ([`crate::keys::inherits`]).
//! Names, descriptions and versions never do. A container's identity comes
//! down through [`crate::frontmatter::ContainerIdentity::fill_unset_from`], and
//! a note over the request through
//! [`crate::frontmatter::ContainerIdentity::overlay`], so neither lists the
//! fields it moves.
//!
//! A container is known by its folder, as its members are. A marketplace
//! note's `name` is only what it publishes under.
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

use std::collections::BTreeMap;

use serde_json::map::Entry;
use serde_json::{Map, Value};

use crate::discover::DiscoveredComponent;
use crate::export::MarketplaceConfig;
use crate::frontmatter::EntityKind;
use crate::harness::manifest::{plugin_block, plugin_identity, plugin_targets};
use crate::keys;
use crate::namespace::Namespace;
use crate::vault::VaultNote;

/// The containers above some notes, folded into one config.
#[derive(Debug, Clone)]
pub struct Cascade {
    /// The request's config with the marketplace note over it, and each plugin
    /// note in [`MarketplaceConfig::plugins`].
    pub config: MarketplaceConfig,
    /// The marketplace folded in.
    market: Option<String>,
    /// The request's own `targets`, for a component of another marketplace.
    requested_targets: Option<Vec<String>>,
}

impl Cascade {
    /// Folds the marketplace `market` and the plugin notes under it, out of a
    /// scan, over `requested`.
    pub fn over(
        containers: &[DiscoveredComponent],
        market: Option<&str>,
        requested: &MarketplaceConfig,
    ) -> Self {
        let mut config = requested.clone();

        // A marketplace's own note belongs to no marketplace, so its folder
        // finds it.
        let note = market.and_then(|name| {
            containers
                .iter()
                .find(|c| c.kind == EntityKind::Marketplace && c.folder() == name)
        });
        if let Some(note) = note {
            config.name = Some(note.name.clone());
            // A marketplace describes itself where a skill describes itself.
            take_description(&mut config.description, note);
            config.identity.overlay(&note.identity);
            // `targets` is not identity data: see `keys::identity_keys`.
            take(&mut config.targets, note.targets.clone());
            merge_blocks(&mut config.blocks, &note.blocks);
        }
        // Failing a note, the marketplace's own name is what it is called: the
        // folder named the level, and a distribution named `house` on disk
        // should not need a second place to say so.
        if config.name.is_none() {
            config.name = market.map(str::to_owned);
        }
        // `version` is not inherited: a marketplace's version is its own
        // note's, never the request's.
        if market.is_some() {
            config.identity.version = note.and_then(|n| n.identity.version.clone());
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
            take_description(&mut overrides.description, note);
            overrides.identity.overlay(&note.identity);
            take(&mut overrides.targets, note.targets.clone());
            merge_blocks(&mut overrides.blocks, &note.blocks);
        }

        Self {
            config,
            market: market.map(str::to_owned),
            requested_targets: requested.targets.clone(),
        }
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
            note.frontmatter.targets = plugin_targets(&self.config, plugin);
        }
        // A container's own `license` and blocks are the manifest's, read from
        // the config.
        if kind.holds_components() {
            return;
        }
        if note.projection.content.license.is_none() && keys::inherits(kind, "license") {
            note.projection.content.license = plugin_identity(&self.config, plugin).license;
        }
        let own_metadata = crate::projection::skill_metadata(&note.projection.content, ns);
        for harness in keys::HARNESS_BLOCKS
            .iter()
            .filter(|harness| keys::inherits(kind, harness))
        {
            let Some(profile) = crate::harness::profile_named(harness) else {
                continue;
            };
            let reaching = profile.member_block_keys(kind);
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

/// Whether this note sits in the folder of the marketplace being built,
/// `marketplaces/<market>/`. Its own note does too.
pub(super) fn belongs_to(component: &DiscoveredComponent, market: Option<&str>) -> bool {
    market.is_some_and(|market| {
        component
            .vault_relative_path
            .strip_prefix(crate::layout::MARKETPLACES_DIR)
            .and_then(|rest| rest.strip_prefix('/'))
            .and_then(|rest| rest.strip_prefix(market))
            .is_some_and(|rest| rest.starts_with('/'))
    })
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

/// [`take`] for a note's `description`. A container note that writes none
/// is silent and leaves the request's value in place.
fn take_description(slot: &mut Option<String>, note: &DiscoveredComponent) {
    if !note.description.is_empty() {
        *slot = Some(note.description.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A harness with no plugin format has no container block to take keys
    /// from, so it gives a member none, whatever its kind.
    #[test]
    fn a_harness_with_no_plugin_format_gives_a_member_no_block_keys() {
        let without: Vec<&crate::harness::HarnessProfile> = crate::harness::PROFILES
            .iter()
            .filter(|profile| profile.plugin.is_none())
            .collect();
        assert!(!without.is_empty());
        for profile in without {
            for kind in EntityKind::ALL {
                assert!(
                    profile.member_block_keys(kind).is_empty(),
                    "{} {kind:?}",
                    profile.name
                );
            }
        }
        assert!(
            !crate::harness::profile::CLAUDE
                .member_block_keys(EntityKind::Skill)
                .is_empty()
        );
    }
}
