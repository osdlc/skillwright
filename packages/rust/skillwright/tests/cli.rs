//! argv → exit-code mapping.
//!
//! Every case drives [`skillwright::cli::run_with`] in-process, with the
//! working directory on the context rather than the process — so these run in
//! parallel without changing the process's directory.

mod common;

use common::{Run, run, vault_str};

use std::fs;
use std::path::{Path, PathBuf};

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

// --- the usage line ----------------------------------------------------------

#[test]
fn no_argv_is_the_usage_line_and_exit_3() {
    let dir = tempfile::tempdir().unwrap();
    let run = run(&[], dir.path());
    assert_eq!(run.code, 3);
    assert_eq!(run.stdout, "");
    assert_eq!(
        run.stderr,
        "usage: skillwright <init|build|check|bump> [options]\n"
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
    let commands = run
        .stdout
        .split("Commands:")
        .nth(1)
        .and_then(|rest| rest.split("Options:").next())
        .unwrap_or_else(|| panic!("no command list in {}", run.stdout));
    let verbs: Vec<&str> = commands
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    assert_eq!(verbs, ["init", "build", "check", "bump"], "{}", run.stdout);
}

// --- the four commands -------------------------------------------------------

/// Every command v2 dropped, as a user would type it.
const REMOVED_COMMANDS: &[&[&str]] = &[
    &["publish"],
    &["harness", "install", "claude"],
    &["status"],
    &["list"],
    &["adopt"],
    &["author"],
    &["watch"],
    &["unpublish", "note.md"],
    &["new", "skill", "x"],
    &["rename", "a", "b"],
    &["move", "a"],
    &["delete", "a"],
    &["convert", "a"],
];

#[test]
fn a_removed_command_is_an_unknown_command_with_no_removed_hint() {
    let dir = tempfile::tempdir().unwrap();
    for argv in REMOVED_COMMANDS {
        let ran = run(argv, dir.path());
        assert_eq!(ran.code, 3, "{argv:?}: {}", ran.stderr);
        assert!(
            ran.stderr.contains("unrecognized subcommand"),
            "{argv:?}: {}",
            ran.stderr
        );
        assert!(
            !ran.stderr.to_lowercase().contains("removed"),
            "{argv:?}: {}",
            ran.stderr
        );
    }
}

#[test]
fn watch_on_build_and_dry_run_or_yes_on_check_are_unknown_flags() {
    let dir = tempfile::tempdir().unwrap();
    for argv in [
        &["build", "--watch"][..],
        &["check", "--dry-run"],
        &["check", "--yes"],
    ] {
        let ran = run(argv, dir.path());
        assert_eq!(ran.code, 3, "{argv:?}: {}", ran.stderr);
        assert!(
            ran.stderr.contains("unexpected argument"),
            "{argv:?}: {}",
            ran.stderr
        );
    }
}

#[test]
fn destinations_and_scan_locations_in_the_root_file_are_refused() {
    for key in ["destinations", "scan_locations"] {
        let (_dir, vault) = fixture();
        let value = if key == "destinations" {
            "\n    - {root: dest, target: claude}"
        } else {
            " [shared]"
        };
        fs::write(
            vault.join("skillwright.md"),
            format!("---\nname: house\nbuild:\n  out: dist\n  {key}:{value}\n---\n"),
        )
        .unwrap();
        let ran = run(&["check"], &vault);
        assert_eq!(ran.code, 2, "{key}: {}", ran.stderr);
        assert!(
            ran.stderr.contains(&format!("`build.{key}`")),
            "{key}: {}",
            ran.stderr
        );
        assert!(!ran.stderr.contains("warning:"), "{key}: {}", ran.stderr);
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
    for noun in ["vault", "marketplace", "target"] {
        assert!(run.stdout.contains(noun));
    }
}

// --- the per-verb global sets ------------------------------------------------

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

// --- init, build and bump together -------------------------------------------

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
    let run = run(&["build", "--nope"], dir.path());
    assert_eq!(run.code, 3);
    assert!(run.stderr.contains("--nope"), "{}", run.stderr);
}

#[test]
fn an_unknown_verb_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(run(&["frobnicate"], dir.path()).code, 3);
}

// --- finding the vault and reading the root file -----------------------------

#[test]
fn check_with_no_root_file_and_no_convention_folder_is_exit_4() {
    let dir = tempfile::tempdir().unwrap();
    let run = run(&["check"], dir.path());
    assert_eq!(run.code, 4);
    assert!(run.stderr.contains("skillwright.md"), "{}", run.stderr);
}

#[test]
fn check_with_a_config_that_names_no_file_is_exit_4() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nothing-here.md");
    let run = run(
        &["check", "--config", &missing.display().to_string()],
        dir.path(),
    );
    assert_eq!(run.code, 4);
    assert!(run.stderr.contains("not there"), "{}", run.stderr);
}

#[test]
fn check_over_a_vault_that_is_not_there_is_exit_2() {
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("no-vault");
    let run = run(
        &["check", "--vault", &absent.display().to_string()],
        dir.path(),
    );
    assert_eq!(run.code, 2);
    assert_eq!(run.stdout, "");
    assert!(run.stderr.contains("vault not found"), "{}", run.stderr);
}

#[test]
fn a_root_file_above_the_working_directory_marks_the_vault() {
    let (_dir, vault) = fixture();
    fs::write(
        vault.join("skillwright.md"),
        "---\nname: house\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    let run = run(&["build", "--yes", "--quiet"], &vault.join("skills"));
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(vault.join("dist").is_dir());
    assert_eq!(run.stderr.matches("note:").count(), 0, "{}", run.stderr);
}

#[test]
fn an_unknown_root_file_key_is_a_warning_and_the_run_goes_on() {
    let (_dir, vault) = fixture();
    fs::write(
        vault.join("skillwright.md"),
        "---\nname: house\nflavour: mint\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    let run = run(&["check"], &vault);
    // Nothing is built yet, so `check` reports drift and goes on.
    assert_eq!(run.code, 1, "{}", run.stderr);
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
    let run = run(&["check"], &vault);
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

// --- helpers for the build and check cases -----------------------------------

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

// --- assets -------------------------------------------------

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

// --- a note with a `name` and no `description` ----------------------------------

/// A vault holding a name-only skill, a described one, and a root file.
fn name_only_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, frontmatter) in [("quiet", "name: quiet"), ("said", "description: Says.")] {
        let skill = dir.path().join("skills").join(name);
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join(format!("{name}.md")),
            format!("---\n{frontmatter}\n---\nBody.\n"),
        )
        .unwrap();
    }
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nname: demo\nbuild:\n  out: out\n---\n",
    )
    .unwrap();
    dir
}

#[test]
fn build_and_check_refuse_a_missing_description() {
    let dir = name_only_project();
    for argv in [&["build", "--yes"][..], &["check"][..]] {
        let refused = run(argv, dir.path());
        assert_eq!(refused.code, 2, "{argv:?}: {}", refused.stderr);
        assert!(
            refused.stderr.contains("missing-description")
                && refused.stderr.contains("skills/quiet/quiet.md"),
            "{argv:?}: {}",
            refused.stderr
        );
    }
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

/// `cursor.naming` is a key Cursor's block reads on a marketplace and a
/// plugin; Claude's and Codex's blocks do not read `naming`.
#[test]
fn check_accepts_cursor_naming() {
    let checked = |harness: &str| {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let plugin = vault.join("plugins/tools");
        fs::create_dir_all(plugin.join("skills")).unwrap();
        fs::write(
            vault.join("skillwright.md"),
            format!(
                "---\nname: house\nowner: acme\n{harness}:\n  naming: prefixed\nbuild:\n  out: out\n---\n"
            ),
        )
        .unwrap();
        fs::write(
            plugin.join("tools.md"),
            format!("---\nname: tools\nversion: 1.0.0\n{harness}:\n  naming: bare\n---\n"),
        )
        .unwrap();
        fs::write(
            plugin.join("skills/review.md"),
            "---\nname: review\ndescription: Use review.\n---\n\n# review\n",
        )
        .unwrap();
        let built = run(&["build", "--yes"], &vault);
        assert_eq!(built.code, 0, "{}", built.stderr);
        let check = run(&["check"], &vault);
        assert_eq!(check.code, 0, "{}{}", check.stdout, check.stderr);
        format!("{}{}", check.stdout, check.stderr)
    };
    let cursor = checked("cursor");
    assert!(!cursor.contains("cursor.naming"), "{cursor}");
    for harness in ["claude", "codex"] {
        let out = checked(harness);
        assert!(out.contains(&format!("{harness}.naming")), "{out}");
    }
}
