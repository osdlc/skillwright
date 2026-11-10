//! Loading a vault's root file: find it, fold in its workspace, and resolve
//! what it names into a build's inputs. [`load_marketplace`] does all of it
//! in one call. The pieces it is made of are public: [`find_root_file`],
//! [`load_effective_root`] for the read and the fold, and
//! [`marketplace_inputs`].
//!
//! Nothing here reads the environment. A host passes the home directory.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::discover::{PathRole, path_role, tail};
use crate::error::CoreError;
use crate::export::{AssetSpec, HookFileSpec, MarketplaceConfig};
use crate::namespace::Namespace;
use crate::settings::{ROOT_FILE, RootFile, parse_root_file};
use crate::vfs::Vfs;

/// The folder a workspace keeps its marketplaces in.
pub const WORKSPACE_DIR: &str = "marketplaces";

/// Join `rel` onto `base` and lexically collapse `.` and `..`, without
/// touching the filesystem. A `..` with nothing to pop is kept.
#[doc(hidden)]
pub fn normalize_join(base: &Path, rel: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in base.join(rel).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `to` expressed relative to `from`, lexically. Both sides are collapsed
/// first, and a target outside `from` comes back with the `..` segments that
/// reach it. An empty result means the two are the same path.
#[doc(hidden)]
pub fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from = normalize_join(from, Path::new(""));
    let to = normalize_join(to, Path::new(""));
    let mut from = from.components().peekable();
    let mut to = to.components().peekable();
    while from.peek().is_some() && from.peek() == to.peek() {
        from.next();
        to.next();
    }
    let mut out = PathBuf::new();
    for _ in from {
        out.push("..");
    }
    for component in to {
        out.push(component.as_os_str());
    }
    out
}

/// Resolve `path` as a shell would: absolute as given, `~` or a `~/` prefix
/// against `home`, anything else relative to `base`. `~name` and `~\` are
/// relative paths.
pub fn resolve_path(path: &str, base: &Path, home: &Path) -> PathBuf {
    if path == "~" || path == "~/" {
        return home.to_path_buf();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home.join(rest);
    }
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        normalize_join(base, p)
    }
}

/// The nearest root file at or above `start`.
///
/// A `skillwright.md` that sits where a component note sits —
/// `<kind>/skillwright/skillwright.md`, a plugin or skill named
/// `skillwright` — is that component's note, not a root file, and the walk
/// goes past it. So does a reserved `<kind>/skillwright.md` inside a vault.
pub fn find_root_file(vfs: &dyn Vfs, start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(here) = dir {
        let candidate = here.join(ROOT_FILE);
        let is_file = vfs.exists(&candidate) && !vfs.metadata(&candidate).is_ok_and(|m| m.is_dir);
        let component = matches!(
            path_role(&tail(&candidate, 3)),
            PathRole::ComponentEntry { .. }
        );
        // Reserved is decided from the vault root, the root file further up:
        // the last parts alone cannot show it, as `marketplaces/skills/` is a
        // member's folder only from there.
        let reserved = || {
            path_role(&tail(&candidate, 2)) == PathRole::Reserved
                && here
                    .parent()
                    .and_then(|up| find_root_file(vfs, up))
                    .and_then(|above| above.parent().map(Path::to_path_buf))
                    .and_then(|vault| candidate.strip_prefix(vault).ok().map(path_role))
                    .is_some_and(|role| role == PathRole::Reserved)
        };
        if is_file && !component && !reserved() {
            return Some(candidate);
        }
        dir = here.parent();
    }
    None
}

/// The root file of each member of the workspace at `dir`:
/// `marketplaces/<m>/skillwright.md` for each `<m>` that holds one, sorted.
pub fn workspace_members(vfs: &dyn Vfs, dir: &Path) -> Vec<PathBuf> {
    let folder = dir.join(WORKSPACE_DIR);
    let mut members: Vec<PathBuf> = vfs
        .read_dir(&folder)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| {
            let file = crate::discover::entry_file(
                crate::frontmatter::EntityKind::Marketplace,
                &entry.name,
            );
            folder.join(&entry.name).join(file)
        })
        .filter(|file| vfs.exists(file) && !vfs.metadata(file).is_ok_and(|m| m.is_dir))
        .collect();
    members.sort();
    members
}

/// Whether `path` is a root config file for the vault at `vault`: its own
/// `skillwright.md`, directly under the vault root, or the workspace file
/// above it when `vault` is a workspace member. Either configures the vault;
/// neither is a note of any kind.
///
/// Both sides are compared by their real paths, so a spelling through a link
/// and the real spelling are one file.
///
/// # Errors
///
/// `refused: unsafe-path` when a path is relative or the host cannot tell
/// where it leads, as [`crate::guard::real_path`]: the check fails closed.
pub(crate) fn is_vault_root_file(
    vfs: &dyn Vfs,
    vault: &Path,
    path: &Path,
) -> Result<bool, CoreError> {
    let real = |p: &Path| crate::guard::real_path(vfs, p);
    let path = real(path)?;
    if path == real(&vault.join(ROOT_FILE))? {
        return Ok(true);
    }
    match workspace_at(vfs, vault) {
        Some(at) => Ok(path == real(&at.file)?),
        None => Ok(false),
    }
}

/// The workspace root file above the marketplace whose root is `dir`, when
/// `dir` is `<workspace>/marketplaces/<m>/`, as given or by its real path,
/// and that file says `workspace: true`. A file that cannot be read is
/// returned too: an exclusion it holds must not fail open, so loading it
/// refuses the run.
pub fn workspace_above(vfs: &dyn Vfs, dir: &Path) -> Option<PathBuf> {
    let file = workspace_at(vfs, dir)?.file;
    match read_root(vfs, &file, RootRead::Full) {
        Ok(None) => None,
        Ok(Some(root)) if !root.workspace => None,
        _ => Some(file),
    }
}

/// How much of a root file a read checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootRead {
    /// Every key, as [`parse_root_file`] reads it.
    Full,
    /// Only what decides which files are notes and where they go, as
    /// [`crate::settings::parse_root_layout`] reads it: a bad value elsewhere
    /// is for the verbs that read it to refuse.
    Layout,
}

/// The workspace file above the marketplace at `dir`, read as `read` says,
/// when `dir` is `<workspace>/marketplaces/<m>/` and the file says
/// `workspace: true`, with where it was found.
///
/// # Errors
///
/// As [`read_root`].
pub(crate) fn read_workspace_above(
    vfs: &dyn Vfs,
    dir: &Path,
    read: RootRead,
) -> Result<Option<(WorkspaceAt, RootFile)>, RootFileError> {
    let Some(at) = workspace_at(vfs, dir) else {
        return Ok(None);
    };
    Ok(read_root(vfs, &at.file, read)?
        .filter(|root| root.workspace)
        .map(|root| (at, root)))
}

/// Where the workspace above the member at `dir` sits, by its shape:
/// `<ws>/marketplaces/<m>/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceAt {
    /// `<ws>/skillwright.md`.
    pub file: PathBuf,
    /// `<m>`, from the spelling that matched.
    pub member: String,
    /// Whether the match came from the real path, not `dir` as given.
    pub through_link: bool,
}

impl WorkspaceAt {
    /// The workspace file as a member's messages name it: `../../skillwright.md`,
    /// or its absolute path when found through a link, where `../../` names
    /// another folder.
    pub(crate) fn label(&self) -> String {
        if self.through_link {
            self.file.display().to_string()
        } else {
            format!("../../{ROOT_FILE}")
        }
    }
}

/// The workspace above the member at `dir`: `dir` as given first, then its
/// real path through [`Vfs::canonicalize`]. The first spelling whose
/// `<ws>/skillwright.md` exists wins, so a member folder that is itself a
/// link inside `marketplaces/` keeps its workspace, and a link to a member
/// from elsewhere finds the one it leads to.
pub(crate) fn workspace_at(vfs: &dyn Vfs, dir: &Path) -> Option<WorkspaceAt> {
    let by_shape = |dir: &Path, through_link: bool| {
        let parent = dir.parent()?;
        if parent.file_name()? != WORKSPACE_DIR {
            return None;
        }
        let file = parent.parent()?.join(ROOT_FILE);
        let member = dir.file_name()?.to_string_lossy().into_owned();
        vfs.exists(&file).then_some(WorkspaceAt {
            file,
            member,
            through_link,
        })
    };
    if let Some(at) = by_shape(dir, false) {
        return Some(at);
    }
    let real = vfs.canonicalize(dir).ok()?;
    (real != dir).then(|| by_shape(&real, true))?
}

/// The root file at `file`, read as `read` says, `None` when there is none.
///
/// # Errors
///
/// [`RootFileError::Malformed`] when it is not UTF-8, its frontmatter is not
/// a YAML mapping, or a key `read` checks holds a value of the wrong shape.
/// [`RootFileError::Unreadable`] when it is there but cannot be read.
pub(crate) fn read_root(
    vfs: &dyn Vfs,
    file: &Path,
    read: RootRead,
) -> Result<Option<RootFile>, RootFileError> {
    let Some(text) = root_text(vfs, file)? else {
        return Ok(None);
    };
    let parsed = match read {
        RootRead::Full => parse_root_file(&text),
        RootRead::Layout => crate::settings::parse_root_layout(&text),
    };
    parsed
        .map(Some)
        .map_err(|err| RootFileError::malformed(file, err.to_string()))
}

/// A root file's layout and its full reading, which may have failed.
pub(crate) type BothReadings = (RootFile, Result<RootFile, CoreError>);

/// The root file at `file` read both ways from one read of its text: the
/// layout, which must be readable, and the full reading, which may hold a bad
/// value. `None` when there is no file.
///
/// # Errors
///
/// `refused: malformed-root-file` when the layout cannot be read, as
/// [`read_root`] with [`RootRead::Layout`].
pub(crate) fn read_root_both(
    vfs: &dyn Vfs,
    file: &Path,
) -> Result<Option<BothReadings>, CoreError> {
    let Some(text) = root_text(vfs, file)? else {
        return Ok(None);
    };
    let layout = crate::settings::parse_root_layout(&text)
        .map_err(|err| RootFileError::malformed(file, err.to_string()))?;
    let full = parse_root_file(&text)
        .map_err(|err| RootFileError::malformed(file, err.to_string()).into());
    Ok(Some((layout, full)))
}

/// The text of the root file at `file`, `None` when there is none.
fn root_text(vfs: &dyn Vfs, file: &Path) -> Result<Option<String>, RootFileError> {
    if !vfs.exists(file) {
        return Ok(None);
    }
    let bytes = vfs
        .read_file(file)
        .map_err(|err| RootFileError::Unreadable {
            file: file.to_path_buf(),
            source: err.into_io_error(),
        })?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| RootFileError::malformed(file, "it is not UTF-8"))
}

/// Why a root file could not be loaded.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum RootFileError {
    /// The file could not be read: missing, a folder, permissions.
    #[error("reading {}: {source}", file.display())]
    Unreadable {
        /// The file.
        file: PathBuf,
        /// The host's error.
        #[source]
        source: std::io::Error,
    },
    /// The file was read but is not a root file: not UTF-8, frontmatter not
    /// a mapping, or a key of the wrong shape.
    #[error("{}: {detail}", file.display())]
    Malformed {
        /// The file.
        file: PathBuf,
        /// What is wrong with it.
        detail: String,
    },
}

impl RootFileError {
    fn malformed(file: &Path, detail: impl Into<String>) -> Self {
        Self::Malformed {
            file: file.to_path_buf(),
            detail: detail.into(),
        }
    }
}

/// `Unreadable` is [`CoreError::Io`]. `Malformed` is
/// `refused: malformed-root-file`.
impl From<RootFileError> for CoreError {
    fn from(err: RootFileError) -> Self {
        match err {
            RootFileError::Unreadable { file, source } => CoreError::io(file, source),
            RootFileError::Malformed { file, detail } => CoreError::refused(
                crate::publish::ReasonToken::MalformedRootFile.as_str(),
                format!("{}: {detail}. Nothing was written", file.display()),
            ),
        }
    }
}

/// A root file with the workspace above it folded in.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct EffectiveRoot {
    /// The root file with the workspace's shared settings folded in.
    /// `root.warnings` holds the root file's own warnings only, as
    /// [`RootFile::inherit`] leaves it.
    pub root: RootFile,
    /// The workspace file folded in, if any.
    pub workspace: Option<PathBuf>,
    /// Every warning to show, each with the file it came from: the root
    /// file's first, then the workspace file's.
    pub warnings: Vec<(PathBuf, String)>,
}

/// The root file at `file`, when given, with the workspace above `dir`
/// folded in, unless that root file is itself a workspace.
///
/// `dir` is the vault. `file` is the root file the caller chose; `None` means
/// the vault has none, and it reads as every default before the fold. This
/// is the one place a workspace is folded into a full root file.
///
/// # Errors
///
/// [`RootFileError::Unreadable`] when `file` is missing or cannot be read,
/// [`RootFileError::Malformed`] when it is not a root file. The same for a
/// workspace file that is there: an exclusion it holds must not fail open.
pub fn load_effective_root(
    vfs: &dyn Vfs,
    dir: &Path,
    file: Option<&Path>,
) -> Result<EffectiveRoot, RootFileError> {
    let mut root = match file {
        Some(file) => {
            read_root(vfs, file, RootRead::Full)?.ok_or_else(|| RootFileError::Unreadable {
                file: file.to_path_buf(),
                source: std::io::Error::from(std::io::ErrorKind::NotFound),
            })?
        }
        None => RootFile::default(),
    };
    let mut warnings: Vec<(PathBuf, String)> = file
        .map(|file| {
            root.warnings
                .iter()
                .map(|warning| (file.to_path_buf(), warning.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut workspace = None;
    if !root.workspace
        && let Some((at, shared)) = read_workspace_above(vfs, dir, RootRead::Full)?
    {
        root.inherit(&shared);
        warnings.extend(
            shared
                .warnings
                .into_iter()
                .map(|warning| (at.file.clone(), warning)),
        );
        workspace = Some(at.file);
    }
    Ok(EffectiveRoot {
        root,
        workspace,
        warnings,
    })
}

/// What a marketplace load reads from the root file besides the vault: the
/// identity, the harnesses, the assets and the hooks files.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct MarketplaceInputs {
    /// The build config, named by the root file or else the folder.
    pub config: MarketplaceConfig,
    /// The harnesses to build for. Empty is every one the export supports.
    pub harnesses: BTreeSet<String>,
    /// The `build.assets` entries, `from` resolved against the root file's
    /// folder.
    pub assets: Vec<AssetSpec>,
    /// The `build.hooks` entries, `from` resolved the same way.
    pub hooks: Vec<HookFileSpec>,
}

/// The root file as the inputs of an [`crate::ExportRequest`]: config,
/// harnesses, assets and hooks. Pass the result's fields to the request, or a
/// build gets no assets and no hooks. [`load_marketplace`] calls this after
/// folding in the workspace; call it directly only with a `root` that is
/// already complete.
///
/// `folder_name` is the marketplace's name when the file gives none.
/// `config_dir` is the root file's folder: each `from` resolves against it,
/// because the core reads no ambient config. `home` expands a leading `~`.
pub fn marketplace_inputs(
    root: &RootFile,
    folder_name: &str,
    config_dir: &Path,
    home: &Path,
) -> MarketplaceInputs {
    let assets = root
        .build
        .assets
        .iter()
        .map(|entry| AssetSpec {
            source: resolve_path(&entry.from, config_dir, home),
            dest: entry.to.clone(),
            plugin: entry.plugin.clone(),
        })
        .collect();
    let hooks = root
        .build
        .hooks
        .iter()
        .map(|entry| {
            let source = resolve_path(&entry.from, config_dir, home);
            // Root-relative when the file is under the root folder; a file
            // outside it is named by its `..` path.
            let from = relative_path(config_dir, &source)
                .to_string_lossy()
                .replace('\\', "/");
            HookFileSpec {
                source,
                from,
                plugin: entry.plugin.clone(),
            }
        })
        .collect();
    MarketplaceInputs {
        config: root.to_config(folder_name),
        harnesses: root.harness_set(),
        assets,
        hooks,
    }
}

/// One marketplace as a build needs it, loaded from a root file.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct LoadedMarketplace {
    /// The root file that was read.
    pub root_file: PathBuf,
    /// The folder holding it: the vault.
    pub dir: PathBuf,
    /// The root file, with the workspace's shared settings folded in.
    pub root: RootFile,
    /// `build.namespace`, or the default bare namespace.
    pub namespace: Namespace,
    /// `build.out` resolved against `dir`, when set. This is the
    /// [`crate::ExportRequest::out_root`].
    pub out_root: Option<PathBuf>,
    /// Config, harnesses, assets and hooks for the [`crate::ExportRequest`].
    pub inputs: MarketplaceInputs,
}

/// Load the marketplace whose root file is at or above `start`: find the
/// root file, fold in the workspace above it, check the namespace, and
/// resolve the build inputs. `Ok(None)` means no root file was found.
///
/// A workspace member inherits the workspace's `build:` settings and shared
/// fields ([`RootFile::inherit`]); a consumer that reads the member file alone
/// builds without them. A root file that is itself a workspace
/// (`root.workspace`) builds nothing: its members are the
/// `marketplaces/<m>/skillwright.md` files below it, each loaded by passing
/// its folder as `start`.
///
/// # Errors
///
/// [`CoreError::Refused`] when a root file cannot be read or parsed, or its
/// namespace is invalid.
pub fn load_marketplace(
    vfs: &dyn Vfs,
    start: &Path,
    home: &Path,
) -> Result<Option<LoadedMarketplace>, CoreError> {
    let Some(root_file) = find_root_file(vfs, start) else {
        return Ok(None);
    };
    let dir = root_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let root = load_effective_root(vfs, &dir, Some(&root_file))?.root;
    let namespace = match &root.build.namespace {
        Some(stem) => Namespace::new(stem).map_err(|invalid| {
            CoreError::refused(
                "namespace",
                format!("{invalid} (`build.namespace` in {})", root_file.display()),
            )
        })?,
        None => Namespace::default(),
    };
    let out_root = root
        .build
        .out
        .as_deref()
        .map(|out| resolve_path(out, &dir, home));
    let folder = vfs.canonicalize(&dir).unwrap_or_else(|_| dir.clone());
    let folder = folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let inputs = marketplace_inputs(&root, &folder, &dir, home);
    Ok(Some(LoadedMarketplace {
        root_file,
        dir,
        root,
        namespace,
        out_root,
        inputs,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::parse_root_file;
    use crate::vfs::MemVfs;

    #[test]
    fn workspace_members_lists_member_root_files() {
        let vfs = MemVfs::new();
        for file in [
            "/w/skillwright.md",
            "/w/marketplaces/b/skillwright.md",
            "/w/marketplaces/a/skillwright.md",
            "/w/marketplaces/none/plugins/p/p.md",
        ] {
            vfs.insert(file, "---\nname: x\n---\n").unwrap();
        }
        assert_eq!(
            workspace_members(&vfs, Path::new("/w")),
            [
                PathBuf::from("/w/marketplaces/a/skillwright.md"),
                PathBuf::from("/w/marketplaces/b/skillwright.md"),
            ]
        );
        assert!(workspace_members(&vfs, Path::new("/elsewhere")).is_empty());
    }

    fn put(vfs: &MemVfs, path: &str) {
        vfs.insert(path, "---\n---\n").unwrap();
    }

    #[test]
    fn the_walk_up_finds_the_nearest_root_file() {
        let vfs = MemVfs::new();
        put(&vfs, "/v/a/skillwright.md");
        vfs.create_dir_all(Path::new("/v/a/plugins/p/skills"))
            .unwrap();
        assert_eq!(
            find_root_file(&vfs, Path::new("/v/a/plugins/p/skills")),
            Some(PathBuf::from("/v/a/skillwright.md"))
        );
    }

    #[test]
    fn a_component_named_skillwright_is_not_a_root_file() {
        let vfs = MemVfs::new();
        put(&vfs, "/v/plugins/skillwright/skillwright.md");
        put(&vfs, "/v/skillwright.md");
        vfs.create_dir_all(Path::new("/v/plugins/skillwright/skills"))
            .unwrap();
        assert_eq!(
            find_root_file(&vfs, Path::new("/v/plugins/skillwright/skills")),
            Some(PathBuf::from("/v/skillwright.md"))
        );
    }

    #[test]
    fn a_member_named_skillwright_keeps_its_root_file() {
        let vfs = MemVfs::new();
        put(&vfs, "/v/marketplaces/skillwright/skillwright.md");
        put(&vfs, "/v/skillwright.md");
        assert_eq!(
            find_root_file(&vfs, Path::new("/v/marketplaces/skillwright")),
            Some(PathBuf::from("/v/marketplaces/skillwright/skillwright.md"))
        );
    }

    /// A path the guard cannot resolve is refused, never answered as written.
    #[test]
    fn the_root_file_check_fails_closed() {
        let vfs = MemVfs::new();
        put(&vfs, "/v/skillwright.md");
        assert!(is_vault_root_file(&vfs, Path::new("/v"), Path::new("/v/skillwright.md")).unwrap());
        let err = is_vault_root_file(&vfs, Path::new("v"), Path::new("v/skillwright.md"))
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("refused: unsafe-path: "), "{err}");
    }

    #[test]
    fn the_walk_up_passes_a_reserved_note() {
        let vfs = MemVfs::new();
        put(&vfs, "/v/skills/skillwright.md");
        put(&vfs, "/v/skillwright.md");
        assert_eq!(
            find_root_file(&vfs, Path::new("/v/skills")),
            Some(PathBuf::from("/v/skillwright.md"))
        );
    }

    #[test]
    fn a_nested_marketplace_file_is_a_root_file_to_the_walk() {
        let vfs = MemVfs::new();
        put(&vfs, "/v/skillwright.md");
        put(&vfs, "/v/marketplaces/a/skillwright.md");
        put(&vfs, "/v/marketplaces/a/marketplaces/b/skillwright.md");
        assert_eq!(
            find_root_file(&vfs, Path::new("/v/marketplaces/a/marketplaces/b")),
            Some(PathBuf::from(
                "/v/marketplaces/a/marketplaces/b/skillwright.md"
            ))
        );
    }

    #[test]
    fn a_member_loads_with_its_workspaces_settings() {
        let vfs = MemVfs::new();
        vfs.create_dir_all(Path::new("/w/marketplaces/m1/plugins/p/skills"))
            .unwrap();
        vfs.write_file(
            Path::new("/w/skillwright.md"),
            b"---\nworkspace: true\nbuild:\n  harnesses: [claude]\n  namespace: sw\n  assets:\n    - from: shared/run.sh\n      to: bin/run.sh\n---\n",
        )
        .unwrap();
        vfs.write_file(
            Path::new("/w/marketplaces/m1/skillwright.md"),
            b"---\nname: m1\nbuild:\n  out: ~/dist\n---\n",
        )
        .unwrap();
        let loaded = load_marketplace(
            &vfs,
            Path::new("/w/marketplaces/m1/plugins/p/skills"),
            Path::new("/home/u"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(loaded.dir, PathBuf::from("/w/marketplaces/m1"));
        assert_eq!(loaded.inputs.harnesses.len(), 1);
        assert_eq!(loaded.inputs.assets.len(), 1);
        assert_eq!(
            loaded.inputs.assets[0].source,
            PathBuf::from("/w/marketplaces/m1/shared/run.sh")
        );
        assert_eq!(loaded.out_root, Some(PathBuf::from("/home/u/dist")));
        assert!(!loaded.namespace.is_bare());
        assert!(
            load_marketplace(&vfs, Path::new("/nowhere"), Path::new("/"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn marketplace_inputs_resolve_assets_and_hooks_against_the_root_folder() {
        let root = parse_root_file(
            "---\nname: m\nbuild:\n  harnesses: [codex]\n  assets:\n    - from: scripts/run.sh\n      to: bin/run.sh\n  hooks:\n    - from: ~/h/hooks.json\n---\n",
        )
        .unwrap();
        let inputs = marketplace_inputs(&root, "folder", Path::new("/v/m"), Path::new("/home/u"));
        assert_eq!(
            inputs.assets[0].source,
            PathBuf::from("/v/m/scripts/run.sh")
        );
        assert_eq!(inputs.assets[0].dest, "bin/run.sh");
        assert_eq!(
            inputs.hooks[0].source,
            PathBuf::from("/home/u/h/hooks.json")
        );
        assert_eq!(inputs.hooks[0].from, "../../home/u/h/hooks.json");
        assert_eq!(inputs.harnesses.len(), 1);
        assert_eq!(inputs.config.name.as_deref(), Some("m"));
    }
}
