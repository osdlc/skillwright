//! Loading a vault's root file: find it, fold in its workspace, and resolve
//! what it names into a build's inputs. [`load_marketplace`] does all of it
//! in one call. The pieces it is made of are public.
//!
//! Nothing here reads the environment. A host passes the home directory.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::discover::KIND_DIRS;
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
/// goes past it.
pub fn find_root_file(vfs: &dyn Vfs, start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(here) = dir {
        let candidate = here.join(ROOT_FILE);
        let is_file = vfs.exists(&candidate) && !vfs.metadata(&candidate).is_ok_and(|m| m.is_dir);
        // A flat `<kind>/skillwright.md` inside a vault is a refused note,
        // not a root file; the vault's root file is further up.
        let note_inside = is_flat_root_file_name(&candidate)
            && here
                .parent()
                .and_then(|up| find_root_file(vfs, up))
                .and_then(|above| above.parent().map(Path::to_path_buf))
                .is_some_and(|vault| is_reserved_note(&vault, &candidate));
        if is_file && !is_component_note(&candidate) && !note_inside {
            return Some(candidate);
        }
        dir = here.parent();
    }
    None
}

/// Whether `path` is `<kind>/skillwright/skillwright.md`, a component note
/// and not a root file. `marketplaces/skillwright/skillwright.md` is not one:
/// that is the root file of a workspace member named `skillwright`.
fn is_component_note(path: &Path) -> bool {
    let folder = path.parent();
    let named = folder
        .and_then(Path::file_name)
        .is_some_and(|name| name == "skillwright");
    let in_kind = folder
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name != "marketplaces" && KIND_DIRS.iter().any(|(dir, _)| *dir == name)
        });
    named && in_kind
}

/// Whether `path`, under the vault at `vault`, is a reserved note: a
/// `skillwright.md` directly inside a kind folder, such as
/// `skills/skillwright.md`. `marketplaces/<m>/skillwright.md` is never one,
/// even when `<m>` is a kind folder's name: that is a marketplace's own file.
fn is_reserved_note(vault: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(vault).unwrap_or(path);
    let parts: Vec<_> = relative.components().collect();
    let marketplace_file = parts.len() == 3
        && parts[0].as_os_str() == WORKSPACE_DIR
        && parts[2].as_os_str() == ROOT_FILE;
    is_flat_root_file_name(path) && !marketplace_file
}

/// Whether `path` is a `skillwright.md` directly inside a folder named after a
/// kind folder. Discovery refuses it unless it is a marketplace's file, which
/// discovery places as one before it gets here.
pub(crate) fn is_flat_root_file_name(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == ROOT_FILE)
        && path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .is_some_and(|name| KIND_DIRS.iter().any(|(dir, _)| *dir == name))
}

/// The workspace root file above the marketplace whose root is `dir`, when
/// `dir` is `<workspace>/marketplaces/<m>/` and that file says
/// `workspace: true`. A file that cannot be read is returned too: an exclusion
/// it holds must not fail open, so loading it refuses the run.
pub fn workspace_above(vfs: &dyn Vfs, dir: &Path) -> Option<PathBuf> {
    let file = workspace_file(dir)?;
    match read_root(vfs, &file) {
        Ok(None) => None,
        Ok(Some(root)) if !root.workspace => None,
        _ => Some(file),
    }
}

/// The workspace file above the marketplace at `dir`, read, when `dir` is
/// `<workspace>/marketplaces/<m>/` and the file says `workspace: true`.
///
/// # Errors
///
/// `refused: malformed-root-file`, as [`read_root`].
pub(crate) fn read_workspace_above(
    vfs: &dyn Vfs,
    dir: &Path,
) -> Result<Option<RootFile>, CoreError> {
    let Some(file) = workspace_file(dir) else {
        return Ok(None);
    };
    Ok(read_root(vfs, &file)?.filter(|root| root.workspace))
}

/// Where a workspace file above `dir` would be, when `dir` is
/// `<workspace>/marketplaces/<m>/`.
fn workspace_file(dir: &Path) -> Option<PathBuf> {
    let parent = dir.parent()?;
    if parent.file_name()? != WORKSPACE_DIR {
        return None;
    }
    Some(parent.parent()?.join(ROOT_FILE))
}

/// The root file at `file`, `None` when there is none.
///
/// # Errors
///
/// `refused: malformed-root-file` when it is not UTF-8 or does not parse: an
/// exclusion that cannot be read must not fail open.
pub(crate) fn read_root(vfs: &dyn Vfs, file: &Path) -> Result<Option<RootFile>, CoreError> {
    if !vfs.exists(file) {
        return Ok(None);
    }
    let malformed = |detail: String| {
        CoreError::refused(
            "malformed-root-file",
            format!("{}: {detail}. Nothing was written", file.display()),
        )
    };
    let text = String::from_utf8(vfs.read_file(file)?)
        .map_err(|_| malformed("it is not UTF-8".to_owned()))?;
    parse_root_file(&text)
        .map(Some)
        .map_err(|err| malformed(err.to_string()))
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
    let mut root = read_root(vfs, &root_file)?.unwrap_or_default();
    if !root.workspace
        && let Some(shared) = read_workspace_above(vfs, &dir)?
    {
        root.inherit(&shared);
    }
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

    fn put(vfs: &MemVfs, path: &str) {
        let path = Path::new(path);
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        vfs.write_file(path, b"---\n---\n").unwrap();
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
