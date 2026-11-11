//! What each harness is, and how the marketplace model is written for it.
//!
//! [`profile`] holds one row of data per harness (D-FDLQ). Everything a
//! harness does differently that a value can say is a field there; the rest
//! is code in that harness's [`translator`], whose default methods do the
//! shared work from the row. [`manifest`] holds the manifest shapes the
//! translators choose between.

pub(crate) mod field_types;
pub mod knowledge;
pub mod manifest;
pub mod profile;
pub mod translator;

pub use profile::{
    CliInvocation, HarnessProfile, InstallCli, KNOWN_TARGETS, PLUGIN_HOOKS_PATH, PROFILES,
    PluginDialect, PluginFormat, SkillNaming, profile_named, with_plugin_format,
};
pub use translator::{Destination, FileSource, FileTree, Layout, TreeFile, write_component};
pub(crate) use translator::{HarnessTranslator, translator_for};
