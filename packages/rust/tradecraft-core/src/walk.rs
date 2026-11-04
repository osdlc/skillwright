//! Directory walks, through `markdown-vault-fs`: the one vault walk the osdlc
//! tools share.
//!
//! The walk keeps every name, dot-prefixed ones included, because a skill
//! bundle may ship one and a link may name one. It skips `.DS_Store` (Finder
//! drops it everywhere and it is never content) and never enters a folder
//! named in `prune`. Entries come in name order within each folder, depth
//! first, so everything keyed off a walk is the same on every host. A folder
//! that cannot be listed, or a file that cannot be statted, fails the walk:
//! an unreadable corner of a skills source must not under-publish silently.
//!
//! A symlinked folder is never entered. Whether a symlink is listed or
//! refused is the caller's choice.

use std::path::{Path, PathBuf};

use markdown_vault_fs::walk::{self as shared, Entry, Hide, Visibility, WalkError, WalkOptions};

use crate::error::CoreError;
use crate::vfs::Vfs;

/// The one name skipped wherever it appears.
const SKIPPED: &str = ".DS_Store";

/// The walk's options. `Everything` keeps dot-prefixed names (it drops only
/// `.git`), and hides nothing: a caller that honours the vault's excluded
/// files filters with [`excluded_files`].
fn options(prune: &[&str], refuse_symlinks: bool, propagate_errors: bool) -> WalkOptions {
    WalkOptions {
        visibility: Visibility::Everything,
        notes: true,
        attachments: true,
        hide: Hide::Off,
        refuse_symlinks,
        propagate_errors,
        skip_names: vec![SKIPPED.to_owned()],
        prune_dirs: prune.iter().map(|name| (*name).to_owned()).collect(),
        ..WalkOptions::default()
    }
}

/// Every file under `root`. A symlinked file is listed like any other.
pub(crate) fn files(vfs: &dyn Vfs, root: &Path, prune: &[&str]) -> Result<Vec<PathBuf>, CoreError> {
    shared::walk(vfs, root, &options(prune, false, true))
        .map(|entries| entries.into_iter().map(|entry| entry.abs).collect())
        .map_err(|err| match err {
            WalkError::Io(err) => err.into(),
            // Not asked for, so never returned; mapped rather than unwrapped.
            WalkError::Symlink(path) => CoreError::PathEscapesRoot(path),
        })
}

/// Every file under `root`, refusing a symlink anywhere in it with the error
/// `refuse` makes from its path.
pub(crate) fn files_refusing_symlinks(
    vfs: &dyn Vfs,
    root: &Path,
    prune: &[&str],
    refuse: impl Fn(&Path) -> CoreError,
) -> Result<Vec<Entry>, CoreError> {
    shared::walk(vfs, root, &options(prune, true, true)).map_err(|err| match err {
        WalkError::Io(err) => err.into(),
        WalkError::Symlink(path) => refuse(&path),
    })
}

/// Every file under `root` this walk can read, without failing: a folder that
/// cannot be listed is skipped.
pub(crate) fn readable_files(vfs: &dyn Vfs, root: &Path) -> Vec<Entry> {
    shared::walk(vfs, root, &options(&[], false, false)).unwrap_or_default()
}
