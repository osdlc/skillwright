//! What a build or check reports: per marketplace, per harness, per plugin.

use serde::{Deserialize, Serialize};

use super::ExportMode;
use crate::discover::DiscoveryWarning;

use crate::frontmatter::EntityKind;
use crate::harness::Harness;

/// What one export did, or would do — per plugin, per root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ExportReport {
    /// The mode the report describes, which gives each [`UnitStatus`] its
    /// verb.
    pub mode: ExportMode,
    /// How many skills discovery found.
    pub checked: usize,
    /// Anything differs or is stale: drift for a check, a swap for a build.
    pub changed: bool,
    /// One unit per plugin per root, one per marketplace manifest, plus whatever the
    /// destination holds that the vault no longer produces, plus the members a
    /// layout cannot carry. Ordered: the members no root carries first; then roots in
    /// `claude`, `codex`, `cursor`, `pi` order; within a root, plugins by name, then the
    /// manifest, then stale paths, then the members that root cannot carry.
    pub units: Vec<ExportUnit>,
    /// What the scan and the build have to say about admitted notes, in path
    /// order.
    #[serde(default)]
    pub warnings: Vec<DiscoveryWarning>,
}

/// One comparable unit of the marketplace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ExportUnit {
    /// One plugin directory in one root — its manifest and its skills.
    Plugin {
        /// The harness root the plugin sits in.
        root: Harness,
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
        root: Harness,
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
        root: Harness,
        /// The stale path, relative to the destination.
        path: String,
        /// What the stale path is.
        kind: StaleKind,
    },
    /// A component the marketplace layout has no place for, named rather than
    /// dropped.
    ///
    /// A pack has a place for skills, and for commands on a root whose
    /// format carries them ([`crate::harness::PluginFormat::carries`]).
    /// Everything else is named here rather than dropped; what a build must
    /// not do is leave silently.
    ///
    /// `root` is the root that cannot carry it — a command on Codex, say.
    /// `None` when no root can, as for an agent: the reason is the layout's,
    /// so it is listed once.
    NotCarried {
        /// The root that cannot carry the component, or `None` when no root can.
        root: Option<Harness>,
        /// The component's path in the vault.
        path: String,
        /// The component's kind.
        kind: EntityKind,
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
#[non_exhaustive]
pub enum StaleKind {
    /// A plugin directory this root no longer carries — one unit for the
    /// whole tree, never a unit per file inside it.
    Plugin,
    /// A file the render does not produce.
    File,
    /// A directory holding nothing, invisible to a file comparison.
    EmptyDir,
}

/// Whether a unit matches what is installed. [`ExportReport::mode`] gives
/// the verb: a check says `ok` or `drift`, a build `unchanged` or `written`.
/// Closed: a unit either matches or does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitStatus {
    /// The unit matches what is installed.
    Same,
    /// The unit differs from what is installed; a build replaces it.
    Differs,
}

/// A plugin whose members changed since it was last built, with its own
/// `version` unmoved since: a suggestion to run `bump`.
///
/// The version at the last build is the built `plugin.json`'s own
/// `"version"` field, read back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionBumpSuggestion {
    /// The note's current `version`.
    pub current_version: String,
    /// The `version` the installed pack manifest carries. Equal to
    /// `current_version` by construction — a suggestion exists exactly
    /// because nothing moved to answer the members having changed.
    pub published_version: String,
}
