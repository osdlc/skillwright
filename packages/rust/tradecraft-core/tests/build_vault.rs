//! The root file as the one config channel at the API, and `build_vault`:
//! every marketplace planned, then all written or none.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tradecraft_core::vfs::{DirEntry, Metadata, VfsError};
use tradecraft_core::{
    CoreError, ExportMode, ExportRequest, MemVfs, VaultRequest, Vfs, build_vault, export_skills,
    load_vault, model::LoadRequest, plan_loaded,
};

const SKILL: &str = "---\ndescription: Use it.\n---\n\nBody.\n";

/// A vault at `/v` whose root file holds `root`, with one marketplace per
/// name in `markets`, each holding plugin `tools` and skill `review`.
fn vault(root: &str, markets: &[&str]) -> MemVfs {
    let vfs = MemVfs::new();
    let mut files = vec![("skillwright.md".to_owned(), format!("---\n{root}---\n"))];
    for m in markets {
        files.push((format!("marketplaces/{m}/{m}.md"), "---\n---\n".to_owned()));
        files.push((
            format!("marketplaces/{m}/plugins/tools/tools.md"),
            "---\n---\n".to_owned(),
        ));
        files.push((
            format!("marketplaces/{m}/plugins/tools/skills/review/review.md"),
            SKILL.to_owned(),
        ));
    }
    for (path, text) in files {
        write(&vfs, &format!("/v/{path}"), &text);
    }
    vfs
}

fn write(vfs: &MemVfs, path: &str, text: &str) {
    let path = Path::new(path);
    vfs.create_dir_all(path.parent().unwrap()).unwrap();
    vfs.write_file(path, text.as_bytes()).unwrap();
}

fn request(marketplace: &str) -> ExportRequest {
    let mut request = ExportRequest::new("/v");
    request.marketplace = Some(marketplace.to_owned());
    request
}

/// Every file under `dir`, with its bytes.
fn files_under(vfs: &MemVfs, dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    if !vfs.exists(dir) {
        return out;
    }
    for entry in vfs.read_dir(dir).unwrap() {
        let path = dir.join(&entry.name);
        if entry.is_dir {
            out.extend(files_under(vfs, &path));
        } else {
            out.insert(path.clone(), vfs.read_file(&path).unwrap());
        }
    }
    out
}

#[test]
fn every_request_refuses_defaults() {
    let json = r#"{ "vault_root": "/v", "defaults": { "license": "MIT" } }"#;
    for err in [
        serde_json::from_str::<ExportRequest>(json).unwrap_err(),
        serde_json::from_str::<LoadRequest>(json).unwrap_err(),
        serde_json::from_str::<VaultRequest>(json).unwrap_err(),
    ] {
        assert!(err.to_string().contains("defaults"), "{err}");
    }
}

#[test]
fn the_core_reads_the_root_files_defaults() {
    let vfs = vault("license: MIT\nhomepage: https://root.example\n", &["acme"]);
    export_skills(&vfs, &request("acme")).unwrap();
    let manifest = vfs
        .read_file(Path::new(
            "/v/dist/acme/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
        ))
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    assert_eq!(manifest["license"], "MIT");
    assert_eq!(manifest["homepage"], "https://root.example");
}

#[test]
fn the_root_files_harnesses_decide_with_no_filter() {
    let vfs = vault("build:\n  harnesses: [claude, codex]\n", &["acme"]);
    export_skills(&vfs, &request("acme")).unwrap();
    assert!(vfs.exists(Path::new("/v/dist/acme/.claude-plugin")));
    assert!(vfs.exists(Path::new("/v/dist/acme/.agents")));
    assert!(!vfs.exists(Path::new("/v/dist/acme/.cursor-plugin")));
    assert!(!vfs.exists(Path::new("/v/dist/acme/.pi")));
}

#[test]
fn the_harness_filter_narrows_the_root_files_list() {
    let vfs = vault("build:\n  harnesses: [claude, codex]\n", &["acme"]);
    let mut narrowed = request("acme");
    narrowed.harnesses = BTreeSet::from(["codex".to_owned()]);
    export_skills(&vfs, &narrowed).unwrap();
    assert!(vfs.exists(Path::new("/v/dist/acme/.agents")));
    assert!(!vfs.exists(Path::new("/v/dist/acme/.claude-plugin")));
}

#[test]
fn the_harness_filter_never_widens_the_root_files_list() {
    let vfs = vault("build:\n  harnesses: [claude]\n", &["acme"]);
    let mut widened = request("acme");
    widened.harnesses = BTreeSet::from(["claude".to_owned(), "pi".to_owned()]);
    let err = export_skills(&vfs, &widened).unwrap_err().to_string();
    assert!(err.contains("refused: harness-not-built"), "{err}");
    assert!(err.contains("`pi`"), "{err}");
    assert!(!vfs.exists(Path::new("/v/dist")));

    // With no list in the root file, every harness is built, so any narrows.
    let vfs = vault("", &["acme"]);
    let mut narrowed = request("acme");
    narrowed.harnesses = BTreeSet::from(["pi".to_owned()]);
    export_skills(&vfs, &narrowed).unwrap();
    assert!(vfs.exists(Path::new("/v/dist/acme/.pi")));
    assert!(!vfs.exists(Path::new("/v/dist/acme/.claude-plugin")));
}

#[test]
fn build_vault_builds_every_marketplace() {
    let vfs = vault("", &["acme", "shop"]);
    let report = build_vault(&vfs, &VaultRequest::new("/v")).unwrap();
    let names: Vec<&str> = report
        .marketplaces
        .iter()
        .map(|m| m.marketplace.as_str())
        .collect();
    assert_eq!(names, ["acme", "shop"]);
    assert!(report.marketplaces.iter().all(|m| m.report.changed));
    for m in ["acme", "shop"] {
        assert!(vfs.exists(&Path::new("/v/dist").join(m).join(".claude-plugin")));
    }

    let mut check = VaultRequest::new("/v");
    check.mode = ExportMode::Check;
    let report = build_vault(&vfs, &check).unwrap();
    assert!(report.marketplaces.iter().all(|m| !m.report.changed));
}

/// A caller that has already loaded the vault plans it without a second load.
#[test]
fn a_loaded_vault_plans_each_of_its_marketplaces() {
    let vfs = vault("", &["acme", "shop"]);
    let loaded = load_vault(&vfs, Path::new("/v"), Some("shop")).unwrap();
    let plan = plan_loaded(&vfs, &loaded, ExportMode::Check, &BTreeSet::new());
    let names: Vec<&str> = plan
        .marketplaces
        .iter()
        .map(|(market, _)| market.name.as_str())
        .collect();
    assert_eq!(names, ["shop"]);
    assert!(plan.marketplaces.iter().all(|(_, planned)| planned.is_ok()));
}

#[test]
fn a_marketplace_that_fails_to_plan_writes_no_marketplace() {
    let vfs = vault("", &["acme", "shop"]);
    // `shop` holds a skill with no description.
    write(
        &vfs,
        "/v/marketplaces/shop/plugins/tools/skills/review/review.md",
        "---\n---\n\nNo description.\n",
    );
    let err = build_vault(&vfs, &VaultRequest::new("/v")).unwrap_err();
    assert!(matches!(err, CoreError::Refused { .. }), "{err}");
    assert!(!vfs.exists(Path::new("/v/dist")));
}

#[test]
fn a_failed_swap_in_the_second_marketplace_rolls_back_the_first() {
    let vfs = vault("", &["acme", "shop"]);
    let failing = FailingRename {
        inner: &vfs,
        to: PathBuf::from("/v/dist/shop/.agents/packs"),
        remaining: Cell::new(1),
    };
    let err = build_vault(&failing, &VaultRequest::new("/v")).unwrap_err();
    assert!(matches!(err, CoreError::SwapFailed(_)), "{err}");
    assert!(err.to_string().contains("rolled back"), "{err}");
    // Neither marketplace holds a built file, and no stage is left.
    assert_eq!(files_under(&vfs, Path::new("/v/dist")), BTreeMap::new());

    // Over a built vault, a failed rebuild leaves both as they were.
    build_vault(&vfs, &VaultRequest::new("/v")).unwrap();
    for m in ["acme", "shop"] {
        write(
            &vfs,
            &format!("/v/marketplaces/{m}/plugins/tools/skills/review/review.md"),
            "---\ndescription: Use it.\n---\n\nRevised.\n",
        );
    }
    let before = files_under(&vfs, Path::new("/v/dist"));
    let failing = FailingRename {
        inner: &vfs,
        to: PathBuf::from("/v/dist/shop/.agents/packs"),
        remaining: Cell::new(1),
    };
    build_vault(&failing, &VaultRequest::new("/v")).unwrap_err();
    assert_eq!(files_under(&vfs, Path::new("/v/dist")), before);
}

/// Fails the next `remaining` renames onto `to`.
struct FailingRename<'a> {
    inner: &'a MemVfs,
    to: PathBuf,
    remaining: Cell<u32>,
}

impl Vfs for FailingRename<'_> {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        self.inner.read_file(path)
    }
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        self.inner.read_dir(path)
    }
    fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<(), VfsError> {
        self.inner.write_file(path, bytes)
    }
    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        self.inner.create_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn metadata(&self, path: &Path) -> Result<Metadata, VfsError> {
        self.inner.metadata(path)
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.inner.is_executable(path)
    }
    fn set_executable(&self, path: &Path, yes: bool) -> Result<(), VfsError> {
        self.inner.set_executable(path, yes)
    }
    fn rename(&self, from: &Path, to: &Path) -> Result<(), VfsError> {
        if to == self.to && self.remaining.get() > 0 {
            self.remaining.set(self.remaining.get() - 1);
            return Err(VfsError::io(to, std::io::Error::other("injected failure")));
        }
        self.inner.rename(from, to)
    }
    fn remove(&self, path: &Path) -> Result<(), VfsError> {
        self.inner.remove(path)
    }
}
