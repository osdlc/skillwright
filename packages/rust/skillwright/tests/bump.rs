//! `skillwright bump` — the CLI spec's `bump` section, driven in-process the
//! way `tests/cli.rs` drives every other verb: [`run_with`] over a temp copy
//! of a vault, never a checked-in fixture mutated in place.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::with_machine_env;
use skillwright::cli::run_with;
use skillwright::ctx::CommandContext;

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

/// One invocation, with the machine config relocated into `cwd` so the
/// developer's real one never leaks into a run — the same helper
/// `tests/cli.rs` uses.
fn run(argv: &[&str], cwd: &Path) -> Run {
    let owned: Vec<String> = argv.iter().map(|s| (*s).to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = with_machine_env(Some(&cwd.join("no-machine-config.yaml")), || {
        let mut ctx = CommandContext::new(cwd, &mut out, &mut err);
        run_with(&owned, &mut ctx)
    });
    Run {
        code,
        stdout: String::from_utf8(out).unwrap(),
        stderr: String::from_utf8(err).unwrap(),
    }
}

fn vault_str(vault: &Path) -> String {
    vault.display().to_string()
}

/// One plugin note, discoverable by structure alone —
/// `marketplaces/<mkt>/plugins/<plugin>/<plugin>.md` — the shape
/// the shared `freeform-vault` fixture's `core.md`
/// uses, with no `sw-kind` needed. `version_line` is spliced in as a whole
/// frontmatter line, or omitted for a note with no `sw-version` at all.
fn fixture(version_line: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let plugin_dir = vault
        .join("marketplaces")
        .join("acme")
        .join("plugins")
        .join("widget");
    fs::create_dir_all(&plugin_dir).unwrap();
    let version = version_line.map(|v| format!("{v}\n")).unwrap_or_default();
    let note = format!(
        "---\nname: widget\ndescription: A plugin for bump tests.\n{version}---\n\nBody text, untouched by a bump.\n"
    );
    fs::write(plugin_dir.join("widget.md"), &note).unwrap();
    (dir, vault)
}

fn note_path(vault: &Path) -> PathBuf {
    vault
        .join("marketplaces")
        .join("acme")
        .join("plugins")
        .join("widget")
        .join("widget.md")
}

const REL: &str = "marketplaces/acme/plugins/widget/widget.md";

#[test]
fn dry_run_prints_old_arrow_new_and_leaves_the_note_untouched() {
    let (dir, vault) = fixture(Some("sw-version: 0.2.1"));
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
fn a_real_bump_rewrites_only_the_sw_version_line() {
    let (dir, vault) = fixture(Some("sw-version: 0.2.1"));
    let path = note_path(&vault);
    let before = fs::read_to_string(&path).unwrap();

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
        format!("{REL}  written · sw-version 0.2.1 -> 0.3.0\n")
    );

    let after = fs::read_to_string(&path).unwrap();
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
            assert_eq!(*b, "sw-version: 0.2.1");
            assert_eq!(*a, "sw-version: 0.3.0");
        }
    }
    assert_eq!(changed, 1, "exactly one line may change");
}

#[test]
fn no_sw_version_at_all_bumps_from_the_core_default() {
    let (dir, vault) = fixture(None);
    let path = note_path(&vault);
    let before = fs::read_to_string(&path).unwrap();
    assert!(!before.contains("sw-version"));

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
        format!("{REL}  written · sw-version 0.1.0 -> 0.1.1\n")
    );

    let after = fs::read_to_string(&path).unwrap();
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    assert_eq!(
        after_lines.len(),
        before_lines.len() + 1,
        "gaining a missing sw-version must add exactly one line, no stray \
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
    assert_eq!(after_lines[insert_at], "sw-version: 0.1.1");
    assert_eq!(before_lines[insert_at..], after_lines[insert_at + 1..]);
}

#[test]
fn a_non_semver_sw_version_is_refused_and_the_note_is_untouched() {
    // Quoted: unquoted `0.3` is a YAML float, and `sw-version` must be a
    // string at discovery time (`frontmatter::take_string` on
    // `ContainerIdentity.version`) — a float there fails the whole note's
    // discovery, not just this field, so `bump` would never even find the
    // plugin to refuse. A quoted string is the shape that reaches `bump`'s
    // own non-semver refusal, which is what the CLI spec's transcript shows.
    let (dir, vault) = fixture(Some("sw-version: \"0.3\""));
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
        format!("{REL}  refused · non-semver: `sw-version` reads `0.3`\n")
    );
    let after = fs::read(&path).unwrap();
    assert_eq!(before, after, "a refusal must not touch the note's bytes");
}

#[test]
fn no_yes_off_a_tty_is_a_usage_error_and_writes_nothing() {
    let (dir, vault) = fixture(Some("sw-version: 0.2.1"));
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

#[test]
fn a_version_in_a_nested_block_is_bumped_in_place() {
    let (dir, vault) = fixture(Some("sw:\n  version: 2.3.0"));
    let path = note_path(&vault);

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
        format!("{REL}  written · sw.version 2.3.0 -> 2.3.1\n")
    );
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("sw:\n  version: 2.3.1\n"), "{after}");
    assert!(
        !after.contains("sw-version"),
        "a flat key beside the block is a mixed shape: {after}"
    );
}

// --- `--changed-since` ------------------------------------------------------

/// A project with two plugins, `widget` (`0.2.1`) and `gadget` (`1.0.0`), one
/// skill each, building into `market/`.
fn project(extra_config: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("skillwright.yaml"),
        format!(
            "vault: vault\nmarketplace:\n  name: acme\n  default_plugin: widget\n  out: market\n{extra_config}"
        ),
    )
    .unwrap();
    for (plugin, version) in [("widget", "0.2.1"), ("gadget", "1.0.0")] {
        let base = root.join("vault/marketplaces/acme/plugins").join(plugin);
        fs::create_dir_all(base.join("skills")).unwrap();
        fs::write(
            base.join(format!("{plugin}.md")),
            format!("---\nname: {plugin}\ndescription: The {plugin} plugin.\nsw-version: {version}\n---\n"),
        )
        .unwrap();
        write_skill(root, plugin, "First draft.");
    }
    dir
}

fn write_skill(root: &Path, plugin: &str, body: &str) {
    fs::write(
        root.join("vault/marketplaces/acme/plugins")
            .join(plugin)
            .join("skills")
            .join(format!("{plugin}-skill.md")),
        format!("---\nname: {plugin}-skill\ndescription: Use {plugin}.\n---\n\n{body}\n"),
    )
    .unwrap();
}

fn plugin_note(root: &Path, plugin: &str) -> String {
    fs::read_to_string(
        root.join("vault/marketplaces/acme/plugins")
            .join(plugin)
            .join(format!("{plugin}.md")),
    )
    .unwrap()
}

fn git(repo: &Path, args: &[&str]) -> String {
    let mut cmd = typed_git::git();
    cmd.dir(repo)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@t.invalid")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@t.invalid");
    cmd.run().unwrap()
}

#[test]
fn changed_since_a_built_folder_bumps_only_the_plugin_that_changed() {
    let dir = project("");
    let root = dir.path();
    let built = run(&["build", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    write_skill(root, "widget", "Second draft.");

    let dry = run(
        &["bump", "--changed-since", "market", "--dry-run", "--json"],
        root,
    );
    assert_eq!(dry.code, 0, "{}", dry.stderr);
    let report: serde_json::Value = serde_json::from_str(&dry.stdout).unwrap();
    assert_eq!(report["baseline"], "market");
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
    assert!(plugin_note(root, "widget").contains("sw-version: 0.2.1"));

    let real = run(&["bump", "--changed-since", "market", "--yes"], root);
    assert_eq!(real.code, 0, "{}", real.stderr);
    assert!(
        real.stdout.contains(
            "marketplaces/acme/plugins/widget/widget.md  written · sw-version 0.2.1 -> 0.2.2"
        ),
        "{}",
        real.stdout
    );
    assert!(plugin_note(root, "widget").contains("sw-version: 0.2.2"));
    assert!(plugin_note(root, "gadget").contains("sw-version: 1.0.0"));

    // The version has now moved since the baseline, so a second run leaves
    // it alone.
    let again = run(&["bump", "--changed-since", "market", "--yes"], root);
    assert_eq!(again.code, 0, "{}", again.stderr);
    assert!(
        again.stdout.contains("widget.md  skipped"),
        "{}",
        again.stdout
    );
    assert!(plugin_note(root, "widget").contains("sw-version: 0.2.2"));
}

#[test]
fn changed_since_a_git_ref_builds_it_in_a_worktree_and_cleans_up() {
    let dir = project("bump:\n  patch: timestamp\n");
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "one"]);
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
    assert!(plugin_note(root, "gadget").contains(&format!("sw-version: {new}")));
    let widget = rows.iter().find(|r| r["plugin"] == "widget").unwrap();
    assert_eq!(widget["status"], "unchanged", "{rows:?}");

    // The temporary worktree is gone, and nothing was built into `market/`.
    let worktrees = git(root, &["worktree", "list", "--porcelain"]);
    assert_eq!(worktrees.matches("worktree ").count(), 1, "{worktrees}");
    assert!(!root.join("market").exists());
}

#[test]
fn changed_since_a_name_that_is_neither_a_folder_nor_a_ref_is_blocked() {
    let dir = project("");
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "one"]);

    let result = run(&["bump", "--changed-since", "no-such-ref", "--yes"], root);
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(
        result.stderr.contains("neither a directory nor a commit"),
        "{}",
        result.stderr
    );
}

#[test]
fn a_plugin_and_changed_since_together_are_a_usage_error() {
    let dir = project("");
    let result = run(
        &["bump", "widget", "--changed-since", "market", "--yes"],
        dir.path(),
    );
    assert_eq!(result.code, 3, "{}", result.stderr);
}
