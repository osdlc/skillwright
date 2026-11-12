//! Every `refusal` token is used by the core, the wasm binding or the CLI.
//! Reads sibling crates' source, so it lives in `tests/`, which is not published.

use std::path::Path;

/// Appends every `.rs` file under `dir` to `text`.
fn sources(dir: &Path, text: &mut String) {
    for path in std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()) {
        if path.is_dir() {
            sources(&path, text);
        } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("refusal.rs") {
            text.push_str(&std::fs::read_to_string(&path).unwrap());
        }
    }
}

/// Whether `refusal::<name>` appears as a whole identifier.
fn is_used(used: &str, name: &str) -> bool {
    let needle = format!("refusal::{name}");
    used.match_indices(&needle).any(|(i, _)| {
        !used[i + needle.len()..].starts_with(|c: char| c == '_' || c.is_ascii_alphanumeric())
    })
}

#[test]
fn every_token_is_used() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut used = String::new();
    for krate in ["tradecraft-core", "tradecraft-wasm", "skillwright"] {
        sources(&crates.join(krate).join("src"), &mut used);
    }
    let mut seen = 0;
    for line in include_str!("../src/refusal.rs").lines() {
        let Some(name) = line
            .strip_prefix("pub const ")
            .and_then(|rest| rest.split_once(": Token = Token("))
            .map(|(name, _)| name)
        else {
            continue;
        };
        seen += 1;
        assert!(is_used(&used, name), "`{name}` is never used");
    }
    assert!(seen > 0, "no token parsed");
}
