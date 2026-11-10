//! The vault as a build reads it: a [`Vfs`] that hides generated output and
//! answers for the root file the run named.
//!
//! Generated output must never be read as notes, and the output can sit in
//! the vault. [`notes_view`] hides every output the vault's root file
//! configures, and every core function that reads notes reads through it, so
//! no reader (discovery, the link index, a bundle's sweep) needs to know where
//! the output is. A host says which root file is in use (`--config`) and where
//! `~` leads, through [`SourceView::with_root_file`] and
//! [`SourceView::with_home`].
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
use crate::guard::{real_path, within};
use crate::harness::manifest::{
    check_packs, check_packs_against_sources, first_not_a_pack, packs_of,
};
use crate::harness::{HarnessProfile, with_plugin_format};
use crate::model::cascade::Cascade;
use crate::model::load::ASSETS_DIR;
use crate::namespace::Namespace;
use crate::patterns::Patterns;
use crate::settings::{ROOT_FILE, RootFile};
use crate::vfs::{DirEntry, Metadata, Vfs, VfsError, normalize};

/// `inner`, less the hidden paths, with `<vault>/skillwright.md` standing for
/// the root file in use.
pub struct SourceView<'v> {
    inner: &'v dyn Vfs,
    /// `<vault>/skillwright.md`, and the file that answers for it.
    root_file: Option<(PathBuf, PathBuf)>,
    hidden: Vec<PathBuf>,
    /// What `~` means in a root file's `build.out`.
    home: Option<PathBuf>,
}

impl<'v> SourceView<'v> {
    /// A view that changes nothing yet.
    pub fn over(inner: &'v dyn Vfs) -> Self {
        Self {
            inner,
            root_file: None,
            hidden: Vec::new(),
            home: None,
        }
    }

    /// Resolves a `~` in a root file's `build.out` against `home`, so
    /// [`notes_view`] hides that output. It holds for a view built over this
    /// one too, as every core function builds one over the `vfs` it is given.
    /// Without a home, a `~` output is not hidden.
    #[must_use]
    pub fn with_home(mut self, home: &Path) -> Self {
        self.home = Some(home.to_path_buf());
        self
    }

    /// Reads `file` wherever `vault_root`'s root file is asked for.
    #[must_use]
    pub fn with_root_file(mut self, vault_root: &Path, file: &Path) -> Self {
        self.root_file = Some((normalize(&vault_root.join(ROOT_FILE)), file.to_path_buf()));
        self
    }

    /// Hides each of `paths` and all it holds, beside what it hides already.
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

/// The path a [`SourceView`] answers with its home when asked to
/// canonicalize it. The [`Vfs`] trait has no way to ask for a home, and a view
/// over the host's view must still find it. The path holds a NUL, so a native
/// filesystem refuses it and a lexical one leaves it relative. A JS host's
/// `realpath` receives it unchanged; wasm sets no home, and [`home_of`]
/// takes no answer there. Replace this with a home method on [`Vfs`] once
/// `vfs-seam` has one.
const HOME_PROBE: &str = "\0home";

/// The home a [`SourceView`] in `vfs`, at any depth, was given. A relative
/// answer is none; on wasm32 `is_absolute` is always false, so every answer
/// is.
fn home_of(vfs: &dyn Vfs) -> Option<PathBuf> {
    vfs.canonicalize(Path::new(HOME_PROBE))
        .ok()
        .filter(|home| home.is_absolute())
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
        match &self.home {
            Some(home) if path == Path::new(HOME_PROBE) => Ok(home.clone()),
            _ => self.inner.canonicalize(path),
        }
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
#[non_exhaustive]
pub struct VaultScope<'a> {
    /// The vault's root.
    pub vault_root: &'a Path,
    /// The skills source, when it is not the vault.
    pub skills_root: Option<&'a Path>,
    /// Which marketplace, when the run names one.
    pub marketplace: Option<&'a str>,
}

impl<'a> VaultScope<'a> {
    /// The vault at `vault_root`, its own skills source, no marketplace
    /// named. Set the other fields after.
    pub fn new(vault_root: &'a Path) -> Self {
        Self {
            vault_root,
            skills_root: None,
            marketplace: None,
        }
    }
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
    let notes = marketplace_notes(vfs, scope, ns);
    match &notes[..] {
        [note] => with_note(config, note),
        _ => config.clone(),
    }
}

/// [`effective_config`] for a run that may touch any marketplace: one config
/// for each one `scope` allows, so several notes each give their own `packs`.
/// With no note, `config` stands.
pub fn effective_configs(
    vfs: &dyn Vfs,
    scope: VaultScope<'_>,
    config: &MarketplaceConfig,
    ns: &Namespace,
) -> Vec<MarketplaceConfig> {
    let notes = marketplace_notes(vfs, scope, ns);
    if notes.is_empty() {
        return vec![config.clone()];
    }
    notes.iter().map(|note| with_note(config, note)).collect()
}

/// `config` with the harness blocks of the marketplace `note`.
fn with_note(config: &MarketplaceConfig, note: &DiscoveredComponent) -> MarketplaceConfig {
    let mut config = config.clone();
    config.blocks = Cascade::over(std::slice::from_ref(note), Some(note.folder()), &config)
        .config
        .blocks;
    config
}

/// The folder of the marketplace published as `name`, for a caller that holds
/// a built manifest's name. A marketplace's `name` need not match its folder.
pub fn marketplace_folder(
    vfs: &dyn Vfs,
    scope: VaultScope<'_>,
    name: &str,
    ns: &Namespace,
) -> Option<String> {
    let every = VaultScope {
        marketplace: None,
        ..scope
    };
    marketplace_notes(vfs, every, ns)
        .iter()
        .find(|note| note.name == name)
        .map(|note| note.folder().to_owned())
}

/// The notes of the marketplaces `scope` reads: the one whose folder it names,
/// else all of them. Only the notes are read, so they can be found before the
/// vault's output is known.
fn marketplace_notes(
    vfs: &dyn Vfs,
    scope: VaultScope<'_>,
    ns: &Namespace,
) -> Vec<DiscoveredComponent> {
    let root = normalize(scope.skills_root.unwrap_or(scope.vault_root));
    let folder = root.join(crate::paths::WORKSPACE_DIR);
    let mut files = Vec::new();
    for entry in vfs.read_dir(&folder).unwrap_or_default() {
        let note = folder.join(&entry.name).join(crate::discover::entry_file(
            EntityKind::Marketplace,
            &entry.name,
        ));
        if entry.is_dir && vfs.exists(&note) {
            files.push(note);
        }
    }
    let source = Source { root, files };
    let Ok(patterns) = Patterns::load(vfs, scope.vault_root) else {
        return Vec::new();
    };
    let Ok(notes) = discover_in(vfs, scope.vault_root, &patterns, &source, ns) else {
        return Vec::new();
    };
    notes
        .components
        .into_iter()
        .filter(|component| component.kind == EntityKind::Marketplace)
        .filter(|note| scope.marketplace.is_none_or(|name| note.folder() == name))
        .collect()
}

/// What a build owns under `out_root`, for each harness, that sits in `root`,
/// spelled from `root`. `keep` says whether a packs folder counts.
///
/// A path the host cannot resolve is taken as written: this builds a list to
/// hide, and the build refuses such a path on its own.
fn output_in(
    vfs: &dyn Vfs,
    root: &Path,
    out_root: &Path,
    config: &MarketplaceConfig,
    keep: &dyn Fn(&HarnessProfile, &Path) -> bool,
) -> Vec<PathBuf> {
    let resolved = |path: &Path| real_path(vfs, path).unwrap_or_else(|_| normalize(path));
    let (inside, out) = (resolved(root), resolved(out_root));
    with_plugin_format()
        .flat_map(|profile| {
            let packs = packs_of(config, profile);
            let mut owned = profile.owned_paths_at(packs);
            owned.retain(|path| *path != packs || keep(profile, &out.join(packs)));
            owned
        })
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
    output_where(vfs, scope, out_root, config, &|_, _| true)
}

fn output_where(
    vfs: &dyn Vfs,
    scope: VaultScope<'_>,
    out_root: &Path,
    config: &MarketplaceConfig,
    keep: &dyn Fn(&HarnessProfile, &Path) -> bool,
) -> Vec<PathBuf> {
    let mut paths = output_in(vfs, scope.vault_root, out_root, config, keep);
    if let Some(root) = scope.skills_root {
        paths.extend(output_in(vfs, root, out_root, config, keep));
    }
    paths
}

/// Whether the packs folder at `folder` holds nothing but packs, each with its
/// plugin manifest. A default folder always counts, and so does one with
/// nothing to list. Used to hide output, not to replace it, so it asks for no
/// marker: a commands-only pack carries none.
fn holds_packs(
    vfs: &dyn Vfs,
    profile: &HarnessProfile,
    config: &MarketplaceConfig,
    folder: &Path,
) -> bool {
    if packs_of(config, profile) == profile.format().packs {
        return true;
    }
    let Ok(files) = crate::walk::files(vfs, folder, &[]) else {
        return true;
    };
    let files: Vec<String> = files
        .iter()
        .filter_map(|file| file.strip_prefix(folder).ok())
        .map(|file| format!("packs/{}", crate::skill::forward_slashed(file)))
        .collect();
    let manifest = profile.format().plugin_manifest;
    first_not_a_pack(
        "packs",
        manifest,
        files.iter().map(String::as_str),
        |_| false,
        |_| true,
    )
    .is_none()
}

/// Every check on `config`'s `packs` that needs no destination on disk:
/// [`check_packs`], and, when the output and the vault share a folder (one
/// holds the other, once links are resolved), that a moved folder is clear of
/// the folders the vault reads notes from.
///
/// # Errors
///
/// As [`check_packs`] and [`check_packs_against_sources`], and
/// `refused: unsafe-path` when either path is relative or the host cannot
/// tell where it leads.
pub(crate) fn check_pack_location(
    vfs: &dyn Vfs,
    vault_root: &Path,
    out_root: &Path,
    config: &MarketplaceConfig,
    patterns: &Patterns,
) -> Result<(), CoreError> {
    check_packs(config)?;
    if !within(vfs, vault_root, out_root)? && !within(vfs, out_root, vault_root)? {
        return Ok(());
    }
    let mut sources: BTreeSet<String> = patterns.source_folders();
    sources.extend(
        crate::discover::KIND_DIRS
            .iter()
            .map(|(dir, _)| (*dir).to_owned()),
    );
    sources.extend([ASSETS_DIR.to_owned(), ROOT_FILE.to_owned()]);
    let (vault, out) = (real_path(vfs, vault_root)?, real_path(vfs, out_root)?);
    check_packs_against_sources(config, &sources, |packs| {
        normalize(&out.join(packs))
            .strip_prefix(&vault)
            .ok()
            .map(crate::skill::forward_slashed)
    })
}

/// What the root file at `vault_root` configures, for a caller that holds no
/// configuration of its own, over what a workspace file above it sets.
/// Default when there is no root file, or when it holds a bad value a build
/// would refuse: a build says what is wrong with it. A workspace file with a
/// bad value is left out the same way.
///
/// # Errors
///
/// `refused: malformed-root-file` when either file cannot say what it
/// excludes, as [`crate::patterns::Patterns::load`] refuses it: the scan this
/// configures would refuse it too.
pub fn vault_config(vfs: &dyn Vfs, vault_root: &Path) -> Result<MarketplaceConfig, CoreError> {
    use crate::paths::{read_root_both, workspace_at};
    let Some((layout, full)) = read_root_both(vfs, &vault_root.join(ROOT_FILE))? else {
        return Ok(MarketplaceConfig::default());
    };
    let shared = match workspace_at(vfs, vault_root).filter(|_| !layout.workspace) {
        Some(at) => read_root_both(vfs, &at.file)?.filter(|(layout, _)| layout.workspace),
        None => None,
    };
    let Ok(mut root) = full else {
        return Ok(MarketplaceConfig::default());
    };
    if let Some((_, Ok(shared))) = shared {
        root.inherit(&shared);
    }
    Ok(root.to_config(""))
}

/// `base`, also less the output at `out_root`, for each marketplace the run
/// may touch: for a host whose output folder the root file does not configure
/// ([`notes_view`] hides the one it does). A
/// `packs` a build would refuse hides nothing, so the run sees the vault as it
/// is and the build says what is wrong, and a moved folder that holds more
/// than packs stays visible.
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
    let paths = output_to_hide(&base, scope, None, out_root, config, ns);
    base.hiding(paths)
}

/// What [`source_view`] hides, and what of it also sits in `workspace`.
fn output_to_hide(
    vfs: &dyn Vfs,
    scope: VaultScope<'_>,
    workspace: Option<&Path>,
    out_root: &Path,
    config: &MarketplaceConfig,
    ns: &Namespace,
) -> Vec<PathBuf> {
    // A root file that cannot be read hides nothing, as a `packs` a build
    // would refuse: the scan or build that reads through this view refuses it.
    let Ok(patterns) = Patterns::load(vfs, scope.vault_root) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for config in effective_configs(vfs, scope, config, ns) {
        if check_pack_location(vfs, scope.vault_root, out_root, &config, &patterns).is_ok() {
            let keep = |profile: &HarnessProfile, folder: &Path| {
                holds_packs(vfs, profile, &config, folder)
            };
            paths.extend(output_where(vfs, scope, out_root, &config, &keep));
            if let Some(root) = workspace {
                paths.extend(output_in(vfs, root, out_root, &config, &keep));
            }
        }
    }
    paths
}

/// `base`, less every build output the vault's root file configures: its own
/// `build.out`, or at a workspace root each member's. `config`, when given,
/// says where the packs folders are in place of the root file's. Writes pass
/// through. A root file that cannot be read hides nothing: the scan that
/// reads through this view refuses it.
///
/// The root file is read through `base`, so a view that answers for it with
/// another file ([`SourceView::with_root_file`]) hides that file's output. A
/// `~` in `build.out` resolves against [`SourceView::with_home`]; with no
/// home, that output is not hidden.
pub fn notes_view<'v>(
    base: SourceView<'v>,
    scope: VaultScope<'_>,
    config: Option<&MarketplaceConfig>,
    ns: &Namespace,
) -> SourceView<'v> {
    let Some(root) = root_at(&base, scope.vault_root) else {
        return base;
    };
    let mut paths = Vec::new();
    if root.workspace {
        // A workspace file's own `build.out` builds nothing: each member
        // sets its own.
        for file in crate::paths::workspace_members(&base, scope.vault_root) {
            let Some(dir) = file.parent() else { continue };
            if let Some(member) = root_at(&base, dir) {
                let config = member.to_config(&folder_name(dir));
                paths.extend(configured_output(
                    &base,
                    VaultScope::new(dir),
                    Some(scope.vault_root),
                    &member,
                    &config,
                    ns,
                ));
            }
        }
    } else {
        let own;
        let config = match config {
            Some(config) => config,
            None => {
                own = root.to_config(&folder_name(scope.vault_root));
                &own
            }
        };
        paths = configured_output(&base, scope, None, &root, config, ns);
    }
    base.hiding(paths)
}

/// [`notes_view`] over `vfs` for the vault at `vault_root`, its skills source
/// at `skills_root`.
pub(crate) fn notes_over<'v>(
    vfs: &'v dyn Vfs,
    vault_root: &Path,
    skills_root: Option<&Path>,
    config: Option<&MarketplaceConfig>,
    ns: &Namespace,
) -> SourceView<'v> {
    let mut scope = VaultScope::new(vault_root);
    scope.skills_root = skills_root;
    notes_view(SourceView::over(vfs), scope, config, ns)
}

/// The root file at `vault_root`, with the workspace above it folded in.
/// `None` when there is none or it cannot be read.
fn root_at(vfs: &dyn Vfs, vault_root: &Path) -> Option<RootFile> {
    let file = vault_root.join(ROOT_FILE);
    if !vfs.exists(&file) {
        return None;
    }
    crate::paths::load_effective_root(vfs, vault_root, Some(&file))
        .ok()
        .map(|loaded| loaded.root)
}

fn folder_name(dir: &Path) -> String {
    dir.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What a build into `root`'s `build.out` owns in the vault `scope` reads,
/// or in the `workspace` that holds it.
fn configured_output(
    base: &SourceView<'_>,
    scope: VaultScope<'_>,
    workspace: Option<&Path>,
    root: &RootFile,
    config: &MarketplaceConfig,
    ns: &Namespace,
) -> Vec<PathBuf> {
    let Some(out) = root.build.out.as_deref() else {
        return Vec::new();
    };
    // Ask for the home only for a `~` out: the probe reaches the host's
    // `canonicalize`.
    let home = if out == "~" || out.starts_with("~/") {
        match home_of(base) {
            Some(home) => home,
            None => return Vec::new(),
        }
    } else {
        PathBuf::new()
    };
    let out_root = crate::paths::resolve_path(out, scope.vault_root, &home);
    output_to_hide(base, scope, workspace, &out_root, config, ns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::MemVfs;

    const BUILT: &str = ".claude-plugin/marketplace.json";

    fn vault(files: &[(&str, &str)]) -> MemVfs {
        let vfs = MemVfs::new();
        for (path, text) in files {
            vfs.insert(path, text).unwrap();
        }
        vfs
    }

    fn notes<'v>(base: SourceView<'v>, vault_root: &str) -> SourceView<'v> {
        notes_view(
            base,
            VaultScope::new(Path::new(vault_root)),
            None,
            &Namespace::bare(),
        )
    }

    #[test]
    fn notes_view_hides_the_configured_out() {
        let vfs = vault(&[
            (
                "/v/skillwright.md",
                "---\nname: v\nbuild:\n  out: dist\n---\n",
            ),
            ("/v/skills/a.md", "---\ndescription: A.\n---\n"),
            (&format!("/v/dist/{BUILT}"), "{}"),
        ]);
        let view = notes(SourceView::over(&vfs), "/v");
        assert!(!view.exists(&Path::new("/v/dist").join(BUILT)));
        assert!(view.exists(Path::new("/v/skills/a.md")));
    }

    #[test]
    fn notes_view_reads_the_root_file_the_view_answers_with() {
        let vfs = vault(&[
            (
                "/v/skillwright.md",
                "---\nname: v\nbuild:\n  out: dist\n---\n",
            ),
            ("/v/other.md", "---\nname: v\nbuild:\n  out: other\n---\n"),
            (&format!("/v/dist/{BUILT}"), "{}"),
            (&format!("/v/other/{BUILT}"), "{}"),
        ]);
        let base = SourceView::over(&vfs).with_root_file(Path::new("/v"), Path::new("/v/other.md"));
        let view = notes(base, "/v");
        assert!(!view.exists(&Path::new("/v/other").join(BUILT)));
        assert!(view.exists(&Path::new("/v/dist").join(BUILT)));
    }

    #[test]
    fn notes_view_hides_each_workspace_member_s_out() {
        let vfs = vault(&[
            ("/w/skillwright.md", "---\nworkspace: true\n---\n"),
            (
                "/w/marketplaces/a/skillwright.md",
                "---\nname: a\nbuild:\n  out: dist\n---\n",
            ),
            (
                "/w/marketplaces/b/skillwright.md",
                "---\nname: b\nbuild:\n  out: built\n---\n",
            ),
            (&format!("/w/marketplaces/a/dist/{BUILT}"), "{}"),
            (&format!("/w/marketplaces/b/built/{BUILT}"), "{}"),
        ]);
        let view = notes(SourceView::over(&vfs), "/w");
        assert!(!view.exists(&Path::new("/w/marketplaces/a/dist").join(BUILT)));
        assert!(!view.exists(&Path::new("/w/marketplaces/b/built").join(BUILT)));
        assert!(view.exists(Path::new("/w/marketplaces/a/skillwright.md")));
    }

    #[test]
    fn notes_view_hides_a_member_s_out_outside_its_folder() {
        let vfs = vault(&[
            ("/w/skillwright.md", "---\nworkspace: true\n---\n"),
            (
                "/w/marketplaces/a/skillwright.md",
                "---\nname: a\nbuild:\n  out: ../../dist/a\n---\n",
            ),
            (&format!("/w/dist/a/{BUILT}"), "{}"),
        ]);
        let view = notes(SourceView::over(&vfs), "/w");
        assert!(!view.exists(&Path::new("/w/dist/a").join(BUILT)));
    }

    #[test]
    fn notes_view_at_a_workspace_root_ignores_its_own_out() {
        let vfs = vault(&[
            (
                "/w/skillwright.md",
                "---\nworkspace: true\nbuild:\n  out: dist\n---\n",
            ),
            (&format!("/w/dist/{BUILT}"), "{}"),
            ("/w/dist/skills/a.md", "---\ndescription: A.\n---\n"),
        ]);
        let view = notes(SourceView::over(&vfs), "/w");
        assert!(view.exists(&Path::new("/w/dist").join(BUILT)));
        assert!(view.exists(Path::new("/w/dist/skills/a.md")));
    }

    #[test]
    fn notes_view_with_an_unreadable_root_file_hides_nothing() {
        let vfs = vault(&[
            (
                "/v/skillwright.md",
                "---\nbuild:\n  out: dist\nworkspace: [\n---\n",
            ),
            (&format!("/v/dist/{BUILT}"), "{}"),
        ]);
        let view = notes(SourceView::over(&vfs), "/v");
        assert!(view.exists(&Path::new("/v/dist").join(BUILT)));
    }

    #[test]
    fn notes_view_resolves_a_home_out_only_with_home() {
        let vfs = vault(&[
            (
                "/h/v/skillwright.md",
                "---\nname: v\nbuild:\n  out: ~/v/dist\n---\n",
            ),
            (&format!("/h/v/dist/{BUILT}"), "{}"),
        ]);
        let built = Path::new("/h/v/dist").join(BUILT);
        let homed = notes(SourceView::over(&vfs).with_home(Path::new("/h")), "/h/v");
        assert!(!homed.exists(&built));
        let homeless = notes(SourceView::over(&vfs), "/h/v");
        assert!(homeless.exists(&built));
        // A view over the host's view, as a core function builds one.
        let host = SourceView::over(&vfs).with_home(Path::new("/h"));
        let over = notes(SourceView::over(&host), "/h/v");
        assert!(!over.exists(&built));
    }

    #[test]
    fn notes_view_uses_the_given_config_for_packs() {
        let vfs = vault(&[
            (
                "/v/skillwright.md",
                "---\nname: v\nbuild:\n  out: dist\n---\n",
            ),
            ("/v/dist/moved/p/.claude-plugin/plugin.json", "{}"),
            (
                "/v/dist/.claude-plugin/packs/p/.claude-plugin/plugin.json",
                "{}",
            ),
        ]);
        let moved = Path::new("/v/dist/moved/p/.claude-plugin/plugin.json");
        let config = crate::settings::parse_root_file("---\nclaude: {packs: moved}\n---\n")
            .unwrap()
            .to_config("v");
        let view = notes_view(
            SourceView::over(&vfs),
            VaultScope::new(Path::new("/v")),
            Some(&config),
            &Namespace::bare(),
        );
        assert!(!view.exists(moved));
        let own = notes(SourceView::over(&vfs), "/v");
        assert!(own.exists(moved));
    }

    #[test]
    fn a_double_wrap_hides_the_same_paths() {
        let files = [
            (
                "/v/skillwright.md",
                "---\nname: v\nclaude: {packs: moved}\nbuild:\n  out: dist\n---\n",
            ),
            ("/v/skills/a.md", "---\ndescription: A.\n---\n"),
            ("/v/dist/moved/p/.claude-plugin/plugin.json", "{}"),
            (
                "/v/dist/moved/p/skills/a/SKILL.md",
                "---\ndescription: A.\n---\n",
            ),
            ("/v/dist/.claude-plugin/marketplace.json", "{}"),
        ];
        let vfs = vault(&files);
        let once = notes(SourceView::over(&vfs), "/v");
        let twice = notes(notes(SourceView::over(&vfs), "/v"), "/v");
        for (path, _) in files {
            assert_eq!(
                once.exists(Path::new(path)),
                twice.exists(Path::new(path)),
                "{path}"
            );
        }
        assert!(!once.exists(Path::new("/v/dist/moved/p/skills/a/SKILL.md")));
        assert!(once.exists(Path::new("/v/skills/a.md")));
    }

    /// A relative vault cannot be compared with its output, so the pack
    /// check refuses, and a view that would refuse hides nothing.
    #[test]
    fn a_relative_vault_hides_nothing() {
        let vfs = MemVfs::new();
        vfs.insert("v/skillwright.md", "---\nname: v\n---\n")
            .unwrap();
        let built = Path::new("v/dist/.claude-plugin/marketplace.json");
        vfs.insert(built, "{}").unwrap();
        let view = source_view(
            SourceView::over(&vfs),
            VaultScope::new(Path::new("v")),
            Some(Path::new("v/dist")),
            &MarketplaceConfig::default(),
            &Namespace::bare(),
        );
        assert!(view.exists(built));
    }

    #[test]
    fn vault_config_folds_in_the_workspace_above() {
        let vfs = MemVfs::new();
        let member = Path::new("/w/marketplaces/m");
        vfs.create_dir_all(member).unwrap();
        vfs.write_file(
            Path::new("/w/skillwright.md"),
            b"---\nworkspace: true\nclaude: {packs: alt}\nlicense: MIT\n---\n",
        )
        .unwrap();
        vfs.write_file(&member.join(ROOT_FILE), b"---\nname: m\n---\n")
            .unwrap();
        let config = vault_config(&vfs, member).unwrap();
        assert_eq!(config.license.as_deref(), Some("MIT"));
        assert!(config.blocks.contains_key("claude"));
    }

    #[test]
    fn vault_config_refuses_a_malformed_workspace_file() {
        let vfs = MemVfs::new();
        let member = Path::new("/w/marketplaces/m");
        vfs.create_dir_all(member).unwrap();
        vfs.write_file(Path::new("/w/skillwright.md"), b"---\nworkspace: [\n---\n")
            .unwrap();
        vfs.write_file(&member.join(ROOT_FILE), b"---\nname: m\n---\n")
            .unwrap();
        let err = vault_config(&vfs, member).unwrap_err().to_string();
        assert!(err.contains("malformed-root-file"), "{err}");
    }
}
