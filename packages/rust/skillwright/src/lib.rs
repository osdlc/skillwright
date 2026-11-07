//! The Skillwright CLI: exit codes, root-file configuration, per-run
//! vault/namespace resolution, report rendering, and the verbs themselves.
//!
//! `cli::run` is the seam the binary in `main.rs` and every test in this
//! crate drive; `cli::run_with` is the same entry with the
//! `ctx::CommandContext` supplied.
//!
//! # Stability
//!
//! [`apply`] is the supported library API: it applies an authoring op that
//! `tradecraft-core` plans, relinking the vault. Every other module is the
//! CLI's own and may change in any release; they are public only so the
//! binary and this crate's tests can reach them, and are hidden from these
//! docs.
//!
//! Its types are `#[non_exhaustive]`: build an [`apply::Vault`] with
//! `Vault::new`, read an [`apply::Outcome`] by field, and give a `match` on
//! [`apply::Applied`] or [`apply::ApplyError`] a `_` arm.

pub mod apply;
#[doc(hidden)]
pub mod cli;
#[doc(hidden)]
pub mod commands;
#[doc(hidden)]
pub mod config;
#[doc(hidden)]
pub mod ctx;
#[doc(hidden)]
pub mod dirs;
#[doc(hidden)]
pub mod exit;
#[doc(hidden)]
pub mod git_source;
#[doc(hidden)]
pub mod prompt;
#[doc(hidden)]
pub mod report;
#[doc(hidden)]
pub mod resolve;
