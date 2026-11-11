//! A plain file walk through `vfs-walk`, for tests. What a build ships comes
//! from the layout walk ([`crate::layout::read_layout`]).
//!
//! A walk skips `.DS_Store` (Finder drops it everywhere and it is never
//! content) and `.git`, and never enters a folder named in `prune`. Entries
//! come in name order within each folder, depth first.

use std::path::Path;

use vfs_walk::{Errors, Symlinks};

use crate::vfs::Vfs;

/// The names skipped wherever they appear.
const SKIPPED: [&str; 2] = [".DS_Store", ".git"];

/// A plain walk's options.
fn plain_options(
    prune: &[&str],
    symlinks: Symlinks,
    errors: Errors,
) -> vfs_walk::WalkOptions<'static> {
    vfs_walk::WalkOptions {
        skip_names: SKIPPED.iter().map(|name| (*name).to_owned()).collect(),
        prune_dirs: prune.iter().map(|name| (*name).to_owned()).collect(),
        filter: None,
        symlinks,
        errors,
    }
}

/// Every file under `root` this walk can read, without failing, a plain walk:
/// a folder that cannot be listed is skipped, and so is each of `prune`.
pub(crate) fn readable_files(vfs: &dyn Vfs, root: &Path, prune: &[&str]) -> Vec<vfs_walk::Entry> {
    vfs_walk::walk(
        vfs,
        root,
        &plain_options(prune, Symlinks::List, Errors::Skip),
    )
    .unwrap_or_default()
}
