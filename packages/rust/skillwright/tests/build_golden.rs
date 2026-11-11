//! Every harness's build of the fixture vaults, pinned file by file.
//!
//! Each fixture vault is copied to a temporary folder and built there. Each
//! listing under `tests/fixtures/golden/` is a sorted `path  sha256` line per
//! file under the copy's `dist/`. A change to what a build writes shows here first.
//! When the change is meant, refresh the listings from `packages/rust` with
//! `TRADECRAFT_BLESS=1 cargo test -p skillwright --test build_golden`.

mod common;

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Every file under `root`, relative to it with `/` separators, sorted.
fn files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for entry in fs::read_dir(root.join(&rel)).unwrap() {
            let entry = entry.unwrap();
            let child = rel.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                stack.push(child);
            } else {
                out.push(child.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

/// A recursive copy. `fs::copy` keeps each file's permissions.
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// `vault`'s build output as a `path  sha256` listing: a copy of the fixture
/// built in a temporary folder, every file under its `dist/`.
fn listing(vault: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(vault);
    copy_tree(&fixtures().join(vault), &root);
    let run = common::run(&["build", "--yes", "--quiet"], &root);
    assert_eq!(run.code, 0, "{vault}: {}", run.stderr);
    let dist = root.join("dist");
    let mut text = String::new();
    for file in files(&dist) {
        let hash = Sha256::digest(fs::read(dist.join(&file)).unwrap());
        writeln!(text, "{file}  {hash:x}").unwrap();
    }
    text
}

#[test]
fn every_harness_builds_the_committed_tree() {
    let bless = std::env::var_os("TRADECRAFT_BLESS").is_some();
    for (vault, markets) in [
        ("freeform-vault", &["acme", "labs"][..]),
        ("vault", &["studio"][..]),
        ("identity-vault", &["guild"][..]),
    ] {
        let generated = listing(vault);
        for market in markets {
            for root in [".claude-plugin/", ".agents/", ".cursor-plugin/", ".pi/"] {
                assert!(
                    generated.contains(&format!("{market}/{root}")),
                    "{vault} built nothing under dist/{market}/{root}"
                );
            }
        }
        let golden = fixtures().join(format!("golden/{vault}.txt"));
        if bless {
            fs::create_dir_all(golden.parent().unwrap()).unwrap();
            fs::write(&golden, &generated).unwrap();
        }
        let committed = fs::read_to_string(&golden).unwrap();
        assert!(
            committed == generated,
            "{vault}'s build drifted from {}; see this test's module doc.\n{generated}",
            golden.display()
        );
    }
}
