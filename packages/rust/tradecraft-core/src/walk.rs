//! Directory walks: plain file walks through `vfs-walk`, and vault walks
//! through `markdown-vault-fs`, the one vault walk the osdlc tools share.
//!
//! A plain walk (assets, scripts, a skill bundle, an adoption source, a
//! test's output tree) knows nothing about vaults. A vault walk (the skills
//! source, the vault-wide link index, a component's folder) goes through the
//! vault layer.
//!
//! Both keep every name, dot-prefixed ones included, because a skill bundle
//! may ship one and a link may name one. They skip `.DS_Store` (Finder drops
//! it everywhere and it is never content) and `.git`, and never enter a
//! folder named in `prune`. Entries come in name order within each folder,
//! depth first, so everything keyed off a walk is the same on every host. A
//! folder that cannot be listed, or a file that cannot be statted, fails the
//! walk: an unreadable corner of a skills source must not under-publish
//! silently.
//!
//! A symlinked folder is never entered. Whether a symlink is listed or
//! refused is the caller's choice.

use std::path::{Path, PathBuf};

use markdown_vault_fs::walk::{self as vault, Hide, Visibility};
use vfs_walk::{Errors, Symlinks, WalkError};

use crate::error::CoreError;
use crate::vfs::Vfs;

/// The names skipped wherever they appear. `.git` is also what a vault walk
/// under `Visibility::Everything` drops, so both walks see the same files.
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

/// A vault walk's options. `Everything` keeps dot-prefixed names (it drops
/// only `.git`), and hides nothing: a caller that honours the vault's
/// excluded files filters with [`excluded_files`].
fn vault_options(
    prune: &[&str],
    refuse_symlinks: bool,
    propagate_errors: bool,
) -> vault::WalkOptions {
    vault::WalkOptions {
        visibility: Visibility::Everything,
        notes: true,
        attachments: true,
        hide: Hide::Off,
        refuse_symlinks,
        propagate_errors,
        skip_names: vec![SKIPPED[0].to_owned()],
        prune_dirs: prune.iter().map(|name| (*name).to_owned()).collect(),
        ..vault::WalkOptions::default()
    }
}

/// A walk error as core's, with `refuse` making the error for a symlink.
pub(crate) fn core_error(err: WalkError, refuse: impl Fn(&Path) -> CoreError) -> CoreError {
    match err {
        WalkError::Io(err) => err.into(),
        WalkError::Symlink(path) => refuse(&path),
    }
}

/// For a walk that lists symlinks, so never returns [`WalkError::Symlink`]:
/// mapped rather than unwrapped.
fn not_refused(path: &Path) -> CoreError {
    CoreError::PathEscapesRoot(path.to_path_buf())
}

/// Every file under `root`, a plain walk. A symlinked file is listed like any
/// other.
pub(crate) fn files(vfs: &dyn Vfs, root: &Path, prune: &[&str]) -> Result<Vec<PathBuf>, CoreError> {
    vfs_walk::walk(
        vfs,
        root,
        &plain_options(prune, Symlinks::List, Errors::Propagate),
    )
    .map(|entries| entries.into_iter().map(|entry| entry.abs).collect())
    .map_err(|err| core_error(err, not_refused))
}

/// Every file under `root`, a plain walk, refusing a symlink anywhere in it
/// with the error `refuse` makes from its path.
pub(crate) fn files_refusing_symlinks(
    vfs: &dyn Vfs,
    root: &Path,
    prune: &[&str],
    refuse: impl Fn(&Path) -> CoreError,
) -> Result<Vec<vfs_walk::Entry>, CoreError> {
    vfs_walk::walk(
        vfs,
        root,
        &plain_options(prune, Symlinks::Refuse, Errors::Propagate),
    )
    .map_err(|err| core_error(err, refuse))
}

/// Every file under `root` this walk can read, without failing, a plain walk:
/// a folder that cannot be listed is skipped.
#[cfg(test)]
pub(crate) fn readable_files(vfs: &dyn Vfs, root: &Path) -> Vec<vfs_walk::Entry> {
    vfs_walk::walk(vfs, root, &plain_options(&[], Symlinks::List, Errors::Skip)).unwrap_or_default()
}

/// Every file under the vault folder `root`. A symlinked file is listed like
/// any other.
pub(crate) fn vault_files(
    vfs: &dyn Vfs,
    root: &Path,
    prune: &[&str],
) -> Result<Vec<PathBuf>, CoreError> {
    vault::walk(vfs, root, &vault_options(prune, false, true))
        .map(|entries| entries.into_iter().map(|entry| entry.abs).collect())
        .map_err(|err| core_error(err, not_refused))
}

/// The walk a skills source is read with, as a corpus walk: what
/// [`vault_files`] lists under `prune`, but a symlink anywhere in it fails the
/// walk instead of being listed.
pub(crate) fn source_walk(prune: &[&str]) -> vault::WalkOptions {
    vault_options(prune, true, true)
}

/// Every file under the vault folder `root` this walk can read, without
/// failing: a folder that cannot be listed is skipped.
pub(crate) fn readable_vault_files(vfs: &dyn Vfs, root: &Path) -> Vec<vault::Entry> {
    vault::walk(vfs, root, &vault_options(&[], false, false)).unwrap_or_default()
}

/// The vault's excluded files (Obsidian's `userIgnoreFilters`, read from
/// `.obsidian/app.json` under `vault_root`), as a test on a vault-relative
/// path. A missing or unreadable file excludes nothing.
pub(crate) fn excluded_files(vfs: &dyn Vfs, vault_root: &Path) -> impl Fn(&str) -> bool {
    let rules = markdown_vault_fs::hide::load(vfs, vault_root).rules;
    move |relative: &str| !rules.is_empty() && rules.hides_path(relative, false)
}
