//! The cascade: what a note takes from the containers above it (spec
//! section 7).
//!
//! Every path that reads a note goes through here. The order, nearest first:
//!
//! 1. the note's own frontmatter;
//! 2. its plugin note;
//! 3. its marketplace note, `marketplaces/<m>/<m>.md`;
//! 4. the root file's shared defaults, read from `skillwright.md`.
//!
//! Which keys come down is the key table's answer ([`crate::keys::inherits`]).
//! Names, descriptions and versions never do. A container's identity comes
//! down through [`crate::frontmatter::ContainerIdentity::fill_unset_from`], and
//! a note over the root through
//! [`crate::frontmatter::ContainerIdentity::overlay`], so neither lists the
//! fields it moves.
//!
//! A container is known by its folder, as its members are.
//!
//! # A harness block key goes where the harness reads it
//!
//! For one key, the order is: the note's own harness block, the note's own
//! frontmatter, then for each container, nearest first, its block and then
//! its frontmatter, and last what skillwright generates. So a container's
//! plain `author` beats a farther container's `claude.author`.
//! `metadata` follows that order key by key.
//!
//! Blocks inherit key by key, with these limits:
//!
//! * A key the member's kind does not accept stays with the container.
//! * A key the container's own manifests read stays with the container.
//!   `description`, `license` and Claude's `metadata` sit on both lists; on
//!   a container they describe the pack. A note's own block can still set
//!   them. `name` is refused in every block.
//! * A plugin takes neither the marketplace manifest's top-level keys nor
//!   [`crate::keys::BLOCK_IDENTITY_KEYS`] from a marketplace block.
//!
//! A block key that reaches no file warns.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::map::Entry;
use serde_json::{Map, Value};

use crate::discover::DiscoveredComponent;
use crate::error::CoreError;
use crate::export::MarketplaceConfig;
use crate::frontmatter::{ContainerIdentity, EntityKind, VaultFrontmatter};
use crate::harness::manifest::{plugin_block, plugin_identity};
use crate::keys;
use crate::settings::SharedDefaults;
use crate::vault::VaultNote;

/// The containers above some notes, folded into one config.
#[derive(Debug, Clone)]
pub(crate) struct Cascade {
    /// The root's shared defaults with the marketplace note over them, and
    /// each plugin note in [`MarketplaceConfig::plugins`].
    pub(crate) config: MarketplaceConfig,
}

impl Cascade {
    /// Folds the marketplace `market` and the plugin notes under it, out of a
    /// scan, over `root`, the root file's shared defaults.
    pub(crate) fn over(
        containers: &[DiscoveredComponent],
        market: Option<&str>,
        root: &SharedDefaults,
    ) -> Self {
        let mut config = root_config(root);

        // A marketplace's own note belongs to no marketplace, so its folder
        // finds it.
        let note = market.and_then(|name| {
            containers
                .iter()
                .find(|c| c.kind == EntityKind::Marketplace && c.folder() == name)
        });
        if let Some(note) = note {
            config.name = Some(note.name.clone());
            take_description(&mut config.description, note);
            drop_shadowed(&mut config.blocks, &note.identity, EntityKind::Marketplace);
            config.identity.overlay(&note.identity);
            merge_blocks(&mut config.blocks, &note.blocks);
        }
        if config.name.is_none() {
            config.name = market.map(str::to_owned);
        }
        // A marketplace's version is its own note's, never the root's.
        if market.is_some() {
            config.identity.version = note.and_then(|n| n.identity.version.clone());
        }

        // Each plugin note fills its own entry. The marketplace's blocks reach
        // it through `plugin_block`, less what `plain_inherited` names.
        for note in containers
            .iter()
            .filter(|c| c.kind == EntityKind::Plugin)
            .filter(|c| belongs_to(c, market))
        {
            let overrides = config.plugins.entry(note.folder().to_owned()).or_default();
            take_description(&mut overrides.description, note);
            overrides.identity.overlay(&note.identity);
            merge_blocks(&mut overrides.blocks, &note.blocks);
        }

        Self { config }
    }

    /// Gives one read note what it leaves out: a skill's `license` and each
    /// harness block key by key. `plugin` is the plugin holding it.
    pub(crate) fn apply(&self, note: &mut VaultNote, kind: EntityKind, plugin: Option<&str>) {
        let plugin = plugin.unwrap_or_default();
        // A container's own `license` and blocks are the manifest's, read from
        // the config.
        if kind.holds_components() {
            return;
        }
        if note.content.license.is_none() && keys::inherits(kind, "license") {
            note.content.license = plugin_identity(&self.config, plugin).license;
        }
        let own_metadata = crate::projection::skill_metadata(&note.content);
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
        // Anything but a mapping is refused by `fold_harness_blocks`.
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

/// The inherited keys `identity` sets as plain frontmatter on a note of
/// `kind`: `author`, `license`, `repository` and `homepage` when set.
pub(crate) fn plain_inherited(identity: &ContainerIdentity, kind: EntityKind) -> BTreeSet<String> {
    identity
        .set_fields()
        .into_iter()
        .map(|(key, _)| key)
        .filter(|key| keys::inherits(kind, key))
        .collect()
}

/// Drops from farther blocks each key a nearer note sets as plain
/// frontmatter: the nearer plain value wins (section 7 rule 5).
fn drop_shadowed(
    blocks: &mut BTreeMap<String, Map<String, Value>>,
    nearer: &ContainerIdentity,
    kind: EntityKind,
) {
    let plain = plain_inherited(nearer, kind);
    for block in blocks.values_mut() {
        for key in &plain {
            block.remove(key);
        }
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

/// The root file's shared defaults as the config a marketplace note folds
/// over. They name no marketplace and no plugin.
pub(crate) fn root_config(root: &SharedDefaults) -> MarketplaceConfig {
    MarketplaceConfig {
        identity: ContainerIdentity {
            author: root.author.clone(),
            license: root.license.clone(),
            repository: root.repository.clone(),
            homepage: root.homepage.clone(),
            ..ContainerIdentity::default()
        },
        blocks: root.blocks.clone(),
        ..MarketplaceConfig::default()
    }
}

/// Sets `slot` to a note's `description` when it writes one.
fn take_description(slot: &mut Option<String>, note: &DiscoveredComponent) {
    if !note.description.is_empty() {
        *slot = Some(note.description.clone());
    }
}

/// A component's harness blocks, each checked against the harness that will
/// receive it. Every block is
/// folded and checked, whichever harnesses are being built. A container's
/// blocks are checked where its manifests are written.
///
/// **Each block keeps only the keys its harness reads.** The rest are
/// dropped; discovery warns about them, and they are never refused. A target
/// the matrix has no declaration for keeps no keys, so an unmapped harness
/// never receives unchecked ones. A kept key with the wrong kind of value is
/// refused.
///
/// # Errors
///
/// * `refused: malformed` — a key a block's harness reads holds the wrong
///   kind of value, such as `metadata` that is not a mapping. Not when
///   `kind` is an agent: an agent's blocks never ship.
pub fn fold_harness_blocks(
    note: &VaultFrontmatter,
    note_path: &Path,
    kind: EntityKind,
) -> Result<BTreeMap<String, serde_json::Map<String, serde_json::Value>>, CoreError> {
    let mut folded: BTreeMap<String, serde_json::Map<String, serde_json::Value>> = BTreeMap::new();
    for (harness, block) in &note.blocks {
        // `VaultFrontmatter::take_from` keeps only the harness blocks
        // ([`crate::keys::HARNESS_BLOCKS`]), each a mapping.
        debug_assert!(crate::harness::KNOWN_TARGETS.contains(&harness.as_str()));
        let block = block.as_object().cloned().unwrap_or_default();
        folded.insert(harness.clone(), block);
    }

    // A key a harness does not read warns at discovery and is dropped here,
    // never refused. A key it reads takes the form every harness reads: a
    // flag such as `yes` becomes a real boolean, a list hint its text.
    for (canonical, block) in &mut folded {
        let accepted = accepted_override_keys(canonical);
        // Not `Map::retain`: the serde_json floor predates it.
        *block = std::mem::take(block)
            .into_iter()
            .filter(|(key, _)| accepted.contains(key))
            .map(|(key, value)| {
                let value = crate::harness::field_types::normalized(EntityKind::Skill, &key, value);
                (key, value)
            })
            .collect();
    }
    // A key a harness reads with the wrong kind of value is refused, in
    // every block that can ship.
    if kind == EntityKind::Agent {
        return Ok(folded);
    }
    for (canonical, block) in &folded {
        for (key, value) in block {
            if let Some(ty) = crate::harness::field_types::component_type(EntityKind::Skill, key)
                && !ty.accepts(value)
            {
                return Err(CoreError::wrong_kind(
                    note_path,
                    format!("`{canonical}.{key}` {}", ty.refusal(value)),
                ));
            }
        }
    }
    Ok(folded)
}

/// Every key one harness reads in an override block, from that runtime's
/// own capability declaration.
///
/// Empty for a name this build maps to no runtime, which is what makes every
/// key under such a target a refusal rather than an unchecked write.
fn accepted_override_keys(canonical: &str) -> std::collections::BTreeSet<String> {
    crate::harness::profile_named(canonical)
        .map(|p| p.accepted_override_keys())
        .unwrap_or_default()
}
