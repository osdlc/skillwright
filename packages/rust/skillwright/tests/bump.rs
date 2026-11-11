//! `skillwright bump` — the CLI spec's `bump` section, driven in-process the
//! way `tests/cli.rs` drives every other verb: [`run_with`] over a temp copy
//! of a vault, never a checked-in fixture mutated in place.

mod common;

use common::{Run, git, run, vault_str};

use std::fs;
use std::path::{Path, PathBuf};

/// One plugin note, discoverable by structure alone —
/// `marketplaces/<mkt>/plugins/<plugin>/<plugin>.md` — the shape
/// the shared `freeform-vault` fixture's `core.md`
/// uses, by its folder. `version_line` is spliced in as a whole
/// frontmatter line, or omitted for a note with no `version` at all.
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
        format!("{REL}  written · version 0.2.1 -> 0.3.0\n")
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
    let before = fs::read_to_string(&path).unwrap();
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

    let after = fs::read_to_string(&path).unwrap();
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
    // Quoted: unquoted `0.3` is a YAML float, and `version` must be a
    // string at discovery time (`frontmatter::take_string` on
    // `ContainerIdentity.version`) — a float there fails the whole note's
    // discovery, not just this field, so `bump` would never even find the
    // plugin to refuse. A quoted string is the shape that reaches `bump`'s
    // own non-semver refusal, which is what the CLI spec's transcript shows.
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
            "--namespace",
            "sw",
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
    project_in(dir.path(), extra_config);
    dir
}

/// [`project`]'s vault, written into `root`.
fn project_in(root: &Path, extra_config: &str) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("skillwright.md"),
        format!("---\nname: acme\nbuild:\n  out: market\n{extra_config}---\n"),
    )
    .unwrap();
    for (plugin, version) in [("widget", "0.2.1"), ("gadget", "1.0.0")] {
        let base = root.join("marketplaces/acme/plugins").join(plugin);
        fs::create_dir_all(base.join("skills")).unwrap();
        fs::write(
            base.join(format!("{plugin}.md")),
            format!(
                "---\nname: {plugin}\ndescription: The {plugin} plugin.\nversion: {version}\n---\n"
            ),
        )
        .unwrap();
        write_skill(root, plugin, "First draft.");
    }
}

fn write_skill(root: &Path, plugin: &str, body: &str) {
    fs::write(
        root.join("marketplaces/acme/plugins")
            .join(plugin)
            .join("skills")
            .join(format!("{plugin}-skill.md")),
        format!("---\nname: {plugin}-skill\ndescription: Use {plugin}.\n---\n\n{body}\n"),
    )
    .unwrap();
}

fn plugin_note(root: &Path, plugin: &str) -> String {
    fs::read_to_string(
        root.join("marketplaces/acme/plugins")
            .join(plugin)
            .join(format!("{plugin}.md")),
    )
    .unwrap()
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
    assert!(plugin_note(root, "widget").contains("version: 0.2.1"));

    let real = run(&["bump", "--changed-since", "market", "--yes"], root);
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
    let again = run(&["bump", "--changed-since", "market", "--yes"], root);
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
    let note = root.join("marketplaces/acme/plugins/widget/widget.md");
    let text = fs::read_to_string(&note).unwrap();
    fs::write(&note, text.replace("0.2.1", "0.3.0")).unwrap();
    write_skill(root, "widget", "Second draft.");
    let base = root.join("marketplaces/acme/plugins/gizmo");
    fs::create_dir_all(base.join("skills")).unwrap();
    fs::write(
        base.join("gizmo.md"),
        "---\nname: gizmo\ndescription: The gizmo plugin.\nversion: 0.1.0\n---\n",
    )
    .unwrap();
    write_skill(root, "gizmo", "First draft.");
    // Only the manifest moves: a new description.
    let note = root.join("marketplaces/acme/plugins/gadget/gadget.md");
    let text = fs::read_to_string(&note).unwrap();
    fs::write(
        &note,
        text.replace("The gadget plugin.", "The new gadget plugin."),
    )
    .unwrap();

    let result = run(
        &["bump", "--changed-since", "market", "--yes", "--json"],
        root,
    );
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
    let note = root.join("marketplaces/acme/plugins/gadget/gadget.md");
    let text = fs::read_to_string(&note).unwrap();
    fs::write(&note, text.replace("version: 1.0.0\n", "")).unwrap();
    let built = run(&["build", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    write_skill(root, "gadget", "Second draft.");

    let status = |result: &Run| {
        let report: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
        let rows = report["plugins"].as_array().unwrap().clone();
        let gadget = rows.iter().find(|r| r["plugin"] == "gadget").unwrap();
        gadget["status"].as_str().unwrap().to_owned()
    };
    let first = run(
        &["bump", "--changed-since", "market", "--yes", "--json"],
        root,
    );
    assert_eq!(first.code, 0, "{}", first.stderr);
    assert_eq!(status(&first), "written", "{}", first.stdout);
    assert!(plugin_note(root, "gadget").contains("version: 0.1.1"));

    let second = run(
        &["bump", "--changed-since", "market", "--yes", "--json"],
        root,
    );
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
    let gadget = root.join("marketplaces/acme/plugins/gadget");
    let note = gadget.join("gadget.md");
    fs::set_permissions(&note, fs::Permissions::from_mode(0o444)).unwrap();
    fs::set_permissions(&gadget, fs::Permissions::from_mode(0o555)).unwrap();

    let result = run(
        &["bump", "--changed-since", "market", "--yes", "--json"],
        root,
    );
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
fn changed_since_a_git_ref_builds_it_in_a_temp_folder_and_cleans_up() {
    let dir = project("  bump:\n    patch: timestamp\n");
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
    assert!(plugin_note(root, "gadget").contains(&format!("version: {new}")));
    let widget = rows.iter().find(|r| r["plugin"] == "widget").unwrap();
    assert_eq!(widget["status"], "unchanged", "{rows:?}");

    // No worktree was left registered, and nothing was built into `market/`.
    let worktrees = git(root, &["worktree", "list", "--porcelain"]);
    assert_eq!(worktrees.matches("worktree ").count(), 1, "{worktrees}");
    assert!(!root.join("market").exists());
}

/// The number of worktrees `repo` has registered, its own included.
fn worktree_count(repo: &Path) -> usize {
    git(repo, &["worktree", "list", "--porcelain"])
        .matches("worktree ")
        .count()
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
        result.stderr.contains("compare against a built folder"),
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

#[test]
fn a_plugin_named_after_the_marketplace_bumps_the_root_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("skillwright.md"),
        "---\nname: acme\nversion: 1.0.0\nbuild:\n  out: market\n---\n",
    )
    .unwrap();
    let skill = root.join("skills/a.md");
    fs::create_dir_all(skill.parent().unwrap()).unwrap();
    fs::write(&skill, "---\ndescription: Use a.\n---\n\nOne.\n").unwrap();
    assert_eq!(run(&["build", "--yes"], root).code, 0);
    fs::write(&skill, "---\ndescription: Use a.\n---\n\nTwo.\n").unwrap();

    let result = run(&["bump", "--changed-since", "market", "--yes"], root);
    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    let text = fs::read_to_string(root.join("skillwright.md")).unwrap();
    assert!(text.contains("version: 1.0.1"), "{text}");
}

#[test]
fn a_changed_plugin_with_no_note_is_refused_and_says_so() {
    let dir = project("");
    let root = dir.path();
    let base = root.join("marketplaces/acme/plugins/bare/skills");
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("b.md"), "---\ndescription: Use b.\n---\n\nOne.\n").unwrap();
    assert_eq!(run(&["build", "--yes"], root).code, 0);
    fs::write(base.join("b.md"), "---\ndescription: Use b.\n---\n\nTwo.\n").unwrap();

    let result = run(&["bump", "--changed-since", "market", "--yes"], root);
    assert_eq!(result.code, 1, "{}{}", result.stdout, result.stderr);
    assert!(
        result.stdout.contains("no note to bump"),
        "{}",
        result.stdout
    );
}

#[test]
fn changed_since_a_built_folder_reads_moved_packs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("skillwright.md"),
        "---\nname: acme\nclaude: {packs: moved}\nbuild:\n  out: market\n  harnesses: [claude]\n---\n",
    )
    .unwrap();
    let base = root.join("plugins/widget");
    fs::create_dir_all(base.join("skills")).unwrap();
    fs::write(
        base.join("widget.md"),
        "---\ndescription: W.\nversion: 0.2.1\n---\n",
    )
    .unwrap();
    fs::write(
        base.join("skills/a.md"),
        "---\ndescription: A.\n---\n\nOne.\n",
    )
    .unwrap();
    assert_eq!(run(&["build", "--yes"], root).code, 0);
    fs::write(
        base.join("skills/a.md"),
        "---\ndescription: A.\n---\n\nTwo.\n",
    )
    .unwrap();

    let result = run(&["bump", "--changed-since", "market", "--yes"], root);
    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    let note = fs::read_to_string(base.join("widget.md")).unwrap();
    assert!(note.contains("version: 0.2.2"), "{note}");
}

/// A vault with a root `skills/hello.md` and this root file frontmatter.
fn solo(root: &Path, frontmatter: &str) -> PathBuf {
    fs::write(
        root.join("skillwright.md"),
        format!("---\n{frontmatter}---\n"),
    )
    .unwrap();
    let skill = root.join("skills/hello.md");
    fs::create_dir_all(skill.parent().unwrap()).unwrap();
    fs::write(&skill, "---\ndescription: Hi.\n---\n\nOne.\n").unwrap();
    skill
}

#[test]
fn bump_of_the_marketplace_edits_the_bare_version_under_a_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let config = "name: solo\nversion: 1.0.0\nbuild:\n  out: dist\n  namespace: sw\n";
    let skill = solo(root, config);

    let one = run(&["bump", "solo", "--yes"], root);
    assert_eq!(one.code, 0, "{}", one.stderr);
    let text = fs::read_to_string(root.join("skillwright.md")).unwrap();
    assert!(text.contains("\nversion: 1.0.1\n"), "{text}");
    assert!(!text.contains("sw-version"), "{text}");

    assert_eq!(run(&["build", "--yes"], root).code, 0);
    fs::write(&skill, "---\ndescription: Hi.\n---\n\nTwo.\n").unwrap();
    let changed = run(&["bump", "--changed-since", "dist", "--yes"], root);
    assert_eq!(changed.code, 0, "{}{}", changed.stdout, changed.stderr);
    let text = fs::read_to_string(root.join("skillwright.md")).unwrap();
    assert!(text.contains("\nversion: 1.0.2\n"), "{text}");
    assert!(!text.contains("sw-version"), "{text}");
}

#[test]
fn bump_of_a_plugin_note_under_a_namespace_edits_the_key_it_reads() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("skillwright.md"),
        "---\nname: m\nbuild:\n  out: dist\n  namespace: sw\n---\n",
    )
    .unwrap();
    let note = root.join("plugins/p/p.md");
    fs::create_dir_all(note.parent().unwrap()).unwrap();
    fs::write(&note, "---\nsw-description: P.\nsw-version: 2.0.0\n---\n").unwrap();
    let result = run(&["bump", "p", "--yes"], root);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let text = fs::read_to_string(&note).unwrap();
    assert!(text.contains("sw-version: 2.0.1"), "{text}");
}

/// The version `plugin` ships as in the Claude pack under `root/dist`.
fn built_version(root: &Path, plugin: &str) -> String {
    let manifest = root
        .join("dist/.claude-plugin/packs")
        .join(plugin)
        .join(".claude-plugin/plugin.json");
    let doc: serde_json::Value = serde_json::from_slice(&fs::read(manifest).unwrap()).unwrap();
    doc["version"].as_str().unwrap().to_owned()
}

#[test]
fn a_plugin_folder_named_after_the_marketplace_never_takes_the_root_version() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("skillwright.md"),
        "---\nname: acme\nversion: 1.0.0\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    let base = root.join("plugins/acme");
    fs::create_dir_all(base.join("skills")).unwrap();
    let skill = base.join("skills/a.md");
    fs::write(&skill, "---\ndescription: A.\n---\n\nOne.\n").unwrap();

    // A folder with no note: the warning, the manifest and `bump` agree.
    let built = run(&["build", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert_eq!(built_version(root, "acme"), "0.1.0");
    assert!(
        built.stderr.contains("published as `0.1.0`"),
        "{}",
        built.stderr
    );
    fs::write(&skill, "---\ndescription: A.\n---\n\nTwo.\n").unwrap();
    let check = run(&["check"], root);
    assert!(!check.stderr.contains("bump acme"), "{}", check.stderr);
    let refused = run(&["bump", "acme", "--dry-run"], root);
    assert!(
        refused.stderr.contains("add `plugins/acme/acme.md`"),
        "{}",
        refused.stderr
    );

    // A note with no version: the same, and a bump moves up from there.
    fs::write(base.join("acme.md"), "---\ndescription: A.\n---\n").unwrap();
    let built = run(&["build", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert_eq!(built_version(root, "acme"), "0.1.0");
    assert!(
        built.stderr.contains("published as `0.1.0`"),
        "{}",
        built.stderr
    );
    assert_eq!(run(&["bump", "acme", "--yes"], root).code, 0);
    assert_eq!(run(&["build", "--yes"], root).code, 0);
    assert_eq!(built_version(root, "acme"), "0.1.1");
    let text = fs::read_to_string(root.join("skillwright.md")).unwrap();
    assert!(text.contains("version: 1.0.0"), "{text}");
}

#[test]
fn the_marketplaces_own_plugin_takes_its_version_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let skill = solo(root, "name: solo\nversion: 1.0.0\nbuild:\n  out: dist\n");
    let built = run(&["build", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert_eq!(built_version(root, "solo"), "1.0.0");
    assert!(!built.stderr.contains("published as"), "{}", built.stderr);

    fs::write(&skill, "---\ndescription: Hi.\n---\n\nTwo.\n").unwrap();
    let check = run(&["check"], root);
    assert!(check.stderr.contains("bump solo"), "{}", check.stderr);
    assert_eq!(run(&["bump", "solo", "--yes"], root).code, 0);
    assert_eq!(run(&["build", "--yes"], root).code, 0);
    assert_eq!(built_version(root, "solo"), "1.0.1");
}

#[test]
fn a_ref_from_before_a_packs_move_still_finds_the_change() {
    let dir = project("  harnesses: [claude]\n");
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "one"]);
    fs::write(
        root.join("skillwright.md"),
        "---\nname: acme\nclaude: {packs: moved}\nbuild:\n  out: market\n  harnesses: [claude]\n---\n",
    )
    .unwrap();
    write_skill(root, "widget", "Second draft.");

    let result = run(&["bump", "--changed-since", "HEAD", "--yes"], root);
    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    assert!(
        plugin_note(root, "widget").contains("version: 0.2.2"),
        "{}",
        result.stdout
    );
    assert!(
        plugin_note(root, "gadget").contains("version: 1.0.0"),
        "{}",
        result.stdout
    );
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A built `project` whose Pi packs sit in a plain folder inside `out`, which
/// only `build.out` marks as output.
fn built_with_moved_packs() -> tempfile::TempDir {
    let dir = project("  harnesses: [claude, pi]\n");
    let root = dir.path();
    let config = fs::read_to_string(root.join("skillwright.md")).unwrap();
    fs::write(
        root.join("skillwright.md"),
        config.replace("name: acme\n", "name: acme\npi: {packs: pi-packs}\n"),
    )
    .unwrap();
    let built = run(&["build", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(root.join("market/pi-packs/widget").is_dir());
    dir
}

#[test]
fn changed_since_another_folder_does_not_read_the_moved_packs_in_out() {
    let dir = built_with_moved_packs();
    let root = dir.path();
    let baseline = tempfile::tempdir().unwrap();
    copy_dir(&root.join("market"), baseline.path());
    write_skill(root, "widget", "Second draft.");

    let since = baseline.path().display().to_string();
    let result = run(&["bump", "--changed-since", &since, "--yes"], root);
    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    assert!(plugin_note(root, "widget").contains("version: 0.2.2"));
    assert!(plugin_note(root, "gadget").contains("version: 1.0.0"));
}

#[test]
fn changed_since_a_ref_with_out_committed_does_not_read_its_moved_packs() {
    let dir = built_with_moved_packs();
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "one"]);
    write_skill(root, "widget", "Second draft.");

    let result = run(&["bump", "--changed-since", "HEAD", "--yes"], root);
    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    assert!(plugin_note(root, "widget").contains("version: 0.2.2"));
    assert!(plugin_note(root, "gadget").contains("version: 1.0.0"));
}

#[test]
fn build_into_another_folder_does_not_read_the_moved_packs_in_out() {
    let dir = built_with_moved_packs();
    let root = dir.path();
    let other = tempfile::tempdir().unwrap();
    let out = other.path().display().to_string();
    let result = run(&["build", "--out", &out, "--yes"], root);
    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    assert!(other.path().join("pi-packs/widget").is_dir());
}

#[test]
fn check_against_another_folder_does_not_read_the_moved_packs_in_out() {
    let dir = built_with_moved_packs();
    let root = dir.path();
    let other = tempfile::tempdir().unwrap();
    let out = other.path().display().to_string();
    let result = run(&["check", "--out", &out], root);
    // Drift against an empty folder, not a refusal of the packs in `out`.
    assert_eq!(result.code, 1, "{}{}", result.stdout, result.stderr);
    let said = format!("{}{}", result.stdout, result.stderr);
    assert!(!said.contains("unreadable-note"), "{said}");
}

/// A vault of two marketplaces, each with a plugin `p`, building to `dist`.
fn two_marketplaces(root: &Path) {
    fs::write(
        root.join("skillwright.md"),
        "---\nname: v\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    for market in ["one", "two"] {
        let base = root.join("marketplaces").join(market);
        fs::create_dir_all(base.join("plugins/p/skills")).unwrap();
        fs::write(
            base.join("skillwright.md"),
            format!("---\nname: {market}\ndescription: M.\n---\n"),
        )
        .unwrap();
        fs::write(
            base.join("plugins/p/p.md"),
            "---\ndescription: P.\nversion: 1.0.0\n---\n",
        )
        .unwrap();
        fs::write(
            base.join("plugins/p/skills/a.md"),
            "---\ndescription: A.\n---\n\nOne.\n",
        )
        .unwrap();
    }
}

fn market_note(root: &Path, market: &str) -> String {
    fs::read_to_string(
        root.join("marketplaces")
            .join(market)
            .join("plugins/p/p.md"),
    )
    .unwrap()
}

#[test]
fn marketplace_picks_one_of_several_for_a_bump() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    two_marketplaces(root);

    let flagless = run(&["bump", "p", "--yes"], root);
    assert_eq!(flagless.code, 3, "{}", flagless.stderr);
    assert!(
        flagless.stderr.contains("--marketplace"),
        "{}",
        flagless.stderr
    );
    let one = run(&["bump", "p", "--marketplace", "two", "--yes"], root);
    assert_eq!(one.code, 0, "{}", one.stderr);
    assert!(market_note(root, "two").contains("version: 1.0.1"));
    assert!(market_note(root, "one").contains("version: 1.0.0"));

    let unknown = run(&["bump", "p", "--marketplace", "nope", "--yes"], root);
    assert_eq!(unknown.code, 3, "{}", unknown.stderr);
    assert!(
        unknown.stderr.contains("no marketplace `nope`"),
        "{}",
        unknown.stderr
    );
}

#[test]
fn changed_since_works_on_one_of_several_marketplaces() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    two_marketplaces(root);
    let skill = root.join("marketplaces/one/plugins/p/skills/a.md");
    let built = run(&["build", "--marketplace", "one", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    fs::write(&skill, "---\ndescription: A.\n---\n\nTwo.\n").unwrap();

    let argv = [
        "bump",
        "--changed-since",
        "dist",
        "--marketplace",
        "one",
        "--yes",
    ];
    let result = run(&argv, root);
    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    assert!(market_note(root, "one").contains("version: 1.0.1"));
    assert!(market_note(root, "two").contains("version: 1.0.0"));

    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    git(root, &["add", "skillwright.md", "marketplaces"]);
    git(root, &["commit", "--quiet", "-m", "one"]);
    fs::write(&skill, "---\ndescription: A.\n---\n\nThree.\n").unwrap();
    let argv = [
        "bump",
        "--changed-since",
        "HEAD",
        "--marketplace",
        "one",
        "--yes",
    ];
    let from_ref = run(&argv, root);
    assert_eq!(from_ref.code, 0, "{}{}", from_ref.stdout, from_ref.stderr);
    assert!(market_note(root, "one").contains("version: 1.0.2"));
}

#[test]
fn marketplace_picks_a_workspace_member_for_a_bump() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("skillwright.md"), "---\nworkspace: true\n---\n").unwrap();
    let member = root.join("marketplaces/one");
    fs::create_dir_all(member.join("plugins/p")).unwrap();
    fs::write(
        member.join("skillwright.md"),
        "---\nname: one\nbuild:\n  out: built\n---\n",
    )
    .unwrap();
    let note = member.join("plugins/p/p.md");
    fs::write(&note, "---\ndescription: P.\nversion: 1.0.0\n---\n").unwrap();

    let bare = run(&["bump", "p", "--yes"], root);
    assert_ne!(bare.code, 0, "{}", bare.stderr);
    assert!(
        bare.stderr.contains("--marketplace <name>"),
        "{}",
        bare.stderr
    );
    let one = run(&["bump", "p", "--marketplace", "one", "--yes"], root);
    assert_eq!(one.code, 0, "{}", one.stderr);
    let text = fs::read_to_string(&note).unwrap();
    assert!(text.contains("version: 1.0.1"), "{text}");
}

#[test]
fn check_advises_a_bump_that_runs_in_a_vault_of_several_marketplaces() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    two_marketplaces(root);
    assert_eq!(
        run(&["build", "--marketplace", "two", "--yes"], root).code,
        0
    );
    fs::write(
        root.join("marketplaces/two/plugins/p/skills/a.md"),
        "---\ndescription: A.\n---\n\nTwo.\n",
    )
    .unwrap();

    let check = run(&["check", "--marketplace", "two"], root);
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
    assert_eq!(advised, "skillwright bump p --marketplace two");
    let mut argv: Vec<&str> = advised.split(' ').skip(1).collect();
    argv.push("--yes");
    let bumped = run(&argv, root);
    assert_eq!(bumped.code, 0, "{}", bumped.stderr);
    assert!(market_note(root, "two").contains("version: 1.0.1"));
    assert!(market_note(root, "one").contains("version: 1.0.0"));
}

#[test]
fn check_at_a_workspace_root_advises_a_bump_that_runs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("skillwright.md"), "---\nworkspace: true\n---\n").unwrap();
    let member = root.join("marketplaces/one");
    fs::create_dir_all(member.join("plugins/p/skills")).unwrap();
    fs::write(
        member.join("skillwright.md"),
        "---\nname: one\nbuild:\n  out: built\n---\n",
    )
    .unwrap();
    let note = member.join("plugins/p/p.md");
    fs::write(&note, "---\ndescription: P.\nversion: 1.0.0\n---\n").unwrap();
    let skill = member.join("plugins/p/skills/a.md");
    fs::write(&skill, "---\ndescription: A.\n---\n\nOne.\n").unwrap();
    assert_eq!(run(&["build", "--yes"], root).code, 0);
    fs::write(&skill, "---\ndescription: A.\n---\n\nTwo.\n").unwrap();

    let check = run(&["check"], root);
    assert!(
        check
            .stderr
            .contains("`skillwright bump p --marketplace one`"),
        "{}",
        check.stderr
    );
    let bumped = run(&["bump", "p", "--marketplace", "one", "--yes"], root);
    assert_eq!(bumped.code, 0, "{}", bumped.stderr);
}

#[test]
fn changed_since_a_ref_refuses_packs_on_a_harness_path_as_build_does() {
    let dir = project("  harnesses: [claude]\n");
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch", "main"]);
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "one"]);
    fs::write(
        root.join("skillwright.md"),
        "---\nname: acme\nclaude: {packs: .claude-plugin}\nbuild:\n  out: market\n  harnesses: [claude]\n---\n",
    )
    .unwrap();

    let build = run(&["build", "--yes"], root);
    assert!(build.stderr.contains("path-collision"), "{}", build.stderr);
    let result = run(&["bump", "--changed-since", "HEAD", "--yes"], root);
    assert_eq!(result.code, build.code, "{}", result.stderr);
    assert!(
        result.stderr.contains("path-collision"),
        "{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("moving its packs"),
        "{}",
        result.stderr
    );
    assert_eq!(worktree_count(root), 1);
}

#[test]
fn a_nested_marketplace_never_takes_the_root_files_version() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("skillwright.md"),
        "---\nname: home\nversion: 9.0.0\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    let barn = root.join("marketplaces/barn");
    fs::create_dir_all(barn.join("skills")).unwrap();
    fs::write(
        barn.join("skillwright.md"),
        "---\ndescription: Barn.\n---\n",
    )
    .unwrap();
    fs::write(
        barn.join("skills/two.md"),
        "---\ndescription: Two.\n---\n\nTwo.\n",
    )
    .unwrap();

    let built = run(&["build", "--marketplace", "barn", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(built.stderr.contains("missing-version"), "{}", built.stderr);
    assert_eq!(built_version(root, "barn"), "0.1.0");

    let bumped = run(&["bump", "barn", "--marketplace", "barn", "--yes"], root);
    assert_eq!(bumped.code, 0, "{}", bumped.stderr);
    assert_eq!(
        run(&["build", "--marketplace", "barn", "--yes"], root).code,
        0
    );
    assert_eq!(built_version(root, "barn"), "0.1.1");
    let outer = fs::read_to_string(root.join("skillwright.md")).unwrap();
    assert!(outer.contains("version: 9.0.0"), "{outer}");
}

// --- `--changed-since` a git ref, what the baseline build reads -------------

/// Stages and commits everything in `repo`.
fn commit_all(repo: &Path, message: &str) {
    git(repo, &["add", "."]);
    git(repo, &["commit", "--quiet", "-m", message]);
}

/// A repository whose vault is `vault/`: [`project`]'s marketplace with
/// `extra_config` in its root file, plus what `extra` writes into the
/// repository, committed once. The repository path is canonical.
fn repo_with_vault(extra_config: &str, extra: impl FnOnce(&Path)) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().canonicalize().unwrap();
    project_in(&repo.join("vault"), extra_config);
    extra(&repo);
    git(&repo, &["init", "--quiet", "--initial-branch", "main"]);
    commit_all(&repo, "one");
    (dir, repo)
}

/// Writes `text` to `path`, creating its folder.
fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
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

const WIDGET_ASSET: &str =
    "  assets:\n    - {from: ../tools/run.sh, to: bin/run.sh, plugin: widget}\n";

#[test]
fn an_asset_outside_the_vault_changed_since_the_ref_bumps_its_plugin() {
    let (_dir, repo) = repo_with_vault(WIDGET_ASSET, |repo| {
        write(&repo.join("tools/run.sh"), "echo one\n");
    });
    write(&repo.join("tools/run.sh"), "echo two\n");

    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    assert_eq!(
        statuses(&result),
        rows(&[("widget", "dry-run"), ("gadget", "unchanged")])
    );
}

#[test]
fn an_unchanged_asset_outside_the_vault_bumps_nothing() {
    let (_dir, repo) = repo_with_vault(WIDGET_ASSET, |repo| {
        write(&repo.join("tools/run.sh"), "echo one\n");
    });

    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    assert_eq!(
        statuses(&result),
        rows(&[("widget", "unchanged"), ("gadget", "unchanged")])
    );
}

fn hooks_json(command: &str) -> String {
    format!(
        "{{\"hooks\": {{\"SessionStart\": [{{\"hooks\": [{{\"type\": \"command\", \"command\": \"{command}\"}}]}}]}}}}\n"
    )
}

#[test]
fn a_hooks_file_outside_the_vault_changed_since_the_ref_bumps_its_plugin() {
    let (_dir, repo) = repo_with_vault(
        "  hooks:\n    - {from: ../hooks/hooks.json, plugin: widget}\n",
        |repo| write(&repo.join("hooks/hooks.json"), &hooks_json("echo one")),
    );
    write(&repo.join("hooks/hooks.json"), &hooks_json("echo two"));

    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    assert_eq!(
        statuses(&result),
        rows(&[("widget", "dry-run"), ("gadget", "unchanged")])
    );
}

#[test]
fn an_asset_outside_the_repository_is_read_from_disk() {
    let outside = tempfile::tempdir().unwrap();
    let asset = outside.path().canonicalize().unwrap().join("run.sh");
    write(&asset, "echo one\n");
    let config = format!(
        "  assets:\n    - {{from: {}, to: bin/run.sh, plugin: widget}}\n",
        asset.display()
    );
    let (_dir, repo) = repo_with_vault(&config, |_| {});

    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    assert_eq!(
        statuses(&result),
        rows(&[("widget", "unchanged"), ("gadget", "unchanged")])
    );
}

/// A workspace at the repository's top whose file sets `license: MIT` and
/// `workspace_build`, with members `m1` and `m2`, each holding plugin
/// `p1`/`p2` and one skill, committed once.
fn workspace_repo(workspace_build: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().canonicalize().unwrap();
    write(
        &repo.join("skillwright.md"),
        &format!("---\nworkspace: true\nlicense: MIT\n{workspace_build}---\n"),
    );
    for (member, plugin) in [("m1", "p1"), ("m2", "p2")] {
        let root = repo.join("marketplaces").join(member);
        write(
            &root.join("skillwright.md"),
            &format!("---\nname: {member}\nbuild:\n  out: built\n---\n"),
        );
        write(
            &root.join(format!("plugins/{plugin}/{plugin}.md")),
            &format!("---\nname: {plugin}\ndescription: P.\nversion: 1.0.0\n---\n"),
        );
        member_skill(&repo, member, plugin, "One.");
    }
    git(&repo, &["init", "--quiet", "--initial-branch", "main"]);
    commit_all(&repo, "one");
    (dir, repo)
}

fn member_skill(repo: &Path, member: &str, plugin: &str, body: &str) {
    write(
        &repo.join(format!(
            "marketplaces/{member}/plugins/{plugin}/skills/s.md"
        )),
        &format!("---\nname: s-{plugin}\ndescription: S.\n---\n\n{body}\n"),
    );
}

#[test]
fn changed_since_in_a_workspace_member_reads_the_workspace_file_at_the_ref() {
    let (_dir, repo) = workspace_repo("");
    let m1 = repo.join("marketplaces/m1");
    let unchanged = rows(&[("p1", "unchanged")]);
    let from_member = dry_bump(&m1, "HEAD", &[]);
    assert_eq!(statuses(&from_member), unchanged);
    let from_top = dry_bump(&repo, "HEAD", &["--marketplace", "m1"]);
    assert_eq!(statuses(&from_top), unchanged);

    member_skill(&repo, "m2", "p2", "Two.");
    assert_eq!(statuses(&dry_bump(&m1, "HEAD", &[])), unchanged);

    member_skill(&repo, "m1", "p1", "Two.");
    let changed = rows(&[("p1", "dry-run")]);
    assert_eq!(statuses(&dry_bump(&m1, "HEAD", &[])), changed);
    assert_eq!(
        statuses(&dry_bump(&repo, "HEAD", &["--marketplace", "m1"])),
        changed
    );
}

#[test]
fn a_workspace_member_inherits_an_asset_at_the_ref() {
    let (_dir, repo) = workspace_repo(
        "build:\n  assets:\n    - {from: ../../shared/NOTICE, to: NOTICE, plugin: p1}\n",
    );
    // Committed after the fixture's own commit, so the ref holds it.
    write(&repo.join("shared/NOTICE"), "one\n");
    commit_all(&repo, "notice");
    let m1 = repo.join("marketplaces/m1");
    assert_eq!(
        statuses(&dry_bump(&m1, "HEAD", &[])),
        rows(&[("p1", "unchanged")])
    );

    write(&repo.join("shared/NOTICE"), "two\n");
    assert_eq!(
        statuses(&dry_bump(&m1, "HEAD", &[])),
        rows(&[("p1", "dry-run")])
    );
}

#[test]
fn an_asset_added_since_the_ref_reads_as_it_did() {
    let (_dir, repo) = repo_with_vault(WIDGET_ASSET, |_| {});
    write(&repo.join("tools/run.sh"), "echo one\n");

    // The ref's build cannot find the asset, as a full checkout of the ref
    // could not.
    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(
        result
            .stderr
            .contains("--changed-since `HEAD`: building it failed: missing-asset"),
        "{}",
        result.stderr
    );
    assert!(result.stdout.is_empty(), "{}", result.stdout);
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
        result.stderr.contains("holds no x/skillwright.md here"),
        "{}",
        result.stderr
    );
    assert_eq!(worktree_count(root), 1);
}

#[cfg(unix)]
#[test]
fn a_symlink_in_the_vault_reads_its_target_at_the_ref() {
    // The current tree refuses a symlinked note, so only the ref holds one.
    let skill = "vault/marketplaces/acme/plugins/widget/skills/widget-skill.md";
    let note = "---\nname: widget-skill\ndescription: Use widget.\n---\n\nLinked.\n";
    let (_dir, repo) = repo_with_vault("", |repo| {
        write(&repo.join("notes/widget-skill.md"), note);
        let link = repo.join(skill);
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("../../../../../../notes/widget-skill.md", &link).unwrap();
    });
    fs::remove_file(repo.join(skill)).unwrap();
    write(&repo.join(skill), note);

    // The ref's build refuses the link, as it refuses one on disk.
    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    assert_symlink_refused(&result, skill);
}

#[cfg(unix)]
#[test]
fn a_root_file_that_is_a_symlink_at_the_ref_builds_as_it_did() {
    // The current tree refuses a symlinked root file, so only the ref holds one.
    let (_dir, repo) = repo_with_vault("", |repo| {
        let root_file = repo.join("vault/skillwright.md");
        let text = fs::read_to_string(&root_file).unwrap();
        write(&repo.join("shared/skillwright.md"), &text);
        fs::remove_file(&root_file).unwrap();
        std::os::unix::fs::symlink("../shared/skillwright.md", &root_file).unwrap();
    });
    let root_file = repo.join("vault/skillwright.md");
    fs::remove_file(&root_file).unwrap();
    fs::copy(repo.join("shared/skillwright.md"), &root_file).unwrap();

    // The link resolves, so the ref's root file is found and read; its
    // build then refuses the link.
    let result = dry_bump(&repo.join("vault"), "HEAD", &[]);
    assert_symlink_refused(&result, "vault/skillwright.md");
}

/// The ref's build was refused for the symlink at the repository path `rel`.
fn assert_symlink_refused(result: &Run, rel: &str) {
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(
        result
            .stderr
            .contains("--changed-since `HEAD`: building it failed: unsafe-path:"),
        "{}",
        result.stderr
    );
    assert!(
        result
            .stderr
            .contains(&format!("{rel}: a symlink in a skills source")),
        "{}",
        result.stderr
    );
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
    let skill = "vault/marketplaces/acme/plugins/widget/skills/widget-skill.md";
    let (_dir, repo) = repo_with_vault("", |repo| {
        write(
            &repo.join(skill),
            "---\nname: [widget-skill\n---\n\nBroken.\n",
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
        stderr.contains("compare against a built folder"),
        "{stderr}"
    );
    assert_eq!(baselines_in(tmp.path()), ["guard"]);
    assert_eq!(leftovers(root), before);
}
