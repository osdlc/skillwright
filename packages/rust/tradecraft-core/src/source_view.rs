//! The vault as a build reads it: a [`Vfs`] that hides generated output and
//! answers for the root file the run named.
//!
//! Generated output must never be read as source, and the output can sit in
//! the vault. Hiding it here covers every reader at once (discovery, the link
//! index, a bundle's sweep) without any of them knowing where the output is,
//! and the same goes for `--config`, which only the host knows about.
//!
//! Writes pass straight through. Whoever compares or replaces the output uses
//! the [`Vfs`] underneath, which still sees it.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use crate::discover::{DiscoveredComponent, Source, discover_in};
use crate::error::CoreError;
use crate::export::{MarketplaceConfig, STAGE_DIR};
use crate::frontmatter::EntityKind;
use crate::harness::manifest::{check_packs, check_packs_against_sources, packs_of};
use crate::harness::with_plugin_format;
use crate::model::cascade::Cascade;
use crate::model::load::ASSETS_DIR;
use crate::namespace::Namespace;
use crate::patterns::Patterns;
use crate::settings::ROOT_FILE;
use crate::vfs::{DirEntry, Metadata, Vfs, VfsError, normalize};

/// `inner`, less the hidden paths, with `<vault>/skillwright.md` standing for
/// the root file in use.
pub struct SourceView<'v> {
    inner: &'v dyn Vfs,
    /// `<vault>/skillwright.md`, and the file that answers for it.
    root_file: Option<(PathBuf, PathBuf)>,
    hidden: Vec<PathBuf>,
}

impl<'v> SourceView<'v> {
    /// A view that changes nothing yet.
    pub fn over(inner: &'v dyn Vfs) -> Self {
        Self {
            inner,
            root_file: None,
            hidden: Vec::new(),
        }
    }

    /// Reads `file` wherever `vault_root`'s root file is asked for.
    #[must_use]
    pub fn with_root_file(mut self, vault_root: &Path, file: &Path) -> Self {
        self.root_file = Some((normalize(&vault_root.join(ROOT_FILE)), file.to_path_buf()));
        self
    }

    /// Hides each of `paths` and all it holds.
    #[must_use]
    pub fn hiding(mut self, paths: Vec<PathBuf>) -> Self {
        self.hidden.extend(paths.iter().map(|path| normalize(path)));
        self
    }

    fn is_hidden(&self, path: &Path) -> bool {
        !self.hidden.is_empty() && {
            let path = normalize(path);
            self.hidden.iter().any(|hidden| path.starts_with(hidden))
        }
    }

    /// The file that answers for `path`.
    fn answering<'p>(&'p self, path: &'p Path) -> &'p Path {
        match &self.root_file {
            Some((named, file)) if normalize(path) == *named => file,
            _ => path,
        }
    }
}

fn not_found(path: &Path) -> VfsError {
    VfsError::io(path, io::Error::from(io::ErrorKind::NotFound))
}

impl Vfs for SourceView<'_> {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        if self.is_hidden(path) {
            return Err(not_found(path));
        }
        self.inner.read_file(self.answering(path))
    }
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        if self.is_hidden(path) {
            return Err(not_found(path));
        }
        let mut entries = self.inner.read_dir(path)?;
        entries.retain(|entry| !self.is_hidden(&path.join(&entry.name)));
        Ok(entries)
    }
    fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<(), VfsError> {
        self.inner.write_file(path, bytes)
    }
    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        self.inner.create_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        !self.is_hidden(path) && self.inner.exists(self.answering(path))
    }
    fn metadata(&self, path: &Path) -> Result<Metadata, VfsError> {
        if self.is_hidden(path) {
            return Err(not_found(path));
        }
        self.inner.metadata(self.answering(path))
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.inner.is_executable(path)
    }
    fn set_executable(&self, path: &Path, yes: bool) -> Result<(), VfsError> {
        self.inner.set_executable(path, yes)
    }
    fn rename(&self, from: &Path, to: &Path) -> Result<(), VfsError> {
        self.inner.rename(from, to)
    }
    fn remove(&self, path: &Path) -> Result<(), VfsError> {
        self.inner.remove(path)
    }
    fn is_symlink(&self, path: &Path) -> bool {
        self.inner.is_symlink(path)
    }
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, VfsError> {
        self.inner.canonicalize(path)
    }
    fn sees_links(&self) -> bool {
        self.inner.sees_links()
    }
    fn read_link(&self, path: &Path) -> Result<PathBuf, VfsError> {
        self.inner.read_link(path)
    }
}

/// What a run says about which marketplace it reads.
#[derive(Debug, Clone, Copy)]
pub struct VaultScope<'a> {
    /// The vault's root.
    pub vault_root: &'a Path,
    /// The skills source, when it is not the vault.
    pub skills_root: Option<&'a Path>,
    /// Which marketplace, when the run names one.
    pub marketplace: Option<&'a str>,
}

/// `config` with the harness blocks the cascade gives the marketplace: every
/// source of a `packs` value, resolved. A build, and anything else that asks
/// where a harness's packs are, starts here, so all of them hold the one
/// value. Only the blocks: the load folds the note's other fields itself.
///
/// With no note, or several and none named, `config` stands.
pub fn effective_config(
    vfs: &dyn Vfs,
    scope: VaultScope<'_>,
    config: &MarketplaceConfig,
    ns: &Namespace,
) -> MarketplaceConfig {
    let mut config = config.clone();
    if let Some(note) = marketplace_note(vfs, scope, ns) {
        let name = note.name.clone();
        config.blocks = Cascade::over(&[note], Some(&name), &config).config.blocks;
    }
    config
}

/// The note of the marketplace `scope` reads. Only the marketplace folders are
/// read, so it can be found before the vault's output is known.
fn marketplace_note(
    vfs: &dyn Vfs,
    scope: VaultScope<'_>,
    ns: &Namespace,
) -> Option<DiscoveredComponent> {
    let root = normalize(scope.skills_root.unwrap_or(scope.vault_root));
    let folder = root.join("marketplaces");
    let mut files = Vec::new();
    for entry in vfs.read_dir(&folder).ok()? {
        let path = folder.join(&entry.name);
        if entry.is_dir {
            let inside = vfs.read_dir(&path).unwrap_or_default();
            files.extend(
                inside
                    .iter()
                    .filter(|file| !file.is_dir)
                    .map(|file| path.join(&file.name)),
            );
        } else {
            files.push(path);
        }
    }
    let source = Source { root, files };
    let notes = discover_in(vfs, scope.vault_root, &source, ns).ok()?;
    let mut notes: Vec<DiscoveredComponent> = notes
        .components
        .into_iter()
        .filter(|component| component.kind == EntityKind::Marketplace)
        .collect();
    match scope.marketplace {
        Some(name) => notes.into_iter().find(|note| note.name == name),
        None if notes.len() == 1 => notes.pop(),
        None => None,
    }
}

/// `path` with its links resolved, as far as it exists.
fn resolved(vfs: &dyn Vfs, path: &Path) -> PathBuf {
    let path = normalize(path);
    vfs.canonicalize(&path).unwrap_or(path)
}

/// Whether the output and the vault share a folder: one holds the other.
pub(crate) fn output_overlaps_vault(vfs: &dyn Vfs, vault_root: &Path, out_root: &Path) -> bool {
    let (vault, out) = (resolved(vfs, vault_root), resolved(vfs, out_root));
    vault.starts_with(&out) || out.starts_with(&vault)
}

/// What a build owns under `out_root`, for each harness, that sits in `root`,
/// spelled from `root`.
fn output_in(
    vfs: &dyn Vfs,
    root: &Path,
    out_root: &Path,
    config: &MarketplaceConfig,
) -> Vec<PathBuf> {
    let (inside, out) = (resolved(vfs, root), resolved(vfs, out_root));
    with_plugin_format()
        .flat_map(|profile| profile.owned_paths_at(packs_of(config, profile)))
        .chain([STAGE_DIR])
        .filter_map(|owned| {
            let path = normalize(&out.join(owned));
            path.strip_prefix(&inside)
                .ok()
                .map(|rest| normalize(root).join(rest))
        })
        .collect()
}

/// [`output_in`] over the vault and the skills source.
pub(crate) fn output_paths(
    vfs: &dyn Vfs,
    scope: VaultScope<'_>,
    out_root: &Path,
    config: &MarketplaceConfig,
) -> Vec<PathBuf> {
    let mut paths = output_in(vfs, scope.vault_root, out_root, config);
    if let Some(root) = scope.skills_root {
        paths.extend(output_in(vfs, root, out_root, config));
    }
    paths
}

/// Every check on `config`'s `packs` that needs no destination on disk:
/// [`check_packs`], and, when the output overlaps the vault, that a moved
/// folder is clear of the folders the vault reads notes from.
///
/// # Errors
///
/// As [`check_packs`] and [`check_packs_against_sources`].
pub(crate) fn check_pack_location(
    vfs: &dyn Vfs,
    vault_root: &Path,
    out_root: &Path,
    config: &MarketplaceConfig,
    patterns: &Patterns,
) -> Result<(), CoreError> {
    check_packs(config)?;
    if !output_overlaps_vault(vfs, vault_root, out_root) {
        return Ok(());
    }
    let mut sources: BTreeSet<String> = patterns.source_folders();
    sources.extend(
        crate::discover::KIND_DIRS
            .iter()
            .map(|(dir, _)| (*dir).to_owned()),
    );
    sources.extend([ASSETS_DIR.to_owned(), ROOT_FILE.to_owned()]);
    let (vault, out) = (resolved(vfs, vault_root), resolved(vfs, out_root));
    check_packs_against_sources(config, &sources, |packs| {
        normalize(&out.join(packs))
            .strip_prefix(&vault)
            .ok()
            .map(crate::skill::forward_slashed)
    })
}

/// The view of the vault for a run that does not build: `base`, less the
/// output at `out_root`. A `packs` a build would refuse hides nothing, so the
/// run sees the vault as it is and the build says what is wrong.
pub fn source_view<'v>(
    base: SourceView<'v>,
    scope: VaultScope<'_>,
    out_root: Option<&Path>,
    config: &MarketplaceConfig,
    ns: &Namespace,
) -> SourceView<'v> {
    let Some(out_root) = out_root else {
        return base;
    };
    let config = effective_config(&base, scope, config, ns);
    let patterns = Patterns::read(&base, scope.vault_root);
    if check_pack_location(&base, scope.vault_root, out_root, &config, &patterns).is_err() {
        return base;
    }
    let paths = output_paths(&base, scope, out_root, &config);
    base.hiding(paths)
}
