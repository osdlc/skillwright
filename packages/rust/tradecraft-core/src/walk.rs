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
use crate::vfs::{Vfs, VfsError};

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

/// What a build writes under its output folder, as path fragments a vault
/// walk leaves out: each harness's packs folder and marketplace manifest, and
/// the stage. The output can land inside the vault, and its built copies of the
/// vault's notes are not notes. Fragments
/// rather than whole folder names, so a plugin folder's own
/// `.claude-plugin/plugin.json` is still read.
pub fn built_paths() -> Vec<String> {
    let mut paths = vec![format!("{}/", crate::export::STAGE_DIR)];
    for profile in crate::harness::with_plugin_format() {
        let format = profile.format();
        paths.push(format!("{}/", format.packs));
        paths.extend(format.marketplace_manifest.map(str::to_owned));
    }
    paths
}

/// Whether vault-relative `rel` sits under a plugin folder's `assets/`.
/// Those files are copied as they are, and are not notes. A vault walk has no
/// hook for a folder found by shape, so each one filters its entries by this.
pub(crate) fn in_plugin_assets(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    parts[..parts.len() - 1]
        .windows(3)
        .any(|part| part[0] == "plugins" && part[2] == crate::model::load::ASSETS_DIR)
}

/// A vault walk's options. `Everything` keeps dot-prefixed names (it drops
/// only `.git`), and hides nothing.
///
/// [`vault_walk_errors`] repeats this walk's skips and prunes as a plain walk;
/// a change to what this walk enters must change that one too.
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
        exclude: built_paths(),
        ..vault::WalkOptions::default()
    }
}

/// A walk error as core's, with `refuse` making the error for a symlink.
fn core_error(err: WalkError, refuse: impl Fn(&Path) -> CoreError) -> CoreError {
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

/// Every file under `root` this walk can read, without failing, a plain walk:
/// a folder that cannot be listed is skipped, and so is each of `prune`.
#[cfg(test)]
pub(crate) fn readable_files(vfs: &dyn Vfs, root: &Path, prune: &[&str]) -> Vec<vfs_walk::Entry> {
    vfs_walk::walk(
        vfs,
        root,
        &plain_options(prune, Symlinks::List, Errors::Skip),
    )
    .unwrap_or_default()
}

/// The options of [`vault_files`]'s walk.
pub(crate) fn vault_walk(prune: &[&str]) -> vault::WalkOptions {
    vault_options(prune, false, true)
}

/// What [`vault_walk`]'s walk under `root` cannot read: each folder it
/// cannot list and each file it cannot stat, in walk order. A plain walk with
/// the same skips and prunes, so it meets the same folders.
pub(crate) fn vault_walk_errors(vfs: &dyn Vfs, root: &Path, prune: &[&str]) -> Vec<VfsError> {
    vfs_walk::walk_subtree(
        vfs,
        root,
        "",
        &plain_options(prune, Symlinks::List, Errors::Report),
    )
    .map_or_else(|_| Vec::new(), |outcome| outcome.errors)
}

/// The paths of a walk of `root`'s `entries`, plugin assets left out. Each
/// entry is matched by its path relative to `vault_root`, wherever the walk
/// started.
fn notes_and_resources(vault_root: &Path, root: &Path, entries: Vec<vault::Entry>) -> Vec<PathBuf> {
    // The walk root's own path from the vault root, once: empty for a walk of
    // the vault itself, the usual case, where an entry's path is already
    // vault-relative.
    let prefix = root
        .strip_prefix(vault_root)
        .map(crate::skill::forward_slashed)
        .unwrap_or_default();
    entries
        .into_iter()
        .filter(|entry| {
            if prefix.is_empty() {
                !in_plugin_assets(&entry.rel)
            } else {
                !in_plugin_assets(&format!("{prefix}/{}", entry.rel))
            }
        })
        .map(|entry| entry.abs)
        .collect()
}

/// Every file under the vault folder `root`. A symlinked file is listed like
/// any other.
pub(crate) fn vault_files(
    vfs: &dyn Vfs,
    root: &Path,
    prune: &[&str],
) -> Result<Vec<PathBuf>, CoreError> {
    vault::walk(vfs, root, &vault_walk(prune))
        .map(|entries| notes_and_resources(root, root, entries))
        .map_err(|err| core_error(err, not_refused))
}

/// Every file under `root` a vault walk meets, vault-relative, plugin assets
/// included, without failing. What a glob can name.
pub(crate) fn glob_files(vfs: &dyn Vfs, root: &Path) -> Vec<String> {
    vault::walk(vfs, root, &vault_options(&[], false, false))
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.rel)
        .collect()
}
