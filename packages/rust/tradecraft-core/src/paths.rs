//! Loading a vault: check its layout, read its root file, and list its
//! marketplaces with each one's note. [`load_vault`] does it in one call.
//!
//! Nothing here reads the environment. A host passes the home directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::layout::{DIST_DIR, Layout, MARKETPLACES_DIR, read_layout};
use crate::refusal;
use crate::settings::{ROOT_FILE, RootFile, parse_root_file};
use crate::vfs::Vfs;

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

/// Which part of the vault a run covers. On the wire it is the optional
/// `marketplace` key: absent is [`BuildScope::Vault`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Option<String>", into = "Option<String>")]
pub enum BuildScope {
    /// Every marketplace. A load of one marketplace takes the vault's only
    /// one.
    #[default]
    Vault,
    /// The marketplace of this folder.
    Marketplace(String),
}

impl BuildScope {
    /// The marketplace this scope names, `None` for the whole vault.
    pub fn marketplace(&self) -> Option<&str> {
        match self {
            Self::Vault => None,
            Self::Marketplace(name) => Some(name),
        }
    }
}

impl From<Option<String>> for BuildScope {
    fn from(marketplace: Option<String>) -> Self {
        marketplace.map_or(Self::Vault, Self::Marketplace)
    }
}

impl From<BuildScope> for Option<String> {
    fn from(scope: BuildScope) -> Self {
        scope.marketplace().map(str::to_owned)
    }
}

/// One marketplace of a loaded vault.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct VaultMarketplace {
    /// The marketplace's folder name, which is its name.
    pub name: String,
    /// `<root>/marketplaces/<m>/`.
    pub dir: PathBuf,
    /// `<root>/marketplaces/<m>/<m>.md`.
    pub note: PathBuf,
    /// Where a build writes it: `<root>/dist/<m>/`.
    pub dist: PathBuf,
}

/// A vault as a build needs it: its root file and its marketplaces.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct LoadedVault {
    /// The vault root.
    pub root: PathBuf,
    /// `<root>/skillwright.md`.
    pub root_file: PathBuf,
    /// The root file, parsed.
    pub settings: RootFile,
    /// The marketplaces the load was asked for, by folder name: every one, or
    /// the one named.
    pub marketplaces: Vec<VaultMarketplace>,
    /// The vault's layout, read once.
    pub layout: Layout,
}

impl LoadedVault {
    /// The marketplace named `name`, if the load listed it.
    pub fn marketplace(&self, name: &str) -> Option<&VaultMarketplace> {
        self.marketplaces.iter().find(|m| m.name == name)
    }
}

/// Where a build writes the marketplace `name` of the vault at `root`:
/// `<root>/dist/<name>/` (spec section 9 rule 1).
pub fn dist_dir(root: &Path, name: &str) -> PathBuf {
    root.join(DIST_DIR).join(name)
}

/// Load the vault at `root`: check its layout, read its root file, and list
/// the marketplaces `scope` covers. A build reads each marketplace's note.
/// Find `root` first with [`crate::layout::find_vault_root`], or check a
/// given one with [`crate::layout::check_vault_root`].
///
/// # Errors
///
/// * `refused: layout` — `root` is not a vault root, or the vault breaks the
///   layout (spec section 11).
/// * `refused: malformed-root-file` — the root file cannot be read or parsed.
/// * `refused: unknown-marketplace` — `scope` names no marketplace.
pub fn load_vault(
    vfs: &dyn Vfs,
    root: &Path,
    scope: &BuildScope,
) -> Result<LoadedVault, CoreError> {
    let marketplace = scope.marketplace();
    let root = &crate::vfs::normalize(root);
    let layout = read_layout(vfs, root)?;
    let root_file = root.join(ROOT_FILE);
    let settings = read_root_file(vfs, root)?;
    if let Some(name) = marketplace
        && !layout.marketplaces.iter().any(|m| m == name)
    {
        return Err(CoreError::refused(
            refusal::UNKNOWN_MARKETPLACE,
            format!(
                "no marketplace `{name}` in {}: it holds {}",
                root.display(),
                if layout.marketplaces.is_empty() {
                    "none".to_owned()
                } else {
                    layout
                        .marketplaces
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ),
        ));
    }
    let mut marketplaces = Vec::new();
    for name in layout
        .marketplaces
        .iter()
        .filter(|name| marketplace.is_none_or(|wanted| wanted == name.as_str()))
    {
        let dir = root.join(MARKETPLACES_DIR).join(name);
        let note = dir.join(format!("{name}.md"));
        marketplaces.push(VaultMarketplace {
            name: name.clone(),
            dir,
            note,
            dist: dist_dir(root, name),
        });
    }
    Ok(LoadedVault {
        root: root.to_path_buf(),
        root_file,
        settings,
        marketplaces,
        layout,
    })
}

/// Read and parse the root file of the vault at `root`.
///
/// # Errors
///
/// The read's I/O error, or `refused: malformed-root-file` — the file is
/// not UTF-8 or cannot be parsed.
pub fn read_root_file(vfs: &dyn Vfs, root: &Path) -> Result<RootFile, CoreError> {
    let file = root.join(ROOT_FILE);
    let text = String::from_utf8(vfs.read_file(&file)?)
        .map_err(|_| malformed_root(&file, "it is not UTF-8"))?;
    parse_root_file(&text).map_err(|err| malformed_root(&file, &err.to_string()))
}

/// The refusal for a root file at `file` that cannot be read.
fn malformed_root(file: &Path, detail: &str) -> CoreError {
    CoreError::refused(
        refusal::MALFORMED_ROOT_FILE,
        format!("{}: {detail}. Nothing was written", file.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::MemVfs;

    fn vault() -> MemVfs {
        let vfs = MemVfs::new();
        for (path, text) in [
            ("/v/skillwright.md", "---\nlicense: MIT\n---\n"),
            (
                "/v/marketplaces/house/house.md",
                "---\ndescription: The house.\n---\n",
            ),
            ("/v/marketplaces/shop/shop.md", "---\n---\n"),
        ] {
            let path = Path::new(path);
            vfs.create_dir_all(path.parent().unwrap()).unwrap();
            vfs.write_file(path, text.as_bytes()).unwrap();
        }
        vfs
    }

    #[test]
    fn a_vault_lists_each_marketplace_with_its_note_and_output() {
        let vfs = vault();
        let loaded = load_vault(&vfs, Path::new("/v"), &BuildScope::Vault).unwrap();
        let names: Vec<&str> = loaded
            .marketplaces
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(names, ["house", "shop"]);
        let house = loaded.marketplace("house").unwrap();
        assert_eq!(house.note, Path::new("/v/marketplaces/house/house.md"));
        assert_eq!(house.dist, Path::new("/v/dist/house"));
    }

    #[test]
    fn a_root_with_dot_dot_loads_normalized() {
        let vfs = vault();
        let loaded = load_vault(&vfs, Path::new("/v/marketplaces/.."), &BuildScope::Vault).unwrap();
        assert_eq!(loaded.root, Path::new("/v"));
        assert_eq!(
            loaded.marketplace("shop").unwrap().dist,
            Path::new("/v/dist/shop")
        );
    }

    #[test]
    fn one_marketplace_can_be_picked_and_an_unknown_one_is_refused() {
        let vfs = vault();
        let loaded = load_vault(
            &vfs,
            Path::new("/v"),
            &BuildScope::Marketplace("shop".into()),
        )
        .unwrap();
        assert_eq!(loaded.marketplaces.len(), 1);
        assert_eq!(loaded.marketplaces[0].name, "shop");
        let err = load_vault(
            &vfs,
            Path::new("/v"),
            &BuildScope::Marketplace("mall".into()),
        )
        .unwrap_err();
        let said = err.to_string();
        assert!(said.contains("unknown-marketplace"), "{said}");
        assert!(said.contains("`house`, `shop`"), "{said}");
    }
}
