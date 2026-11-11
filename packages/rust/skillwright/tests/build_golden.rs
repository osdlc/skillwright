//! Every harness's build of the fixture vaults, pinned file by file.
//!
//! Each listing under `tests/fixtures/golden/` is a sorted `path  sha256`
//! line per output file. A change to what a build writes shows here first.
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

/// `vault`'s build output as a `path  sha256` listing.
fn listing(vault: &str, extra: &[&str]) -> String {
    let source = fixtures().join(vault);
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let mut argv = vec![
        "build".to_owned(),
        "--vault".to_owned(),
        common::vault_str(&source),
        "--out".to_owned(),
        out.display().to_string(),
        "--yes".to_owned(),
        "--quiet".to_owned(),
    ];
    argv.extend(extra.iter().map(|arg| (*arg).to_owned()));
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    let run = common::run(&argv, dir.path());
    assert_eq!(run.code, 0, "{vault}: {}", run.stderr);
    let mut text = String::new();
    for file in files(&out) {
        let hash = Sha256::digest(fs::read(out.join(&file)).unwrap());
        writeln!(text, "{file}  {hash:x}").unwrap();
    }
    text
}

#[test]
fn every_harness_builds_the_committed_tree() {
    let bless = std::env::var_os("TRADECRAFT_BLESS").is_some();
    for (vault, extra) in [
        ("freeform-vault", &["--marketplace", "acme"][..]),
        ("vault", &[][..]),
        ("identity-vault", &[][..]),
    ] {
        let generated = listing(vault, extra);
        for root in [".claude-plugin/", ".agents/", ".cursor-plugin/", ".pi/"] {
            assert!(
                generated.contains(root),
                "{vault} built nothing under {root}"
            );
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
