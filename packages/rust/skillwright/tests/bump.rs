//! `skillwright bump` — spec section 10 rule 4, driven in-process the way
//! `tests/cli.rs` drives every other verb: [`run_with`] over a vault written
//! into a temp folder.

mod common;

use common::{Run, git, run, vault_str};

use std::fs;
use std::path::{Path, PathBuf};

// --- Writing a vault --------------------------------------------------------

/// Writes `text` to `path`, creating its folder.
fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// The note of plugin `plugin` in marketplace `market`.
fn plugin_path(root: &Path, market: &str, plugin: &str) -> PathBuf {
    root.join("marketplaces")
        .join(market)
        .join("plugins")
        .join(plugin)
        .join(format!("{plugin}.md"))
}

/// The note of marketplace `market`.
fn market_path(root: &Path, market: &str) -> PathBuf {
    root.join("marketplaces")
        .join(market)
        .join(format!("{market}.md"))
}

/// Plugins by name, each with its `version` or none.
type Plugins<'a> = &'a [(&'a str, Option<&'a str>)];

/// Writes a v2 vault at `root`: the root file holding `root_fm`, and each
/// marketplace with its plugins. A plugin note holds `description` and,
/// when given, `version`; each plugin has one skill, `<plugin>-skill`.
fn vault(root: &Path, root_fm: &str, markets: &[(&str, Plugins)]) {
    write(
        &root.join("skillwright.md"),
        &format!("---\n{root_fm}---\n"),
    );
    for (market, plugins) in markets {
        write(&market_path(root, market), "---\n---\n");
        for (plugin, version) in *plugins {
            let version = version
                .map(|v| format!("version: {v}\n"))
                .unwrap_or_default();
            write(
                &plugin_path(root, market, plugin),
                &format!("---\ndescription: The {plugin} plugin.\n{version}---\n"),
            );
            skill_in(root, market, plugin, "First draft.");
        }
    }
}

/// Writes the skill `<plugin>-skill` of `plugin` in `market` with `body`.
fn skill_in(root: &Path, market: &str, plugin: &str, body: &str) {
    let name = format!("{plugin}-skill");
    write(
        &root
            .join("marketplaces")
            .join(market)
            .join("plugins")
            .join(plugin)
            .join("skills")
            .join(&name)
            .join(format!("{name}.md")),
        &format!("---\ndescription: Use {plugin}.\n---\n\n{body}\n"),
    );
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

// --- One plugin -------------------------------------------------------------

/// A vault at `<dir>/vault` with plugin `widget` in marketplace `acme`.
/// `version_line` is spliced into its note as a whole frontmatter line, or
/// left out for a note with no `version`.
fn fixture(version_line: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    vault(&root, "", &[("acme", &[("widget", None)])]);
    let version = version_line.map(|v| format!("{v}\n")).unwrap_or_default();
    write(
        &note_path(&root),
        &format!(
            "---\ndescription: A plugin for bump tests.\n{version}---\n\nBody text, untouched by a bump.\n"
        ),
    );
    (dir, root)
}

fn note_path(vault: &Path) -> PathBuf {
    plugin_path(vault, "acme", "widget")
}

const REL: &str = "marketplaces/acme/plugins/widget/widget.md";

#[test]
fn dry_run_prints_old_arrow_new_and_leaves_the_note_untouched() {
    let (dir, vault) = fixture(Some("version: 0.2.1"));
    let path = note_path(&vault);
    let before = fs::read(&path).unwrap();

    let result = run(
        &[
            "bump",
            "widget",
            "--minor",
            "--dry-run",
            "--vault",
            &vault_str(&vault),
        ],
        dir.path(),
    );

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, format!("{REL}  0.2.1 \u{2192} 0.3.0\n"));
    let after = fs::read(&path).unwrap();
    assert_eq!(before, after, "dry-run must not touch the note's bytes");
}

#[test]
fn a_real_bump_rewrites_only_the_version_line() {
    let (dir, vault) = fixture(Some("version: 0.2.1"));
    let path = note_path(&vault);
    let before = read(&path);

    let result = run(
        &[
            "bump",
            "widget",
            "--minor",
            "--yes",
            "--vault",
            &vault_str(&vault),
        ],
        dir.path(),
    );

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        result.stdout,
        format!("{REL}  written · version 0.2.1 -> 0.3.0\n")
    );

    let after = read(&path);
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    assert_eq!(
        before_lines.len(),
        after_lines.len(),
        "a bump must not add or remove a line"
    );
    let mut changed = 0;
    for (b, a) in before_lines.iter().zip(after_lines.iter()) {
        if b != a {
            changed += 1;
            assert_eq!(*b, "version: 0.2.1");
            assert_eq!(*a, "version: 0.3.0");
        }
    }
    assert_eq!(changed, 1, "exactly one line may change");
}

#[test]
fn no_version_at_all_bumps_from_the_core_default() {
    let (dir, vault) = fixture(None);
    let path = note_path(&vault);
    let before = read(&path);
    assert!(!before.contains("version"));

    let result = run(
        &[
            "bump",
            "widget",
            "--patch",
            "--yes",
            "--vault",
            &vault_str(&vault),
        ],
        dir.path(),
    );

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        result.stdout,
        format!("{REL}  written · version 0.1.0 -> 0.1.1\n")
    );

    let after = read(&path);
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    assert_eq!(
        after_lines.len(),
        before_lines.len() + 1,
        "gaining a missing version must add exactly one line, no stray \
         blank line alongside it\nbefore: {before_lines:?}\nafter: {after_lines:?}"
    );

    // The lines up to the insertion point are untouched, and every line
    // after it reappears unchanged, shifted by one.
    let insert_at = before_lines
        .iter()
        .zip(after_lines.iter())
        .position(|(b, a)| b != a)
        .unwrap_or(before_lines.len());
    assert_eq!(before_lines[..insert_at], after_lines[..insert_at]);
    assert_eq!(after_lines[insert_at], "version: 0.1.1");
    assert_eq!(before_lines[insert_at..], after_lines[insert_at + 1..]);
}

#[test]
fn a_non_semver_version_is_refused_and_the_note_is_untouched() {
    // Quoted: an unquoted `0.3` is a YAML float, which fails the note's
    // discovery before `bump` reaches its own non-semver refusal.
    let (dir, vault) = fixture(Some("version: \"0.3\""));
    let path = note_path(&vault);
    let before = fs::read(&path).unwrap();

    let result = run(
        &[
            "bump",
            "widget",
            "--patch",
            "--yes",
            "--vault",
            &vault_str(&vault),
        ],
        dir.path(),
    );

    assert_eq!(result.code, 1, "{}", result.stderr);
    assert_eq!(
        result.stdout,
        format!("{REL}  refused · non-semver: `version` reads `0.3`\n")
    );
    let after = fs::read(&path).unwrap();
    assert_eq!(before, after, "a refusal must not touch the note's bytes");
}

#[test]
fn no_yes_off_a_tty_is_a_usage_error_and_writes_nothing() {
    let (dir, vault) = fixture(Some("version: 0.2.1"));
    let path = note_path(&vault);
    let before = fs::read(&path).unwrap();

    let result = run(
        &["bump", "widget", "--minor", "--vault", &vault_str(&vault)],
        dir.path(),
    );

    assert_eq!(result.code, 3, "{}", result.stderr);
    assert_eq!(result.stdout, "");
    let after = fs::read(&path).unwrap();
    assert_eq!(before, after, "a declined/unconfirmable run must not write");
}

// --- Which note a bump writes -----------------------------------------------

#[test]
fn bump_of_a_plugin_writes_its_note() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    vault(root, "", &[("acme", &[("tools", Some("1.2.3"))])]);
    write(&market_path(root, "acme"), "---\nversion: 4.0.0\n---\n");

    let result = run(&["bump", "tools", "--yes"], root);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        result.stdout,
        "marketplaces/acme/plugins/tools/tools.md  written · version 1.2.3 -> 1.2.4\n"
    );
    assert!(read(&plugin_path(root, "acme", "tools")).contains("version: 1.2.4"));
    assert_eq!(
        read(&market_path(root, "acme")),
        "---\nversion: 4.0.0\n---\n"
    );
}

#[test]
fn bump_of_a_marketplace_writes_its_note() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    vault(root, "", &[("acme", &[("tools", Some("1.2.3"))])]);
    write(&market_path(root, "acme"), "---\nversion: 4.0.0\n---\n");

    let result = run(&["bump", "--marketplace", "acme", "--yes"], root);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        result.stdout,
        "marketplaces/acme/acme.md  written · version 4.0.0 -> 4.0.1\n"
    );
    assert_eq!(
        read(&market_path(root, "acme")),
        "---\nversion: 4.0.1\n---\n"
    );
    assert!(read(&plugin_path(root, "acme", "tools")).contains("version: 1.2.3"));
}

#[test]
fn bump_of_the_marketplace_init_writes_adds_one_version_key() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = run(&["init", "acme", "tools"], root);
    assert_eq!(init.code, 0, "{}", init.stderr);
    skill_in(root, "acme", "tools", "One.");
    let note = market_path(root, "acme");
    assert!(read(&note).starts_with("---\n---\n"), "{}", read(&note));

    let result = run(&["bump", "--marketplace", "acme", "--yes"], root);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let text = read(&note);
    assert!(text.starts_with("---\nversion: 0.1.1\n---\n"), "{text}");
    assert_eq!(text.matches(':').count(), 1, "{text}");
}

#[test]
fn a_plugin_named_after_its_marketplace_is_just_a_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    vault(root, "", &[("osdlc", &[("osdlc", Some("1.0.0"))])]);
    write(&market_path(root, "osdlc"), "---\nversion: 3.0.0\n---\n");

    let result = run(&["bump", "osdlc", "--yes"], root);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(read(&plugin_path(root, "osdlc", "osdlc")).contains("version: 1.0.1"));
    assert_eq!(
        read(&market_path(root, "osdlc")),
        "---\nversion: 3.0.0\n---\n"
    );
}

// --- A plugin name two marketplaces share -----------------------------------

/// A vault whose marketplaces `a` and `b` each hold plugin `tools` at `1.0.0`.
fn shared_plugin(root: &Path) {
    let tools: Plugins = &[("tools", Some("1.0.0"))];
    vault(root, "", &[("a", tools), ("b", tools)]);
}

fn tools_note(root: &Path, market: &str) -> String {
    read(&plugin_path(root, market, "tools"))
}

#[test]
fn a_plugin_name_two_marketplaces_share_needs_marketplace() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    shared_plugin(root);

    let flagless = run(&["bump", "tools", "--yes"], root);
    assert_eq!(flagless.code, 3, "{}", flagless.stderr);
    assert!(
        flagless.stderr.contains("--marketplace"),
        "{}",
        flagless.stderr
    );
    assert!(tools_note(root, "a").contains("version: 1.0.0"));
    assert!(tools_note(root, "b").contains("version: 1.0.0"));

    let one = run(&["bump", "tools", "--marketplace", "b", "--yes"], root);
    assert_eq!(one.code, 0, "{}", one.stderr);
    assert!(tools_note(root, "b").contains("version: 1.0.1"));
    assert!(tools_note(root, "a").contains("version: 1.0.0"));

    let unknown = run(&["bump", "tools", "--marketplace", "nope", "--yes"], root);
    assert_eq!(unknown.code, 2, "{}", unknown.stderr);
    assert!(unknown.stderr.contains("nope"), "{}", unknown.stderr);
}

#[test]
fn check_advises_a_bump_that_runs_in_a_vault_of_several_marketplaces() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    shared_plugin(root);
    let built = run(&["build", "--marketplace", "b", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    skill_in(root, "b", "tools", "Two.");

    let check = run(&["check", "--marketplace", "b"], root);
    let advised = check
        .stderr
        .lines()
        .find_map(|line| {
            line.split('`')
                .nth(1)
                .filter(|c| c.starts_with("skillwright bump"))
        })
        .unwrap_or_else(|| panic!("no bump advice: {}", check.stderr))
        .to_owned();
    assert_eq!(advised, "skillwright bump tools --marketplace b");
    let mut argv: Vec<&str> = advised.split(' ').skip(1).collect();
    argv.push("--yes");
    let bumped = run(&argv, root);
    assert_eq!(bumped.code, 0, "{}", bumped.stderr);
    assert!(tools_note(root, "b").contains("version: 1.0.1"));
    assert!(tools_note(root, "a").contains("version: 1.0.0"));
}

// --- `build.bump.patch: timestamp` ------------------------------------------

/// The UTC time as `build.bump.patch: timestamp` writes it.
fn utc_stamp() -> u64 {
    chrono::Utc::now()
        .format("%Y%m%d%H%M%S")
        .to_string()
        .parse()
        .unwrap()
}

/// A vault with the timestamp scheme and plugin `tools` at `version`, after
/// `bump tools`: the new version.
fn timestamp_bump(version: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    vault(
        root,
        "build:\n  bump:\n    patch: timestamp\n",
        &[("acme", &[("tools", Some(version))])],
    );
    let result = run(&["bump", "tools", "--yes", "--json"], root);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let row: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    let new = row["new"].as_str().unwrap().to_owned();
    assert!(tools_note(root, "acme").contains(&format!("version: {new}\n")));
    new
}

#[test]
fn a_timestamp_patch_is_the_utc_time() {
    let before = utc_stamp();
    let new = timestamp_bump("1.2.3");
    let after = utc_stamp();

    let patch = new.strip_prefix("1.2.").unwrap();
    assert_eq!(patch.len(), 14, "YYYYMMDDHHMMSS: {new}");
    assert!(patch.bytes().all(|b| b.is_ascii_digit()), "{new}");
    let patch: u64 = patch.parse().unwrap();
    assert!(
        before <= patch && patch <= after,
        "{before} <= {patch} <= {after}"
    );
}

#[test]
fn a_timestamp_patch_larger_than_now_rises_by_one() {
    assert_eq!(timestamp_bump("1.2.99999999999999"), "1.2.100000000000000");
}

// --- `--changed-since` a built folder ---------------------------------------

/// Plugins `widget` (`0.2.1`) and `gadget` (`1.0.0`) in marketplace `acme`,
/// one skill each, with `root_fm` in the root file.
fn project(root_fm: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    project_in(dir.path(), root_fm);
    dir
}

/// [`project`]'s vault, written into `root`.
fn project_in(root: &Path, root_fm: &str) {
    vault(
        root,
        root_fm,
        &[(
            "acme",
            &[("widget", Some("0.2.1")), ("gadget", Some("1.0.0"))],
        )],
    );
}

fn write_skill(root: &Path, plugin: &str, body: &str) {
    skill_in(root, "acme", plugin, body);
}

fn plugin_note(root: &Path, plugin: &str) -> String {
    read(&plugin_path(root, "acme", plugin))
}

/// Where `build` writes marketplace `acme`.
const BUILT: &str = "dist/acme";

#[test]
fn changed_since_a_built_folder_bumps_only_the_plugin_that_changed() {
    let dir = project("");
    let root = dir.path();
    let built = run(&["build", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    write_skill(root, "widget", "Second draft.");

    let dry = run(
        &["bump", "--changed-since", BUILT, "--dry-run", "--json"],
        root,
    );
    assert_eq!(dry.code, 0, "{}", dry.stderr);
    let report: serde_json::Value = serde_json::from_str(&dry.stdout).unwrap();
    assert_eq!(report["baseline"], BUILT);
    let rows = report["plugins"].as_array().unwrap();
    let row = |name: &str| {
        rows.iter()
            .find(|r| r["plugin"] == name)
            .unwrap_or_else(|| panic!("no {name}: {rows:?}"))
    };
    assert_eq!(row("widget")["status"], "dry-run");
    assert_eq!(row("widget")["current"], "0.2.1");
    assert_eq!(row("widget")["new"], "0.2.2");
    assert_eq!(row("gadget")["status"], "unchanged");
    assert!(plugin_note(root, "widget").contains("version: 0.2.1"));

    let real = run(&["bump", "--changed-since", BUILT, "--yes"], root);
    assert_eq!(real.code, 0, "{}", real.stderr);
    assert!(
        real.stdout.contains(
            "marketplaces/acme/plugins/widget/widget.md  written · version 0.2.1 -> 0.2.2"
        ),
        "{}",
        real.stdout
    );
    assert!(plugin_note(root, "widget").contains("version: 0.2.2"));
    assert!(plugin_note(root, "gadget").contains("version: 1.0.0"));

    // The version has now moved since the baseline, so a second run leaves
    // it alone.
    let again = run(&["bump", "--changed-since", BUILT, "--yes"], root);
    assert_eq!(again.code, 0, "{}", again.stderr);
    assert!(
        again.stdout.contains("widget.md  skipped · version-moved"),
        "{}",
        again.stdout
    );
    assert!(plugin_note(root, "widget").contains("version: 0.2.2"));
}

#[test]
fn each_skip_names_its_own_cause() {
    let dir = project("");
    let root = dir.path();
    let built = run(&["build", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    // Moved by hand since the build, and a plugin the build never held.
    let note = plugin_path(root, "acme", "widget");
    fs::write(&note, read(&note).replace("0.2.1", "0.3.0")).unwrap();
    write_skill(root, "widget", "Second draft.");
    write(
        &plugin_path(root, "acme", "gizmo"),
        "---\ndescription: The gizmo plugin.\nversion: 0.1.0\n---\n",
    );
    write_skill(root, "gizmo", "First draft.");
    // Only the manifest moves: a new description.
    let note = plugin_path(root, "acme", "gadget");
    fs::write(
        &note,
        read(&note).replace("The gadget plugin.", "The new gadget plugin."),
    )
    .unwrap();

    let result = run(&["bump", "--changed-since", BUILT, "--yes", "--json"], root);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let report: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    let rows = report["plugins"].as_array().unwrap();
    let reason = |name: &str| {
        let row = rows.iter().find(|r| r["plugin"] == name).unwrap();
        assert_eq!(row["status"], "skipped", "{rows:?}");
        row["reason"].as_str().unwrap().to_owned()
    };
    assert_eq!(reason("widget"), "version-moved");
    assert_eq!(reason("gizmo"), "not-in-baseline");
    assert_eq!(reason("gadget"), "manifest-only");
}

#[test]
fn changed_since_a_folder_with_no_plugin_manifest_is_refused() {
    let dir = project("");
    let root = dir.path();

    let result = run(&["bump", "--changed-since", "marketplaces", "--yes"], root);
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(
        result.stderr.contains("holds no plugin manifest"),
        "{}",
        result.stderr
    );
    assert!(plugin_note(root, "widget").contains("version: 0.2.1"));
}

#[test]
fn an_unversioned_plugin_is_bumped_once_then_skipped() {
    let dir = project("");
    let root = dir.path();
    let note = plugin_path(root, "acme", "gadget");
    fs::write(&note, read(&note).replace("version: 1.0.0\n", "")).unwrap();
    let built = run(&["build", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    write_skill(root, "gadget", "Second draft.");

    let status = |result: &Run| {
        let report: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
        let rows = report["plugins"].as_array().unwrap().clone();
        let gadget = rows.iter().find(|r| r["plugin"] == "gadget").unwrap();
        gadget["status"].as_str().unwrap().to_owned()
    };
    let first = run(&["bump", "--changed-since", BUILT, "--yes", "--json"], root);
    assert_eq!(first.code, 0, "{}", first.stderr);
    assert_eq!(status(&first), "written", "{}", first.stdout);
    assert!(plugin_note(root, "gadget").contains("version: 0.1.1"));

    let second = run(&["bump", "--changed-since", BUILT, "--yes", "--json"], root);
    assert_eq!(second.code, 0, "{}", second.stderr);
    assert_eq!(status(&second), "skipped", "{}", second.stdout);
    assert!(plugin_note(root, "gadget").contains("version: 0.1.1"));
}

#[cfg(unix)]
#[test]
fn a_failed_write_is_refused_on_its_row_and_the_others_are_still_reported() {
    use std::os::unix::fs::PermissionsExt;

    let dir = project("");
    let root = dir.path();
    let built = run(&["build", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    write_skill(root, "widget", "Second draft.");
    write_skill(root, "gadget", "Second draft.");
    // `gadget` sorts first, so its failure comes before `widget`'s write.
    let note = plugin_path(root, "acme", "gadget");
    let gadget = note.parent().unwrap().to_path_buf();
    fs::set_permissions(&note, fs::Permissions::from_mode(0o444)).unwrap();
    fs::set_permissions(&gadget, fs::Permissions::from_mode(0o555)).unwrap();

    let result = run(&["bump", "--changed-since", BUILT, "--yes", "--json"], root);
    fs::set_permissions(&gadget, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&note, fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(result.code, 1, "{}", result.stderr);
    let report: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    let rows = report["plugins"].as_array().unwrap();
    let row = |name: &str| rows.iter().find(|r| r["plugin"] == name).unwrap();
    assert_eq!(row("gadget")["status"], "refused", "{rows:?}");
    assert!(
        row("gadget")["reason"]
            .as_str()
            .unwrap()
            .starts_with("the write failed"),
        "{rows:?}"
    );
    assert_eq!(row("widget")["status"], "written", "{rows:?}");
    assert!(plugin_note(root, "widget").contains("version: 0.2.2"));
}

#[test]
fn a_plugin_and_changed_since_together_are_a_usage_error() {
    let dir = project("");
    let result = run(
        &["bump", "widget", "--changed-since", BUILT, "--yes"],
        dir.path(),
    );
    assert_eq!(result.code, 3, "{}", result.stderr);
}

#[test]
fn changed_since_works_on_one_of_several_marketplaces() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    shared_plugin(root);
    let built = run(&["build", "--marketplace", "a", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    skill_in(root, "a", "tools", "Two.");

    // A built folder is one marketplace, so several need `--marketplace`.
    let flagless = run(&["bump", "--changed-since", "dist/a", "--yes"], root);
    assert_eq!(flagless.code, 3, "{}", flagless.stderr);
    assert!(
        flagless.stderr.contains("--marketplace"),
        "{}",
        flagless.stderr
    );
    assert!(tools_note(root, "a").contains("version: 1.0.0"));

    let argv = [
        "bump",
        "--changed-since",
        "dist/a",
        "--marketplace",
        "a",
        "--yes",
    ];
    let result = run(&argv, root);
    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    assert!(tools_note(root, "a").contains("version: 1.0.1"));
    assert!(tools_note(root, "b").contains("version: 1.0.0"));

    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    git(root, &["add", "skillwright.md", "marketplaces"]);
    git(root, &["commit", "--quiet", "-m", "one"]);
    skill_in(root, "a", "tools", "Three.");
    let argv = [
        "bump",
        "--changed-since",
        "HEAD",
        "--marketplace",
        "a",
        "--yes",
    ];
    let from_ref = run(&argv, root);
    assert_eq!(from_ref.code, 0, "{}{}", from_ref.stdout, from_ref.stderr);
    assert!(tools_note(root, "a").contains("version: 1.0.2"));
    assert!(tools_note(root, "b").contains("version: 1.0.0"));
}

// --- `--changed-since` a git ref --------------------------------------------

/// Stages and commits everything in `repo`.
fn commit_all(repo: &Path, message: &str) {
    git(repo, &["add", "."]);
    git(repo, &["commit", "--quiet", "-m", message]);
}

/// The number of worktrees `repo` has registered, its own included.
fn worktree_count(repo: &Path) -> usize {
    git(repo, &["worktree", "list", "--porcelain"])
        .matches("worktree ")
        .count()
}

#[test]
fn changed_since_a_git_ref_builds_it_in_a_temp_folder_and_cleans_up() {
    let dir = project("build:\n  bump:\n    patch: timestamp\n");
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    commit_all(root, "one");
    write_skill(root, "gadget", "Second draft.");

    let result = run(
        &["bump", "--changed-since", "HEAD", "--yes", "--json"],
        root,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    let report: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    let rows = report["plugins"].as_array().unwrap();
    let gadget = rows.iter().find(|r| r["plugin"] == "gadget").unwrap();
    assert_eq!(gadget["status"], "written", "{rows:?}");
    let new = gadget["new"].as_str().unwrap();
    let patch = new.strip_prefix("1.0.").unwrap();
    assert_eq!(patch.len(), 14, "a UTC timestamp patch: {new}");
    assert!(plugin_note(root, "gadget").contains(&format!("version: {new}")));
    let widget = rows.iter().find(|r| r["plugin"] == "widget").unwrap();
    assert_eq!(widget["status"], "unchanged", "{rows:?}");

    // No worktree was left registered, and nothing was built into `dist/`.
    assert_eq!(worktree_count(root), 1);
    assert!(!root.join("dist").exists());
}

#[test]
fn a_ref_without_a_root_file_is_blocked_and_cleans_up() {
    let dir = project("");
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    git(root, &["add", "marketplaces"]);
    git(root, &["commit", "--quiet", "-m", "notes only"]);

    let result = run(&["bump", "--changed-since", "HEAD", "--yes"], root);
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(
        result.stderr.contains("Compare against a built folder"),
        "{}",
        result.stderr
    );
    assert_eq!(worktree_count(root), 1);
}

#[test]
fn changed_since_a_name_that_is_neither_a_folder_nor_a_ref_is_blocked() {
    let dir = project("");
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    commit_all(root, "one");

    let result = run(&["bump", "--changed-since", "no-such-ref", "--yes"], root);
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(
        result.stderr.contains("neither a directory nor a commit"),
        "{}",
        result.stderr
    );
}

#[test]
fn changed_since_a_ref_refuses_packs_in_a_block_as_build_does() {
    let dir = project("build:\n  harnesses: [claude]\n");
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    commit_all(root, "one");
    write(
        &root.join("skillwright.md"),
        "---\nclaude: {packs: moved}\nbuild:\n  harnesses: [claude]\n---\n",
    );

    let build = run(&["build", "--yes"], root);
    assert!(build.stderr.contains("`claude.packs`"), "{}", build.stderr);
    let result = run(&["bump", "--changed-since", "HEAD", "--yes"], root);
    assert_eq!(result.code, build.code, "{}", result.stderr);
    assert!(
        result.stderr.contains("`claude.packs`"),
        "{}",
        result.stderr
    );
    assert_eq!(worktree_count(root), 1);
}

// --- What the ref's baseline build reads ------------------------------------

/// A repository whose vault is `vault/`: [`project`]'s marketplace with
/// `root_fm` in its root file, plus what `extra` writes into the
/// repository, committed once. The repository path is canonical.
fn repo_with_vault(root_fm: &str, extra: impl FnOnce(&Path)) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().canonicalize().unwrap();
    project_in(&repo.join("vault"), root_fm);
    extra(&repo);
    git(&repo, &["init", "--quiet", "--initial-branch", "main"]);
    commit_all(&repo, "one");
    (dir, repo)
}

/// A `--json --dry-run` bump against `since`, run in `cwd` with `extra` flags.
fn dry_bump(cwd: &Path, since: &str, extra: &[&str]) -> Run {
    let mut argv = vec!["bump", "--changed-since", since, "--dry-run", "--json"];
    argv.extend_from_slice(extra);
    run(&argv, cwd)
}

/// Each plugin's status in a successful `--json` report.
fn statuses(result: &Run) -> std::collections::BTreeMap<String, String> {
    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    let report: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    report["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let status = row["status"].as_str().unwrap();
            let status = match row["reason"].as_str() {
                Some(reason) => format!("{status} · {reason}"),
                None => status.to_owned(),
            };
            (row["plugin"].as_str().unwrap().to_owned(), status)
        })
        .collect()
}

/// `(plugin, status)` pairs as owned strings, for comparing with [`statuses`].
fn rows(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(plugin, status)| ((*plugin).to_owned(), (*status).to_owned()))
        .collect()
}

fn hooks_json(command: &str) -> String {
    format!(
        "{{\"hooks\": {{\"SessionStart\": [{{\"hooks\": [{{\"type\": \"command\", \"command\": \"{command}\"}}]}}]}}}}\n"
    )
}

#[test]
fn a_plugins_hooks_file_changed_since_the_ref_bumps_its_plugin() {
    let hooks = |repo: &Path| repo.join("vault/marketplaces/acme/plugins/widget/hooks/hooks.json");
    let (_dir, repo) = repo_with_vault("", |repo| write(&hooks(repo), &hooks_json("echo one")));
    write(&hooks(&repo), &hooks_json("echo two"));

    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    assert_eq!(
        statuses(&result),
        rows(&[("widget", "dry-run"), ("gadget", "unchanged")])
    );
}

#[test]
fn a_vault_flag_naming_a_folder_the_ref_lacks_is_blocked_and_cleans_up() {
    let dir = project("");
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    commit_all(root, "one");
    project_in(&root.join("x"), "");

    let result = run(
        &["bump", "--changed-since", "HEAD", "--vault", "x", "--yes"],
        root,
    );
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(
        result
            .stderr
            .contains("holds no `skillwright.md` and `marketplaces/` at x"),
        "{}",
        result.stderr
    );
    assert_eq!(worktree_count(root), 1);
}

#[cfg(unix)]
#[test]
fn a_symlink_in_the_vault_reads_its_target_at_the_ref() {
    // The current tree refuses a note linked from outside its marketplace,
    // so only the ref holds one.
    let skill = "vault/marketplaces/acme/plugins/widget/skills/widget-skill/widget-skill.md";
    let note = "---\ndescription: Use widget.\n---\n\nLinked.\n";
    let (_dir, repo) = repo_with_vault("", |repo| {
        write(&repo.join("notes/widget-skill.md"), note);
        let link = repo.join(skill);
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("../../../../../../../notes/widget-skill.md", &link).unwrap();
    });
    fs::remove_file(repo.join(skill)).unwrap();
    write(&repo.join(skill), note);

    // The ref's vault is refused for the link, as one on disk is.
    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    // Refused as a link spelling its target is (spec section 4, containment
    // rule 2).
    let in_vault = skill.strip_prefix("vault/").unwrap();
    assert_symlink_refused(&result, "link-out-of-scope", &format!("{in_vault}: `"));
}

#[cfg(unix)]
#[test]
fn a_root_file_that_is_a_symlink_at_the_ref_builds_as_it_did() {
    // The current tree refuses a symlinked root file, so only the ref holds one.
    let (_dir, repo) = repo_with_vault("", |repo| {
        let root_file = repo.join("vault/skillwright.md");
        write(&repo.join("shared/skillwright.md"), &read(&root_file));
        fs::remove_file(&root_file).unwrap();
        std::os::unix::fs::symlink("../shared/skillwright.md", &root_file).unwrap();
    });
    let root_file = repo.join("vault/skillwright.md");
    fs::remove_file(&root_file).unwrap();
    fs::copy(repo.join("shared/skillwright.md"), &root_file).unwrap();

    // The link resolves, so the ref's root file is found; its vault is
    // then refused for the link.
    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    assert_symlink_refused(
        &result,
        "unsafe-path",
        "vault/skillwright.md: `skillwright.md` may not be a symlink",
    );
}

/// The ref's vault was refused with `token`, the message naming the symlink
/// as `said` does.
fn assert_symlink_refused(result: &Run, token: &str, said: &str) {
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(
        result.stderr.contains(&format!("refused: {token}: ")),
        "{}",
        result.stderr
    );
    assert!(result.stderr.contains(said), "{}", result.stderr);
    assert!(result.stdout.is_empty(), "{}", result.stdout);
}

#[test]
fn gitattributes_at_the_ref_apply_to_the_baseline() {
    let (_dir, repo) = repo_with_vault("", |repo| {
        write(&repo.join(".gitattributes"), "*.md text eol=crlf\n");
    });

    // The ref's notes check out with CRLF endings and the render keeps them,
    // so every plugin reads as changed against the LF notes on disk.
    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    assert_eq!(
        statuses(&result),
        rows(&[("widget", "dry-run"), ("gadget", "dry-run")])
    );
}

/// A two-commit repository, its depth-1 clone's parent folder, and the
/// clone.
fn shallow_clone() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
    let dir = project("");
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    commit_all(root, "one");
    write_skill(root, "widget", "Second draft.");
    commit_all(root, "two");
    let clones = tempfile::tempdir().unwrap();
    let url = format!("file://{}", root.display());
    git(
        clones.path(),
        &["clone", "--quiet", "--depth", "1", &url, "c"],
    );
    let clone = clones.path().join("c");
    (dir, clones, clone)
}

#[test]
fn a_shallow_clone_builds_a_ref_it_holds() {
    let (_dir, _clones, clone) = shallow_clone();
    write_skill(&clone, "gadget", "Third draft.");

    let result = dry_bump(&clone, "HEAD", &[]);
    assert_eq!(
        statuses(&result),
        rows(&[("widget", "unchanged"), ("gadget", "dry-run")])
    );
}

#[test]
fn a_ref_beyond_a_shallow_clone_is_blocked() {
    let (dir, _clones, clone) = shallow_clone();
    git(dir.path(), &["rev-parse", "--verify", "HEAD~1"]);

    let result = dry_bump(&clone, "HEAD~1", &[]);
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(
        result.stderr.contains("neither a directory nor a commit"),
        "{}",
        result.stderr
    );
}

#[test]
fn a_ref_baseline_never_checks_out_files_outside_what_the_build_reads() {
    let (_dir, repo) = repo_with_vault("", |repo| {
        write(&repo.join("junk/big.bin"), "junk\n");
    });
    // A filter that fails on every checkout of `junk/`: the bump succeeds
    // only if the baseline never writes it.
    write(&repo.join(".git/info/attributes"), "junk/** filter=boom\n");
    git(&repo, &["config", "filter.boom.smudge", "false"]);
    git(&repo, &["config", "filter.boom.required", "true"]);

    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    assert_eq!(
        statuses(&result),
        rows(&[("widget", "unchanged"), ("gadget", "unchanged")])
    );
}

// --- The baseline folder, seen from outside the process ---------------------

/// What a run can leave in the repository.
#[cfg(unix)]
#[derive(Debug, PartialEq)]
struct Leftovers {
    status: String,
    config: Vec<u8>,
    worktrees: String,
    worktrees_dir: bool,
}

#[cfg(unix)]
fn leftovers(repo: &Path) -> Leftovers {
    Leftovers {
        status: git(repo, &["status", "--porcelain"]),
        config: fs::read(repo.join(".git/config")).unwrap(),
        worktrees: git(repo, &["worktree", "list", "--porcelain"]),
        worktrees_dir: repo.join(".git/worktrees").exists(),
    }
}

/// Runs the binary in `cwd` with its temp folder set to `tmp`, so the
/// baselines folder is this test's own. Its exit code and stderr.
#[cfg(unix)]
fn bump_with_tmp(cwd: &Path, tmp: &Path, argv: &[&str]) -> (i32, String) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_skillwright"))
        .args(argv)
        .current_dir(cwd)
        .env("TMPDIR", tmp)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

/// What the baselines folder under `tmp` holds.
#[cfg(unix)]
fn baselines_in(tmp: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(tmp.join("skillwright-bump-baselines"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(unix)]
#[test]
fn a_successful_ref_bump_leaves_no_baseline_behind() {
    let (_dir, repo) = repo_with_vault("", |_| {});
    write_skill(&repo.join("vault"), "widget", "Second draft.");
    let tmp = tempfile::tempdir().unwrap();
    let before = leftovers(&repo);

    let argv = ["bump", "--changed-since", "HEAD", "--dry-run"];
    let (code, stderr) = bump_with_tmp(&repo.join("vault"), tmp.path(), &argv);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(baselines_in(tmp.path()), ["guard"]);
    assert_eq!(leftovers(&repo), before);
    assert_eq!(worktree_count(&repo), 1);
    assert!(!before.worktrees_dir);
}

#[cfg(unix)]
#[test]
fn a_failed_ref_build_leaves_no_baseline_behind() {
    let skill = "vault/marketplaces/acme/plugins/widget/skills/widget-skill/widget-skill.md";
    let (_dir, repo) = repo_with_vault("", |repo| {
        write(
            &repo.join(skill),
            "---\ndescription: [Use widget.\n---\n\nBroken.\n",
        );
    });
    write_skill(&repo.join("vault"), "widget", "Fixed.");
    let tmp = tempfile::tempdir().unwrap();
    let before = leftovers(&repo);

    let argv = ["bump", "--changed-since", "HEAD", "--dry-run"];
    let (code, stderr) = bump_with_tmp(&repo.join("vault"), tmp.path(), &argv);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("building it failed"), "{stderr}");
    assert_eq!(baselines_in(tmp.path()), ["guard"]);
    assert_eq!(leftovers(&repo), before);
    assert!(!before.worktrees_dir);
}

#[cfg(unix)]
#[test]
fn a_ref_without_a_root_file_leaves_no_baseline_behind() {
    let dir = project("");
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    git(root, &["add", "marketplaces"]);
    git(root, &["commit", "--quiet", "-m", "notes only"]);
    let tmp = tempfile::tempdir().unwrap();
    let before = leftovers(root);

    let argv = ["bump", "--changed-since", "HEAD", "--yes"];
    let (code, stderr) = bump_with_tmp(root, tmp.path(), &argv);
    assert_eq!(code, 2, "{stderr}");
    assert!(
        stderr.contains("Compare against a built folder"),
        "{stderr}"
    );
    assert_eq!(baselines_in(tmp.path()), ["guard"]);
    assert_eq!(leftovers(root), before);
}

/// Appendix A6, the marketplace half: `--marketplace osdlc` bumps the
/// marketplace and leaves its plugin `osdlc` alone.
#[test]
fn a_marketplace_named_like_its_plugin_bumps_with_marketplace() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    vault(root, "", &[("osdlc", &[("osdlc", Some("1.0.0"))])]);
    write(&market_path(root, "osdlc"), "---\nversion: 3.0.0\n---\n");

    let result = run(&["bump", "--marketplace", "osdlc", "--yes"], root);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        read(&market_path(root, "osdlc")),
        "---\nversion: 3.0.1\n---\n"
    );
    assert!(read(&plugin_path(root, "osdlc", "osdlc")).contains("version: 1.0.0"));
}
