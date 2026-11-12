//! The repository's own `plugin/` vault builds with no warning and matches
//! its committed `plugin/dist/`, as the `plugin` workflow checks.

mod common;

use std::path::PathBuf;

use common::run;

#[test]
fn the_repo_plugin_builds_with_no_warning_and_no_drift() {
    let vault = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../plugin");
    let vault = vault.canonicalize().unwrap();
    let ran = run(
        &["check", "--json", "--vault", &vault.display().to_string()],
        &vault,
    );
    let report: serde_json::Value = serde_json::from_str(&ran.stdout)
        .unwrap_or_else(|e| panic!("{e}\nstdout:\n{}\nstderr:\n{}", ran.stdout, ran.stderr));
    assert_eq!(report["warnings"], serde_json::json!([]), "{}", ran.stderr);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
}
