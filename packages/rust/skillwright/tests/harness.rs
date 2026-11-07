//! `harness install|update|uninstall <harness>` driven through [`run_with`] over a temp
//! project, with a scripted fake in place of the harness CLIs.

mod common;

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use skillwright::cli::run_with;
use skillwright::commands::harness::cli_runner::{RunResult, Runner};
use skillwright::ctx::CommandContext;
use skillwright::exit::{EXIT_BLOCKED, EXIT_USAGE};

/// Answers each list command with canned stdout and every other command
/// with success, recording each argv it is given. A command containing
/// `fail_on` exits 1 with `boom` on stderr.
#[derive(Default)]
struct FakeRunner {
    markets: String,
    plugins: String,
    fail_on: Option<&'static str>,
    calls: RefCell<Vec<String>>,
}

impl FakeRunner {
    fn new(markets: &str, plugins: &str) -> Self {
        Self {
            markets: markets.to_string(),
            plugins: plugins.to_string(),
            ..Self::default()
        }
    }

    fn failing_on(mut self, part: &'static str) -> Self {
        self.fail_on = Some(part);
        self
    }

    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl Runner for FakeRunner {
    fn run(&self, argv: &[String], _cwd: &Path) -> RunResult {
        let line = argv.join(" ");
        self.calls.borrow_mut().push(line.clone());
        if self.fail_on.is_some_and(|part| line.contains(part)) {
            return RunResult {
                code: Some(1),
                stdout: String::new(),
                stderr: "boom".to_string(),
            };
        }
        let stdout = if line.contains("marketplace list") {
            self.markets.clone()
        } else if line.ends_with("plugin list --json") {
            self.plugins.clone()
        } else {
            String::new()
        };
        RunResult {
            code: Some(0),
            stdout,
            stderr: String::new(),
        }
    }
}

const CLAUDE_MARKETS: &str = "claude plugin marketplace list --json";
const CLAUDE_PLUGINS: &str = "claude plugin list --json";

/// The packs folder of each harness `harness` drives, relative to the built
/// marketplace.
const PACKS: [&str; 4] = [
    ".claude-plugin/packs",
    ".agents/packs",
    ".cursor-plugin/packs",
    ".pi/packs",
];

/// A built `tools` pack under `root` for each harness, so `harness` finds
/// the plugin to install without `--plugin`.
fn write_packs(root: &Path) {
    for packs in PACKS {
        fs::create_dir_all(root.join(packs).join("tools")).unwrap();
    }
}

/// A project whose built marketplace at `published/acme` carries a manifest
/// for each harness root, all naming it `acme`, and a `tools` pack. Returns
/// that root.
fn write_project(dir: &Path) -> String {
    fs::write(
        dir.join("skillwright.md"),
        "---\nbuild:\n  out: published/acme\n---\n",
    )
    .unwrap();
    let root = dir.join("published/acme");
    write_packs(&root);
    for manifest in [
        ".claude-plugin/marketplace.json",
        ".agents/plugins/marketplace.json",
        ".cursor-plugin/marketplace.json",
    ] {
        let path = root.join(manifest);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, r#"{"name": "acme", "plugins": []}"#).unwrap();
    }
    root.display().to_string()
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(argv: &[&str], cwd: &Path, runner: &dyn Runner) -> Run {
    let owned: Vec<String> = argv.iter().map(|s| (*s).to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = {
        let mut ctx = CommandContext::new(cwd, &mut out, &mut err).with_runner(runner);
        run_with(&owned, &mut ctx)
    };
    Run {
        code,
        stdout: String::from_utf8(out).unwrap(),
        stderr: String::from_utf8(err).unwrap(),
    }
}

fn claude_market(path: &str) -> String {
    format!(r#"[{{"name": "acme", "source": "directory", "path": "{path}"}}]"#)
}

fn claude_plugin(scope: &str) -> String {
    format!(r#"[{{"id": "tools@acme", "scope": "{scope}", "enabled": true}}]"#)
}

#[test]
fn ac1_a_missing_marketplace_is_added_then_its_plugin_installed() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let fake = FakeRunner::new("[]", "[]");

    let result = run(
        &[
            "harness", "install", "claude", "--scope", "user", "--plugin", "tools", "--yes",
            "--json",
        ],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        fake.calls(),
        vec![
            CLAUDE_MARKETS.to_string(),
            CLAUDE_PLUGINS.to_string(),
            format!("claude plugin marketplace add {root} --scope user"),
            "claude plugin install tools@acme --scope user".to_string(),
        ]
    );
    let doc: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(doc["outcome"], "added");
}

#[test]
fn ac2_a_matching_install_runs_only_the_list_commands_and_reports_exists() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let fake = FakeRunner::new(&claude_market(&root), &claude_plugin("user"));

    let result = run(
        &["harness", "install", "claude", "--plugin", "tools", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fake.calls(), vec![CLAUDE_MARKETS, CLAUDE_PLUGINS]);
    assert_eq!(result.stdout, "claude  unchanged · exists\n");
    assert!(
        result.stderr.contains("nothing to change"),
        "{}",
        result.stderr
    );
}

#[test]
fn ac3_a_marketplace_from_another_path_is_removed_and_added_again() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let fake = FakeRunner::new(&claude_market("/somewhere/else"), &claude_plugin("user"));

    let result = run(
        &["harness", "install", "claude", "--yes", "--json"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        &fake.calls()[2..],
        [
            "claude plugin marketplace remove acme --scope user".to_string(),
            format!("claude plugin marketplace add {root} --scope user"),
            "claude plugin install tools@acme --scope user".to_string(),
        ]
    );
    let doc: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(doc["outcome"], "updated");
}

#[test]
fn a_plugin_installed_at_another_scope_is_installed_at_the_wanted_one() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let fake = FakeRunner::new(&claude_market(&root), &claude_plugin("user"));
    let declared = format!(
        r#"{{"extraKnownMarketplaces": {{"acme": {{"source": {{"source": "directory", "path": "{root}"}}}}}}}}"#
    );
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    fs::write(dir.path().join(".claude/settings.json"), declared).unwrap();

    let result = run(
        &[
            "harness", "install", "claude", "--scope", "project", "--yes",
        ],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        fake.calls(),
        [
            CLAUDE_PLUGINS,
            "claude plugin install tools@acme --scope project"
        ]
    );
}

/// `marketplace list` merges every scope, so a user-scope registration does
/// not stand in for the project's own declaration.
#[test]
fn a_project_scope_install_declares_the_marketplace_in_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let fake = FakeRunner::new(&claude_market(&root), "[]");

    let result = run(
        &[
            "harness", "install", "claude", "--scope", "project", "--yes",
        ],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        fake.calls()[1..],
        [
            format!("claude plugin marketplace add {root} --scope project"),
            "claude plugin install tools@acme --scope project".to_string(),
        ]
    );
}

#[test]
fn a_marketplace_listed_under_another_name_names_the_plugin_ids() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let markets = format!(r#"[{{"name": "old", "source": "directory", "path": "{root}"}}]"#);
    let fake = FakeRunner::new(&markets, "[]");

    let result = run(
        &["harness", "install", "claude", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        &fake.calls()[2..],
        ["claude plugin install tools@old --scope user"]
    );
}

#[test]
fn a_failing_mutating_command_blocks_and_names_what_already_ran() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let fake = FakeRunner::new("[]", "[]").failing_on("plugin install");

    let result = run(
        &["harness", "install", "claude", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, EXIT_BLOCKED);
    assert!(result.stderr.contains("boom"), "{}", result.stderr);
    assert!(
        result.stderr.contains(&format!(
            "already ran: claude plugin marketplace add {root} --scope user"
        )),
        "{}",
        result.stderr
    );
}

#[test]
fn without_yes_off_a_tty_a_change_is_a_usage_error_and_runs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let fake = FakeRunner::new("[]", "[]");

    let result = run(&["harness", "install", "claude"], dir.path(), &fake);
    assert_eq!(result.code, EXIT_USAGE);
    assert!(result.stderr.contains("--yes"), "{}", result.stderr);
    assert_eq!(fake.calls(), vec![CLAUDE_MARKETS, CLAUDE_PLUGINS]);
}

#[test]
fn git_ref_is_passed_on_by_codex_and_cursor_and_refused_by_claude() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let url = "https://example.com/acme.git";
    let argv = |harness: &'static str| {
        [
            "harness",
            "install",
            harness,
            "--git-url",
            url,
            "--git-ref",
            "v1",
            "--yes",
        ]
    };

    let codex = FakeRunner::new(r#"{"marketplaces": []}"#, r#"{"installed": []}"#);
    assert_eq!(run(&argv("codex"), dir.path(), &codex).code, 0);
    assert_eq!(
        codex.calls()[2],
        format!("codex plugin marketplace add {url} --ref v1")
    );

    let cursor = FakeRunner::new("[]", "");
    assert_eq!(run(&argv("cursor"), dir.path(), &cursor).code, 0);
    assert_eq!(
        cursor.calls()[1],
        format!("cursor-agent plugin marketplace add {url} --git-ref v1")
    );

    let claude = FakeRunner::default();
    let result = run(&argv("claude"), dir.path(), &claude);
    assert_eq!(result.code, EXIT_USAGE);
    assert!(result.stderr.contains("--git-ref"), "{}", result.stderr);
    assert!(claude.calls().is_empty());
}

#[test]
fn cursor_replaces_a_marketplace_registered_from_another_url() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let url = "https://example.com/acme.git";
    let fake = FakeRunner::new(
        r#"[{"name": "acme", "gitUrl": "https://old.example/acme.git"}]"#,
        "",
    );

    let result = run(
        &["harness", "install", "cursor", "--git-url", url, "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        fake.calls()[1..],
        [
            "cursor-agent plugin marketplace remove acme".to_string(),
            format!("cursor-agent plugin marketplace add {url}"),
        ]
    );
}

#[test]
fn ac4_codex_rejects_a_scope_other_than_user_and_runs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let fake = FakeRunner::default();

    let result = run(
        &["harness", "install", "codex", "--scope", "project", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, EXIT_USAGE);
    assert!(
        result.stderr.contains("--scope user only"),
        "{}",
        result.stderr
    );
    assert!(fake.calls().is_empty(), "{:?}", fake.calls());
}

#[test]
fn codex_adds_its_marketplace_and_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let fake = FakeRunner::new(r#"{"marketplaces": []}"#, r#"{"installed": []}"#);

    let result = run(&["harness", "install", "codex", "--yes"], dir.path(), &fake);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        &fake.calls()[2..],
        [
            format!("codex plugin marketplace add {root}"),
            "codex plugin add tools@acme".to_string(),
        ]
    );
}

#[test]
fn ac5_cursor_without_a_git_url_is_a_usage_error_naming_the_flag() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let fake = FakeRunner::default();

    let result = run(
        &["harness", "install", "cursor", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, EXIT_USAGE);
    assert!(result.stderr.contains("--git-url"), "{}", result.stderr);
    assert!(fake.calls().is_empty());
}

#[test]
fn ac5_cursor_adds_its_git_url_only_when_it_is_not_listed() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let url = "https://example.com/acme.git";
    let argv = ["harness", "install", "cursor", "--git-url", url, "--yes"];

    let missing = FakeRunner::new("[]", "");
    let result = run(&argv, dir.path(), &missing);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        missing.calls(),
        [
            "cursor-agent plugin marketplace list --format json".to_string(),
            format!("cursor-agent plugin marketplace add {url}"),
        ]
    );
    assert!(
        result.stdout.contains("enable in Cursor · tools"),
        "{}",
        result.stdout
    );

    let listed = FakeRunner::new(&format!(r#"[{{"name": "acme", "gitUrl": "{url}"}}]"#), "");
    let result = run(&argv, dir.path(), &listed);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(listed.calls().len(), 1, "{:?}", listed.calls());
}

#[test]
fn ac6_dry_run_prints_the_plan_and_runs_only_the_list_commands() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let fake = FakeRunner::new("[]", "[]");

    let result = run(
        &["harness", "install", "claude", "--dry-run"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fake.calls(), vec![CLAUDE_MARKETS, CLAUDE_PLUGINS]);
    assert!(
        result.stdout.contains(&format!(
            "claude  would run · claude plugin marketplace add {root} --scope user"
        )),
        "{}",
        result.stdout
    );
    assert!(result.stderr.contains("(dry run)"), "{}", result.stderr);
}

#[test]
fn a_failing_list_command_blocks_with_its_stderr() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let fake = FakeRunner::default().failing_on("marketplace list");

    let result = run(
        &["harness", "install", "claude", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, EXIT_BLOCKED);
    assert!(result.stderr.contains("boom"), "{}", result.stderr);
    assert_eq!(fake.calls(), vec![CLAUDE_MARKETS]);
}

/// No manifest at the target root: the root file's `name` names the
/// marketplace, so it is what the plugin id carries.
#[test]
fn a_configured_marketplace_name_is_the_fallback_for_an_unbuilt_root() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nname: configured\nbuild:\n  out: published/acme\n---\n",
    )
    .unwrap();
    write_packs(&dir.path().join("published/acme"));
    let fake = FakeRunner::new("[]", "[]");

    let result = run(
        &["harness", "install", "claude", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        fake.calls().last().unwrap(),
        "claude plugin install tools@configured --scope user"
    );
}

/// A manifest that is not valid JSON blocks instead of falling back to
/// configuration, and nothing runs.
#[test]
fn a_malformed_manifest_blocks_before_any_command() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    fs::write(
        Path::new(&root).join(".claude-plugin/marketplace.json"),
        "{not json",
    )
    .unwrap();
    let fake = FakeRunner::default();

    let result = run(
        &["harness", "install", "claude", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, EXIT_BLOCKED, "{}", result.stdout);
    assert!(
        result.stderr.contains("marketplace.json"),
        "{}",
        result.stderr
    );
    assert!(fake.calls().is_empty());
}

/// Point every harness root's `marketplace.json` at a `tools` plugin. Claude
/// and Cursor carry `version` on the entry; Codex's entry has none, so its
/// pack's own plugin manifest carries it.
fn publish_tools(root: &str, version: &str) {
    let root = Path::new(root);
    let entry =
        format!(r#"{{"name": "tools", "source": "./packs/tools", "version": "{version}"}}"#);
    for manifest in [
        ".claude-plugin/marketplace.json",
        ".cursor-plugin/marketplace.json",
    ] {
        fs::write(
            root.join(manifest),
            format!(r#"{{"name": "acme", "plugins": [{entry}]}}"#),
        )
        .unwrap();
    }
    fs::write(
        root.join(".agents/plugins/marketplace.json"),
        r#"{"name": "acme", "plugins": [{"name": "tools", "source": {"source": "local", "path": "./.agents/packs/tools"}}]}"#,
    )
    .unwrap();
    let pack = root.join(".agents/packs/tools/.codex-plugin");
    fs::create_dir_all(&pack).unwrap();
    fs::write(
        pack.join("plugin.json"),
        format!(r#"{{"name": "tools", "version": "{version}"}}"#),
    )
    .unwrap();
}

fn claude_plugin_at(version: &str) -> String {
    format!(r#"[{{"id": "tools@acme", "scope": "user", "version": "{version}"}}]"#)
}

#[test]
fn update_ac1_an_outdated_claude_plugin_is_updated() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    publish_tools(&root, "0.2.0");
    let fake = FakeRunner::new(&claude_market(&root), &claude_plugin_at("0.1.0"));

    let result = run(
        &[
            "harness", "update", "claude", "--plugin", "tools", "--yes", "--json",
        ],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        fake.calls(),
        [
            CLAUDE_MARKETS,
            CLAUDE_PLUGINS,
            "claude plugin update tools@acme --scope user"
        ]
    );
    let doc: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(doc["outcome"], "updated");
}

#[test]
fn update_ac2_a_current_plugin_runs_only_the_list_commands() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    publish_tools(&root, "0.2.0");
    let fake = FakeRunner::new(&claude_market(&root), &claude_plugin_at("0.2.0"));

    let result = run(
        &["harness", "update", "claude", "--plugin", "tools", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fake.calls(), [CLAUDE_MARKETS, CLAUDE_PLUGINS]);
    assert_eq!(result.stdout, "claude  unchanged · exists\n");
}

#[test]
fn update_installs_a_missing_plugin_instead_of_updating_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    publish_tools(&root, "0.2.0");
    let fake = FakeRunner::new(&claude_market(&root), "[]");

    let result = run(
        &["harness", "update", "claude", "--yes", "--json"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        &fake.calls()[2..],
        ["claude plugin install tools@acme --scope user"]
    );
    let doc: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(doc["outcome"], "added");
}

#[test]
fn update_ac3_codex_removes_and_adds_an_outdated_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    publish_tools(&root, "0.2.0");
    let markets = format!(
        r#"{{"marketplaces": [{{"name": "acme", "marketplaceSource": {{"source": "{root}"}}}}]}}"#
    );
    let fake = FakeRunner::new(
        &markets,
        r#"{"installed": [{"pluginId": "tools@acme", "version": "0.1.0"}]}"#,
    );

    let result = run(&["harness", "update", "codex", "--yes"], dir.path(), &fake);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        &fake.calls()[2..],
        [
            "codex plugin remove tools@acme",
            "codex plugin add tools@acme"
        ]
    );
}

#[test]
fn update_refreshes_a_registered_cursor_marketplace_and_only_adds_a_missing_one() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let url = "https://example.com/acme.git";
    let argv = ["harness", "update", "cursor", "--git-url", url, "--yes"];

    let listed = FakeRunner::new(&format!(r#"[{{"name": "acme", "gitUrl": "{url}"}}]"#), "");
    assert_eq!(run(&argv, dir.path(), &listed).code, 0);
    assert_eq!(
        &listed.calls()[1..],
        ["cursor-agent plugin marketplace update acme"]
    );

    let missing = FakeRunner::new("[]", "");
    assert_eq!(run(&argv, dir.path(), &missing).code, 0);
    assert_eq!(
        missing.calls()[1..],
        [format!("cursor-agent plugin marketplace add {url}")]
    );
}

#[test]
fn update_ac6_codex_rejects_a_scope_other_than_user() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let fake = FakeRunner::default();

    let result = run(
        &["harness", "update", "codex", "--scope", "project", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, EXIT_USAGE);
    assert!(
        result.stderr.contains("--scope user only"),
        "{}",
        result.stderr
    );
    assert!(fake.calls().is_empty());
}

#[test]
fn update_dry_run_plans_the_update_and_runs_only_the_list_commands() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    publish_tools(&root, "0.2.0");
    let fake = FakeRunner::new(&claude_market(&root), &claude_plugin_at("0.1.0"));

    let result = run(
        &["harness", "update", "claude", "--dry-run"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fake.calls(), [CLAUDE_MARKETS, CLAUDE_PLUGINS]);
    assert!(
        result
            .stdout
            .contains("claude  would run · claude plugin update tools@acme --scope user"),
        "{}",
        result.stdout
    );
}

#[test]
fn uninstall_ac4_removes_an_installed_plugin_and_a_rerun_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let argv = [
        "harness",
        "uninstall",
        "claude",
        "--plugin",
        "tools",
        "--yes",
        "--json",
    ];

    let installed = FakeRunner::new(&claude_market(&root), &claude_plugin("user"));
    let result = run(&argv, dir.path(), &installed);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        installed.calls(),
        [
            CLAUDE_MARKETS,
            CLAUDE_PLUGINS,
            "claude plugin uninstall tools@acme --scope user"
        ]
    );
    let doc: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(doc["outcome"], "removed");

    let absent = FakeRunner::new(&claude_market(&root), "[]");
    let result = run(&argv, dir.path(), &absent);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(absent.calls(), [CLAUDE_MARKETS, CLAUDE_PLUGINS]);
    let doc: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(doc["outcome"], "exists");
}

#[test]
fn uninstall_ac5_removes_the_marketplace_only_with_the_flag() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());

    let with_flag = FakeRunner::new(&claude_market(&root), &claude_plugin("user"));
    let result = run(
        &["harness", "uninstall", "claude", "--marketplace", "--yes"],
        dir.path(),
        &with_flag,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        &with_flag.calls()[2..],
        [
            "claude plugin uninstall tools@acme --scope user",
            "claude plugin marketplace remove acme --scope user",
        ]
    );

    let without = FakeRunner::new(&claude_market(&root), &claude_plugin("user"));
    let result = run(
        &["harness", "uninstall", "claude", "--yes"],
        dir.path(),
        &without,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        &without.calls()[2..],
        ["claude plugin uninstall tools@acme --scope user"]
    );
}

#[test]
fn uninstall_cursor_needs_no_git_url_and_matches_the_marketplace_by_name() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let fake = FakeRunner::new(
        r#"[{"name": "acme", "gitUrl": "https://example.com/acme.git"}]"#,
        "",
    );

    let result = run(
        &["harness", "uninstall", "cursor", "--marketplace", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        &fake.calls()[1..],
        ["cursor-agent plugin marketplace remove acme"]
    );
    assert!(!result.stdout.contains("enable in"), "{}", result.stdout);
}

#[test]
fn update_from_git_refreshes_the_marketplace_before_updating_a_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    publish_tools(&root, "0.2.0");
    let url = "https://example.com/acme.git";
    let markets = format!(r#"[{{"name": "acme", "source": "git", "url": "{url}"}}]"#);
    let fake = FakeRunner::new(&markets, &claude_plugin_at("0.1.0"));

    let result = run(
        &["harness", "update", "claude", "--git-url", url, "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        &fake.calls()[2..],
        [
            "claude plugin marketplace update acme",
            "claude plugin update tools@acme --scope user",
        ]
    );
}

/// The harness installs from its own snapshot of a git marketplace, so a
/// plugin added to the repository since that snapshot is installed only after
/// the refresh, and needs one even when nothing installed is out of date.
#[test]
fn update_from_git_refreshes_the_marketplace_before_installing_a_new_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    publish_tools(&root, "0.2.0");
    let url = "https://example.com/acme.git";
    let markets = format!(r#"[{{"name": "acme", "source": "git", "url": "{url}"}}]"#);

    let both = FakeRunner::new(&markets, &claude_plugin_at("0.1.0"));
    let result = run(
        &[
            "harness",
            "update",
            "claude",
            "--git-url",
            url,
            "--plugin",
            "tools",
            "--plugin",
            "newp",
            "--yes",
        ],
        dir.path(),
        &both,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        &both.calls()[2..],
        [
            "claude plugin marketplace update acme",
            "claude plugin install newp@acme --scope user",
            "claude plugin update tools@acme --scope user",
        ]
    );

    let only_new = FakeRunner::new(&markets, &claude_plugin_at("0.1.0"));
    let result = run(
        &[
            "harness",
            "update",
            "claude",
            "--git-url",
            url,
            "--plugin",
            "newp",
            "--yes",
        ],
        dir.path(),
        &only_new,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        &only_new.calls()[2..],
        [
            "claude plugin marketplace update acme",
            "claude plugin install newp@acme --scope user",
        ]
    );
}

#[test]
fn update_skips_a_plugin_whose_listing_has_no_version() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    publish_tools(&root, "0.2.0");
    let fake = FakeRunner::new(&claude_market(&root), &claude_plugin("user"));

    let result = run(&["harness", "update", "claude", "--yes"], dir.path(), &fake);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fake.calls(), [CLAUDE_MARKETS, CLAUDE_PLUGINS]);
}

#[test]
fn uninstall_codex_removes_its_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let markets = format!(
        r#"{{"marketplaces": [{{"name": "acme", "marketplaceSource": {{"source": "{root}"}}}}]}}"#
    );
    let fake = FakeRunner::new(
        &markets,
        r#"{"installed": [{"pluginId": "tools@acme", "version": "0.1.0"}]}"#,
    );

    let result = run(
        &["harness", "uninstall", "codex", "--yes"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(&fake.calls()[2..], ["codex plugin remove tools@acme"]);
}

#[test]
fn uninstall_at_project_scope_removes_the_project_declaration() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let declared = format!(
        r#"{{"extraKnownMarketplaces": {{"acme": {{"source": {{"source": "directory", "path": "{root}"}}}}}}}}"#
    );
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    fs::write(dir.path().join(".claude/settings.json"), declared).unwrap();
    let fake = FakeRunner::new("[]", &claude_plugin("project"));

    let result = run(
        &[
            "harness",
            "uninstall",
            "claude",
            "--scope",
            "project",
            "--marketplace",
            "--yes",
        ],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        fake.calls(),
        [
            CLAUDE_PLUGINS,
            "claude plugin uninstall tools@acme --scope project",
            "claude plugin marketplace remove acme --scope project",
        ]
    );
}

#[test]
fn uninstall_refuses_a_git_ref() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let fake = FakeRunner::default();

    let result = run(
        &[
            "harness",
            "uninstall",
            "codex",
            "--git-url",
            "https://example.com/acme.git",
            "--git-ref",
            "v1",
            "--yes",
        ],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, EXIT_USAGE);
    assert!(result.stderr.contains("--git-ref"), "{}", result.stderr);
    assert!(fake.calls().is_empty());
}

#[test]
fn install_finds_the_plugins_of_a_moved_packs_folder() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nname: m\nbuild:\n  out: dist\nclaude: {packs: moved/claude}\n---\n",
    )
    .unwrap();
    let note = dir.path().join("plugins/rev/skills/a.md");
    fs::create_dir_all(note.parent().unwrap()).unwrap();
    fs::write(note, "---\ndescription: Use a.\n---\n\nbody\n").unwrap();
    let fake = FakeRunner::new("[]", "[]");
    let built = run(&["build", "--yes"], dir.path(), &fake);
    assert_eq!(built.code, 0, "{}", built.stderr);

    let result = run(
        &["harness", "install", "claude", "--dry-run"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.contains("rev@m"), "{}", result.stdout);
}

#[test]
fn install_finds_the_packs_of_one_of_several_marketplaces() {
    let dir = tempfile::tempdir().unwrap();
    let file = |rel: &str, text: &str| {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    };
    file("skillwright.md", "---\nname: m\nbuild:\n  out: .\n---\n");
    file(
        "marketplaces/a/skillwright.md",
        "---\nname: a\nclaude: {packs: mva}\n---\n",
    );
    file("marketplaces/b/skillwright.md", "---\nname: b\n---\n");
    file(
        "marketplaces/a/plugins/p/skills/s.md",
        "---\ndescription: Use s.\n---\n\nbody\n",
    );
    let fake = FakeRunner::new("[]", "[]");
    let built = run(&["build", "--marketplace", "a", "--yes"], dir.path(), &fake);
    assert_eq!(built.code, 0, "{}", built.stderr);

    let result = run(
        &["harness", "install", "claude", "--dry-run"],
        dir.path(),
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.contains("p@a"), "{}", result.stdout);
}

#[test]
fn install_refuses_a_packs_that_leaves_the_output() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nname: m\nbuild:\n  out: dist\nclaude: {packs: ../../..}\n---\n",
    )
    .unwrap();
    let fake = FakeRunner::new("[]", "[]");
    let result = run(
        &["harness", "install", "claude", "--dry-run"],
        dir.path(),
        &fake,
    );
    assert_ne!(result.code, 0);
    assert!(result.stderr.contains("unsafe-path"), "{}", result.stderr);
    assert!(fake.calls().is_empty());
}
// --- pi ---------------------------------------------------------------------

/// `run`, with `home` standing in for the user's home directory.
fn run_home(argv: &[&str], cwd: &Path, home: &Path, runner: &dyn Runner) -> Run {
    let owned: Vec<String> = argv.iter().map(|s| (*s).to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let mut ctx = CommandContext::new(cwd, &mut out, &mut err)
        .with_runner(runner)
        .with_home(home);
    let code = run_with(&owned, &mut ctx);
    drop(ctx);
    Run {
        code,
        stdout: String::from_utf8(out).unwrap(),
        stderr: String::from_utf8(err).unwrap(),
    }
}

/// A built skill folder at `<root>/<packs>/<plugin>/skills/<skill>`.
fn write_built_skill(root: &Path, packs: &str, plugin: &str, skill: &str) -> std::path::PathBuf {
    let dir = root.join(packs).join(plugin).join("skills").join(skill);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), format!("---\nname: {skill}\n---\n")).unwrap();
    dir
}

#[test]
fn pi_install_runs_pi_install_for_each_pack_the_settings_do_not_list() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let home = dir.path().join("home");
    write_built_skill(Path::new(&root), ".pi/packs", "tools", "tools-a");
    let pack = format!("{root}/.pi/packs/tools");
    let fake = FakeRunner::new("", "");

    let result = run_home(
        &["harness", "install", "pi", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fake.calls(), [format!("pi install {pack}")]);

    // Project scope passes `-l`.
    let fake = FakeRunner::new("", "");
    let result = run_home(
        &["harness", "install", "pi", "--scope", "project", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fake.calls(), [format!("pi install {pack} -l")]);

    // Listed in the user settings: nothing runs, on install or update.
    let settings = home.join(".pi/agent/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, format!(r#"{{"packages": ["{pack}"]}}"#)).unwrap();
    for verb in ["install", "update"] {
        let fake = FakeRunner::new("", "");
        let result = run_home(&["harness", verb, "pi", "--yes"], dir.path(), &home, &fake);
        assert_eq!(result.code, 0, "{}", result.stderr);
        assert!(fake.calls().is_empty(), "{verb}: {:?}", fake.calls());
        assert!(
            result.stdout.contains("pi  unchanged · exists"),
            "{}",
            result.stdout
        );
    }
}

#[test]
fn pi_uninstall_runs_nothing_and_names_the_manual_step() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let home = dir.path().join("home");
    let pack = format!("{root}/.pi/packs/tools");
    let settings = home.join(".pi/agent/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, format!(r#"{{"packages": ["{pack}"]}}"#)).unwrap();
    let fake = FakeRunner::new("", "");

    let result = run_home(
        &["harness", "uninstall", "pi", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(result.code, EXIT_BLOCKED, "{}", result.stderr);
    assert!(fake.calls().is_empty(), "{:?}", fake.calls());
    assert!(result.stderr.contains(&pack), "{}", result.stderr);
    assert!(result.stderr.contains("settings.json"), "{}", result.stderr);
}

#[test]
fn pi_update_installs_a_pack_the_settings_do_not_list() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let home = dir.path().join("home");
    write_built_skill(Path::new(&root), ".pi/packs", "tools", "tools-a");
    let fake = FakeRunner::new("", "");

    let result = run_home(
        &["harness", "update", "pi", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fake.calls(), [format!("pi install {root}/.pi/packs/tools")]);
}

#[test]
fn pi_uninstall_with_nothing_installed_reports_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let home = dir.path().join("home");
    let fake = FakeRunner::new("", "");

    let result = run_home(
        &["harness", "uninstall", "pi", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains("pi  unchanged · not installed"),
        "{}",
        result.stdout
    );
    let json = run_home(
        &["harness", "uninstall", "pi", "--yes", "--json"],
        dir.path(),
        &home,
        &fake,
    );
    let doc: serde_json::Value = serde_json::from_str(&json.stdout).unwrap();
    assert_eq!(doc["outcome"], "not-installed");
    assert!(fake.calls().is_empty());
}

#[test]
fn pi_refuses_local_scope_a_git_url_and_a_marketplace() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let home = dir.path().join("home");
    for argv in [
        &["harness", "install", "pi", "--scope", "local", "--yes"][..],
        &[
            "harness",
            "install",
            "pi",
            "--git-url",
            "https://x/r.git",
            "--yes",
        ][..],
        &["harness", "uninstall", "pi", "--marketplace", "--yes"][..],
    ] {
        let fake = FakeRunner::new("", "");
        let result = run_home(argv, dir.path(), &home, &fake);
        assert_eq!(result.code, EXIT_USAGE, "{argv:?}: {}", result.stderr);
        assert!(fake.calls().is_empty());
    }
}

// --- link -------------------------------------------------------------------

#[test]
fn link_symlinks_each_built_skill_and_unlink_removes_only_those_links() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let root = Path::new(&root).canonicalize().unwrap();
    let home = dir.path().join("home");
    let a = write_built_skill(&root, ".claude-plugin/packs", "tools", "a");
    let b = write_built_skill(&root, ".claude-plugin/packs", "other", "b");
    let fake = FakeRunner::new("", "");
    let skills = home.join(".claude/skills");

    let dry = run_home(
        &["harness", "link", "claude", "--dry-run"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(dry.code, 0, "{}", dry.stderr);
    assert!(dry.stdout.contains("would-link"), "{}", dry.stdout);
    assert!(!skills.exists());

    let linked = run_home(
        &["harness", "link", "claude", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(linked.code, 0, "{}", linked.stderr);
    assert_eq!(fs::read_link(skills.join("a")).unwrap(), a);
    assert_eq!(fs::read_link(skills.join("b")).unwrap(), b);
    assert!(fake.calls().is_empty(), "link runs no harness CLI");

    // A second run changes nothing.
    let again = run_home(
        &["harness", "link", "claude", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(again.code, 0, "{}", again.stderr);
    assert_eq!(
        again.stdout.matches("unchanged").count(),
        2,
        "{}",
        again.stdout
    );

    // A real folder of the same name is left alone and unlink skips it.
    fs::remove_file(skills.join("b")).unwrap();
    fs::create_dir_all(skills.join("b")).unwrap();
    fs::write(skills.join("b/SKILL.md"), "mine\n").unwrap();
    let refused = run_home(
        &["harness", "link", "claude", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(refused.code, 1, "{}", refused.stderr);
    assert!(refused.stdout.contains("refused"), "{}", refused.stdout);
    assert_eq!(
        fs::read_to_string(skills.join("b/SKILL.md")).unwrap(),
        "mine\n"
    );

    let unlinked = run_home(
        &["harness", "unlink", "claude", "--yes", "--json"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(unlinked.code, 0, "{}", unlinked.stderr);
    let doc: serde_json::Value = serde_json::from_str(&unlinked.stdout).unwrap();
    let statuses: Vec<&str> = doc["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["unlinked", "unchanged"]);
    assert!(fs::symlink_metadata(skills.join("a")).is_err());
    assert!(skills.join("b/SKILL.md").is_file());
}

#[test]
fn link_uses_the_codex_folder_and_a_plugin_filter_and_warns_that_it_is_shared() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let root = Path::new(&root).canonicalize().unwrap();
    let home = dir.path().join("home");
    write_built_skill(&root, ".agents/packs", "tools", "a");
    write_built_skill(&root, ".agents/packs", "other", "b");
    let fake = FakeRunner::new("", "");

    let codex = run_home(
        &["harness", "link", "codex", "--plugin", "tools", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(codex.code, 0, "{}", codex.stderr);
    assert!(fs::read_link(home.join(".agents/skills/a")).is_ok());
    assert!(fs::symlink_metadata(home.join(".agents/skills/b")).is_err());
    assert!(
        codex.stderr.contains("also read by Cursor and Pi"),
        "{}",
        codex.stderr
    );

    let scoped = run_home(
        &["harness", "link", "codex", "--scope", "project", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(scoped.code, EXIT_USAGE, "{}", scoped.stderr);
}

#[test]
fn link_refuses_cursor_and_pi_naming_the_open_fact() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let root = Path::new(&root).canonicalize().unwrap();
    let home = dir.path().join("home");
    write_built_skill(&root, ".cursor-plugin/packs", "tools", "a");
    write_built_skill(&root, ".pi/packs", "tools", "a");
    for harness in ["cursor", "pi"] {
        for verb in ["link", "unlink"] {
            let fake = FakeRunner::new("", "");
            let result = run_home(
                &["harness", verb, harness, "--yes"],
                dir.path(),
                &home,
                &fake,
            );
            assert_eq!(result.code, EXIT_USAGE, "{harness}: {}", result.stderr);
            assert!(
                result
                    .stderr
                    .contains(&format!("facts/{harness}/skills/discovery-walk.md")),
                "{}",
                result.stderr
            );
        }
    }
    assert!(!home.exists(), "nothing is written");
}

#[test]
#[cfg(unix)]
fn link_refuses_a_shared_folder_name_and_a_foreign_link_and_unlink_leaves_that_link() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let root = Path::new(&root).canonicalize().unwrap();
    let home = dir.path().join("home");
    write_built_skill(&root, ".claude-plugin/packs", "tools", "a");
    write_built_skill(&root, ".claude-plugin/packs", "other", "a");
    write_built_skill(&root, ".claude-plugin/packs", "tools", "b");
    let skills = home.join(".claude/skills");
    fs::create_dir_all(&skills).unwrap();
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, skills.join("b")).unwrap();
    let fake = FakeRunner::new("", "");

    let linked = run_home(
        &["harness", "link", "claude", "--yes", "--json"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(linked.code, 1, "{}", linked.stderr);
    let doc: serde_json::Value = serde_json::from_str(&linked.stdout).unwrap();
    let rows = doc["links"].as_array().unwrap();
    assert_eq!(rows[0]["status"], "refused");
    assert!(
        rows[0]["reason"]
            .as_str()
            .unwrap()
            .contains("share this folder name"),
        "{rows:?}"
    );
    assert_eq!(rows[1]["status"], "refused");
    assert!(
        rows[1]["reason"].as_str().unwrap().contains("a link to"),
        "{rows:?}"
    );
    assert!(fs::symlink_metadata(skills.join("a")).is_err());
    assert_eq!(fs::read_link(skills.join("b")).unwrap(), elsewhere);

    let unlinked = run_home(
        &["harness", "unlink", "claude", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(unlinked.code, 1, "{}", unlinked.stderr);
    assert!(
        unlinked.stdout.contains("not a link this build made"),
        "{}",
        unlinked.stdout
    );
    assert_eq!(fs::read_link(skills.join("b")).unwrap(), elsewhere);
}

#[test]
#[cfg(unix)]
fn unlink_removes_links_to_skills_no_longer_built_even_with_no_build() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let root = Path::new(&root).canonicalize().unwrap();
    let home = dir.path().join("home");
    write_built_skill(&root, ".claude-plugin/packs", "tools", "a");
    let b = write_built_skill(&root, ".claude-plugin/packs", "tools", "b");
    let skills = home.join(".claude/skills");
    let fake = FakeRunner::new("", "");
    let link = |dir: &Path| run_home(&["harness", "link", "claude", "--yes"], dir, &home, &fake);
    assert_eq!(link(dir.path()).code, 0);

    // A link elsewhere, not into the build, is never a stale link.
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, skills.join("mine")).unwrap();

    // `b` is no longer built: its link dangles, and unlink removes it.
    fs::remove_dir_all(&b).unwrap();
    let unlinked = run_home(
        &["harness", "unlink", "claude", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(unlinked.code, 0, "{}", unlinked.stderr);
    assert!(fs::symlink_metadata(skills.join("a")).is_err());
    assert!(fs::symlink_metadata(skills.join("b")).is_err());
    assert!(
        unlinked.stdout.contains("no longer built"),
        "{}",
        unlinked.stdout
    );

    // The whole build is gone: unlink still removes the links into it.
    assert_eq!(link(dir.path()).code, 0);
    fs::remove_dir_all(&root).unwrap();
    let unlinked = run_home(
        &["harness", "unlink", "claude", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(unlinked.code, 0, "{}", unlinked.stderr);
    assert!(fs::symlink_metadata(skills.join("a")).is_err());
    assert_eq!(fs::read_link(skills.join("mine")).unwrap(), elsewhere);
}

#[test]
#[cfg(unix)]
fn a_link_that_fails_part_way_prints_the_rows_first() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let root = Path::new(&root).canonicalize().unwrap();
    let home = dir.path().join("home");
    write_built_skill(&root, ".claude-plugin/packs", "tools", "a");
    write_built_skill(&root, ".claude-plugin/packs", "tools", "b");
    let skills = home.join(".claude/skills");
    fs::create_dir_all(&skills).unwrap();
    fs::set_permissions(&skills, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(skills.join("probe"), "").is_ok() {
        // Running as root: a read-only folder does not stop the link.
        return;
    }
    let fake = FakeRunner::new("", "");

    let result = run_home(
        &["harness", "link", "claude", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    fs::set_permissions(&skills, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(result.code, EXIT_BLOCKED, "{}", result.stderr);
    assert!(result.stdout.contains("a  refused"), "{}", result.stdout);
    assert!(result.stdout.contains("b  skipped"), "{}", result.stdout);
    assert!(result.stderr.contains("link "), "{}", result.stderr);
}

#[test]
fn link_and_pi_refuse_a_git_ref() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let home = dir.path().join("home");
    for argv in [
        &["harness", "link", "claude", "--git-ref", "main", "--yes"][..],
        &["harness", "install", "pi", "--git-ref", "main", "--yes"][..],
    ] {
        let fake = FakeRunner::new("", "");
        let result = run_home(argv, dir.path(), &home, &fake);
        assert_eq!(result.code, EXIT_USAGE, "{argv:?}: {}", result.stderr);
        assert!(result.stderr.contains("--git-ref"), "{}", result.stderr);
        assert!(fake.calls().is_empty());
    }
}

/// A project building into `out` with Claude's and Pi's packs moved.
fn write_moved_project(dir: &Path) -> PathBuf {
    fs::write(
        dir.join("skillwright.md"),
        "---\nname: m\nclaude: {packs: mc}\npi: {packs: mp}\nbuild:\n  out: out\n---\n",
    )
    .unwrap();
    let root = dir.join("out");
    fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

#[test]
fn link_reads_a_moved_packs_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_moved_project(dir.path());
    let home = dir.path().join("home");
    write_built_skill(&root, "mc", "tools", "a");
    let fake = FakeRunner::new("", "");
    let result = run_home(
        &["harness", "link", "claude", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    let link = fs::read_link(home.join(".claude/skills/a")).unwrap();
    assert!(link.starts_with(root.join("mc")), "{}", link.display());
}

#[test]
fn pi_installs_every_built_pack_of_a_moved_packs_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_moved_project(dir.path());
    let home = dir.path().join("home");
    fs::create_dir_all(root.join("mp/one")).unwrap();
    fs::create_dir_all(root.join("mp/two")).unwrap();
    let fake = FakeRunner::new("", "");
    let result = run_home(
        &["harness", "install", "pi", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    let calls = fake.calls().join("\n");
    assert!(calls.contains("mp/one"), "{calls}");
    assert!(calls.contains("mp/two"), "{calls}");
}

#[test]
fn pi_with_nothing_built_says_to_build() {
    let dir = tempfile::tempdir().unwrap();
    write_moved_project(dir.path());
    let fake = FakeRunner::new("", "");
    let result = run_home(
        &["harness", "install", "pi", "--yes"],
        dir.path(),
        &dir.path().join("home"),
        &fake,
    );
    assert_eq!(result.code, EXIT_USAGE, "{}", result.stderr);
    assert!(
        result.stderr.contains("skillwright build"),
        "{}",
        result.stderr
    );
    assert!(fake.calls().is_empty());
}

#[test]
fn link_and_pi_need_an_output_folder() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("skillwright.md"), "---\nname: m\n---\n").unwrap();
    let home = dir.path().join("home");
    for argv in [
        &["harness", "link", "claude", "--yes"][..],
        &["harness", "install", "pi", "--yes"][..],
    ] {
        let fake = FakeRunner::new("", "");
        let result = run_home(argv, dir.path(), &home, &fake);
        assert_ne!(result.code, 0, "{argv:?}");
        assert!(result.stderr.contains("build.out"), "{}", result.stderr);
        assert!(fake.calls().is_empty());
    }
}

#[test]
fn link_and_pi_are_refused_at_a_workspace_root() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nworkspace: true\n---\n",
    )
    .unwrap();
    let home = dir.path().join("home");
    for argv in [
        &["harness", "link", "claude", "--yes"][..],
        &["harness", "unlink", "claude", "--yes"][..],
        &["harness", "install", "pi", "--yes"][..],
    ] {
        let fake = FakeRunner::new("", "");
        let result = run_home(argv, dir.path(), &home, &fake);
        assert_eq!(result.code, EXIT_BLOCKED, "{argv:?}: {}", result.stderr);
        assert!(
            result.stderr.contains("is a workspace"),
            "{}",
            result.stderr
        );
        assert!(fake.calls().is_empty());
    }
}

#[test]
fn link_works_in_a_workspace_member() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nworkspace: true\n---\n",
    )
    .unwrap();
    let member = dir.path().join("marketplaces/one");
    fs::create_dir_all(&member).unwrap();
    fs::write(
        member.join("skillwright.md"),
        "---\nname: one\nbuild:\n  out: built\n---\n",
    )
    .unwrap();
    let root = member.join("built");
    fs::create_dir_all(&root).unwrap();
    write_built_skill(
        &root.canonicalize().unwrap(),
        ".claude-plugin/packs",
        "tools",
        "a",
    );
    let home = dir.path().join("home");
    let fake = FakeRunner::new("", "");
    let result = run_home(
        &["harness", "link", "claude", "--yes"],
        &member,
        &home,
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(fs::read_link(home.join(".claude/skills/a")).is_ok());
}

#[test]
fn a_pack_with_no_skills_does_not_stop_link() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_project(dir.path());
    let root = Path::new(&root);
    let home = dir.path().join("home");
    write_built_skill(root, ".claude-plugin/packs", "tools", "a");
    fs::create_dir_all(root.join(".claude-plugin/packs/cmd/commands")).unwrap();
    let fake = FakeRunner::new("", "");

    let result = run_home(
        &["harness", "link", "claude", "--yes"],
        dir.path(),
        &home,
        &fake,
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(fs::symlink_metadata(home.join(".claude/skills/a")).is_ok());
    assert!(result.stderr.contains("skipped `cmd`"), "{}", result.stderr);

    let argv = ["harness", "link", "claude", "--plugin", "cmd", "--yes"];
    let named = run_home(&argv, dir.path(), &home, &fake);
    assert_ne!(named.code, 0, "{}", named.stderr);
    assert!(
        named.stderr.contains("`cmd` has no skills"),
        "{}",
        named.stderr
    );
    assert!(
        !named.stderr.contains("skillwright build"),
        "{}",
        named.stderr
    );
}
