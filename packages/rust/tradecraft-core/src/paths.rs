//! Loading a vault: check its layout, read its root file, and list its
//! marketplaces with each one's note. [`load_vault`] does it in one call.
//!
//! Nothing here reads the environment. A host passes the home directory.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::error::CoreError;
use crate::export::{AssetSpec, ExportMode, ExportRequest, HookFileSpec, MarketplaceConfig};
use crate::layout::{DIST_DIR, MARKETPLACES_DIR, read_layout};
use crate::namespace::Namespace;
use crate::settings::{
    MarketplaceFields, ROOT_FILE, RootFile, parse_marketplace_note, parse_root_file,
};
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

/// What a build reads from the root file besides the vault: the shared
/// defaults, the harnesses, the assets and the hooks files.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct MarketplaceInputs {
    /// The build config: the root file's shared defaults.
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
/// `config_dir` is the root file's folder: each `from` resolves against it,
/// because the core reads no ambient config. `home` expands a leading `~`.
pub fn marketplace_inputs(root: &RootFile, config_dir: &Path, home: &Path) -> MarketplaceInputs {
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
        config: root.to_config(),
        harnesses: root.harness_set(),
        assets,
        hooks,
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
    /// The fields its note sets.
    pub fields: MarketplaceFields,
    /// Where a build writes it: `<root>/dist/<m>/`.
    pub out_root: PathBuf,
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
    /// `build.namespace`, or the default bare namespace.
    pub namespace: Namespace,
    /// Config, harnesses, assets and hooks for each [`ExportRequest`].
    pub inputs: MarketplaceInputs,
    /// The marketplaces the load was asked for, by folder name: every one, or
    /// the one named.
    pub marketplaces: Vec<VaultMarketplace>,
}

impl LoadedVault {
    /// The marketplace named `name`, if the load listed it.
    pub fn marketplace(&self, name: &str) -> Option<&VaultMarketplace> {
        self.marketplaces.iter().find(|m| m.name == name)
    }

    /// The build or check of `marketplace` into its `dist/<m>/` folder.
    pub fn export_request(
        &self,
        marketplace: &VaultMarketplace,
        mode: ExportMode,
    ) -> ExportRequest {
        let mut request = ExportRequest::new(
            self.root.clone(),
            marketplace.out_root.clone(),
            self.inputs.config.clone(),
        );
        request.marketplace = Some(marketplace.name.clone());
        request.mode = mode;
        request.harnesses.clone_from(&self.inputs.harnesses);
        request.assets.clone_from(&self.inputs.assets);
        request.hooks.clone_from(&self.inputs.hooks);
        request
    }
}

/// Where a build writes the marketplace `name` of the vault at `root`:
/// `<root>/dist/<name>/` (spec section 9 rule 1).
pub fn dist_dir(root: &Path, name: &str) -> PathBuf {
    root.join(DIST_DIR).join(name)
}

/// Load the vault at `root`: check its layout, read its root file, and read
/// the note of every marketplace, or only of `marketplace` when one is named.
/// Find `root` first with [`crate::layout::find_vault_root`], or check a
/// given one with [`crate::layout::check_vault_root`].
///
/// # Errors
///
/// * `refused: layout` — `root` is not a vault root, or the vault breaks the
///   layout (spec section 11).
/// * `refused: malformed-root-file` — the root file cannot be read or parsed.
/// * `refused: namespace` — `build.namespace` is not a valid stem.
/// * `refused: unknown-marketplace` — `marketplace` names no marketplace.
/// * `refused: malformed` — a marketplace note whose fields cannot be read.
pub fn load_vault(
    vfs: &dyn Vfs,
    root: &Path,
    marketplace: Option<&str>,
    home: &Path,
) -> Result<LoadedVault, CoreError> {
    let layout = read_layout(vfs, root)?;
    let root_file = root.join(ROOT_FILE);
    let text = String::from_utf8(vfs.read_file(&root_file)?)
        .map_err(|_| malformed_root(&root_file, "it is not UTF-8"))?;
    let settings =
        parse_root_file(&text).map_err(|err| malformed_root(&root_file, &err.to_string()))?;
    let namespace = match &settings.build.namespace {
        Some(stem) => Namespace::new(stem).map_err(|invalid| {
            CoreError::refused(
                "namespace",
                format!("{invalid} (`build.namespace` in {})", root_file.display()),
            )
        })?,
        None => Namespace::default(),
    };
    if let Some(name) = marketplace
        && !layout.marketplaces.iter().any(|m| m == name)
    {
        return Err(CoreError::refused(
            "unknown-marketplace",
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
        let text = crate::skill::read_utf8(vfs, &note)?;
        let fields = parse_marketplace_note(&text)
            .map_err(|err| CoreError::malformed(&note, err.to_string()))?;
        marketplaces.push(VaultMarketplace {
            name: name.clone(),
            dir,
            note,
            fields,
            out_root: dist_dir(root, name),
        });
    }
    let inputs = marketplace_inputs(&settings, root, home);
    Ok(LoadedVault {
        root: root.to_path_buf(),
        root_file,
        settings,
        namespace,
        inputs,
        marketplaces,
    })
}

/// The refusal for a root file at `file` that cannot be read.
fn malformed_root(file: &Path, detail: &str) -> CoreError {
    CoreError::refused(
        crate::publish::ReasonToken::MalformedRootFile.as_str(),
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
            (
                "/v/skillwright.md",
                "---\nlicense: MIT\nbuild:\n  namespace: acme\n---\n",
            ),
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
        let loaded = load_vault(&vfs, Path::new("/v"), None, Path::new("/home")).unwrap();
        let names: Vec<&str> = loaded
            .marketplaces
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(names, ["house", "shop"]);
        let house = loaded.marketplace("house").unwrap();
        assert_eq!(house.fields.description.as_deref(), Some("The house."));
        assert_eq!(house.out_root, Path::new("/v/dist/house"));
        assert_eq!(loaded.namespace.stem(), "acme");
        let request = loaded.export_request(house, ExportMode::Build);
        assert_eq!(request.out_root, Path::new("/v/dist/house"));
        assert_eq!(request.vault_root, Path::new("/v"));
    }

    #[test]
    fn one_marketplace_can_be_picked_and_an_unknown_one_is_refused() {
        let vfs = vault();
        let loaded = load_vault(&vfs, Path::new("/v"), Some("shop"), Path::new("/home")).unwrap();
        assert_eq!(loaded.marketplaces.len(), 1);
        assert_eq!(loaded.marketplaces[0].name, "shop");
        let err = load_vault(&vfs, Path::new("/v"), Some("mall"), Path::new("/home")).unwrap_err();
        let said = err.to_string();
        assert!(said.contains("unknown-marketplace"), "{said}");
        assert!(said.contains("`house`, `shop`"), "{said}");
    }

    #[test]
    fn a_bad_marketplace_note_names_its_path() {
        let vfs = vault();
        vfs.write_file(
            Path::new("/v/marketplaces/shop/shop.md"),
            b"---\ntargets: claude\n---\n",
        )
        .unwrap();
        let err = load_vault(&vfs, Path::new("/v"), None, Path::new("/home")).unwrap_err();
        assert!(err.to_string().contains("shop/shop.md"), "{err}");
    }
}
