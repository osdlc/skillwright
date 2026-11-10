//! argv → exit-code mapping.
//!
//! Every case drives [`skillwright::cli::run_with`] in-process, with the
//! working directory on the context rather than the process — so these run in
//! parallel without changing the process's directory.

mod common;

use common::{Run, git, run, vault_str};

use std::fs;
use std::path::{Path, PathBuf};

use tradecraft_core::{Namespace, NativeVfs, PublishRequest, PublishStatus, publish_skill};

/// [`run`] over a fixed argv plus whatever a case adds — the shape every
/// per-verb closure below shares.
fn run_argv(mut base: Vec<String>, extra: &[&str], cwd: &Path) -> Run {
    base.extend(extra.iter().map(|s| (*s).to_string()));
    let borrowed: Vec<&str> = base.iter().map(String::as_str).collect();
    run(&borrowed, cwd)
}

/// The base fixture: one skill note in
/// the canonical shape, inside the folder named for it.
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let skill = vault.join("skills").join("code-review");
    fs::create_dir_all(skill.join("references")).unwrap();
    fs::write(
        skill.join("code-review.md"),
        "---\nname: code-review\ndescription: reviews code\nlicense: MIT\n---\nReview the diff.\n",
    )
    .unwrap();
    fs::write(skill.join("references").join("style.md"), "# Style\n").unwrap();
    (dir, vault)
}

/// The core's own fixture vault, read read-only: it holds a marketplace
/// note, which a build needs and the temp fixture above does not have.
fn donor_vault() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/agent-skills-vault")
        .canonicalize()
        .unwrap()
        .display()
        .to_string()
}

/// The built binary, run as a child process with the environment under the
/// test's control: the in-process [`run`] shares the process's `$HOME`.
fn run_binary(argv: &[&str], cwd: &Path, env: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_skillwright"));
    cmd.args(argv).current_dir(cwd);
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
}

// --- the usage line ----------------------------------------------------------

#[test]
fn no_argv_is_the_usage_line_and_exit_3() {
    let dir = tempfile::tempdir().unwrap();
    let run = run(&[], dir.path());
    assert_eq!(run.code, 3);
    assert_eq!(run.stdout, "");
    assert_eq!(
        run.stderr,
        "usage: skillwright <publish|build|check|list|init|adopt|status|unpublish|harness> [options]\n"
    );
}

#[test]
fn version_prints_the_crate_version() {
    let dir = tempfile::tempdir().unwrap();
    for flag in ["--version", "-V"] {
        let run = run(&[flag], dir.path());
        assert_eq!(run.code, 0);
        assert_eq!(
            run.stdout.trim(),
            format!("skillwright {}", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn the_root_help_names_every_verb_this_binary_declares() {
    let dir = tempfile::tempdir().unwrap();
    let run = run(&["--help"], dir.path());
    assert_eq!(run.code, 0);
    for verb in [
        "publish",
        "build",
        "check",
        "list",
        "init",
        "adopt",
        "status",
        "unpublish",
        "harness",
    ] {
        assert!(
            run.stdout.contains(verb),
            "{verb} missing from {}",
            run.stdout
        );
    }
}

#[test]
fn the_root_help_names_every_global_flag_and_the_three_nouns() {
    let dir = tempfile::tempdir().unwrap();
    let run = run(&["--help"], dir.path());
    assert_eq!(run.code, 0);
    for flag in [
        "--vault",
        "--config",
        "--namespace",
        "--json",
        "--dry-run",
        "--yes",
        "--quiet",
    ] {
        assert!(run.stdout.contains(flag), "{flag} missing");
    }
    // The same sentence the verb prints for it, not a second wording.
    assert!(
        run.stdout
            .contains("Vault root: the folder holding `skillwright.md`.")
    );
    // The two a read-only verb does not declare are marked as such.
    assert!(run.stdout.contains("Only on a verb that writes."));
    for noun in ["vault", "destination", "target"] {
        assert!(run.stdout.contains(noun));
    }
}

// --- the per-verb global sets ------------------------------------------------

#[test]
fn a_write_only_global_is_not_a_flag_on_a_verb_that_writes_nothing() {
    let (dir, vault) = fixture();
    for argv in [
        vec!["list", "--vault", &vault_str(&vault), "--yes"],
        vec!["list", "--vault", &vault_str(&vault), "--dry-run"],
        vec!["status", "--yes"],
        vec!["status", "--dry-run"],
    ] {
        let run = run(&argv, dir.path());
        assert_eq!(run.code, 3, "{argv:?} exited {}", run.code);
    }
}

#[test]
fn a_verb_that_writes_declares_all_seven() {
    let (dir, vault) = fixture();
    // `--dry-run` and `--yes` parse, and the dry run writes nothing — not
    // clap's "unexpected argument".
    let run = run(
        &[
            "build",
            "--vault",
            &vault_str(&vault),
            "--out",
            &vault_str(&vault),
            "--dry-run",
            "--yes",
            "--quiet",
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(!vault.join(".claude-plugin").exists());
    assert!(
        !run.stderr.contains("unexpected argument"),
        "{}",
        run.stderr
    );
}

// --- the not-implemented verbs -----------------------------------------------

#[test]
fn unpublish_with_its_note_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let run = run(
        &["unpublish", "skills/code-review/code-review.md"],
        dir.path(),
    );
    assert_eq!(run.code, 2);
    assert!(
        run.stderr.starts_with("not implemented: unpublish\n"),
        "{}",
        run.stderr
    );
}

#[test]
fn unpublish_without_its_required_note_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(run(&["unpublish"], dir.path()).code, 3);
}

#[test]
fn a_stub_verbs_help_says_it_is_not_implemented_and_names_the_exit() {
    let dir = tempfile::tempdir().unwrap();
    let helped = run(&["unpublish", "--help"], dir.path());
    assert_eq!(helped.code, 0);
    assert!(helped.stdout.contains("not implemented: unpublish"));
    assert!(helped.stdout.contains("exits 2"));
    // The marker is the same sentence the exit prints.
    let ran = run(&["unpublish", "note.md"], dir.path());
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    let sentence = ran.stderr.lines().nth(1).unwrap();
    assert!(!sentence.is_empty());
    assert!(helped.stdout.contains(sentence), "{}", helped.stdout);
}

// --- init --------------------------------------------------------------------

#[test]
fn init_writes_the_name_and_the_output_folder() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("My Skills");
    std::fs::create_dir(&folder).unwrap();
    let ran = run(&["init", "--out", "dist"], &folder);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    let root = tradecraft_core::parse_root_file(
        &std::fs::read_to_string(folder.join("skillwright.md")).unwrap(),
    )
    .unwrap();
    assert_eq!(root.build.out.as_deref(), Some("dist"));
    assert_eq!(root.marketplace.name.as_deref(), Some("my-skills"));
}

#[test]
fn init_takes_a_name_and_refuses_a_bad_one() {
    let dir = tempfile::tempdir().unwrap();
    let bad = run(
        &["init", "--out", "dist", "--name", "Not A Slug"],
        dir.path(),
    );
    assert_eq!(bad.code, 3, "{}", bad.stderr);
    assert!(!dir.path().join("skillwright.md").exists());

    let good = run(&["init", "--out", "dist", "--name", "tools"], dir.path());
    assert_eq!(good.code, 0, "{}", good.stderr);
    let text = std::fs::read_to_string(dir.path().join("skillwright.md")).unwrap();
    assert!(text.contains("name: tools"), "{text}");
}

#[test]
fn init_needs_out_off_a_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let ran = run(&["init"], dir.path());
    assert_eq!(ran.code, 3, "{}", ran.stderr);
    assert!(ran.stderr.contains("--out"), "{}", ran.stderr);
    assert!(!dir.path().join("skillwright.md").exists());
}

#[test]
fn init_refuses_an_existing_root_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("skillwright.md"), "---\nname: kept\n---\n").unwrap();
    let ran = run(&["init", "--out", "dist"], dir.path());
    assert_eq!(ran.code, 1, "{}", ran.stderr);
    assert!(ran.stderr.contains("already exists"), "{}", ran.stderr);
    let text = std::fs::read_to_string(dir.path().join("skillwright.md")).unwrap();
    assert!(text.contains("kept"));
}

#[test]
fn init_refuses_a_blank_out() {
    let dir = tempfile::tempdir().unwrap();
    for out in ["", "  "] {
        let ran = run(&["init", "--out", out], dir.path());
        assert_eq!(ran.code, 3, "{}", ran.stderr);
    }
    assert!(!dir.path().join("skillwright.md").exists());
}

#[cfg(unix)]
#[test]
fn init_does_not_write_through_a_dangling_link() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("elsewhere.md");
    std::os::unix::fs::symlink(&target, dir.path().join("skillwright.md")).unwrap();
    let ran = run(&["init", "--out", "dist"], dir.path());
    assert_eq!(ran.code, 1, "{}", ran.stderr);
    assert!(!target.exists());
}

#[test]
fn a_vault_init_wrote_builds() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(run(&["init", "--out", "dist"], dir.path()).code, 0);
    let skill = dir.path().join("skills/one");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("one.md"),
        "---\nname: one\ndescription: The first skill.\n---\nBody.\n",
    )
    .unwrap();
    let ran = run(&["build", "--yes"], dir.path());
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert!(dir.path().join("dist").is_dir());
}

#[test]
fn a_destination_scope_is_read_and_an_unknown_nested_key_only_warns() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("skillwright.md"),
        "---\nname: v\nbuild:\n  out: dist\n  destinations:\n    - root: dest\n      target: claude-code\n      scope: project\n      flavour: mild\n---\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("skills")).unwrap();
    let ran = run(&["list"], dir.path());
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert!(
        ran.stderr.contains("destinations[].flavour"),
        "{}",
        ran.stderr
    );
    assert!(!ran.stderr.contains("scope"), "{}", ran.stderr);
}

// --- new, in the folder shape ------------------------------------------------

#[test]
fn new_follows_the_folders_of_a_vault_init_wrote() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(run(&["init", "--out", "dist"], dir.path()).code, 0);
    let skill = run(
        &[
            "new",
            "skill",
            "triage",
            "--description",
            "Triage.",
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(skill.code, 0, "{}", skill.stderr);
    assert!(dir.path().join("skills/triage/triage.md").is_file());

    let plugin = run(
        &[
            "new",
            "plugin",
            "review",
            "--description",
            "Review.",
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(plugin.code, 0, "{}", plugin.stderr);
    assert!(dir.path().join("plugins/review/review.md").is_file());

    let built = run(&["build", "--yes"], dir.path());
    assert_eq!(built.code, 0, "{}", built.stderr);
}

#[test]
fn new_refuses_a_marketplace_the_vault_does_not_hold() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(run(&["init", "--out", "dist"], dir.path()).code, 0);
    let ran = run(
        &[
            "new",
            "plugin",
            "review",
            "--marketplace",
            "elsewhere",
            "--description",
            "Review.",
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(ran.code, 3, "{}", ran.stderr);
    assert!(!dir.path().join("marketplaces").exists());
}

#[test]
fn new_marketplace_writes_a_root_file_a_workspace_builds() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nworkspace: true\n---\n",
    )
    .unwrap();
    let ran = run(
        &[
            "new",
            "marketplace",
            "pub",
            "--description",
            "Public.",
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    let file = dir.path().join("marketplaces/pub/skillwright.md");
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("name: pub"), "{text}");
    assert!(!dir.path().join("marketplaces/pub/pub.md").exists());

    fs::write(
        &file,
        text.replacen("---\n", "---\nbuild:\n  out: dist\n", 1),
    )
    .unwrap();
    let skills = dir.path().join("marketplaces/pub/skills");
    fs::create_dir_all(&skills).unwrap();
    fs::write(skills.join("one.md"), "---\ndescription: One.\n---\nB\n").unwrap();
    let built = run(&["build", "--yes"], dir.path());
    assert_eq!(built.code, 0, "{}", built.stderr);

    // A marketplace with a description is no false skip from the workspace.
    let listed = run(&["list"], dir.path());
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    assert!(!listed.stderr.contains("skipped"), "{}", listed.stderr);
    assert!(!listed.stderr.contains("unknown-key"), "{}", listed.stderr);
}

#[test]
fn a_described_note_beside_a_marketplace_folder_is_skipped_as_a_stray() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("marketplaces/house");
    fs::create_dir_all(market.join("plugins/p/skills")).unwrap();
    fs::write(dir.path().join("skillwright.md"), "---\nname: v\n---\n").unwrap();
    fs::write(
        market.join("house.md"),
        "---\nname: house\ndescription: H.\n---\n",
    )
    .unwrap();
    let listed = run(&["list"], dir.path());
    assert!(
        listed
            .stderr
            .contains("marketplaces/house/house.md  skipped"),
        "{}",
        listed.stderr
    );
    assert!(
        listed
            .stderr
            .contains("sits where no component can be read"),
        "{}",
        listed.stderr
    );
}

#[test]
fn the_marketplaces_own_plugin_is_bumped_in_the_root_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nname: home\nversion: 1.0.0\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("skills")).unwrap();
    fs::write(
        dir.path().join("skills/one.md"),
        "---\ndescription: One.\n---\nB\n",
    )
    .unwrap();
    let built = run(&["build", "--yes"], dir.path());
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(
        !built.stderr.contains("missing-version"),
        "{}",
        built.stderr
    );
    let bumped = run(&["bump", "home", "--yes"], dir.path());
    assert_eq!(bumped.code, 0, "{}", bumped.stderr);
    let root = fs::read_to_string(dir.path().join("skillwright.md")).unwrap();
    assert!(root.contains("version: 1.0.1"), "{root}");
}

#[test]
fn list_shows_the_roster_around_a_path_collision() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nname: home\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    for (rel, text) in [
        ("skills/a.md", "---\nname: dup\ndescription: A.\n---\n"),
        ("skills/b.md", "---\nname: dup\ndescription: B.\n---\n"),
        ("skills/c.md", "---\nname: other\ndescription: C.\n---\n"),
    ] {
        fs::write(dir.path().join(rel), text).unwrap_or_else(|_| {
            fs::create_dir_all(dir.path().join("skills")).unwrap();
            fs::write(dir.path().join(rel), text).unwrap();
        });
    }
    let listed = run(&["list"], dir.path());
    assert_eq!(listed.code, 2, "{}", listed.stderr);
    let lines: Vec<&str> = listed.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{}", listed.stdout);
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.ends_with("[path-collision]"))
            .count(),
        2
    );
    assert!(listed.stdout.contains("skills/c.md"), "{}", listed.stdout);
    assert!(
        listed.stderr.contains("duplicate skill name `dup`"),
        "{}",
        listed.stderr
    );
    let built = run(&["build", "--yes"], dir.path());
    assert_ne!(built.code, 0);
}

#[test]
fn a_plugin_with_no_note_warns_about_its_version() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(run(&["init", "--out", "dist"], dir.path()).code, 0);
    let skills = dir.path().join("plugins/review/skills");
    fs::create_dir_all(&skills).unwrap();
    fs::write(skills.join("one.md"), "---\ndescription: One.\n---\nB\n").unwrap();
    let ran = run(&["build", "--yes"], dir.path());
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert!(ran.stderr.contains("missing-version"), "{}", ran.stderr);

    let bumped = run(&["bump", "review", "--yes"], dir.path());
    assert_eq!(bumped.code, 3, "{}", bumped.stderr);
    assert!(
        bumped.stderr.contains("plugins/review/review.md"),
        "{}",
        bumped.stderr
    );
}

#[test]
fn an_unknown_option_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let run = run(&["publish", "--nope"], dir.path());
    assert_eq!(run.code, 3);
    assert!(run.stderr.contains("--nope"), "{}", run.stderr);
}

#[test]
fn an_unknown_verb_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(run(&["frobnicate"], dir.path()).code, 3);
}

// --- harness -------------------------------------------------------------

#[test]
fn harness_install_claude_help_exits_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let run = run(&["harness", "install", "claude", "--help"], dir.path());
    assert_eq!(run.code, 0, "{}", run.stderr);
    for flag in [
        "--plugin",
        "--scope",
        "--path",
        "--git-url",
        "--git-ref",
        "--vault",
        "--dry-run",
        "--yes",
    ] {
        assert!(
            run.stdout.contains(flag),
            "{flag} missing from {}",
            run.stdout
        );
    }
}

#[test]
fn an_unknown_flag_under_harness_install_claude_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let run = run(&["harness", "install", "claude", "--nope"], dir.path());
    assert_eq!(run.code, 3);
    assert!(run.stderr.contains("--nope"), "{}", run.stderr);
}

// --- list --------------------------------------------------------------------

#[test]
fn list_over_a_vault_prints_the_roster_and_the_tally() {
    let (dir, vault) = fixture();
    let run = run(&["list", "--vault", &vault_str(&vault)], dir.path());
    assert_eq!(run.code, 0);
    assert!(run.stdout.contains("skills/code-review/code-review.md"));
    assert!(run.stdout.contains("reviews code"));
    assert_eq!(run.stderr, "1 skill\n");
}

#[test]
fn list_json_carries_the_same_rows_the_table_does() {
    let (dir, vault) = fixture();
    let run = run(
        &["list", "--vault", &vault_str(&vault), "--json", "--quiet"],
        dir.path(),
    );
    assert_eq!(run.code, 0);
    assert_eq!(run.stderr, "");
    let rows: Vec<serde_json::Value> = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["path"], "skills/code-review/code-review.md");
    assert_eq!(rows[0]["name"], "code-review");
    assert_eq!(rows[0]["kind"], "skill");
    // Absent is `null`, which is not the same instruction as `[]`.
    assert!(rows[0]["targets"].is_null());
    assert!(rows[0].get("folder_kind").is_some());
}

// `--model` prints the marketplace model `build` translates, from the same
// root file, and leaves `--json`'s rows as they were.
#[test]
fn list_model_prints_the_loaded_marketplace() {
    let run = run(
        &["list", "--model", "--quiet", "--vault", &donor_vault()],
        Path::new(&donor_vault()),
    );
    assert_eq!(run.code, 0, "{}", run.stderr);

    let model: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    let tools = &model["plugins"]["tools"];
    assert_eq!(tools["name"], "tools");
    let skill = &tools["skills"][0];
    assert_eq!(skill["kind"], "skill");
    assert!(
        skill["resolved"]["body"]["segments"].is_array(),
        "{}",
        run.stdout
    );
}

// Members outside `plugins/` load into the plugin named after the
// marketplace, which is the root folder's name when nothing names it.
#[test]
fn list_model_puts_loose_skills_in_a_plugin_named_after_the_marketplace() {
    let (dir, vault) = fixture();
    let run = run(&["list", "--model", "--quiet"], &vault);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let model: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(model["plugins"]["vault"]["name"], "vault", "{}", run.stdout);
    drop(dir);
}

#[test]
fn list_with_no_root_file_and_no_convention_folder_is_exit_4() {
    let dir = tempfile::tempdir().unwrap();
    let run = run(&["list"], dir.path());
    assert_eq!(run.code, 4);
    assert!(run.stderr.contains("skillwright.md"), "{}", run.stderr);
}

#[test]
fn list_with_a_config_that_names_no_file_is_exit_4() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nothing-here.md");
    let run = run(
        &["list", "--config", &missing.display().to_string()],
        dir.path(),
    );
    assert_eq!(run.code, 4);
    assert!(run.stderr.contains("not there"), "{}", run.stderr);
}

#[test]
fn list_over_a_vault_that_is_not_there_is_exit_2() {
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("no-vault");
    let run = run(
        &["list", "--vault", &absent.display().to_string()],
        dir.path(),
    );
    assert_eq!(run.code, 2);
    assert_eq!(run.stdout, "");
    assert!(run.stderr.contains("vault not found"), "{}", run.stderr);
}

#[test]
fn a_root_file_above_the_working_directory_marks_the_vault() {
    let (_dir, vault) = fixture();
    fs::write(vault.join("skillwright.md"), "---\nname: house\n---\n").unwrap();
    let run = run(&["list"], &vault.join("skills"));
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(run.stdout.contains("code-review"));
    assert_eq!(run.stderr.matches("note:").count(), 0, "{}", run.stderr);
}

#[test]
fn an_unknown_root_file_key_is_a_warning_and_the_run_goes_on() {
    let (_dir, vault) = fixture();
    fs::write(
        vault.join("skillwright.md"),
        "---\nname: house\nflavour: mint\n---\n",
    )
    .unwrap();
    let run = run(&["list"], &vault);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(run.stderr.contains("warning:"), "{}", run.stderr);
    assert!(run.stderr.contains("`flavour`"), "{}", run.stderr);
}

#[test]
fn a_malformed_root_file_is_a_run_level_blocker() {
    let (_dir, vault) = fixture();
    fs::write(
        vault.join("skillwright.md"),
        "---\nbuild:\n  harnesses: claude\n---\n",
    )
    .unwrap();
    let run = run(&["list"], &vault);
    assert_eq!(run.code, 2, "{}", run.stderr);
    assert!(
        run.stderr.contains("configuration malformed"),
        "{}",
        run.stderr
    );
}

// There is no default output: nothing builds into a vault unless asked.
#[test]
fn build_and_check_with_no_output_folder_refuse_and_say_to_set_one() {
    let (_dir, vault) = fixture();
    for argv in [&["build", "--yes"][..], &["check"]] {
        let refused = run(argv, &vault);
        assert_eq!(refused.code, 4, "{}", refused.stderr);
        assert!(refused.stderr.contains("--out"), "{}", refused.stderr);
        assert!(refused.stderr.contains("build.out"), "{}", refused.stderr);
    }
    assert!(!vault.join(".claude-plugin").exists());
}

// `out: .` builds beside the root file.
#[test]
fn out_dot_writes_beside_the_vault() {
    let (_dir, vault) = fixture();
    fs::write(vault.join("skillwright.md"), "---\nbuild:\n  out: .\n---\n").unwrap();
    let built = run(&["build", "--yes", "--quiet"], &vault);
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(
        vault
            .join(".claude-plugin/packs/vault/skills/code-review/SKILL.md")
            .is_file()
    );
    // The output is not read back as notes.
    let again = run(&["check", "--quiet"], &vault);
    assert_eq!(again.code, 0, "{}{}", again.stdout, again.stderr);
}

// A workspace roster shows what each marketplace's own build uses.
#[test]
fn a_workspace_roster_shows_each_marketplaces_targets() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nworkspace: true\ntargets: [claude, codex]\n---\n",
    )
    .unwrap();
    let member = dir.path().join("marketplaces/a");
    fs::create_dir_all(member.join("skills")).unwrap();
    fs::write(
        member.join("skillwright.md"),
        "---\nname: a\ntargets: [claude]\nbuild:\n  out: .\n---\n",
    )
    .unwrap();
    fs::write(
        member.join("skills/s.md"),
        "---\ndescription: A skill.\n---\nBody.\n",
    )
    .unwrap();
    let listed = run(&["list"], dir.path());
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    let row = listed
        .stdout
        .lines()
        .find(|line| line.contains("skills/s.md"))
        .unwrap();
    assert!(row.contains("claude"), "{row}");
    assert!(!row.contains("codex"), "{row}");
}

// A workspace builds each marketplace under `marketplaces/`, each to its own
// `build.out`, with the workspace's shared fields.
#[test]
fn a_workspace_builds_each_marketplace_beside_its_root_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nworkspace: true\nlicense: MIT\n---\n",
    )
    .unwrap();
    for name in ["one", "two"] {
        let member = dir.path().join("marketplaces").join(name);
        fs::create_dir_all(member.join("skills")).unwrap();
        fs::write(
            member.join("skillwright.md"),
            format!("---\nname: {name}\nbuild:\n  out: .\n---\n"),
        )
        .unwrap();
        fs::write(
            member.join("skills").join(format!("{name}-skill.md")),
            format!("---\ndescription: The {name} skill.\n---\nBody.\n"),
        )
        .unwrap();
    }
    let built = run(&["build", "--yes", "--quiet"], dir.path());
    assert_eq!(built.code, 0, "{}", built.stderr);
    for name in ["one", "two"] {
        let manifest = dir
            .path()
            .join("marketplaces")
            .join(name)
            .join(".claude-plugin/packs")
            .join(name)
            .join(".claude-plugin/plugin.json");
        let text = fs::read_to_string(&manifest).unwrap();
        assert!(text.contains("\"license\": \"MIT\""), "{text}");
    }
    // A verb that acts on a single marketplace refuses at the workspace.
    for verb in [
        &["publish", "--dry-run"][..],
        &["adopt", "src", "--yes"],
        &["bump", "tools", "--patch"],
        &["new", "skill", "zed", "--description", "Zed."],
        &[
            "new",
            "plugin",
            "zed",
            "--marketplace",
            "alpha",
            "--description",
            "Zed.",
        ],
    ] {
        let refused = run(verb, dir.path());
        assert_eq!(refused.code, 2, "{verb:?}: {}", refused.stderr);
        assert!(refused.stderr.contains("workspace"), "{}", refused.stderr);
    }
}

#[test]
fn a_build_off_a_tty_without_yes_is_a_usage_error_not_a_silent_write() {
    let (dir, vault) = fixture();
    let market = dir.path().join("market");
    let run = run(
        &[
            "build",
            "--vault",
            &vault_str(&vault),
            "--out",
            &market.display().to_string(),
        ],
        dir.path(),
    );
    assert_eq!(run.code, 3);
    assert!(run.stderr.contains("pass --yes"), "{}", run.stderr);
    assert!(!market.exists());
}

#[test]
fn dry_run_on_check_says_it_is_redundant_rather_than_silently_doing_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("market");
    let run = run(
        &[
            "check",
            "--vault",
            &donor_vault(),
            "--out",
            &market.display().to_string(),
            "--dry-run",
        ],
        dir.path(),
    );
    assert!(
        run.stderr
            .contains("check writes nothing under any outcome"),
        "{}",
        run.stderr
    );
    // Nothing built yet, so: drift. The flag does not change the verdict.
    assert_eq!(run.code, 1);
    assert!(!market.exists());
}

#[test]
fn a_build_renders_the_roots_and_a_check_then_agrees_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("market");
    let args = |verb: &'static str| -> Vec<String> {
        vec![
            verb.to_string(),
            "--vault".to_string(),
            donor_vault(),
            "--out".to_string(),
            market.display().to_string(),
        ]
    };
    let mut build: Vec<String> = args("build");
    build.push("--yes".to_string());
    let build_argv: Vec<&str> = build.iter().map(String::as_str).collect();
    let built = run(&build_argv, dir.path());
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(built.stdout.contains("claude/tools  written · 1 skill"));
    assert!(
        built
            .stdout
            .contains("claude  written · marketplace manifest")
    );
    assert!(
        built.stderr.contains("7 checked — 7 written"),
        "{}",
        built.stderr
    );

    let check = args("check");
    let check_argv: Vec<&str> = check.iter().map(String::as_str).collect();
    let checked = run(&check_argv, dir.path());
    assert_eq!(checked.code, 0, "{}", checked.stderr);
    assert!(checked.stdout.contains("claude/tools  ok · 1 skill"));
    assert!(checked.stderr.contains("7 ok"), "{}", checked.stderr);
}

/// A vault whose `.obsidian/app.json` excludes one skill's folder
/// (`userIgnoreFilters`), read read-only.
fn excluded_vault() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/excluded-vault")
        .canonicalize()
        .unwrap()
        .display()
        .to_string()
}

#[test]
fn a_note_the_vault_excludes_is_not_built() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("market");
    let market_arg = market.display().to_string();
    let vault = excluded_vault();
    let argv = ["build", "--vault", &vault, "--out", &market_arg, "--yes"];
    let built = run(&argv, dir.path());
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(
        built.stdout.contains("claude/tools  written · 1 skill"),
        "{}",
        built.stdout
    );
    assert!(find_file(&market, "kept").is_some(), "kept is built");
    assert!(
        find_file(&market, "hidden").is_none(),
        "hidden is not built"
    );
}

#[test]
fn a_note_the_vault_excludes_is_not_published() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("dest");
    let dest_arg = dest.display().to_string();
    let vault = excluded_vault();
    let argv = [
        "publish", "--all", "--vault", &vault, "--to", &dest_arg, "--target", "claude", "--yes",
    ];
    let published = run(&argv, dir.path());
    assert_eq!(published.code, 0, "{}", published.stderr);
    assert!(dest.join(".claude/skills/kept/SKILL.md").is_file());
    assert!(!dest.join(".claude/skills/hidden").exists());
    assert!(!published.stderr.contains("hidden"), "{}", published.stderr);
}

/// `publish <path>` publishes a note that no kind folder places, as a skill.
/// It is the one exception to "the path decides the kind", and the other
/// verbs keep leaving such a note out of the vault.
#[test]
fn publish_by_path_publishes_a_note_no_kind_folder_places() {
    let (dir, vault) = fixture();
    let notes = vault.join("notes");
    fs::create_dir_all(&notes).unwrap();
    fs::write(
        notes.join("stray.md"),
        "---\nname: stray\ndescription: A stray note outside every kind folder\n---\nBody.\n",
    )
    .unwrap();
    let vault_arg = vault.display().to_string();
    let dest = dir.path().join("dest");
    let dest_arg = dest.display().to_string();
    let to = [
        "--vault", &vault_arg, "--to", &dest_arg, "--target", "claude",
    ];

    let listed = run(&["list", "--vault", &vault_arg], dir.path());
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    assert!(!listed.stdout.contains("stray"), "{}", listed.stdout);

    let mut all = vec!["publish", "--all", "--yes"];
    all.extend(to);
    let published = run(&all, dir.path());
    assert_eq!(published.code, 0, "{}", published.stderr);
    assert!(dest.join(".claude/skills/code-review/SKILL.md").is_file());
    assert!(!dest.join(".claude/skills/stray").exists());

    let mut named = vec!["publish", "notes/stray.md", "--yes"];
    named.extend(to);
    let published = run(&named, dir.path());
    assert_eq!(published.code, 0, "{}", published.stderr);
    assert!(dest.join(".claude/skills/stray/SKILL.md").is_file());
}

/// The first path under `root` with a component named `name`, depth first.
fn find_file(root: &Path, name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
        if path.is_dir()
            && let Some(hit) = find_file(&path, name)
        {
            return Some(hit);
        }
    }
    None
}

/// The root file's `license` reaches every pack manifest: no note declares
/// one, so the root file is the only thing that can put it there.
#[test]
fn the_root_files_license_reaches_the_pack_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let skills = dir.path().join("plugins/tools/skills");
    fs::create_dir_all(&skills).unwrap();
    fs::write(
        skills.join("review.md"),
        "---\ndescription: Reviews code.\n---\nReview.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nname: house\nlicense: MIT\nbuild:\n  out: market\n---\n",
    )
    .unwrap();
    let market = dir.path().join("market");

    let run = run(&["build", "--yes"], &skills);
    assert_eq!(run.code, 0, "{}", run.stderr);

    let manifest =
        fs::read_to_string(market.join(".claude-plugin/packs/tools/.claude-plugin/plugin.json"))
            .unwrap();
    assert!(manifest.contains(r#""license": "MIT""#), "{manifest}");
    let package = fs::read_to_string(market.join(".pi/packs/tools/package.json")).unwrap();
    assert!(package.contains(r#""license": "MIT""#), "{package}");
}

// --- status ------------------------------------------------------------------

/// With no `build.destinations`, status reads each harness's user skill
/// folder under `$HOME`. Run as a child process so `$HOME` is the test's.
#[test]
fn status_with_no_destinations_reads_each_harness_s_user_skill_folder() {
    let (dir, vault) = fixture();
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let out = run_binary(
        &["status", "--vault", &vault_str(&vault), "--json", "--quiet"],
        dir.path(),
        &[("HOME", home.to_str().unwrap())],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    let document: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let targets: Vec<&str> = document["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["target"].as_str().unwrap())
        .collect();
    assert_eq!(targets, ["claude", "codex", "cursor", "pi"], "{document}");
}

/// A root file in the vault naming one destination. Returns its path.
fn status_config(dir: &Path, vault: &Path) -> String {
    let dest = dir.join("dest");
    fs::create_dir_all(&dest).unwrap();
    let config = vault.join("skillwright.md");
    fs::write(
        &config,
        format!(
            "---\nbuild:\n  destinations:\n    - root: {}\n      target: claude\n---\n",
            dest.display()
        ),
    )
    .unwrap();
    config.display().to_string()
}

#[test]
fn a_destination_never_written_to_is_never_published_and_passes_check() {
    let (dir, vault) = fixture();
    let config = status_config(dir.path(), &vault);
    let run = run(&["status", "--config", &config, "--check"], dir.path());
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(run.stdout.contains("never-published"), "{}", run.stdout);
    assert!(run.stderr.contains("1 never-published"), "{}", run.stderr);
}

#[test]
fn status_json_carries_the_same_tokens_the_table_prints() {
    let (dir, vault) = fixture();
    let config = status_config(dir.path(), &vault);
    let run = run(
        &["status", "--config", &config, "--json", "--quiet"],
        dir.path(),
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    let document: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    let rows = document["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["state"], "never-published");
    assert_eq!(rows[0]["skill"], "skills/code-review/code-review.md");
    assert_eq!(rows[0]["target"], "claude");
    // No unreadable note, so the key is absent rather than empty.
    assert!(document.get("skipped").is_none());
}

#[test]
fn a_note_that_is_not_there_is_a_blocker_naming_it() {
    let (dir, vault) = fixture();
    let config = status_config(dir.path(), &vault);
    let run = run(
        &["status", "no/such/note.md", "--config", &config],
        dir.path(),
    );
    assert_eq!(run.code, 2);
    assert!(run.stderr.starts_with("note not found: "), "{}", run.stderr);
}

// --- helpers for the build/check, list and status cases ---------------

/// A recursive copy, for the cases that must not write into the committed
/// donor vault.
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

/// Every file under `root`, relative to it, sorted.
fn tree(root: &Path) -> Vec<PathBuf> {
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

/// The donor vault, one output root, and
/// the default plugin both verbs need. No `--marketplace` — that vault names
/// its own.
fn market_run(verb: &str, market: &Path, extra: &[&str], cwd: &Path) -> Run {
    run_argv(
        vec![
            verb.to_string(),
            "--vault".to_string(),
            donor_vault(),
            "--out".to_string(),
            market.display().to_string(),
        ],
        extra,
        cwd,
    )
}

fn build_market(market: &Path, extra: &[&str], cwd: &Path) -> Run {
    let mut with_yes = vec!["--yes"];
    with_yes.extend_from_slice(extra);
    market_run("build", market, &with_yes, cwd)
}

fn check_market(market: &Path, extra: &[&str], cwd: &Path) -> Run {
    market_run("check", market, extra, cwd)
}

/// The top-level folder one harness writes under in the marketplace directory.
fn tree_of(harness: &str) -> &'static str {
    match harness {
        "claude" => ".claude-plugin",
        "codex" => ".agents",
        "cursor" => ".cursor-plugin",
        other => panic!("no such harness: {other}"),
    }
}

/// Where one harness's packs land, relative to the marketplace directory.
fn packs_of(harness: &str) -> PathBuf {
    PathBuf::from(tree_of(harness)).join("packs")
}

/// The one skill the donor vault publishes, in one harness's tree.
fn skill_md(market: &Path, harness: &str) -> PathBuf {
    market
        .join(packs_of(harness))
        .join("tools")
        .join("skills")
        .join("workflow-checklist")
        .join("SKILL.md")
}

// --- build and check ---------------------------------------

#[test]
fn a_drifted_root_is_check_exit_1_and_the_next_build_heals_it() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("market");
    assert_eq!(build_market(&market, &[], dir.path()).code, 0);

    let edited = format!(
        "{}\nhand-edited\n",
        fs::read_to_string(skill_md(&market, "claude")).unwrap()
    );
    fs::write(skill_md(&market, "claude"), &edited).unwrap();

    let drifted = check_market(&market, &[], dir.path());
    assert_eq!(drifted.code, 1, "{}", drifted.stderr);
    assert!(
        drifted.stdout.contains("claude/tools  drift · 1 skill"),
        "{}",
        drifted.stdout
    );
    // Only the root that moved.
    assert!(
        drifted.stdout.contains("codex/tools  ok · 1 skill"),
        "{}",
        drifted.stdout
    );
    // check writes nothing under any outcome — the edit is still there.
    assert!(
        fs::read_to_string(skill_md(&market, "claude"))
            .unwrap()
            .contains("hand-edited")
    );

    let healed = build_market(&market, &[], dir.path());
    assert_eq!(healed.code, 0, "{}", healed.stderr);
    assert!(
        healed.stdout.contains("claude/tools  written · 1 skill"),
        "{}",
        healed.stdout
    );
    assert!(
        !fs::read_to_string(skill_md(&market, "claude"))
            .unwrap()
            .contains("hand-edited")
    );
    assert_eq!(check_market(&market, &[], dir.path()).code, 0);
}

#[test]
fn a_plugin_the_vault_no_longer_produces_is_drift_for_check_and_gone_after_build() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("market");
    assert_eq!(build_market(&market, &[], dir.path()).code, 0);

    let ghost = market.join(packs_of("claude")).join("ghost");
    fs::create_dir_all(&ghost).unwrap();
    fs::write(ghost.join("leftover.md"), "from a previous vault\n").unwrap();

    let drifted = check_market(&market, &[], dir.path());
    assert_eq!(drifted.code, 1, "{}", drifted.stderr);
    assert!(drifted.stdout.contains("stale"), "{}", drifted.stdout);
    assert!(
        drifted.stdout.contains(".claude-plugin/packs/ghost"),
        "{}",
        drifted.stdout
    );
    assert!(ghost.exists(), "check wrote something");

    assert_eq!(build_market(&market, &[], dir.path()).code, 0);
    // The packs folder is generated output: what the render did not produce
    // goes with the swap.
    assert!(!ghost.exists());
}

#[test]
fn a_rebuild_keeps_what_was_authored_beside_the_packs() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("market");
    assert_eq!(build_market(&market, &[], dir.path()).code, 0);

    let mine = market.join(".agents/skills/mine/SKILL.md");
    fs::create_dir_all(mine.parent().unwrap()).unwrap();
    fs::write(&mine, "by hand\n").unwrap();
    fs::write(skill_md(&market, "codex"), "hand-edited\n").unwrap();

    assert_eq!(build_market(&market, &[], dir.path()).code, 0);
    assert_eq!(fs::read_to_string(&mine).unwrap(), "by hand\n");
    assert!(
        !fs::read_to_string(skill_md(&market, "codex"))
            .unwrap()
            .contains("hand-edited")
    );
}

#[test]
fn build_dry_run_reports_what_a_build_would_do_writes_nothing_and_exits_0() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("market");
    let previewed = build_market(&market, &["--dry-run"], dir.path());

    // The CLI spec: a dry run exits what the real run would have. `check` over
    // the same drift is the one that exits 1.
    assert_eq!(previewed.code, 0, "{}", previewed.stderr);
    assert!(
        previewed.stdout.contains("claude/tools  drift · 1 skill"),
        "{}",
        previewed.stdout
    );
    assert!(previewed.stderr.contains("dry run"), "{}", previewed.stderr);
    assert!(
        previewed.stderr.contains("(dry run)"),
        "{}",
        previewed.stderr
    );
    assert!(!market.exists());

    assert_eq!(check_market(&market, &[], dir.path()).code, 1);
}

#[test]
fn json_emits_the_cores_own_report_mode_and_all() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("market");
    let built = build_market(&market, &["--json"], dir.path());
    assert_eq!(built.code, 0, "{}", built.stderr);

    let report: serde_json::Value = serde_json::from_str(&built.stdout).unwrap();
    assert_eq!(report["mode"], "build");
    assert_eq!(report["checked"], 1);
    assert_eq!(report["changed"], true);
    let units = report["units"].as_array().unwrap();
    // One plugin unit per harness tree, and one manifest unit per tree with
    // a marketplace (every one but Pi's).
    assert_eq!(units.iter().filter(|u| u["unit"] == "plugin").count(), 4);
    assert_eq!(units.iter().filter(|u| u["unit"] == "manifest").count(), 3);

    let checked = check_market(&market, &["--json"], dir.path());
    assert_eq!(checked.code, 0, "{}", checked.stderr);
    let report: serde_json::Value = serde_json::from_str(&checked.stdout).unwrap();
    assert_eq!(report["mode"], "check");
}

#[test]
fn an_output_root_inside_the_vault_s_structure_is_refused_before_anything_is_staged() {
    let dir = tempfile::tempdir().unwrap();
    // Against a COPY of the fixture, so a regression in the rail cannot write
    // into the committed vault this suite reads.
    let copied = dir.path().join("vault-copy");
    copy_tree(Path::new(&donor_vault()), &copied);
    let before = tree(&copied);

    // Inside a marketplace folder, where a `.claude-plugin/` can be authored.
    let inside = copied.join("marketplaces/house").display().to_string();
    let run = run(
        &[
            "build",
            "--vault",
            &copied.display().to_string(),
            "--out",
            &inside,
            "--yes",
        ],
        dir.path(),
    );

    // A run-level blocker, not a report of refusals: an export's rails fire
    // before anything is staged.
    assert_eq!(run.code, 2, "{}", run.stderr);
    assert_eq!(run.stdout, "");
    assert!(run.stderr.contains("build failed"), "{}", run.stderr);
    assert_eq!(tree(&copied), before);
}

#[test]
fn a_vault_with_nothing_publishable_is_a_blocker_not_an_empty_success() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("market");
    // Its own vault: a test about emptiness run against a vault with something
    // in it passes for the wrong reason.
    let empty = dir.path().join("empty-vault");
    fs::create_dir_all(empty.join("plugins")).unwrap();

    let run = run(
        &[
            "build",
            "--vault",
            &empty.display().to_string(),
            "--out",
            &market.display().to_string(),
            "--marketplace",
            "demo",
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(run.code, 2, "{}", run.stderr);
    assert!(run.stderr.contains("empty-marketplace"), "{}", run.stderr);
    assert!(!market.exists());
}

#[test]
fn a_marketplace_this_vault_does_not_hold_is_refused_naming_the_ones_it_does() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("market");
    // `--marketplace` selects. The fixture holds `house`, so `demo` is a typo
    // the build must not quietly honour by renaming its output.
    let run = market_run(
        "build",
        &market,
        &["--marketplace", "demo", "--yes"],
        dir.path(),
    );
    assert_eq!(run.code, 2, "{}", run.stderr);
    assert!(run.stderr.contains("unknown-marketplace"), "{}", run.stderr);
    assert!(run.stderr.contains("`house`"), "{}", run.stderr);
    assert!(!market.exists());
}

#[test]
fn check_never_asks_it_writes_nothing_so_there_is_nothing_to_confirm() {
    let dir = tempfile::tempdir().unwrap();
    let market = dir.path().join("market");
    // No --yes, off a TTY — the position that makes `build` a usage error.
    let run = check_market(&market, &[], dir.path());
    assert_eq!(run.code, 1, "{}", run.stderr); // nothing built yet, so: drift
}

#[test]
fn the_root_file_supplies_the_name_and_the_output_with_no_flags() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let skills = vault.join("plugins/tools/skills");
    fs::create_dir_all(&skills).unwrap();
    fs::write(
        skills.join("review.md"),
        "---\ndescription: Reviews code.\n---\nReview.\n",
    )
    .unwrap();
    let config = vault.join("skillwright.md");
    fs::write(&config, "---\nname: demo\nbuild:\n  out: market\n---\n").unwrap();

    let run = run(
        &["build", "--config", &config.display().to_string(), "--yes"],
        dir.path(),
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout.contains("claude/tools  written · 1 skill"),
        "{}",
        run.stdout
    );
    // `out` is relative, so it resolved against the root file's own folder
    // rather than the process's working directory.
    let manifest = vault.join("market/.claude-plugin/marketplace.json");
    let text = fs::read_to_string(manifest).unwrap();
    assert!(text.contains(r#""name": "demo""#), "{text}");
}

// --- adopt --------------------------------------------------

/// The bundle the adopt cases read: a published
/// SKILL.md, exactly as a harness directory holds one.
fn adopt_source(dir: &Path) -> PathBuf {
    let source = dir.join("source");
    fs::create_dir_all(source.join("code-review").join("references")).unwrap();
    fs::write(
        source.join("code-review").join("SKILL.md"),
        "---\nname: code-review\ndescription: reviews code\n---\nSee [the review checklist](references/checklist.md).\n",
    )
    .unwrap();
    fs::write(
        source
            .join("code-review")
            .join("references")
            .join("checklist.md"),
        "# Checklist\n",
    )
    .unwrap();
    source
}

/// `adopt <source> --vault <vault> --yes`, plus whatever `extra` adds.
fn adopt(source: &Path, vault: &Path, extra: &[&str], cwd: &Path) -> Run {
    run_argv(
        vec![
            "adopt".to_string(),
            source.display().to_string(),
            "--vault".to_string(),
            vault_str(vault),
            "--yes".to_string(),
        ],
        extra,
        cwd,
    )
}

fn adopted_note(vault: &Path, plugin: &str) -> PathBuf {
    vault
        .join("plugins")
        .join(plugin)
        .join("skills")
        .join("code-review")
        .join("code-review.md")
}

/// A repository with one commit on `main`, holding a `skills/code-review/`
/// bundle, used by the `--from-git` cases. Returns
/// the commit sha.
fn commit_git_fixture(repo: &Path) -> String {
    fs::create_dir_all(repo.join("skills").join("code-review")).unwrap();
    fs::write(
        repo.join("skills").join("code-review").join("SKILL.md"),
        "---\nname: code-review\ndescription: reviews code\n---\nReview the diff.\n",
    )
    .unwrap();
    git(repo, &["init", "--initial-branch", "main"]);
    git(repo, &["add", "."]);
    git(repo, &["commit", "-m", "one"]);
    typed_git::query::head_sha(repo).unwrap()
}

#[test]
fn adopt_imports_the_bundle_rewrites_links_and_stamps_provenance_exit_0() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    let result = adopt(&source, &vault, &[], dir.path());

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains("code-review.md  adopted"),
        "{}",
        result.stdout
    );
    assert!(result.stderr.contains("1 adopted"), "{}", result.stderr);

    let note = fs::read_to_string(adopted_note(&vault, "adopted")).unwrap();
    assert!(note.contains("name: code-review"));
    // Five of the provenance keys. `adopted.also_at` is the sixth and is absent
    // here on purpose: nothing was cross-referenced.
    assert!(note.contains("adopted:\n"));
    assert!(note.contains("  source:"));
    assert!(note.contains("  format:"));
    assert!(note.contains("  source_hash:"));
    assert!(note.contains("  hash:"));
    assert!(note.contains("  at:"));
    // A relative markdown link became a wikilink.
    assert!(note.contains("[[references/checklist|the review checklist]]"));
    assert!(!note.contains("](references/checklist.md)"));
    assert!(
        vault
            .join("plugins")
            .join("adopted")
            .join("skills")
            .join("code-review")
            .join("references")
            .join("checklist.md")
            .exists()
    );
    // The source was never written to.
    let mut names: Vec<String> = fs::read_dir(source.join("code-review"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["SKILL.md".to_string(), "references".to_string()]
    );
}

#[test]
fn what_it_adopted_it_can_build_the_round_trip_closes() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    assert_eq!(adopt(&source, &vault, &[], dir.path()).code, 0);

    let market = dir.path().join("market");
    let built = run(
        &[
            "build",
            "--vault",
            &vault_str(&vault),
            "--out",
            &market.display().to_string(),
            "--marketplace",
            "demo",
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(built.code, 0, "{}", built.stderr);
    // Grouped under the plugin adoption stamped, not the configured default.
    assert!(
        built.stdout.contains("claude/adopted  written · 1 skill"),
        "{}",
        built.stdout
    );
    assert!(
        market
            .join(packs_of("claude"))
            .join("adopted")
            .join("skills")
            .join("code-review")
            .join("SKILL.md")
            .exists()
    );
}

#[test]
fn re_adopting_an_untouched_tree_is_unchanged_and_exits_0() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    assert_eq!(adopt(&source, &vault, &[], dir.path()).code, 0);
    let before = fs::read_to_string(adopted_note(&vault, "adopted")).unwrap();

    let again = adopt(&source, &vault, &[], dir.path());
    assert_eq!(again.code, 0, "{}", again.stderr);
    assert!(
        again.stdout.contains("code-review.md  unchanged"),
        "{}",
        again.stdout
    );
    assert!(again.stderr.contains("1 unchanged"), "{}", again.stderr);
    assert_eq!(
        fs::read_to_string(adopted_note(&vault, "adopted")).unwrap(),
        before
    );
}

#[test]
fn a_diverged_re_adopt_is_refused_exits_1_and_overwrite_names_the_note() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    assert_eq!(adopt(&source, &vault, &[], dir.path()).code, 0);

    let note_path = adopted_note(&vault, "adopted");
    let mut note = fs::read_to_string(&note_path).unwrap();
    note.push_str("\nedited in the vault\n");
    fs::write(&note_path, &note).unwrap();
    let skill_path = source.join("code-review").join("SKILL.md");
    let mut skill = fs::read_to_string(&skill_path).unwrap();
    skill.push_str("\nA new paragraph.\n");
    fs::write(&skill_path, &skill).unwrap();

    let again = adopt(&source, &vault, &[], dir.path());
    assert_eq!(again.code, 1, "{}", again.stderr);
    assert!(
        again.stdout.contains("refused · diverged:"),
        "{}",
        again.stdout
    );
    // The refusal is the rail: the vault's copy is untouched.
    assert!(
        fs::read_to_string(&note_path)
            .unwrap()
            .contains("edited in the vault")
    );

    // Naming a different note does not lift this one's refusal.
    let other_note = vault
        .join("plugins")
        .join("adopted")
        .join("skills")
        .join("other")
        .join("other.md");
    let wrong = adopt(
        &source,
        &vault,
        &["--overwrite", &other_note.display().to_string()],
        dir.path(),
    );
    assert_eq!(wrong.code, 1);

    let forced = adopt(
        &source,
        &vault,
        &["--overwrite", &note_path.display().to_string()],
        dir.path(),
    );
    assert_eq!(forced.code, 0, "{}", forced.stderr);
    // It says what it discarded before it says it replaced it.
    assert!(
        forced.stdout.contains("updated · overwritten · diverged:"),
        "{}",
        forced.stdout
    );
    let final_note = fs::read_to_string(&note_path).unwrap();
    assert!(!final_note.contains("edited in the vault"));
    assert!(final_note.contains("A new paragraph."));
}

#[test]
fn dry_run_is_the_cores_mode_same_plan_and_nothing_written() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    let dry = run(
        &[
            "adopt",
            &source.display().to_string(),
            "--vault",
            &vault_str(&vault),
            "--dry-run",
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(dry.code, 0, "{}", dry.stderr);
    assert!(!vault.join("plugins").exists());
    // Never prompts, even without --yes and off a TTY — a plan has nothing to
    // confirm.
    assert!(dry.stderr.contains("dry run"), "{}", dry.stderr);

    let real = run(
        &[
            "adopt",
            &source.display().to_string(),
            "--vault",
            &vault_str(&vault),
            "--yes",
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(real.code, 0, "{}", real.stderr);

    let dry_json: serde_json::Value = serde_json::from_str(&dry.stdout).unwrap();
    let real_json: serde_json::Value = serde_json::from_str(&real.stdout).unwrap();
    assert_eq!(dry_json, real_json);
}

#[test]
fn plugin_groups_the_adoption_and_set_targets_narrows_what_it_will_publish_to() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    let result = adopt(
        &source,
        &vault,
        &["--plugin", "reviewing", "--set-targets", "claude"],
        dir.path(),
    );
    assert_eq!(result.code, 0, "{}", result.stderr);

    let note = fs::read_to_string(adopted_note(&vault, "reviewing")).unwrap();
    assert!(note.contains("targets"));
    assert!(note.contains("claude"));
}

#[test]
fn an_unknown_set_targets_value_stops_that_source_before_anything_is_written_exit_1() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    let result = adopt(&source, &vault, &["--set-targets", "claud"], dir.path());

    // A source-level blocker rides the report and the run completes: exit 1,
    // the same shape a destination-level blocker gives a publish.
    assert_eq!(result.code, 1);
    assert_eq!(result.stdout, "");
    assert!(
        result.stderr.contains("adopt failed for"),
        "{}",
        result.stderr
    );
    assert!(
        result.stderr.contains("unknown-target"),
        "{}",
        result.stderr
    );
    assert!(!vault.join("plugins").exists());
}

#[test]
fn one_bad_source_does_not_abandon_the_good_ones() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    // A source the core refuses whole — it exists, and has no SKILL.md
    // anywhere under it — beside one that adopts cleanly.
    let empty = dir.path().join("not-a-skill-source");
    fs::create_dir_all(&empty).unwrap();
    fs::write(empty.join("notes.txt"), "nothing publishable here\n").unwrap();

    let result = run(
        &[
            "adopt",
            &empty.display().to_string(),
            "--from",
            &source.display().to_string(),
            "--vault",
            &vault_str(&vault),
            "--yes",
            "--json",
        ],
        dir.path(),
    );

    // The run completed: exit 1, not 2, and the good source still landed.
    assert_eq!(result.code, 1, "{}", result.stderr);
    assert!(
        result
            .stderr
            .contains(&format!("adopt failed for {}", empty.display())),
        "{}",
        result.stderr
    );
    let parsed: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    let skills = parsed["skills"].as_array().unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0]["name"], "code-review");
    assert_eq!(skills[0]["status"], "adopted");
    let failed = parsed["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["source"], empty.display().to_string());
    assert!(adopted_note(&vault, "adopted").exists());
}

#[test]
fn an_ordinary_runs_document_has_no_failed_key_at_all() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    let result = adopt(&source, &vault, &["--json"], dir.path());
    assert_eq!(result.code, 0, "{}", result.stderr);
    let parsed: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert!(parsed.as_object().unwrap().get("failed").is_none());
}

#[test]
fn namespace_decides_how_the_provenance_is_spelled() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    let result = adopt(&source, &vault, &["--namespace", "x"], dir.path());
    assert_eq!(result.code, 0, "{}", result.stderr);

    let note = fs::read_to_string(adopted_note(&vault, "adopted")).unwrap();
    assert!(note.contains("x-adopted:\n"));
    assert!(!note.contains("\nadopted:"));
}

#[test]
fn a_bare_key_in_a_namespaced_vault_warns_with_its_spelling() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nname: v\nbuild:\n  namespace: sw\n---\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("skills")).unwrap();
    fs::write(
        dir.path().join("skills/s.md"),
        "---\nname: s\ndescription: D.\nadopted: {from: x}\nclaude: {model: opus}\n---\nBody.\n",
    )
    .unwrap();
    let listed = run(&["list"], dir.path());
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    for spelling in ["`adopted` is `sw-adopted`", "`claude` is `sw-claude`"] {
        assert!(listed.stderr.contains(spelling), "{}", listed.stderr);
    }
}

#[test]
fn a_member_reads_an_inherited_asset_from_its_own_folder() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nworkspace: true\nbuild:\n  assets:\n    - {from: NOTICE, to: NOTICE, plugin: p}\n---\n",
    )
    .unwrap();
    let member = dir.path().join("marketplaces/m");
    fs::create_dir_all(member.join("plugins/p/skills")).unwrap();
    fs::write(
        member.join("skillwright.md"),
        "---\nname: m\nbuild:\n  out: built\n---\n",
    )
    .unwrap();
    fs::write(
        member.join("plugins/p/p.md"),
        "---\nname: p\ndescription: P.\nversion: 1.0.0\n---\n",
    )
    .unwrap();
    fs::write(
        member.join("plugins/p/skills/s.md"),
        "---\nname: s\ndescription: D.\n---\nBody.\n",
    )
    .unwrap();
    fs::write(member.join("NOTICE"), "member\n").unwrap();
    let built = run(&["build", "--yes"], dir.path());
    assert_eq!(built.code, 0, "{}", built.stderr);
    let copied: Vec<_> = walkdir_files(&member.join("built"))
        .into_iter()
        .filter(|path| path.ends_with("NOTICE"))
        .collect();
    assert!(!copied.is_empty());
    for path in copied {
        assert_eq!(fs::read_to_string(path).unwrap(), "member\n");
    }
}

fn walkdir_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walkdir_files(&path));
        } else {
            found.push(path);
        }
    }
    found
}

#[test]
fn as_and_promote_are_honestly_not_implemented_exit_2() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    let renamed = adopt(&source, &vault, &["--as", "reviewer"], dir.path());
    assert_eq!(renamed.code, 2);
    assert!(
        renamed.stderr.contains("not implemented: adopt --as"),
        "{}",
        renamed.stderr
    );

    let promoted = adopt(&source, &vault, &["--promote"], dir.path());
    assert_eq!(promoted.code, 2);
    assert!(
        promoted.stderr.contains("not implemented: adopt --promote"),
        "{}",
        promoted.stderr
    );
    assert!(!vault.join("plugins").exists());
}

/// A dry run over several sources sees what the earlier sources would have
/// written, so it reports the collision the real run refuses.
#[test]
fn a_multi_source_dry_run_predicts_the_collision_the_write_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    let mut sources = Vec::new();
    for name in ["src-a", "src-b"] {
        let skill = dir.path().join(name).join("review");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            format!("---\nname: review\ndescription: reviews from {name}\n---\nReview.\n"),
        )
        .unwrap();
        sources.push(dir.path().join(name).display().to_string());
    }
    let argv = |extra: &str| {
        vec![
            "adopt".to_string(),
            sources[0].clone(),
            sources[1].clone(),
            "--vault".to_string(),
            vault_str(&vault),
            extra.to_string(),
        ]
    };

    let planned = run_argv(argv("--dry-run"), &[], dir.path());
    assert!(
        !vault.join("plugins").exists(),
        "a dry run must write nothing"
    );
    let written = run_argv(argv("--yes"), &[], dir.path());
    assert_eq!(written.code, 1, "{}", written.stderr);
    assert_eq!(planned.code, written.code, "{}", planned.stderr);
    assert_eq!(planned.stdout, written.stdout);
}

#[test]
fn adopt_off_a_tty_without_yes_is_a_usage_error_not_a_silent_write() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    let result = run(
        &[
            "adopt",
            &source.display().to_string(),
            "--vault",
            &vault_str(&vault),
        ],
        dir.path(),
    );
    assert_eq!(result.code, 3);
    assert!(!vault.join("plugins").exists());
}

#[test]
fn a_source_that_is_not_there_is_the_cores_own_refusal_not_a_second_copy_of_it() {
    let (dir, vault) = fixture();
    let missing = dir.path().join("nowhere");
    let result = run(
        &[
            "adopt",
            &missing.display().to_string(),
            "--vault",
            &vault_str(&vault),
            "--yes",
        ],
        dir.path(),
    );

    // Exit 1 like any other source a rail stopped, and the message is the
    // core's `RootNotFound` — this CLI does not pre-check the path.
    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("corpus root not found"),
        "{}",
        result.stderr
    );
    assert!(
        result.stderr.contains(&missing.display().to_string()),
        "{}",
        result.stderr
    );
}

#[test]
fn only_across_several_sources_is_a_usage_error_not_a_run_of_spurious_failures() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    // The core refuses a requested name that matched nothing PER SOURCE, so
    // broadcasting one list would fail every source not holding every name.
    let result = run(
        &[
            "adopt",
            &source.display().to_string(),
            "--from",
            &dir.path().display().to_string(),
            "--vault",
            &vault_str(&vault),
            "--only",
            "code-review",
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(result.code, 3);
    assert!(
        result.stderr.contains("--only selects within one source"),
        "{}",
        result.stderr
    );
}

#[test]
fn with_no_source_and_no_user_skill_folders_it_is_a_usage_error_exit_3() {
    let (dir, vault) = fixture();
    // An empty `$HOME`: no harness's user skill folder exists to adopt from.
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let out = run_binary(
        &["adopt", "--vault", &vault_str(&vault), "--yes"],
        dir.path(),
        &[("HOME", home.to_str().unwrap())],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "{stderr}");
    assert!(stderr.contains("build.scan_locations"), "{stderr}");
}

#[test]
fn a_skills_root_flag_is_where_adopt_writes_and_where_build_looks() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    // The agreement `AdoptRequest.skills_root`'s doc requires: the adoption
    // and the scan must name the same tree. Two resolvers with two answers
    // would put adopted notes somewhere `build` never looks, and nothing
    // would report it — so one run proves both ends.
    let library = vault.join("library").display().to_string();
    let adopted = run(
        &[
            "adopt",
            &source.display().to_string(),
            "--vault",
            &vault_str(&vault),
            "--into",
            &library,
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(adopted.code, 0, "{}", adopted.stderr);
    // `plugins/` under the named root, not assumed of it.
    assert!(
        vault
            .join("library")
            .join("plugins")
            .join("adopted")
            .join("skills")
            .join("code-review")
            .join("code-review.md")
            .exists()
    );
    // Not the default root.
    assert!(!vault.join("plugins").exists());

    let built = run(
        &[
            "build",
            "--vault",
            &vault_str(&vault),
            "--skills-root",
            &library,
            "--out",
            &dir.path().join("market").display().to_string(),
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(
        built.stdout.contains("claude/adopted  written · 1 skill"),
        "{}",
        built.stdout
    );
}

#[test]
fn the_configured_scan_locations_are_the_default_source() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    let config_path = vault.join("skillwright.md");
    fs::write(
        &config_path,
        format!(
            "---\nbuild:\n  scan_locations:\n    - {}\n---\n",
            source.display()
        ),
    )
    .unwrap();

    let result = run(
        &[
            "adopt",
            "--config",
            &config_path.display().to_string(),
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains("code-review.md  adopted"),
        "{}",
        result.stdout
    );
}

#[test]
fn from_git_stamps_the_repository_and_the_commit_never_the_checkout_path() {
    let (dir, vault) = fixture();
    // A repository reached over `file://`: the fetch is real, the network is
    // not.
    let repo = dir.path().join("repo");
    let sha = commit_git_fixture(&repo);

    common::with_git_cache(|| {
        let reference = format!("file://{}@main#skills", repo.display());
        let first = run(
            &[
                "adopt",
                "--from-git",
                &reference,
                "--vault",
                &vault_str(&vault),
                "--yes",
            ],
            dir.path(),
        );
        assert_eq!(first.code, 0, "{}", first.stderr);
        assert!(
            first.stdout.contains("code-review.md  adopted"),
            "{}",
            first.stdout
        );

        let note = fs::read_to_string(adopted_note(&vault, "adopted")).unwrap();
        assert!(note.contains("  source:"));
        // The repository and the commit, which still name the same bytes
        // after the cache is purged — not the checkout directory, which does
        // not.
        assert!(note.contains(&format!("file://{}/skills@{sha}", repo.display())));
        assert!(!note.contains("checkouts"));

        // Stamping the label rather than the path is what makes the second
        // import read as the same source instead of as somebody else's.
        let again = run(
            &[
                "adopt",
                "--from-git",
                &reference,
                "--vault",
                &vault_str(&vault),
                "--yes",
            ],
            dir.path(),
        );
        assert_eq!(again.code, 0, "{}", again.stderr);
        assert!(
            again.stdout.contains("code-review.md  unchanged"),
            "{}",
            again.stdout
        );
    });
}

#[test]
fn a_repository_that_will_not_fetch_fails_that_source_alone_not_the_run() {
    let (dir, vault) = fixture();
    let source = adopt_source(dir.path());
    // `file://` at a path that is not a repository: the fetch fails without a
    // network. The good source beside it is one the run would adopt cleanly.
    common::with_git_cache(|| {
        let no_such = dir.path().join("no-such-repo");
        let result = run(
            &[
                "adopt",
                &source.display().to_string(),
                "--from-git",
                &format!("file://{}", no_such.display()),
                "--vault",
                &vault_str(&vault),
                "--yes",
            ],
            dir.path(),
        );
        // One source refused, so the completed run exits 1 — the same code a
        // source a rail stopped produces.
        assert_eq!(result.code, 1, "{}", result.stderr);
        assert!(
            result.stdout.contains("code-review.md  adopted"),
            "{}",
            result.stdout
        );
        assert!(
            result.stderr.contains("adopt failed for "),
            "{}",
            result.stderr
        );
        assert!(result.stderr.contains("no-such-repo"), "{}", result.stderr);
    });
}

#[test]
fn from_git_refuses_a_spelling_that_is_not_a_repository_exit_3() {
    let (dir, vault) = fixture();
    let result = run(
        &[
            "adopt",
            "--from-git",
            "./not-a-repo",
            "--vault",
            &vault_str(&vault),
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(result.code, 3);
    assert!(
        result.stderr.contains("--from-git wants a repository"),
        "{}",
        result.stderr
    );
}

// --- list --------------------------------------------------------------------

/// A second note, so a row's targets and its folder's guess can differ.
fn widen_vault(vault: &Path) {
    fs::create_dir_all(vault.join("commands").join("tidy")).unwrap();
    fs::write(
        vault.join("commands").join("tidy").join("tidy.md"),
        "---\nname: tidy\ndescription: tidies up\ntargets: []\n---\nTidy.\n",
    )
    .unwrap();
    fs::write(
        vault.join("skills").join("code-review").join("code-review.md"),
        "---\nname: code-review\ndescription: reviews code\ntargets: [claude-code, codex]\n---\nReview the diff.\n",
    )
    .unwrap();
}

#[test]
fn the_three_readings_of_a_targets_key_each_render_differently() {
    let (dir, vault) = fixture();
    widen_vault(&vault);
    let listed = run(
        &["list", "--vault", &vault_str(&vault), "--quiet"],
        dir.path(),
    );
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    assert!(
        listed.stdout.contains("claude-code, codex"),
        "{}",
        listed.stdout
    );
    assert!(listed.stdout.contains("nowhere"), "{}", listed.stdout);

    // No key at all is every destination.
    fs::remove_dir_all(vault.join("commands")).unwrap();
    fs::write(
        vault
            .join("skills")
            .join("code-review")
            .join("code-review.md"),
        "---\nname: code-review\ndescription: reviews code\n---\nReview the diff.\n",
    )
    .unwrap();
    let absent = run(
        &["list", "--vault", &vault_str(&vault), "--quiet"],
        dir.path(),
    );
    assert!(
        absent.stdout.contains("every destination"),
        "{}",
        absent.stdout
    );
}

// --- the configurable frontmatter namespace ----------------------------------

/// A root file in the vault with, optionally, a namespace stem.
fn namespace_config(_dir: &Path, vault: &Path, stem: Option<&str>) -> String {
    let path = vault.join("skillwright.md");
    let mut yaml = "---\n".to_owned();
    if let Some(stem) = stem {
        yaml.push_str(&format!("build:\n  namespace: {stem}\n"));
    }
    yaml.push_str("---\n");
    fs::write(&path, yaml).unwrap();
    path.display().to_string()
}

#[test]
fn an_invalid_stem_in_the_config_is_a_run_level_blocker_at_load() {
    let (dir, vault) = fixture();
    let config = namespace_config(dir.path(), &vault, Some("my-vault"));

    let run = run(&["list", "--config", &config], dir.path());

    // A malformed root file is exit 2: nothing attempted, the report empty,
    // stderr naming the reason. Not exit 4 — that one means "no vault here".
    assert_eq!(run.code, 2, "{}", run.stderr);
    assert_eq!(run.stdout, "");
    // The offending stem, and the core's own reason for refusing it.
    assert!(
        run.stderr
            .contains("`my-vault` cannot be a frontmatter namespace"),
        "{}",
        run.stderr
    );
    assert!(
        run.stderr
            .contains("`-` separates the namespace from the field"),
        "{}",
        run.stderr
    );
    // And where it was set, which the core cannot know.
    assert!(run.stderr.contains(&config), "{}", run.stderr);
}

#[test]
fn namespace_overrides_the_configured_stem_for_this_run_and_never_writes_it_back() {
    let (dir, vault) = fixture();
    fs::write(
        vault.join("skills").join("code-review").join("code-review.md"),
        "---\nname: code-review\ndescription: reviews code\nzz-targets: [codex]\nx-targets: [claude-code]\n---\nReview the diff.\n",
    )
    .unwrap();
    let config = namespace_config(dir.path(), &vault, Some("x"));
    let before = fs::read_to_string(&config).unwrap();

    let run = run(
        &["list", "--config", &config, "--namespace", "zz", "--quiet"],
        dir.path(),
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    // The flag's namespace was read; the file's is somebody else's key under
    // `zz`.
    assert!(run.stdout.contains("codex"), "{}", run.stdout);
    assert!(!run.stdout.contains("claude-code"), "{}", run.stdout);
    // The file on disk is byte-identical: an override is for one run.
    assert_eq!(fs::read_to_string(&config).unwrap(), before);
}

#[test]
fn namespace_works_with_no_config_file_at_all() {
    let (dir, vault) = fixture();
    fs::create_dir_all(vault.join("commands").join("tidy")).unwrap();
    fs::write(
        vault.join("commands").join("tidy").join("tidy.md"),
        "---\nname: tidy\ndescription: tidies up\nv-targets: []\n---\nTidy.\n",
    )
    .unwrap();

    let run = run(
        &[
            "list",
            "--vault",
            &vault_str(&vault),
            "--namespace",
            "v",
            "--quiet",
        ],
        dir.path(),
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    // `nowhere` only happens if `v-targets` was read as ours.
    assert!(run.stdout.contains("nowhere"), "{}", run.stdout);
}

// --- status: the states a published destination can be in --------------------

/// The destination `status_config` names.
fn dest_of(dir: &Path) -> PathBuf {
    dir.join("dest")
}

/// One publish through the core, so a `status` test has a destination in a
/// state to report on. These `status` tests drive the core directly rather
/// than the CLI's `publish` verb, which has its own tests.
fn publish(vault: &Path, dest: &Path, note: &str) {
    let report = publish_skill(
        &NativeVfs,
        &{
            let mut value = PublishRequest::new(vault.join(note), dest.to_path_buf(), "claude");
            value.skills_root = Some(vault.to_path_buf());
            value.published_at = Some("2026-01-01T00:00:00Z".to_string());
            value
        },
        &Namespace::default(),
    )
    .unwrap();
    assert!(
        report
            .files
            .iter()
            .any(|f| f.status == PublishStatus::Written),
        "nothing was published"
    );
}

/// The entry file a `claude` publish of the fixture note writes.
fn published_entry(dest: &Path) -> PathBuf {
    dest.join(".claude")
        .join("skills")
        .join("code-review")
        .join("SKILL.md")
}

#[test]
fn a_published_untouched_destination_is_current() {
    let (dir, vault) = fixture();
    let config = status_config(dir.path(), &vault);
    publish(
        &vault,
        &dest_of(dir.path()),
        "skills/code-review/code-review.md",
    );

    let run = run(&["status", "--config", &config], dir.path());
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(run.stdout.contains("current"), "{}", run.stdout);
    assert!(run.stdout.contains("last published "), "{}", run.stdout);
    assert!(run.stderr.contains("1 current"), "{}", run.stderr);
}

#[test]
fn editing_the_note_makes_it_stale_and_check_exits_1() {
    let (dir, vault) = fixture();
    let config = status_config(dir.path(), &vault);
    publish(
        &vault,
        &dest_of(dir.path()),
        "skills/code-review/code-review.md",
    );
    fs::write(
        vault
            .join("skills")
            .join("code-review")
            .join("code-review.md"),
        "---\nname: code-review\ndescription: reviews code, carefully\n---\nReview it twice.\n",
    )
    .unwrap();

    let report = run(&["status", "--config", &config], dir.path());
    assert_eq!(report.code, 0, "{}", report.stderr);
    assert!(report.stdout.contains("stale"), "{}", report.stdout);

    let checked = run(&["status", "--config", &config, "--check"], dir.path());
    assert_eq!(checked.code, 1, "{}", checked.stderr);
}

#[test]
fn a_file_edited_at_the_destination_is_a_conflict_and_the_row_names_the_file() {
    let (dir, vault) = fixture();
    let config = status_config(dir.path(), &vault);
    let dest = dest_of(dir.path());
    publish(&vault, &dest, "skills/code-review/code-review.md");
    fs::write(
        published_entry(&dest),
        "# edited by hand at the destination\n",
    )
    .unwrap();

    let run = run(&["status", "--config", &config, "--check"], dir.path());
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(run.stdout.contains("conflict"), "{}", run.stdout);
    assert!(
        run.stdout.contains(".claude/skills/code-review/SKILL.md"),
        "{}",
        run.stdout
    );
}

#[test]
fn an_unreadable_manifest_is_unknown_with_a_reason_not_an_error() {
    let (dir, vault) = fixture();
    let config = status_config(dir.path(), &vault);
    let dest = dest_of(dir.path());
    publish(&vault, &dest, "skills/code-review/code-review.md");
    fs::write(
        dest.join(".skillwright").join("manifest.json"),
        "{ truncated",
    )
    .unwrap();

    let run = run(&["status", "--config", &config, "--check"], dir.path());
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(run.stdout.contains("unknown"), "{}", run.stdout);
    assert!(run.stdout.contains("bad-manifest"), "{}", run.stdout);
}

#[test]
fn a_destination_the_notes_sw_targets_excludes_reads_as_not_published() {
    let (dir, vault) = fixture();
    let config = status_config(dir.path(), &vault);
    fs::write(
        vault.join("skills").join("code-review").join("code-review.md"),
        "---\nname: code-review\ndescription: reviews code\ntargets:\n  - codex\n---\nReview the diff.\n",
    )
    .unwrap();

    let run = run(&["status", "--config", &config, "--check"], dir.path());
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(run.stdout.contains("not published"), "{}", run.stdout);
    for state in ["never-published", "current", "stale", "conflict", "unknown"] {
        assert!(
            !run.stdout.contains(&format!(")  {state}")),
            "{state} in {}",
            run.stdout
        );
    }
}

#[test]
fn with_no_note_every_publishable_unit_the_scan_finds_gets_a_row() {
    let (dir, vault) = fixture();
    let config = status_config(dir.path(), &vault);
    fs::create_dir_all(vault.join("skills").join("release-notes")).unwrap();
    fs::write(
        vault
            .join("skills")
            .join("release-notes")
            .join("release-notes.md"),
        "---\nname: release-notes\ndescription: writes release notes\n---\nWrite them.\n",
    )
    .unwrap();

    let run = run(&["status", "--config", &config], dir.path());
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout.contains("skills/code-review/code-review.md ->"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("skills/release-notes/release-notes.md ->"),
        "{}",
        run.stdout
    );
}

/// `bump` finds a plugin by its frontmatter `name`, so the advisory has to
/// name it that way too, not by the note's filename.
#[test]
fn the_bump_advisory_names_the_plugin_bump_will_find() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let plugin = vault
        .join("marketplaces")
        .join("acme")
        .join("plugins")
        .join("core-tools");
    fs::create_dir_all(plugin.join("skills")).unwrap();
    fs::write(
        vault
            .join("marketplaces")
            .join("acme")
            .join("skillwright.md"),
        "---\nname: acme\ndescription: Acme skills.\n---\n\nOne marketplace.\n",
    )
    .unwrap();
    fs::write(
        plugin.join("core-tools.md"),
        "---\nname: core-tools\ndescription: The core tools.\nversion: 1.0.0\n---\n\nThe plugin.\n",
    )
    .unwrap();
    let member = plugin.join("skills").join("review.md");
    fs::write(
        &member,
        "---\nname: review\ndescription: Reviews a diff.\n---\n\nRead the diff.\n",
    )
    .unwrap();
    let config = status_config(dir.path(), &vault);
    let pack = plugin.join("core-tools.md");
    let published = run(
        &[
            "publish",
            pack.to_str().unwrap(),
            "--config",
            &config,
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(published.code, 0, "{}", published.stderr);
    fs::write(
        &member,
        "---\nname: review\ndescription: Reviews a diff.\n---\n\nRead the whole diff.\n",
    )
    .unwrap();

    let status = run(&["status", "--config", &config], dir.path());
    assert_eq!(status.code, 0, "{}", status.stderr);
    assert!(
        status
            .stderr
            .contains("`skillwright bump core-tools` is available"),
        "{}",
        status.stderr
    );
    let one = run(
        &[
            "status",
            "marketplaces/acme/plugins/core-tools/core-tools.md",
            "--config",
            &config,
        ],
        dir.path(),
    );
    assert_eq!(one.code, 0, "{}", one.stderr);
    assert!(
        one.stderr
            .contains("`skillwright bump core-tools` is available"),
        "{}",
        one.stderr
    );
    let bump = run(
        &["bump", "core-tools", "--dry-run", "--config", &config],
        dir.path(),
    );
    assert_eq!(bump.code, 0, "{}", bump.stderr);
}

#[test]
fn a_hook_is_n_a_rather_than_unknown_and_status_check_still_passes() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("hook-vault");
    let dest = dir.path().join("dest");
    let plugin = vault
        .join("marketplaces")
        .join("acme")
        .join("plugins")
        .join("review");
    fs::create_dir_all(plugin.join("skills")).unwrap();
    fs::create_dir_all(plugin.join("hooks")).unwrap();
    fs::create_dir_all(&dest).unwrap();
    fs::write(
        vault
            .join("marketplaces")
            .join("acme")
            .join("skillwright.md"),
        "---\nname: acme\ndescription: Acme skills.\n---\n\nOne marketplace.\n",
    )
    .unwrap();
    fs::write(
        plugin.join("review.md"),
        "---\nname: review\ndescription: Skills for reviewing a change.\nversion: 1.0.0\n---\n\nThe review plugin.\n",
    )
    .unwrap();
    fs::write(
        plugin.join("skills").join("code-review.md"),
        "---\nname: code-review\ndescription: Reviews a diff.\n---\n\nRead the diff.\n",
    )
    .unwrap();
    fs::write(
        plugin.join("hooks").join("lint-on-write.md"),
        "---\nname: lint-on-write\ndescription: Runs the linter after a write.\nevent: PostToolUse\nmatcher: Edit|Write\ntimeout: 5000\n---\n\njust lint\n",
    )
    .unwrap();
    let config = vault.join("skillwright.md");
    fs::write(
        &config,
        format!(
            "---\nbuild:\n  destinations:\n    - root: {}\n      target: claude-code\n---\n",
            dest.display()
        ),
    )
    .unwrap();

    let run = run(
        &[
            "status",
            "--config",
            &config.display().to_string(),
            "--check",
        ],
        dir.path(),
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(run.stdout.contains("lint-on-write.md"), "{}", run.stdout);
    assert!(
        run.stdout
            .contains("n/a · a hook is not published to a destination"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("n/a · a marketplace is not published to a destination"),
        "{}",
        run.stdout
    );
    assert!(!run.stdout.contains("unknown"), "{}", run.stdout);
    assert!(run.stderr.contains("n/a"), "{}", run.stderr);
}

#[test]
fn a_note_outside_the_vault_prints_a_relative_path_not_an_absolute_one() {
    let (dir, vault) = fixture();
    let config = status_config(dir.path(), &vault);
    // A sibling of the vault, addressed through it, so the row reads `../…` rather than the whole path.
    let outside = dir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(
        outside.join("stray.md"),
        "---\nname: stray\ndescription: a stray note\n---\nStray.\n",
    )
    .unwrap();

    let run = run(
        &[
            "status",
            "../outside/stray.md",
            "--config",
            &config,
            "--quiet",
        ],
        dir.path(),
    );
    assert!(
        run.stdout.starts_with("../outside/stray.md ->"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_named_note_collapses_dot_segments_the_way_resolve_path_does() {
    let (dir, vault) = fixture();
    let config = status_config(dir.path(), &vault);
    let run = run(
        &[
            "status",
            "./skills/../skills/code-review/code-review.md",
            "--config",
            &config,
            "--quiet",
        ],
        dir.path(),
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout
            .starts_with("skills/code-review/code-review.md ->"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_config_naming_a_directory_is_unreadable_not_missing() {
    let dir = tempfile::tempdir().unwrap();
    let as_dir = dir.path().join("skillwright.md");
    fs::create_dir_all(&as_dir).unwrap();
    // Existence, not "is a file": the path is there, so the failure is the
    // read, and that is a blocker (2) rather than "--config names a file that
    // is not there" (4).
    let run = run(
        &["list", "--config", &as_dir.display().to_string()],
        dir.path(),
    );
    assert_eq!(run.code, 2, "{}", run.stderr);
    assert!(
        run.stderr.starts_with("configuration unreadable: "),
        "{}",
        run.stderr
    );
}

// --- publish: --dry-run and --overwrite --------------------------------------

/// `publish skills/code-review/code-review.md --vault <vault> --to <dest>
/// --target claude --dry-run`, plus whatever `extra` adds.
fn publish_dry(vault: &Path, dest: &Path, extra: &[&str], cwd: &Path) -> Run {
    run_argv(publish_argv(vault, dest, "--dry-run"), extra, cwd)
}

/// `publish <the fixture's one note> --vault … --to … --target claude`, plus
/// the one flag that tells a plan from a write.
fn publish_argv(vault: &Path, dest: &Path, mode: &str) -> Vec<String> {
    vec![
        "publish".to_string(),
        "skills/code-review/code-review.md".to_string(),
        "--vault".to_string(),
        vault_str(vault),
        "--to".to_string(),
        dest.display().to_string(),
        "--target".to_string(),
        "claude".to_string(),
        mode.to_string(),
    ]
}

/// The same note, published for real: `--yes` in place of `--dry-run`, plus
/// whatever `extra` adds.
fn publish_real(vault: &Path, dest: &Path, extra: &[&str], cwd: &Path) -> Run {
    run_argv(publish_argv(vault, dest, "--yes"), extra, cwd)
}

/// The destination-relative path the fixture's one skill renders to under a
/// `claude` destination.
const PUBLISHED_SKILL: &str = ".claude/skills/code-review/SKILL.md";

fn published_skill_path(dest: &Path) -> PathBuf {
    dest.join(".claude")
        .join("skills")
        .join("code-review")
        .join("SKILL.md")
}

/// A sorted `Object.keys()`-equivalent, for comparing a JSON object's key
/// set without caring about field order.
fn sorted_keys(value: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

/// Publish for real, then hand-edit what landed: the next run refuses
/// `conflict`.
fn conflict(vault: &Path, dest: &Path, cwd: &Path) {
    let published = publish_real(vault, dest, &[], cwd);
    assert_eq!(published.code, 0, "{}", published.stderr);
    let path = published_skill_path(dest);
    let mut content = fs::read_to_string(&path).unwrap();
    content.push_str("\nedited here\n");
    fs::write(&path, content).unwrap();
}

#[test]
fn dry_run_over_a_clean_destination_reports_every_file_written_and_leaves_the_destination_empty() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();

    let run = publish_dry(&vault, &dest, &[], dir.path());

    assert_eq!(run.code, 0, "{}", run.stderr);
    // The outcome, not the status alone: an empty destination is a first
    // write, which the same line for a stale one does not say.
    assert!(
        run.stdout
            .contains(".claude/skills/code-review/SKILL.md  written · first-write"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains(".claude/skills/code-review/references/style.md  written · first-write"),
        "{}",
        run.stdout
    );
    // Marked as a plan, not a completed publish — and not gated by --quiet.
    assert!(run.stderr.contains("dry run"), "{}", run.stderr);
    assert!(run.stderr.contains("(dry run)"), "{}", run.stderr);
    // The tree, not just one file: nothing was created anywhere under dest.
    assert_eq!(tree(&dest), Vec::<PathBuf>::new());
}

#[test]
fn dry_run_over_an_identical_destination_reports_unchanged_still_writes_nothing() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();
    let real = publish_real(&vault, &dest, &[], dir.path());
    assert_eq!(real.code, 0, "{}", real.stderr);
    let before = tree(&dest);

    let run = publish_dry(&vault, &dest, &[], dir.path());

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout
            .contains(".claude/skills/code-review/SKILL.md  unchanged"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains(".claude/skills/code-review/references/style.md  unchanged"),
        "{}",
        run.stdout
    );
    // A dry run over an already-published destination touches nothing on it.
    assert_eq!(tree(&dest), before);
}

#[test]
fn a_rewrite_of_stale_output_does_not_read_as_a_first_write_on_the_line_and_in_json() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();
    let real = publish_real(&vault, &dest, &[], dir.path());
    assert_eq!(real.code, 0, "{}", real.stderr);
    fs::write(
        vault.join("skills").join("code-review").join("code-review.md"),
        "---\nname: code-review\ndescription: reviews code, carefully\n---\nReview the diff twice.\n",
    )
    .unwrap();

    let run = publish_dry(&vault, &dest, &[], dir.path());
    assert!(
        run.stdout
            .contains(".claude/skills/code-review/SKILL.md  written · rewrite"),
        "{}",
        run.stdout
    );
    assert!(
        !run.stdout
            .contains(".claude/skills/code-review/SKILL.md  written · first-write"),
        "{}",
        run.stdout
    );

    let json = publish_dry(&vault, &dest, &["--json"], dir.path());
    let parsed: serde_json::Value = serde_json::from_str(&json.stdout).unwrap();
    let entry = parsed["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == PUBLISHED_SKILL)
        .unwrap();
    // The same token either way, so a reader and a script agree.
    assert_eq!(entry["outcome"], "rewrite");
}

#[test]
fn dry_run_never_prompts_even_without_yes_and_off_a_tty() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();

    // No --yes here. An ordinary writing command in this position is a usage
    // error; a dry run must reach the run instead, proving the confirmation
    // gate is skipped outright rather than auto-answered.
    let run = publish_dry(&vault, &dest, &[], dir.path());
    assert_eq!(run.code, 0, "{}", run.stderr);
}

#[test]
fn a_dry_run_whose_real_counterpart_refuses_exits_1_same_as_the_real_run_would() {
    let (dir, vault) = fixture();

    // The destination IS the vault: the destination-guard rail refuses every
    // file.
    let run = publish_dry(&vault, &vault, &[], dir.path());
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(run.stdout.contains("refused"), "{}", run.stdout);
}

#[test]
fn dry_run_json_emits_the_same_document_shape_as_a_real_run() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();

    let run = publish_dry(&vault, &dest, &["--json"], dir.path());
    assert_eq!(run.code, 0, "{}", run.stderr);
    let parsed: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    let files = parsed["files"].as_array().unwrap();
    assert!(
        files.iter().all(|f| f["status"] == "written"),
        "{}",
        run.stdout
    );
    assert_eq!(sorted_keys(&parsed), vec!["files".to_string()]);
    for file in files {
        assert_eq!(
            sorted_keys(file),
            vec![
                "destination".to_string(),
                "outcome".to_string(),
                "path".to_string(),
                "reason".to_string(),
                "skill".to_string(),
                "status".to_string(),
                "target".to_string(),
            ]
        );
    }
    // The document alone, on stdout — not the stderr marker.
    assert!(!run.stdout.contains("dry run"), "{}", run.stdout);
    assert_eq!(tree(&dest), Vec::<PathBuf>::new());
}

#[test]
fn run_dry_then_run_for_real_over_the_same_fixture_the_reports_match() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();

    let dry = publish_dry(&vault, &dest, &["--json"], dir.path());
    assert_eq!(dry.code, 0, "{}", dry.stderr);
    assert_eq!(tree(&dest), Vec::<PathBuf>::new()); // still untouched

    let real = publish_real(&vault, &dest, &["--json"], dir.path());
    assert_eq!(real.code, 0, "{}", real.stderr);

    // The plan and the run it predicted, byte-for-byte at the JSON wire.
    let dry_json: serde_json::Value = serde_json::from_str(&dry.stdout).unwrap();
    let real_json: serde_json::Value = serde_json::from_str(&real.stdout).unwrap();
    assert_eq!(dry_json, real_json);
}

// --- publish --overwrite ------------------------------------------------------

#[test]
fn overwrite_help_says_the_path_is_destination_relative() {
    let dir = tempfile::tempdir().unwrap();
    let run = run(&["publish", "--help"], dir.path());
    assert!(
        run.stdout.contains("destination-relative"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_path_matching_no_projected_file_is_named_on_stderr() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();

    let run = publish_real(
        &vault,
        &dest,
        &["--overwrite", ".claude/skills/typo/SKILL.md"],
        dir.path(),
    );

    assert!(
        run.stderr.contains("matched no projected file"),
        "{}",
        run.stderr
    );
    assert!(
        run.stderr.contains(".claude/skills/typo/SKILL.md"),
        "{}",
        run.stderr
    );
    // Stdout stays the report and nothing else.
    assert!(
        !run.stdout.contains("matched no projected file"),
        "{}",
        run.stdout
    );
    // The run did what it was asked to do, so its exit code is its own: by
    // the time the unmatched set is known the files are written.
    assert_eq!(run.code, 0, "{}", run.stderr);
}

#[test]
fn a_path_matching_a_file_that_needed_no_acknowledgement_is_not_reported() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();
    let first = publish_real(&vault, &dest, &[], dir.path());
    assert_eq!(first.code, 0, "{}", first.stderr);

    let run = publish_real(&vault, &dest, &["--overwrite", PUBLISHED_SKILL], dir.path());

    assert!(
        !run.stderr.contains("matched no projected file"),
        "{}",
        run.stderr
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
}

#[test]
fn a_path_matching_in_one_pair_only_is_not_reported() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();
    // Unmatched means matched no projected file ANYWHERE in the run. The
    // second note projects to a different path, and warning per pair would
    // fire on entirely correct usage.
    fs::create_dir_all(vault.join("skills").join("commit-msg")).unwrap();
    fs::write(
        vault
            .join("skills")
            .join("commit-msg")
            .join("commit-msg.md"),
        "---\nname: commit-msg\ndescription: writes commit messages\n---\nWrite it.\n",
    )
    .unwrap();

    let run = run(
        &[
            "publish",
            "skills/code-review/code-review.md",
            "skills/commit-msg/commit-msg.md",
            "--vault",
            &vault_str(&vault),
            "--to",
            &dest.display().to_string(),
            "--target",
            "claude",
            "--yes",
            "--overwrite",
            PUBLISHED_SKILL,
        ],
        dir.path(),
    );

    assert!(
        !run.stderr.contains("matched no projected file"),
        "{}",
        run.stderr
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
}

#[test]
fn an_absolute_spelling_is_told_that_it_is_absolute() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();
    let absolute = dest.join(PUBLISHED_SKILL).display().to_string();

    let run = publish_real(&vault, &dest, &["--overwrite", &absolute], dir.path());

    assert!(
        run.stderr.contains("matched no projected file"),
        "{}",
        run.stderr
    );
    assert!(run.stderr.contains("absolute"), "{}", run.stderr);
}

#[test]
fn overwrite_dry_run_reports_it_the_same_way_so_the_plan_is_still_the_plan() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();

    let run = publish_dry(
        &vault,
        &dest,
        &["--overwrite", ".claude/skills/typo/SKILL.md"],
        dir.path(),
    );

    assert!(
        run.stderr.contains("matched no projected file"),
        "{}",
        run.stderr
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
}

#[test]
fn overwrite_quiet_still_reports_it() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();

    // `--quiet` drops the progress lines and the tally. An acknowledgement
    // the core never consulted is neither.
    let run = publish_real(
        &vault,
        &dest,
        &["--quiet", "--overwrite", ".claude/skills/typo/SKILL.md"],
        dir.path(),
    );

    assert!(
        run.stderr.contains("matched no projected file"),
        "{}",
        run.stderr
    );
    assert!(!run.stderr.contains("1 written"), "{}", run.stderr);
}

#[test]
fn overwrite_json_is_unchanged_by_the_warning() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();

    let run = publish_real(
        &vault,
        &dest,
        &["--json", "--overwrite", ".claude/skills/typo/SKILL.md"],
        dir.path(),
    );

    let parsed: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(sorted_keys(&parsed), vec!["files".to_string()]);
}

#[test]
fn an_acknowledged_conflict_is_overwritten_and_the_run_exits_0() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();
    conflict(&vault, &dest, dir.path());

    let run = publish_real(&vault, &dest, &["--overwrite", PUBLISHED_SKILL], dir.path());

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout.contains(&format!("{PUBLISHED_SKILL}  written")),
        "{}",
        run.stdout
    );
    let content = fs::read_to_string(published_skill_path(&dest)).unwrap();
    assert!(!content.contains("edited here"), "{content}");
}

#[test]
fn a_run_that_also_refused_a_file_exits_1_warning_and_all() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();
    conflict(&vault, &dest, dir.path());

    let run = publish_real(
        &vault,
        &dest,
        &["--overwrite", ".claude/skills/typo/SKILL.md"],
        dir.path(),
    );

    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(
        run.stdout.contains(&format!("{PUBLISHED_SKILL}  refused")),
        "{}",
        run.stdout
    );
    assert!(
        run.stderr.contains("matched no projected file"),
        "{}",
        run.stderr
    );
    // The refusal held: the edit at the destination is still there.
    let content = fs::read_to_string(published_skill_path(&dest)).unwrap();
    assert!(content.contains("edited here"), "{content}");
}

#[test]
fn a_run_that_projected_nothing_blames_the_flag_for_nothing() {
    let (dir, vault) = fixture();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();
    // Every pair throws before a file list exists, so the run knows no
    // projected paths at all. Reporting every acknowledgement as unmatched
    // there would blame the flag for the failure above it. A plugin note
    // with no member this destination admits reaches the loop with a known
    // target and throws `empty-plugin` there, which is the shape this test
    // needs.
    let pack = vault.join("plugins").join("review-pack");
    fs::create_dir_all(&pack).unwrap();
    fs::write(
        pack.join("review-pack.md"),
        "---\nname: review-pack\ndescription: the review pack\n---\nReview the diff.\n",
    )
    .unwrap();

    let mut argv = publish_argv(&vault, &dest, "--yes");
    argv[1] = "plugins/review-pack/review-pack.md".to_string();
    let run = run_argv(
        argv,
        &["--overwrite", ".claude/skills/typo/SKILL.md"],
        dir.path(),
    );

    // The pair threw: one refused line for the skill itself, exit 1.
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(run.stdout.contains("refused"), "{}", run.stdout);
    assert!(
        !run.stderr.contains("matched no projected file"),
        "{}",
        run.stderr
    );
}

// --- publish: a pack whose folder carries its own manifest -----------------------

// The folder's own `plugin.json` is replaced by the rendered one. It is not a
// member, so the CLI must not tell the user to publish it on its own.
#[test]
fn a_packs_replaced_manifest_is_named_as_replaced_not_as_not_carried() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let base = vault.join("marketplaces/house/plugins/git-tools");
    fs::create_dir_all(base.join("skills")).unwrap();
    fs::create_dir_all(base.join(".claude-plugin")).unwrap();
    fs::write(
        base.join("git-tools.md"),
        "---\nname: git-tools\ndescription: Git helpers.\n---\nThe pack.\n",
    )
    .unwrap();
    fs::write(
        base.join("skills/commit-helper.md"),
        "---\nname: commit-helper\ndescription: Writes commits.\n---\nCommit well.\n",
    )
    .unwrap();
    fs::write(
        base.join(".claude-plugin/plugin.json"),
        "{\"name\": \"git-tools\"}\n",
    )
    .unwrap();
    let dest = dest_of(dir.path());
    fs::create_dir_all(&dest).unwrap();
    let argv = |json: bool| {
        let mut argv = vec![
            "publish".to_string(),
            "marketplaces/house/plugins/git-tools/git-tools.md".to_string(),
            "--vault".to_string(),
            vault_str(&vault),
            "--to".to_string(),
            dest.display().to_string(),
            "--target".to_string(),
            "claude".to_string(),
            "--yes".to_string(),
        ];
        if json {
            argv.push("--json".to_string());
        }
        argv
    };
    let own = "marketplaces/house/plugins/git-tools/.claude-plugin/plugin.json";

    let text = run_argv(argv(false), &[], dir.path());
    assert_eq!(text.code, 0, "{}", text.stderr);
    assert!(
        text.stderr.contains(&format!(
            "marketplaces/house/plugins/git-tools/git-tools.md at {} (claude): the folder's own `{own}` was not published; the pack's manifest is rendered from the plugin note's frontmatter",
            dest.display()
        )),
        "{}",
        text.stderr
    );
    assert!(!text.stderr.contains("not carried"), "{}", text.stderr);

    let json = run_argv(argv(true), &[], dir.path());
    assert_eq!(json.code, 0, "{}", json.stderr);
    let doc: serde_json::Value = serde_json::from_str(&json.stdout).unwrap();
    assert!(doc.get("not_carried").is_none(), "{doc}");
    assert_eq!(doc["replaced_manifests"][0]["manifest"], own, "{doc}");
}

// --- a note with a `name` and no `description` ----------------------------------

/// A vault holding one name-only skill and a root file — with
/// `require_descriptions` when `strict`.
fn name_only_project(strict: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let skill = dir.path().join("skills").join("quiet");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("quiet.md"), "---\nname: quiet\n---\nBody.\n").unwrap();
    let strict = if strict {
        "  require_descriptions: true\n"
    } else {
        ""
    };
    fs::write(
        dir.path().join("skillwright.md"),
        format!("---\nname: demo\nbuild:\n  out: out\n{strict}---\n"),
    )
    .unwrap();
    dir
}

const QUIET_WARNING: &str = "skills/quiet/quiet.md  warning · missing-description: `quiet` has a `name` and no `description`; published with \"No description provided.\"";

#[test]
fn a_missing_description_warns_on_stderr_even_under_quiet() {
    let dir = name_only_project(false);
    let build = run(&["build", "--yes", "--quiet"], dir.path());
    assert_eq!(build.code, 0, "{}", build.stderr);
    assert!(
        build.stderr.lines().any(|line| line == QUIET_WARNING),
        "{}",
        build.stderr
    );
    assert!(!build.stdout.contains("warning"), "{}", build.stdout);

    let list = run(&["list", "--quiet"], dir.path());
    assert_eq!(list.code, 0, "{}", list.stderr);
    assert!(
        list.stderr.lines().any(|line| line == QUIET_WARNING),
        "{}",
        list.stderr
    );
}

#[test]
fn require_descriptions_in_the_root_file_refuses_the_build() {
    let dir = name_only_project(true);
    let build = run(&["build", "--yes"], dir.path());
    assert_eq!(build.code, 2, "{}", build.stderr);
    assert!(
        build.stderr.contains("missing-description")
            && build.stderr.contains("skills/quiet/quiet.md"),
        "{}",
        build.stderr
    );
    assert!(!dir.path().join("out").exists());
}

// --- a root file named with --config, and a moved packs folder ---------------

fn skill(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "---\ndescription: Use it.\n---\n\nbody\n").unwrap();
}

#[test]
fn a_config_file_carries_its_patterns_and_exclusions() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("alt.md"),
        "---\nname: m\nbuild:\n  out: dist\n  exclude: '**/*.draft.md'\n  patterns:\n    plugins: packs/*\n---\n",
    )
    .unwrap();
    skill(&dir.path().join("packs/a/skills/x.md"));
    skill(&dir.path().join("packs/a/skills/z.draft.md"));
    let run = run(&["build", "--yes", "--config", "alt.md"], dir.path());
    assert_eq!(run.code, 0, "{}", run.stderr);
    let pack = dir.path().join("dist/.claude-plugin/packs/a/skills");
    assert!(pack.join("x/SKILL.md").is_file());
    assert!(!pack.join("z-draft").exists());
    assert!(!pack.join("z.draft").exists());
}

#[test]
fn list_leaves_out_the_moved_folder_of_one_of_several_marketplaces() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("skillwright.md"),
        "---\nname: m\nbuild:\n  out: .\n---\n",
    )
    .unwrap();
    for (name, note) in [
        ("a", "---\nname: a\nclaude: {packs: mva}\n---\n"),
        ("b", "---\nname: b\n---\n"),
    ] {
        std::fs::create_dir_all(dir.path().join("marketplaces").join(name)).unwrap();
        std::fs::write(
            dir.path()
                .join(format!("marketplaces/{name}/skillwright.md")),
            note,
        )
        .unwrap();
    }
    skill(&dir.path().join("marketplaces/a/plugins/p/skills/s.md"));
    let built = run(&["build", "--marketplace", "a", "--yes"], dir.path());
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(dir.path().join("mva/p").is_dir());
    let list = run(&["list"], dir.path());
    assert_eq!(list.code, 0, "{}", list.stderr);
    assert!(!list.stdout.contains("mva/"), "{}", list.stdout);
}

#[test]
fn list_leaves_out_a_moved_packs_folder_a_marketplace_note_names() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("skillwright.md"),
        "---\nname: m\nbuild:\n  out: .\n---\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("marketplaces/m")).unwrap();
    std::fs::write(
        dir.path().join("marketplaces/m/skillwright.md"),
        "---\nname: m\nclaude: {packs: built}\n---\n",
    )
    .unwrap();
    skill(&dir.path().join("marketplaces/m/plugins/rev/skills/a.md"));
    let built = run(&["build", "--yes"], dir.path());
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(dir.path().join("built/rev").is_dir());
    let list = run(&["list"], dir.path());
    assert_eq!(list.code, 0, "{}", list.stderr);
    assert!(!list.stdout.contains("built/"), "{}", list.stdout);
}

/// In a vault of several marketplaces that share a plugin name, the advised
/// `bump` has to name the marketplace, or it is ambiguous.
#[test]
fn the_bump_advisory_runs_in_a_vault_of_several_marketplaces() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    for market in ["one", "two"] {
        let base = vault.join("marketplaces").join(market);
        fs::create_dir_all(base.join("plugins/core-tools/skills")).unwrap();
        fs::write(
            base.join("skillwright.md"),
            format!("---\nname: {market}\ndescription: M.\n---\n"),
        )
        .unwrap();
        fs::write(
            base.join("plugins/core-tools/core-tools.md"),
            "---\nname: core-tools\ndescription: The core tools.\nversion: 1.0.0\n---\n",
        )
        .unwrap();
        fs::write(
            base.join("plugins/core-tools/skills/review.md"),
            "---\nname: review\ndescription: Reviews a diff.\n---\n\nRead the diff.\n",
        )
        .unwrap();
    }
    let config = status_config(dir.path(), &vault);
    let pack = vault.join("marketplaces/two/plugins/core-tools/core-tools.md");
    let published = run(
        &[
            "publish",
            pack.to_str().unwrap(),
            "--config",
            &config,
            "--yes",
        ],
        dir.path(),
    );
    assert_eq!(published.code, 0, "{}", published.stderr);
    fs::write(
        vault.join("marketplaces/two/plugins/core-tools/skills/review.md"),
        "---\nname: review\ndescription: Reviews a diff.\n---\n\nRead the whole diff.\n",
    )
    .unwrap();

    let note = "marketplaces/two/plugins/core-tools/core-tools.md";
    let status = run(&["status", note, "--config", &config], dir.path());
    assert_eq!(status.code, 0, "{}", status.stderr);
    let advised = status
        .stderr
        .lines()
        .find_map(|line| {
            line.split('`')
                .nth(1)
                .filter(|c| c.starts_with("skillwright bump"))
        })
        .unwrap_or_else(|| panic!("no bump advice: {}", status.stderr))
        .to_owned();
    let mut argv: Vec<&str> = advised.split(' ').skip(1).collect();
    argv.extend(["--config", &config, "--yes"]);
    let bumped = run(&argv, dir.path());
    assert_eq!(bumped.code, 0, "{advised}: {}", bumped.stderr);
    let text = fs::read_to_string(&pack).unwrap();
    assert!(text.contains("version: 1.0.1"), "{text}");
    let other = vault.join("marketplaces/one/plugins/core-tools/core-tools.md");
    assert!(
        fs::read_to_string(other)
            .unwrap()
            .contains("version: 1.0.0")
    );
}

/// `skillwright.md` directly in a kind folder is a reserved name: refused by
/// name, never read as the root file or as a skill named after its folder.
#[test]
fn a_flat_skillwright_md_note_is_refused_as_a_reserved_name() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("skillwright.md"),
        "---\nname: home\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    for folder in ["skills", "plugins/p/skills"] {
        fs::create_dir_all(root.join(folder)).unwrap();
        fs::write(
            root.join(folder).join("skillwright.md"),
            "---\ndescription: Flat skill named skillwright.\n---\n\nX.\n",
        )
        .unwrap();
    }
    fs::write(
        root.join("skills/ok.md"),
        "---\ndescription: Ok.\n---\n\nY.\n",
    )
    .unwrap();

    for cwd in [
        root.to_path_buf(),
        root.join("skills"),
        root.join("plugins/p/skills"),
    ] {
        let list = run(&["list"], &cwd);
        assert!(list.stdout.contains("skills/ok.md"), "{}", list.stdout);
        assert!(!list.stdout.contains("Flat skill"), "{}", list.stdout);
        let skipped = list.stderr.matches("skipped · reserved-name").count();
        assert_eq!(skipped, 2, "{}", list.stderr);
        let build = run(&["build", "--yes"], &cwd);
        assert_ne!(build.code, 0, "{}", build.stderr);
        assert!(build.stderr.contains("reserved-name"), "{}", build.stderr);
        assert!(
            !build.stderr.contains("No output folder"),
            "{}",
            build.stderr
        );
    }
    assert!(
        !root
            .join("dist/.claude-plugin/packs/home/skills/skills")
            .exists()
    );
}

/// An unknown key in a skill's harness block warns and is left out; a known
/// key with a wrong value is still refused.
#[test]
fn an_unknown_key_in_a_skills_harness_block_warns() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("skillwright.md"),
        "---\nname: home\nversion: 1.0.0\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("skills")).unwrap();
    let skill = root.join("skills/a.md");
    fs::write(
        &skill,
        "---\ndescription: d\nclaude:\n  frob: 1\n---\n\nX.\n",
    )
    .unwrap();

    let build = run(&["build", "--yes"], root);
    assert_eq!(build.code, 0, "{}", build.stderr);
    assert!(build.stderr.contains("`claude.frob`"), "{}", build.stderr);
    let text =
        fs::read_to_string(root.join("dist/.claude-plugin/packs/home/skills/a/SKILL.md")).unwrap();
    assert!(!text.contains("frob"), "{text}");

    fs::write(
        &skill,
        "---\ndescription: d\nclaude:\n  metadata: 1\n---\n\nX.\n",
    )
    .unwrap();
    let wrong = run(&["build", "--yes"], root);
    assert_ne!(wrong.code, 0, "{}", wrong.stderr);
}

/// `list` names a duplicate's plugin rightly, spells its notes as its rows
/// are, and warns once per note.
#[test]
fn list_names_each_duplicate_once_with_workspace_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("skillwright.md"), "---\nworkspace: true\n---\n").unwrap();
    let member = root.join("marketplaces/barn");
    fs::create_dir_all(member.join("skills")).unwrap();
    fs::write(
        member.join("skillwright.md"),
        "---\nname: barn\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    for file in ["a", "b", "c"] {
        fs::write(
            member.join(format!("skills/{file}.md")),
            "---\nname: dup\ndescription: D.\n---\n\nX.\n",
        )
        .unwrap();
    }

    let list = run(&["list"], root);
    assert_eq!(list.code, 2, "{}{}", list.stdout, list.stderr);
    let all = format!("{}{}", list.stdout, list.stderr);
    assert!(!all.contains("in no plugin"), "{all}");
    assert!(all.contains("the marketplace's own plugin"), "{all}");
    let named = "marketplaces/barn/skills/a.md, marketplaces/barn/skills/b.md, \
                 marketplaces/barn/skills/c.md";
    assert!(all.contains(named), "{all}");
    for file in ["a", "b", "c"] {
        let warned = all
            .lines()
            .filter(|l| l.starts_with(&format!("marketplaces/barn/skills/{file}.md  warning")))
            .count();
        assert_eq!(warned, 1, "{file}: {all}");
    }
}

/// `owner` is not inherited: a plugin with no `author` gets the marketplace's
/// `author`, never its `owner`.
#[test]
fn a_plugin_author_never_falls_back_to_the_owner() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("skillwright.md"),
        "---\nname: home\nversion: 1.0.0\nowner: {name: Own}\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::write(
        root.join("skills/a.md"),
        "---\ndescription: A.\n---\n\nX.\n",
    )
    .unwrap();
    let build = run(&["build", "--yes"], root);
    assert_eq!(build.code, 0, "{}", build.stderr);
    let manifest =
        fs::read_to_string(root.join("dist/.claude-plugin/packs/home/.claude-plugin/plugin.json"))
            .unwrap();
    assert!(!manifest.contains("Own"), "{manifest}");
    let market = fs::read_to_string(root.join("dist/.claude-plugin/marketplace.json")).unwrap();
    assert!(market.contains("Own"), "{market}");
}

/// A workspace member named after a kind folder, or `skillwright`, is a
/// marketplace: run from inside it, every verb acts on that member alone.
#[test]
fn a_member_named_after_a_kind_folder_is_its_own_marketplace() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("skillwright.md"), "---\nworkspace: true\n---\n").unwrap();
    let names = [
        "skills",
        "agents",
        "commands",
        "hooks",
        "plugins",
        "marketplaces",
        "skillwright",
    ];
    for name in names.iter().chain(&["barn"]) {
        let member = root.join("marketplaces").join(name);
        fs::create_dir_all(member.join("plugins/p/skills")).unwrap();
        fs::write(
            member.join("skillwright.md"),
            format!("---\nname: {name}\nbuild:\n  out: dist\n---\n"),
        )
        .unwrap();
        fs::write(
            member.join("plugins/p/p.md"),
            "---\ndescription: P.\nversion: 1.0.0\n---\n",
        )
        .unwrap();
        fs::write(
            member.join("plugins/p/skills/a.md"),
            format!("---\ndescription: A in {name}.\n---\n\nX.\n"),
        )
        .unwrap();
    }

    for name in names {
        let member = root.join("marketplaces").join(name);
        for cwd in [member.clone(), member.join("plugins/p/skills")] {
            let list = run(&["list"], &cwd);
            assert_eq!(list.code, 0, "{name}: {}{}", list.stdout, list.stderr);
            assert!(
                list.stdout.contains("plugins/p/skills/a.md"),
                "{name}: {}",
                list.stdout
            );
            assert!(!list.stdout.contains("barn"), "{name}: {}", list.stdout);
        }
        let build = run(&["build", "--yes"], &member);
        assert_eq!(build.code, 0, "{name}: {}", build.stderr);
        assert!(
            member.join("dist/.claude-plugin/marketplace.json").exists(),
            "{name}: {}",
            build.stderr
        );
        assert!(!root.join("marketplaces/barn/dist").exists(), "{name}");
        let bump = run(&["bump", "p", "--yes"], &member);
        assert_eq!(bump.code, 0, "{name}: {}", bump.stderr);
        let note = fs::read_to_string(member.join("plugins/p/p.md")).unwrap();
        assert!(note.contains("version: 1.0.1"), "{name}: {note}");
    }

    // A flat `skillwright.md` inside the `skills` member is still reserved,
    // and running from its folder still finds the member.
    let inner = root.join("marketplaces/skills/skills");
    fs::create_dir_all(&inner).unwrap();
    fs::write(
        inner.join("skillwright.md"),
        "---\ndescription: Flat.\n---\n\nY.\n",
    )
    .unwrap();
    let list = run(&["list"], &inner);
    assert!(
        list.stdout.contains("plugins/p/skills/a.md"),
        "{}",
        list.stdout
    );
    assert!(!list.stdout.contains("barn"), "{}", list.stdout);
    assert_eq!(
        list.stderr.matches("skipped · reserved-name").count(),
        1,
        "{}",
        list.stderr
    );
}

/// In a vault that is not a workspace, `marketplaces/skills/skillwright.md` is
/// a marketplace's note, not a reserved name. A flat `skillwright.md` inside
/// that marketplace's `skills/` still is.
#[test]
fn a_nested_marketplace_named_after_a_kind_folder_is_not_reserved() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("skillwright.md"),
        "---\nname: home\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    let market = root.join("marketplaces/skills");
    fs::create_dir_all(market.join("skills")).unwrap();
    fs::write(market.join("skillwright.md"), "---\nname: skills\n---\n").unwrap();
    fs::write(
        market.join("skills/a.md"),
        "---\ndescription: A.\n---\n\nX.\n",
    )
    .unwrap();

    let list = run(&["list"], root);
    assert!(!list.stderr.contains("reserved-name"), "{}", list.stderr);
    assert!(
        list.stdout.contains("marketplaces/skills/skills/a.md"),
        "{}",
        list.stdout
    );
    let build = run(&["build", "--marketplace", "skills", "--yes"], root);
    assert_eq!(build.code, 0, "{}", build.stderr);

    fs::write(
        market.join("skills/skillwright.md"),
        "---\ndescription: Flat.\n---\n\nY.\n",
    )
    .unwrap();
    let list = run(&["list"], root);
    assert_eq!(
        list.stderr.matches("skipped · reserved-name").count(),
        1,
        "{}",
        list.stderr
    );
    assert!(
        list.stderr
            .contains("marketplaces/skills/skills/skillwright.md"),
        "{}",
        list.stderr
    );
}
