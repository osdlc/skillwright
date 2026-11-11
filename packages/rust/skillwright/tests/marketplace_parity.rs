//! The CLI's `build` verb and a request built the way another host builds
//! it must render byte-identical trees for the same vault.
//!
//! Both sides reach the same `tradecraft_core::export_skills`. The host side
//! is not reachable from here, so this calls that function directly with a
//! request shaped the way a host that embeds the core shapes it: name,
//! display_name, owner and author absent, `plugins` empty. What it proves:
//! the CLI's own flag-to-request resolution in `commands/marketplace.rs` and
//! a host's request-building do not disagree about what an equivalent build
//! means.
//!
//! The vault is `freeform-vault`, the shared parity fixture, with two
//! marketplaces. The CLI builds a copy of it with `--marketplace acme` into
//! the copy's `dist/acme/`; the host names `acme` on its request.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use skillwright::cli::run_with;
use skillwright::ctx::CommandContext;
use tradecraft_core::{ExportRequest, NativeVfs, export_skills};

fn fixture_vault() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/freeform-vault")
        .canonicalize()
        .unwrap()
}

/// Every file under `root`, as a path relative to it, sorted.
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for entry in fs::read_dir(root.join(&rel)).unwrap() {
            let entry = entry.unwrap();
            let child = rel.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                stack.push(child);
            } else {
                out.push(child);
            }
        }
    }
    out.sort();
    out
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o111 != 0
}

/// Windows has no executable bit, so both trees read as not executable.
#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    false
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

#[test]
fn the_cli_and_a_host_request_render_identical_trees() {
    let vault = fixture_vault();
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("freeform-vault");
    copy_tree(&vault, &copy);
    let cli_out = copy.join("dist/acme");
    // The host builds its own copy: a build always writes `<root>/dist/<m>/`.
    let host_copy = dir.path().join("host/freeform-vault");
    copy_tree(&vault, &host_copy);
    let host_out = host_copy.join("dist/acme");

    let argv: Vec<String> = ["build", "--marketplace", "acme", "--yes"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = {
        let mut ctx = CommandContext::new(&copy, &mut out, &mut err);
        run_with(&argv, &mut ctx)
    };
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    assert!(cli_out.is_dir());
    assert!(!copy.join("dist/labs").exists());

    export_skills(&NativeVfs, &{
        // The core reads the root file itself, as the CLI's build does.
        let mut request = ExportRequest::new(host_copy.clone());
        request.marketplace = Some("acme".to_string());
        request
    })
    .unwrap();
    assert!(host_out.is_dir());

    // The tree shape first: a path only one side produced is the whole
    // difference, and deserves its own failure rather than being buried in a
    // per-file loop that never reaches it.
    let cli_files = walk(&cli_out);
    assert_eq!(cli_files, walk(&host_out));

    for rel in &cli_files {
        let a = cli_out.join(rel);
        let b = host_out.join(rel);
        assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap(), "{rel:?}");
        // The fixture's `bundled/scripts/run.sh` exists to prove the execute
        // bit travels through a build — a render that matched every byte but
        // dropped it would not be the identical tree this test requires.
        assert_eq!(is_executable(&a), is_executable(&b), "{rel:?}");
    }
}
