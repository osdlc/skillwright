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

/// [`built_paths`] and each harness's packs folder as `config` places it.
pub fn built_paths_for(config: &crate::export::MarketplaceConfig) -> Vec<String> {
    let mut paths = built_paths();
    paths.extend(
        crate::harness::with_plugin_format()
            .map(|profile| format!("{}/", crate::harness::manifest::packs_of(config, profile))),
    );
    paths
}

/// Whether vault-relative `rel` sits under a plugin folder's `assets/`, by
/// convention or by pattern. Those files are copied as they are, and are not
/// notes. A vault walk has no hook for a folder found by shape, so each one
/// filters its entries by this.
fn in_plugin_assets(patterns: &crate::patterns::Patterns, rel: &str) -> bool {
    let rel = patterns.canonical(rel);
    let parts: Vec<&str> = rel.split('/').collect();
    parts[..parts.len() - 1]
        .windows(3)
        .any(|part| part[0] == "plugins" && part[2] == crate::model::load::ASSETS_DIR)
}

/// A vault walk's options. `Everything` keeps dot-prefixed names (it drops
/// only `.git`), and hides nothing: a caller that honours the vault's
/// excluded files filters with [`excluded_files`].
///
/// [`vault_walk_errors`] repeats this walk's skips and prunes as a plain walk;
/// a change to what this walk enters must change that one too.
fn vault_options(prune: &[&str], refuse_symlinks: bool, errors: Errors) -> vault::WalkOptions {
    vault::WalkOptions {
        visibility: Visibility::Everything,
        notes: true,
        attachments: true,
        hide: Hide::Off,
        refuse_symlinks,
        errors,
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
/// a folder that cannot be listed is skipped, and so is each of `prune`.
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
    vault_options(prune, false, Errors::Propagate)
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

/// The paths of a walk of `root`'s `entries`, plugin assets left out by the
/// patterns of the vault at `vault_root`: each entry is matched by its path
/// relative to the vault root, wherever the walk started.
fn notes_and_resources(
    patterns: &crate::patterns::Patterns,
    vault_root: &Path,
    root: &Path,
    entries: Vec<vault::Entry>,
) -> Vec<PathBuf> {
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
                !in_plugin_assets(patterns, &entry.rel)
            } else {
                !in_plugin_assets(patterns, &format!("{prefix}/{}", entry.rel))
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
    let patterns = crate::patterns::Patterns::load(vfs, root)?;
    vault::walk(vfs, root, &vault_walk(prune))
        .map(|entries| notes_and_resources(&patterns, root, root, entries))
        .map_err(|err| core_error(err, not_refused))
}

/// Every file under the folder `root` of the vault at `vault_root`, whose
/// patterns are `patterns`, refusing a symlink anywhere in it with the error
/// `refuse` makes from its path.
pub(crate) fn vault_files_refusing_symlinks(
    vfs: &dyn Vfs,
    root: &Path,
    patterns: &crate::patterns::Patterns,
    vault_root: &Path,
    prune: &[&str],
    refuse: impl Fn(&Path) -> CoreError,
) -> Result<Vec<PathBuf>, CoreError> {
    vault::walk(vfs, root, &vault_options(prune, true, Errors::Propagate))
        .map(|entries| notes_and_resources(patterns, vault_root, root, entries))
        .map_err(|err| core_error(err, refuse))
}

/// Every file under `root` a vault walk meets, vault-relative, plugin assets
/// included, without failing. What a glob can name.
pub(crate) fn glob_files(vfs: &dyn Vfs, root: &Path) -> Vec<String> {
    vault::walk(vfs, root, &vault_options(&[], false, Errors::Skip))
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.rel)
        .collect()
}

/// Every file under the folder `root` of the vault at `vault_root` this walk
/// can read, without failing: a folder that cannot be listed is skipped.
/// Plugin assets are left out, matched by their path from the vault root.
pub(crate) fn readable_vault_files(vfs: &dyn Vfs, root: &Path, vault_root: &Path) -> Vec<PathBuf> {
    let entries =
        vault::walk(vfs, root, &vault_options(&[], false, Errors::Skip)).unwrap_or_default();
    // Without failing, as promised: a root file that cannot be read keeps
    // every entry, which lists more than it should rather than less.
    let patterns = crate::patterns::Patterns::load(vfs, vault_root).unwrap_or_default();
    notes_and_resources(&patterns, vault_root, root, entries)
}

/// The vault's excluded files (Obsidian's `userIgnoreFilters`, read from
/// `.obsidian/app.json` under `vault_root`), as a test on a vault-relative
/// path. A missing or unreadable file excludes nothing.
pub(crate) fn excluded_files(vfs: &dyn Vfs, vault_root: &Path) -> impl Fn(&str) -> bool {
    let rules = markdown_vault_fs::hide::load(vfs, vault_root).rules;
    move |relative: &str| !rules.is_empty() && rules.hides_path(relative, false)
}
