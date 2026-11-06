//! Lexical path arithmetic and the root file's reading of it: finding the
//! root file, and resolving what it names into a build's inputs.
//!
//! Nothing here reads the environment. A host passes the home directory.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::discover::KIND_DIRS;
use crate::export::{AssetSpec, HookFileSpec, MarketplaceConfig};
use crate::settings::{ROOT_FILE, RootFile};
use crate::vfs::Vfs;

/// Join `rel` onto `base` and lexically collapse `.` and `..`, without
/// touching the filesystem. A `..` with nothing to pop is kept.
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
        if is_file && !is_component_note(&candidate) {
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

/// What a marketplace load reads from the root file besides the vault: the
/// identity, the harnesses, the assets and the hooks files.
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
/// build gets no assets and no hooks.
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
            // Root-relative, so the gap report names no host path.
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
