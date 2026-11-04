//! What each harness is, and how the marketplace model is written for it.
//!
//! [`profile`] holds one row of data per harness (D-FDLQ). Everything a
//! harness does differently that a value can say is a field there; the rest
//! is code in that harness's translator.

pub mod profile;

pub use profile::{
    CliInvocation, HarnessProfile, InstallCli, PROFILES, PluginDialect, PluginFormat, SkillNaming,
    profile_for, profile_named, with_plugin_format,
};
