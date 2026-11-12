//! The root file as the one config channel at the API, and `build_vault`:
//! every marketplace planned, then all written or none.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tradecraft_core::vfs::{DirEntry, Metadata, VfsError};
use tradecraft_core::{
    BuildScope, CoreError, ExportMode, MemVfs, VaultRequest, Vfs, build_vault, load_vault,
    model::LoadRequest,
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

fn request(marketplace: &str) -> VaultRequest {
    let mut request = VaultRequest::new("/v");
    request.scope = BuildScope::Marketplace(marketplace.to_owned());
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
        serde_json::from_str::<LoadRequest>(json).unwrap_err(),
        serde_json::from_str::<VaultRequest>(json).unwrap_err(),
    ] {
        assert!(err.to_string().contains("defaults"), "{err}");
    }
}

#[test]
fn the_core_reads_the_root_files_defaults() {
    let vfs = vault("license: MIT\nhomepage: https://root.example\n", &["acme"]);
    build_vault(&vfs, &request("acme")).unwrap();
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
    build_vault(&vfs, &request("acme")).unwrap();
    assert!(vfs.exists(Path::new("/v/dist/acme/.claude-plugin")));
    assert!(vfs.exists(Path::new("/v/dist/acme/.agents")));
    assert!(!vfs.exists(Path::new("/v/dist/acme/.cursor-plugin")));
    assert!(!vfs.exists(Path::new("/v/dist/acme/.pi")));
}

#[test]
fn the_requests_harnesses_can_narrow_the_root_files_list() {
    let vfs = vault("build:\n  harnesses: [claude, codex]\n", &["acme"]);
    let mut narrowed = request("acme");
    narrowed.options.harnesses = BTreeSet::from(["codex".to_owned()]);
    build_vault(&vfs, &narrowed).unwrap();
    assert!(vfs.exists(Path::new("/v/dist/acme/.agents")));
    assert!(!vfs.exists(Path::new("/v/dist/acme/.claude-plugin")));
}

#[test]
fn the_requests_harnesses_replace_the_root_files_list() {
    let vfs = vault("build:\n  harnesses: [claude]\n", &["acme"]);
    let mut widened = request("acme");
    widened.options.harnesses = BTreeSet::from(["codex".to_owned(), "pi".to_owned()]);
    build_vault(&vfs, &widened).unwrap();
    assert!(vfs.exists(Path::new("/v/dist/acme/.agents")));
    assert!(vfs.exists(Path::new("/v/dist/acme/.pi")));
    assert!(!vfs.exists(Path::new("/v/dist/acme/.claude-plugin")));

    // With no list in the root file, the request still decides.
    let vfs = vault("", &["acme"]);
    let mut narrowed = request("acme");
    narrowed.options.harnesses = BTreeSet::from(["pi".to_owned()]);
    build_vault(&vfs, &narrowed).unwrap();
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
    check.options.mode = ExportMode::Check;
    let report = build_vault(&vfs, &check).unwrap();
    assert!(report.marketplaces.iter().all(|m| !m.report.changed));
}

#[test]
fn a_named_marketplace_is_the_only_one_built() {
    let vfs = vault("", &["acme", "shop"]);
    let loaded = load_vault(
        &vfs,
        Path::new("/v"),
        &BuildScope::Marketplace("shop".into()),
    )
    .unwrap();
    let report = build_vault(
        &vfs,
        &VaultRequest::for_marketplace(&loaded.root, &loaded.marketplaces[0].name),
    )
    .unwrap();
    let names: Vec<&str> = report
        .marketplaces
        .iter()
        .map(|m| m.marketplace.as_str())
        .collect();
    assert_eq!(names, ["shop"]);
    assert!(vfs.exists(Path::new("/v/dist/shop")));
    assert!(!vfs.exists(Path::new("/v/dist/acme")));
}

/// A check can compare with another built folder, as `bump --changed-since`
/// does. A build cannot be pointed anywhere, and one folder cannot stand for
/// several marketplaces.
#[test]
fn a_check_compares_with_another_folder() {
    let vfs = vault("", &["acme", "shop"]);
    build_vault(&vfs, &VaultRequest::new("/v")).unwrap();
    vfs.rename(Path::new("/v/dist/shop"), Path::new("/baseline"))
        .unwrap();

    let mut check = request("shop");
    check.options.mode = ExportMode::Check;
    check.options.compare_with = Some(PathBuf::from("/baseline"));
    let report = build_vault(&vfs, &check).unwrap();
    assert!(!report.marketplaces[0].report.changed);
    // Without it, the check compares with the missing `dist/shop/`.
    check.options.compare_with = None;
    assert!(
        build_vault(&vfs, &check).unwrap().marketplaces[0]
            .report
            .changed
    );

    let mut build = request("shop");
    build.options.compare_with = Some(PathBuf::from("/baseline"));
    let err = build_vault(&vfs, &build).unwrap_err().to_string();
    assert!(err.contains("compare-with"), "{err}");
    assert!(!vfs.exists(Path::new("/v/dist/shop")));

    let mut several = VaultRequest::new("/v");
    several.options.mode = ExportMode::Check;
    several.options.compare_with = Some(PathBuf::from("/baseline"));
    let err = build_vault(&vfs, &several).unwrap_err().to_string();
    assert!(err.contains("compare-with"), "{err}");
    assert!(err.contains("`marketplace`"), "{err}");
}

#[test]
fn compare_with_crosses_on_the_wire() {
    let request: VaultRequest =
        serde_json::from_str(r#"{ "vault_root": "/v", "compare_with": "/baseline" }"#).unwrap();
    assert_eq!(
        request.options.compare_with,
        Some(PathBuf::from("/baseline"))
    );
    let request: VaultRequest = serde_json::from_str(r#"{ "vault_root": "/v" }"#).unwrap();
    assert_eq!(request.options.compare_with, None);
}

fn check() -> VaultRequest {
    let mut check = VaultRequest::new("/v");
    check.options.mode = ExportMode::Check;
    check
}

#[test]
fn a_check_reports_every_drifted_marketplace() {
    let vfs = vault("", &["acme", "shop"]);
    let report = build_vault(&vfs, &check()).unwrap();
    let drifted: Vec<(&str, bool)> = report
        .marketplaces
        .iter()
        .map(|m| (m.marketplace.as_str(), m.report.changed))
        .collect();
    assert_eq!(drifted, [("acme", true), ("shop", true)]);
    assert!(report.refused.is_empty());
    assert!(!vfs.exists(Path::new("/v/dist")));
}

#[test]
fn a_check_reports_every_marketplace_when_two_refuse() {
    let vfs = vault("", &["acme", "shop", "zoo"]);
    // `acme` and `zoo` hold a skill with no description.
    for m in ["acme", "zoo"] {
        write(
            &vfs,
            &format!("/v/marketplaces/{m}/plugins/tools/skills/review/review.md"),
            "---\n---\n\nNo description.\n",
        );
    }
    let report = build_vault(&vfs, &check()).unwrap();
    let [shop] = &report.marketplaces[..] else {
        panic!("{report:?}");
    };
    assert_eq!(shop.marketplace, "shop");
    assert!(shop.report.changed);
    let refused: Vec<&str> = report
        .refused
        .iter()
        .map(|r| r.marketplace.as_str())
        .collect();
    assert_eq!(refused, ["acme", "zoo"]);
    for r in &report.refused {
        assert!(r.token.is_some(), "{r:?}");
    }
    assert!(!vfs.exists(Path::new("/v/dist")));
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

#[test]
fn a_note_that_is_not_utf8_is_skipped_by_the_scan_and_refused_by_the_build() {
    let vfs = vault("", &["m"]);
    let path = Path::new("/v/marketplaces/m/plugins/tools/skills/review/review.md");
    vfs.create_dir_all(path.parent().unwrap()).unwrap();
    vfs.write_file(path, &[0xff, 0xfe]).unwrap();

    let scan = tradecraft_core::discover::discover_components(&vfs, Path::new("/v")).unwrap();
    assert_eq!(scan.skipped.len(), 1, "{:?}", scan.skipped);
    assert_eq!(scan.skipped[0].reason, "unreadable-note");

    let err = build_vault(&vfs, &VaultRequest::new("/v")).unwrap_err();
    assert_eq!(err.token(), Some(tradecraft_core::refusal::UNREADABLE_NOTE));
}
