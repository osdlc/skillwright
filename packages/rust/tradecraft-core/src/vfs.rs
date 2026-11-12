//! The filesystem seam — where this crate stops being pure.
//!
//! Everything above this module is arithmetic on strings and paths; everything
//! below it is a host's problem. That split is what lets one core serve both a
//! native shell and a `wasm32-unknown-unknown` build of itself: callers take a
//! `&dyn Vfs`, and the wasm bridge answers out of the host's file API instead
//! of `std::fs`.
//!
//! The seam itself lives in the `vfs-seam` crate, shared with the osdlc vault
//! crates (`markdown-vault-fs`, `markdown-corpus`, `markdown-relink`), so a
//! vault walk and a build read through one trait. This module re-exports
//! it as `crate::vfs`.
//!
//! A [`Vfs`] method fails with [`VfsError`]. A `?` in a function returning
//! [`crate::CoreError`] turns it into [`crate::CoreError::Io`], with the same
//! path and cause.

pub use vfs_seam::{
    Clock, DirEntry, FixedClock, MemVfs, Metadata, NativeVfs, PlannedOp, PreviewVfs, PreviewWrite,
    Vfs, VfsError, normalize,
};
