//! What `loadMarketplace` hands back: a narrow view of the core's model.
//!
//! The core's `Marketplace` is what the translators read, so it carries their
//! working state: each body as segments and the agent-adapt content. A host
//! that shows a vault's structure needs none of it, and
//! every field that crosses is one the package has to keep stable. So the
//! export converts the model into these types and serializes them instead.
//!
//! `tradecraft-typegen` reads this file beside `lib.rs`. A `Wire` prefix is
//! dropped in the TypeScript, so `WireMarketplace` is `Marketplace` there.

use std::collections::BTreeMap;

use serde::Serialize;
use tradecraft_core::model::load::{LoadRequest, load as core_load};
use tradecraft_core::model::{Asset, Component, Marketplace, Plugin, Segment, SkillId};
use tradecraft_core::{CoreError, DiscoveryWarning, EntityKind, Harness, Vfs};

/// A vault's marketplace, as `loadMarketplace` returns it.
#[derive(Debug, Serialize)]
pub struct WireMarketplace {
    /// The name the marketplace publishes under.
    pub name: String,
    /// Every plugin, by name.
    pub plugins: BTreeMap<String, WirePlugin>,
    /// Components no plugin has a place for, such as agents. In path order.
    pub uncarried: Vec<WireComponent>,
    /// What the scan and the load noticed, in path order.
    pub warnings: Vec<DiscoveryWarning>,
    /// How many skills the marketplace holds.
    pub checked: usize,
}

/// One plugin and what it carries.
#[derive(Debug, Serialize)]
pub struct WirePlugin {
    /// The plugin's name.
    pub name: String,
    /// Its skills, in name order.
    pub skills: Vec<WireComponent>,
    /// Its commands, in name order.
    pub commands: Vec<WireComponent>,
    /// Files copied verbatim into the built plugin.
    pub assets: Vec<Asset>,
    /// The paths of the Markdown files in its shared `references/`, from the
    /// plugin root.
    pub references: Vec<String>,
    /// The harnesses that name this plugin's skills without the plugin
    /// prefix their default adds.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bare_on: Vec<Harness>,
}

/// A skill, a command or an agent.
#[derive(Debug, Serialize)]
pub struct WireComponent {
    /// What it is.
    pub kind: EntityKind,
    /// Its source name, never prefixed.
    pub name: String,
    /// The note, vault-relative.
    pub path: String,
    /// Its harness blocks, by harness.
    pub blocks: BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
    /// Its `description`. `None` when no harness the load is for admits the
    /// component, because the load does not read such a note in full.
    pub description: Option<String>,
    /// Every skill or command its body calls, in body order. Empty when no
    /// harness the load is for admits the component.
    pub calls: Vec<SkillId>,
    /// The paths of the files it carries, relative to its own directory.
    pub resources: Vec<String>,
}

/// Load the marketplace `request` names and convert it to its wire form.
///
/// # Errors
///
/// Everything the core's `load` refuses.
pub fn marketplace(vfs: &dyn Vfs, request: &LoadRequest) -> Result<WireMarketplace, CoreError> {
    Ok(WireMarketplace::from_model(&core_load(vfs, request)?))
}

impl WireMarketplace {
    fn from_model(market: &Marketplace) -> Self {
        Self {
            name: market.name.clone(),
            plugins: market
                .plugins
                .iter()
                .map(|(name, plugin)| (name.clone(), WirePlugin::from_model(plugin)))
                .collect(),
            uncarried: components(&market.uncarried),
            warnings: market.warnings.clone(),
            checked: market.checked,
        }
    }
}

impl WirePlugin {
    fn from_model(plugin: &Plugin) -> Self {
        Self {
            name: plugin.name.clone(),
            skills: components(&plugin.skills),
            commands: components(&plugin.commands),
            assets: plugin.assets.clone(),
            references: plugin.references.iter().map(|r| r.path.clone()).collect(),
            bare_on: plugin.bare_on.clone(),
        }
    }
}

fn components(list: &[Component]) -> Vec<WireComponent> {
    list.iter().map(WireComponent::from_model).collect()
}

impl WireComponent {
    fn from_model(c: &Component) -> Self {
        let resolved = c.resolved.as_ref();
        Self {
            kind: c.kind(),
            name: c.name().to_owned(),
            path: c.path().to_owned(),
            blocks: c.extensions.blocks.clone(),
            description: resolved.map(|r| r.content.description.clone()),
            calls: resolved
                .into_iter()
                .flat_map(|r| &r.body.segments)
                .filter_map(|s| match s {
                    Segment::Call { call } => Some(call.skill.clone()),
                    _ => None,
                })
                .collect(),
            resources: resolved
                .into_iter()
                .flat_map(|r| &r.resources)
                .map(|r| r.path.clone())
                .collect(),
        }
    }
}
