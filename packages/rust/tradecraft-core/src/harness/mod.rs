//! What each harness is, and how the marketplace model is written for it.
//!
//! [`profile`] names each [`Harness`] and holds one row of data per harness
//! (D-FDLQ). Everything a harness does differently that a value can say is a
//! field there; the rest is a `match` on the harness in the [`translator`]
//! and in [`manifest`], which holds each harness's manifest shapes.

pub(crate) mod field_types;
pub mod knowledge;
pub mod manifest;
pub mod profile;
pub mod translator;

pub use profile::{
    CliInvocation, Harness, HarnessProfile, KNOWN_TARGETS, PLUGIN_HOOKS_PATH, PluginFormat,
    SkillNaming,
};
pub(crate) use translator::Translator;
