//! The filesystem seam — where this crate stops being pure.
//!
//! Everything above this module is arithmetic on strings and paths; everything
//! below it is a host's problem. That split is what lets one core serve both a
//! native shell and a `wasm32-unknown-unknown` build of itself: callers take a
//! `&dyn Vfs`, and the wasm bridge answers out of the host's file API instead
//! of `std::fs`.
//!
//! The seam itself lives in the `vfs-seam` crate, shared with the osdlc vault
//! crates (`markdown-vault-fs`, `markdown-vault`, `markdown-relink`), so a
//! vault walk and a publish read through one trait. This module re-exports
//! it under the paths this crate has always used. What it means to a publish
//! that [`PreviewVfs`] captures writes is [`crate::publish`]'s doc.
//!
//! A [`Vfs`] method fails with [`VfsError`]. A `?` in a function returning
//! [`crate::CoreError`] turns it into [`crate::CoreError::Io`], with the same
//! path and cause.

pub use vfs_seam::{
    Clock, DirEntry, FixedClock, MemVfs, Metadata, NativeVfs, PlannedOp, PreviewVfs, PreviewWrite,
    Vfs, VfsError, normalize,
};

#[cfg(test)]
use std::path::Path;

/// A `MemVfs` that fronts for a host whose canonicalize stayed at the
/// lexical default over a real filesystem — the wasm bridge's shape when the
/// host object has no link methods.
#[cfg(test)]
pub(crate) struct LinkBlindVfs(pub(crate) MemVfs);

#[cfg(test)]
impl Vfs for LinkBlindVfs {
    fn sees_links(&self) -> bool {
        false
    }
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        self.0.read_file(path)
    }
    fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<(), VfsError> {
        self.0.write_file(path, bytes)
    }
    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        self.0.create_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.0.exists(path)
    }
    fn metadata(&self, path: &Path) -> Result<Metadata, VfsError> {
        self.0.metadata(path)
    }
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        self.0.read_dir(path)
    }
    fn set_executable(&self, path: &Path, on: bool) -> Result<(), VfsError> {
        self.0.set_executable(path, on)
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.0.is_executable(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> Result<(), VfsError> {
        self.0.rename(from, to)
    }
    fn remove(&self, path: &Path) -> Result<(), VfsError> {
        self.0.remove(path)
    }
}
