//! The Skillwright CLI: exit codes, root-file configuration, per-run
//! vault/namespace resolution, report rendering, and the verbs themselves.
//!
//! `cli::run` is the seam the binary in `main.rs` and every test in this
//! crate drive; `cli::run_with` is the same entry with the
//! `ctx::CommandContext` supplied.
//!
//! # Stability
//!
//! This crate has no supported library API. Its modules are the CLI's own
//! and may change in any release; they are public only so the binary and
//! this crate's tests can reach them, and are hidden from these docs.

#[doc(hidden)]
pub mod cli;
#[doc(hidden)]
pub mod commands;
#[doc(hidden)]
pub mod config;
#[doc(hidden)]
pub mod ctx;
#[doc(hidden)]
pub mod exit;
#[doc(hidden)]
pub mod prompt;
#[doc(hidden)]
pub mod report;
#[doc(hidden)]
pub mod resolve;
