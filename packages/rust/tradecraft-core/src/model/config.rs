//! The marketplace and plugin manifest fields the cascade folds, which the
//! translator and the manifests read.

use std::collections::BTreeMap;

use crate::frontmatter::ContainerIdentity;

/// The marketplace and its plugins as the cascade folds them: the root
/// file's shared defaults, the marketplace note over them, and each plugin
/// note. Built only by [`crate::model::Cascade`], never by a host.
#[derive(Debug, Clone, Default)]
pub(crate) struct MarketplaceConfig {
    /// The name both manifests publish under: the marketplace note's.
    pub name: Option<String>,
    /// The marketplace's own one-line description, from its note. Optional in
    /// every dialect, and placed differently by each — see
    /// `claude_marketplace_manifest` and `cursor_marketplace_manifest`.
    pub description: Option<String>,
    /// The marketplace's manifest fields. Each field the key table says a
    /// plugin inherits is each plugin's default.
    pub identity: ContainerIdentity,
    /// The marketplace's harness blocks, keyed by harness (`codex`, …), each
    /// a map of that harness's own manifest keys. Merged over the generated
    /// marketplace manifest, and inherited by every plugin key by key.
    pub blocks: BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
    /// Each plugin note's manifest fields, keyed by plugin name.
    pub plugins: BTreeMap<String, PluginOverrides>,
}

/// What one plugin note says about its manifest. Every absence has a
/// documented default.
#[derive(Debug, Clone, Default)]
pub(crate) struct PluginOverrides {
    /// Default `Skills: <names>.` over the root's own member list.
    pub description: Option<String>,
    /// The plugin's manifest fields. Each [`crate::keys::inherits`] key it
    /// leaves unset is the marketplace's.
    pub identity: ContainerIdentity,
    /// The plugin note's harness blocks, keyed by harness. Merged over the
    /// marketplace's blocks key by key, then over the plugin's manifest and
    /// its marketplace entry.
    pub blocks: BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
}
