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

/// Write `files` under `root`, each a path and its text.
fn write_all(root: &Path, files: &[(&str, &str)]) {
    for (rel, text) in files {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

/// The notes of plugin `p` in marketplace `m`, with one skill `review`.
fn plugin_files(m: &str, p: &str) -> Vec<(String, String)> {
    vec![
        (format!("marketplaces/{m}/{m}.md"), "---\n---\n".to_owned()),
        (
            format!("marketplaces/{m}/plugins/{p}/{p}.md"),
            "---\ndescription: A plugin.\nversion: 1.0.0\n---\n".to_owned(),
        ),
        (
            format!("marketplaces/{m}/plugins/{p}/skills/review/review.md"),
            "---\ndescription: Reviews code.\nlicense: MIT\n---\nReview the diff.\n".to_owned(),
        ),
    ]
}

/// A v2 vault in `<temp>/vault`: `root` as its root file, marketplace `acme`
/// holding plugin `tools` with one skill, `review`.
fn fixture_with(root: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    write_all(&vault, &[("skillwright.md", root)]);
    for (rel, text) in plugin_files("acme", "tools") {
        write_all(&vault, &[(&rel, &text)]);
    }
    (dir, vault)
}

fn fixture() -> (tempfile::TempDir, PathBuf) {
    fixture_with("---\n---\n")
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

/// A copy of the fixture vault `name` in a temporary folder, since a build
/// writes into the vault's `dist/`.
fn copied(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join(name);
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
        &vault,
    );
    (dir, vault)
}

/// A copy of `agent-skills-vault`: marketplace `house`, plugin `tools`, one
/// skill.
fn donor() -> (tempfile::TempDir, PathBuf) {
    copied("agent-skills-vault")
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

/// The section 11 report's expected layout is printed.
fn assert_expected_layout(stderr: &str) {
    assert!(stderr.contains("The expected layout:"), "{stderr}");
    assert!(stderr.contains("└── marketplaces/<m>/"), "{stderr}");
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
        let value = if key == "destinations" {
            "\n    - {root: dest, target: claude}"
        } else {
            " [shared]"
        };
        let (_dir, vault) = fixture_with(&format!("---\nbuild:\n  {key}:{value}\n---\n"));
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
    for flag in ["--vault", "--json", "--dry-run", "--yes", "--quiet"] {
        assert!(run.stdout.contains(flag), "{flag} missing");
    }
    assert!(!run.stdout.contains("--config"), "{}", run.stdout);
    // The same sentence the verb prints for it, not a second wording.
    assert!(
        run.stdout
            .contains("Vault root: the folder holding `skillwright.md` and `marketplaces/`.")
    );
    // The two a read-only verb does not declare are marked as such.
    assert!(run.stdout.contains("Only on a verb that writes."));
    for noun in ["vault", "marketplace", "target"] {
        assert!(run.stdout.contains(noun));
    }
}

// --- the per-verb global sets ------------------------------------------------

#[test]
fn a_verb_that_writes_declares_all_five() {
    let (dir, vault) = fixture();
    // `--dry-run` and `--yes` parse, and the dry run writes nothing — not
    // clap's "unexpected argument".
    let run = run(
        &[
            "build",
            "--vault",
            &vault_str(&vault),
            "--dry-run",
            "--yes",
            "--quiet",
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(!vault.join("dist").exists());
    assert!(
        !run.stderr.contains("unexpected argument"),
        "{}",
        run.stderr
    );
}

#[test]
fn removed_flags_are_unknown() {
    let (dir, vault) = fixture();
    let vault = vault_str(&vault);
    for argv in [
        &["build", "--vault", &vault, "--out", "x", "--yes"][..],
        &["check", "--vault", &vault, "--out", "x"],
        &["build", "--vault", &vault, "--skills-root", "x", "--yes"],
        &["check", "--config", "skillwright.md"],
        &["init", "acme", "tools", "--out", "dist"],
        &["init", "acme", "tools", "--name", "acme"],
        &["check", "--vault", &vault, "--namespace", "sw"],
        &["build", "--vault", &vault, "--namespace", "sw", "--yes"],
    ] {
        let ran = run(argv, dir.path());
        assert_eq!(ran.code, 3, "{argv:?}: {}", ran.stderr);
        assert!(
            ran.stderr.contains("unexpected argument"),
            "{argv:?}: {}",
            ran.stderr
        );
    }
    assert!(!dir.path().join("vault/dist").exists());
    assert!(!dir.path().join("skillwright.md").exists());
}

// --- init --------------------------------------------------------------------

#[test]
fn init_writes_the_three_notes_and_a_build_of_them_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let ran = run(&["init", "acme", "tools"], dir.path());
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    let mut written = tree(dir.path());
    written.sort();
    assert_eq!(
        written,
        [
            PathBuf::from("marketplaces/acme/acme.md"),
            PathBuf::from("marketplaces/acme/plugins/tools/tools.md"),
            PathBuf::from("skillwright.md"),
        ]
    );
    // The bare skeleton builds: each harness gets the marketplace and the
    // plugin manifests, and no component.
    let built = run(&["build", "--yes"], dir.path());
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(dir.path().join("dist/acme").is_dir());
    assert!(
        dir.path()
            .join("dist/acme/.claude-plugin/packs/tools/.claude-plugin/plugin.json")
            .is_file()
    );
    assert!(
        !dir.path()
            .join("dist/acme/.claude-plugin/packs/tools/skills")
            .exists()
    );
    let checked = run(&["check"], dir.path());
    assert_eq!(checked.code, 0, "{}", checked.stderr);
}

#[test]
fn init_refuses_a_folder_with_a_root_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("skillwright.md"),
        "---\nlicense: MIT\n---\n",
    )
    .unwrap();
    let ran = run(&["init", "acme", "tools"], dir.path());
    assert_eq!(ran.code, 1, "{}", ran.stderr);
    assert!(ran.stderr.contains("already exists"), "{}", ran.stderr);
    let text = fs::read_to_string(dir.path().join("skillwright.md")).unwrap();
    assert!(text.contains("MIT"));
    assert!(!dir.path().join("marketplaces").exists());
}

#[test]
fn init_refuses_a_bad_name() {
    for (m, p) in [
        ("Tools", "tools"),
        ("acme", "Tools"),
        ("a--b", "tools"),
        ("acme", "skills"),
        ("marketplaces", "tools"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let ran = run(&["init", m, p], dir.path());
        assert_eq!(ran.code, 3, "{m} {p}: {}", ran.stderr);
        assert!(
            tree(dir.path()).is_empty(),
            "{m} {p}: {:?}",
            tree(dir.path())
        );
    }
}

#[test]
fn init_takes_both_names() {
    let dir = tempfile::tempdir().unwrap();
    for argv in [&["init"][..], &["init", "acme"]] {
        let ran = run(argv, dir.path());
        assert_eq!(ran.code, 3, "{argv:?}: {}", ran.stderr);
    }
    assert!(tree(dir.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn init_does_not_write_through_a_dangling_link() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("elsewhere.md");
    std::os::unix::fs::symlink(&target, dir.path().join("skillwright.md")).unwrap();
    let ran = run(&["init", "acme", "tools"], dir.path());
    assert_eq!(ran.code, 1, "{}", ran.stderr);
    assert!(!target.exists());
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

/// Section 11 case 1: no root at or above the working directory. A
/// `skillwright.md` passed over is named.
#[test]
fn no_root_at_or_above_the_working_directory_is_exit_4_and_names_a_root_file_passed_over() {
    let dir = tempfile::tempdir().unwrap();
    let skill = dir.path().join("skills/skillwright");
    write_all(
        &skill,
        &[("skillwright.md", "---\ndescription: A skill.\n---\n")],
    );
    for argv in [&["check"][..], &["build", "--yes"], &["bump", "tools"]] {
        let ran = run(argv, &skill);
        assert_eq!(ran.code, 4, "{argv:?}: {}", ran.stderr);
        assert!(ran.stderr.contains("refused: layout"), "{}", ran.stderr);
        assert!(
            ran.stderr
                .contains(&skill.join("skillwright.md").display().to_string()),
            "{}",
            ran.stderr
        );
        assert_expected_layout(&ran.stderr);
    }
}

/// Appendix A1: a skill named `skillwright` is passed over on the walk up.
#[test]
fn the_walk_up_passes_a_skills_own_note_and_stops_at_the_root() {
    let (_dir, vault) = fixture();
    let skill = vault.join("marketplaces/acme/plugins/tools/skills/skillwright");
    write_all(
        &skill,
        &[("skillwright.md", "---\ndescription: A skill.\n---\n")],
    );
    let ran = run(&["build", "--yes", "--quiet"], &skill);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert!(
        vault
            .join("dist/acme/.claude-plugin/packs/tools/skills/skillwright/SKILL.md")
            .is_file()
    );
}

#[test]
fn a_vault_flag_naming_a_folder_with_no_marketplaces_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("skillwright.md"), "---\n---\n").unwrap();
    let ran = run(&["check", "--vault", &vault_str(dir.path())], dir.path());
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    assert!(ran.stderr.contains("refused: layout"), "{}", ran.stderr);
    assert!(ran.stderr.contains("`marketplaces/`"), "{}", ran.stderr);
    assert_expected_layout(&ran.stderr);
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

/// A vault not in the layout is refused before anything is read further,
/// with every case in one report and the expected layout.
#[test]
fn a_layout_error_is_exit_2_with_the_section_11_report() {
    let (_dir, vault) = fixture_with("---\nbuild:\n  out: dist\n---\n");
    write_all(
        &vault,
        &[
            ("skills/x/x.md", "---\ndescription: X.\n---\n"),
            (
                "marketplaces/acme/plugins/tools/skills/flat.md",
                "---\ndescription: F.\n---\n",
            ),
        ],
    );
    for argv in [
        &["check"][..],
        &["build", "--yes"],
        &["bump", "tools", "--yes"],
    ] {
        let ran = run(argv, &vault);
        assert_eq!(ran.code, 2, "{argv:?}: {}", ran.stderr);
        assert_eq!(ran.stdout, "", "{argv:?}");
        for said in [
            "refused: layout",
            "skills/",
            "marketplaces/acme/plugins/tools/skills/flat.md",
            "`build.out`",
        ] {
            assert!(ran.stderr.contains(said), "{argv:?}: {}", ran.stderr);
        }
        assert_expected_layout(&ran.stderr);
    }
    assert!(!vault.join("dist").exists());
}

#[test]
fn each_removed_build_key_is_refused() {
    for key in [
        "out: dist",
        "patterns: {}",
        "exclude: ['*.draft.md']",
        "namespace: sw",
    ] {
        let (_dir, vault) = fixture_with(&format!("---\nbuild:\n  {key}\n---\n"));
        let ran = run(&["check"], &vault);
        assert_eq!(ran.code, 2, "{key}: {}", ran.stderr);
        let name = key.split(':').next().unwrap();
        assert!(
            ran.stderr.contains(&format!("`build.{name}`")),
            "{key}: {}",
            ran.stderr
        );
    }
}

#[test]
fn an_unknown_root_file_key_is_a_warning_and_the_run_goes_on() {
    let (_dir, vault) = fixture_with("---\nflavour: mint\n---\n");
    let run = run(&["check"], &vault);
    // Nothing is built yet, so `check` reports drift and goes on.
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(run.stderr.contains("warning:"), "{}", run.stderr);
    assert!(run.stderr.contains("`flavour`"), "{}", run.stderr);
}

#[test]
fn a_malformed_root_file_is_a_run_level_blocker() {
    let (_dir, vault) = fixture_with("---\nbuild:\n  harnesses: claude\n---\n");
    let run = run(&["check"], &vault);
    assert_eq!(run.code, 2, "{}", run.stderr);
    assert!(run.stderr.contains("malformed-root-file"), "{}", run.stderr);
    assert!(run.stderr.contains("skillwright.md"), "{}", run.stderr);
}

/// `.obsidian/` is never read, so Obsidian's "Excluded files"
/// (`userIgnoreFilters`) do not hide a skill from a build.
#[test]
fn obsidian_excluded_files_are_not_read() {
    let (_dir, vault) = copied("excluded-vault");
    let built = run(&["build", "--yes"], &vault);
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(
        built.stdout.contains("claude/tools  written · 2 skills"),
        "{}",
        built.stdout
    );
    let skills = vault.join("dist/house/.claude-plugin/packs/tools/skills");
    assert!(skills.join("kept/SKILL.md").is_file());
    assert!(skills.join("hidden/SKILL.md").is_file());
}

/// The root file's `license` reaches every pack manifest: no note declares
/// one, so the root file is the only thing that can put it there.
#[test]
fn the_root_files_license_reaches_the_pack_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path();
    write_all(
        vault,
        &[
            ("skillwright.md", "---\nlicense: MIT\n---\n"),
            ("marketplaces/house/house.md", "---\n---\n"),
            (
                "marketplaces/house/plugins/tools/tools.md",
                "---\nversion: 1.0.0\n---\n",
            ),
            (
                "marketplaces/house/plugins/tools/skills/review/review.md",
                "---\ndescription: Reviews code.\n---\nReview.\n",
            ),
        ],
    );

    let run = run(
        &["build", "--yes"],
        &vault.join("marketplaces/house/plugins/tools/skills"),
    );
    assert_eq!(run.code, 0, "{}", run.stderr);

    let market = vault.join("dist/house");
    let manifest =
        fs::read_to_string(market.join(".claude-plugin/packs/tools/.claude-plugin/plugin.json"))
            .unwrap();
    assert!(manifest.contains(r#""license": "MIT""#), "{manifest}");
    let package = fs::read_to_string(market.join(".pi/packs/tools/package.json")).unwrap();
    assert!(package.contains(r#""license": "MIT""#), "{package}");
}

// --- output ------------------------------------------------------------------

#[test]
fn a_bad_note_in_another_marketplace_does_not_block_marketplace() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    write_all(&vault, &[("skillwright.md", "---\n---\n")]);
    for (m, p) in [("acme", "tools"), ("labs", "spike")] {
        for (rel, text) in plugin_files(m, p) {
            write_all(&vault, &[(&rel, &text)]);
        }
    }
    write_all(
        &vault,
        &[(
            "marketplaces/labs/plugins/spike/skills/broken/broken.md",
            "---\ndescription: [unclosed\n---\nBody.\n",
        )],
    );

    let one = run(&["build", "--yes", "--marketplace", "acme"], &vault);
    assert_eq!(one.code, 0, "{}", one.stderr);
    assert!(
        vault
            .join("dist/acme/.claude-plugin/marketplace.json")
            .is_file()
    );

    let all = run(&["build", "--yes"], &vault);
    assert_ne!(all.code, 0, "{}", all.stderr);
    assert!(all.stderr.contains("broken.md"), "{}", all.stderr);
}

#[test]
fn a_two_marketplace_build_writes_two_trees_and_marketplace_builds_one() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    write_all(&vault, &[("skillwright.md", "---\n---\n")]);
    for (m, p) in [("acme", "tools"), ("labs", "spike")] {
        for (rel, text) in plugin_files(m, p) {
            write_all(&vault, &[(&rel, &text)]);
        }
    }

    let one = run(&["build", "--yes", "--marketplace", "labs"], &vault);
    assert_eq!(one.code, 0, "{}", one.stderr);
    assert!(
        vault
            .join("dist/labs/.claude-plugin/marketplace.json")
            .is_file()
    );
    assert!(!vault.join("dist/acme").exists());
    let checked = run(&["check", "--marketplace", "labs"], &vault);
    assert_eq!(checked.code, 0, "{}", checked.stderr);
    // The other marketplace is not built yet: drift.
    assert_eq!(run(&["check"], &vault).code, 1);

    let both = run(&["build", "--yes"], &vault);
    assert_eq!(both.code, 0, "{}", both.stderr);
    assert!(both.stderr.contains("marketplace acme:"), "{}", both.stderr);
    assert!(both.stderr.contains("marketplace labs:"), "{}", both.stderr);
    for (m, p) in [("acme", "tools"), ("labs", "spike")] {
        let tree = vault.join("dist").join(m);
        assert!(
            tree.join(format!(".claude-plugin/packs/{p}/skills/review/SKILL.md"))
                .is_file()
        );
        let manifest = fs::read_to_string(tree.join(".claude-plugin/marketplace.json")).unwrap();
        assert!(
            manifest.contains(&format!(r#""name": "{m}""#)),
            "{manifest}"
        );
    }
    let checked = run(&["check"], &vault);
    assert_eq!(checked.code, 0, "{}", checked.stderr);
}

#[test]
fn a_build_off_a_tty_without_yes_is_a_usage_error_not_a_silent_write() {
    let (dir, vault) = fixture();
    let run = run(&["build", "--vault", &vault_str(&vault)], dir.path());
    assert_eq!(run.code, 3);
    assert!(run.stderr.contains("pass --yes"), "{}", run.stderr);
    assert!(!vault.join("dist").exists());
}

// --- build and check, over a copy of `agent-skills-vault` --------------------

/// `build` or `check` in `vault`, plus whatever a case adds.
fn market_run(verb: &str, vault: &Path, extra: &[&str]) -> Run {
    run_argv(vec![verb.to_string()], extra, vault)
}

fn build_market(vault: &Path, extra: &[&str]) -> Run {
    let mut with_yes = vec!["--yes"];
    with_yes.extend_from_slice(extra);
    market_run("build", vault, &with_yes)
}

fn check_market(vault: &Path, extra: &[&str]) -> Run {
    market_run("check", vault, extra)
}

/// Where the donor vault's one marketplace is built.
fn dist(vault: &Path) -> PathBuf {
    vault.join("dist/house")
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
fn skill_md(vault: &Path, harness: &str) -> PathBuf {
    dist(vault)
        .join(packs_of(harness))
        .join("tools/skills/workflow-checklist/SKILL.md")
}

#[test]
fn a_build_renders_the_roots_and_a_check_then_agrees_with_it() {
    let (_dir, vault) = donor();
    let built = build_market(&vault, &[]);
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
    // The marketplace's name is its folder's.
    let manifest =
        fs::read_to_string(dist(&vault).join(".claude-plugin/marketplace.json")).unwrap();
    assert!(manifest.contains(r#""name": "house""#), "{manifest}");

    let checked = check_market(&vault, &[]);
    assert_eq!(checked.code, 0, "{}", checked.stderr);
    assert!(checked.stdout.contains("claude/tools  ok · 1 skill"));
    assert!(checked.stderr.contains("7 ok"), "{}", checked.stderr);
}

#[test]
fn a_drifted_root_is_check_exit_1_and_the_next_build_heals_it() {
    let (_dir, vault) = donor();
    assert_eq!(build_market(&vault, &[]).code, 0);

    let edited = format!(
        "{}\nhand-edited\n",
        fs::read_to_string(skill_md(&vault, "claude")).unwrap()
    );
    fs::write(skill_md(&vault, "claude"), &edited).unwrap();

    let drifted = check_market(&vault, &[]);
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
        fs::read_to_string(skill_md(&vault, "claude"))
            .unwrap()
            .contains("hand-edited")
    );

    let healed = build_market(&vault, &[]);
    assert_eq!(healed.code, 0, "{}", healed.stderr);
    assert!(
        healed.stdout.contains("claude/tools  written · 1 skill"),
        "{}",
        healed.stdout
    );
    assert!(
        !fs::read_to_string(skill_md(&vault, "claude"))
            .unwrap()
            .contains("hand-edited")
    );
    assert_eq!(check_market(&vault, &[]).code, 0);
}

#[test]
fn a_plugin_the_vault_no_longer_produces_is_drift_for_check_and_gone_after_build() {
    let (_dir, vault) = donor();
    assert_eq!(build_market(&vault, &[]).code, 0);

    let ghost = dist(&vault).join(packs_of("claude")).join("ghost");
    fs::create_dir_all(&ghost).unwrap();
    fs::write(ghost.join("leftover.md"), "from a previous vault\n").unwrap();

    let drifted = check_market(&vault, &[]);
    assert_eq!(drifted.code, 1, "{}", drifted.stderr);
    assert!(drifted.stdout.contains("stale"), "{}", drifted.stdout);
    assert!(
        drifted.stdout.contains(".claude-plugin/packs/ghost"),
        "{}",
        drifted.stdout
    );
    assert!(ghost.exists(), "check wrote something");

    assert_eq!(build_market(&vault, &[]).code, 0);
    // The packs folder is generated output: what the render did not produce
    // goes with the swap.
    assert!(!ghost.exists());
}

#[test]
fn a_rebuild_keeps_what_was_authored_beside_the_packs() {
    let (_dir, vault) = donor();
    assert_eq!(build_market(&vault, &[]).code, 0);

    let mine = dist(&vault).join(".agents/skills/mine/SKILL.md");
    fs::create_dir_all(mine.parent().unwrap()).unwrap();
    fs::write(&mine, "by hand\n").unwrap();
    fs::write(skill_md(&vault, "codex"), "hand-edited\n").unwrap();

    assert_eq!(build_market(&vault, &[]).code, 0);
    assert_eq!(fs::read_to_string(&mine).unwrap(), "by hand\n");
    assert!(
        !fs::read_to_string(skill_md(&vault, "codex"))
            .unwrap()
            .contains("hand-edited")
    );
}

#[test]
fn build_dry_run_reports_what_a_build_would_do_writes_nothing_and_exits_0() {
    let (_dir, vault) = donor();
    let previewed = build_market(&vault, &["--dry-run"]);

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
    assert!(!vault.join("dist").exists());

    assert_eq!(check_market(&vault, &[]).code, 1);
}

#[test]
fn json_emits_the_cores_own_report_mode_and_all() {
    let (_dir, vault) = donor();
    let built = build_market(&vault, &["--json"]);
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

    let checked = check_market(&vault, &["--json"]);
    assert_eq!(checked.code, 0, "{}", checked.stderr);
    let report: serde_json::Value = serde_json::from_str(&checked.stdout).unwrap();
    assert_eq!(report["mode"], "check");
}

#[test]
fn a_marketplace_with_no_plugin_is_a_blocker_not_an_empty_success() {
    let (_dir, vault) = fixture();
    fs::remove_dir_all(vault.join("marketplaces/acme/plugins/tools")).unwrap();
    let run = run(&["build", "--yes"], &vault);
    assert_eq!(run.code, 2, "{}", run.stderr);
    assert!(run.stderr.contains("empty-marketplace"), "{}", run.stderr);
    assert!(!vault.join("dist").exists());
}

#[test]
fn a_marketplace_this_vault_does_not_hold_is_refused_naming_the_ones_it_does() {
    let (_dir, vault) = donor();
    // `--marketplace` selects. The fixture holds `house`, so `demo` is a typo
    // the build must not quietly honour.
    for argv in [&["build", "--yes"][..], &["check"]] {
        let run = run_argv(
            argv.iter().map(|s| (*s).to_string()).collect(),
            &["--marketplace", "demo"],
            &vault,
        );
        let verb = argv[0];
        assert_eq!(run.code, 2, "{verb}: {}", run.stderr);
        assert!(run.stderr.contains("unknown-marketplace"), "{}", run.stderr);
        assert!(run.stderr.contains("`house`"), "{}", run.stderr);
    }
    assert!(!vault.join("dist").exists());
}

#[test]
fn check_never_asks_it_writes_nothing_so_there_is_nothing_to_confirm() {
    let (_dir, vault) = donor();
    // No --yes, off a TTY — the position that makes `build` a usage error.
    let run = check_market(&vault, &[]);
    assert_eq!(run.code, 1, "{}", run.stderr); // nothing built yet, so: drift
}

// --- notes -------------------------------------------------------------------

#[test]
fn build_and_check_refuse_a_missing_description() {
    let (_dir, vault) = fixture();
    write_all(
        &vault,
        &[(
            "marketplaces/acme/plugins/tools/skills/quiet/quiet.md",
            "---\nlicense: MIT\n---\nBody.\n",
        )],
    );
    for argv in [&["build", "--yes"][..], &["check"][..]] {
        let refused = run(argv, &vault);
        assert_eq!(refused.code, 2, "{argv:?}: {}", refused.stderr);
        assert!(
            refused.stderr.contains("missing-description")
                && refused.stderr.contains("skills/quiet/quiet.md"),
            "{argv:?}: {}",
            refused.stderr
        );
    }
    assert!(!vault.join("dist").exists());
}

/// An unknown key in a skill's harness block warns and is left out; a known
/// key with a wrong value is still refused.
#[test]
fn an_unknown_key_in_a_skills_harness_block_warns() {
    let (_dir, vault) = fixture();
    let skill = vault.join("marketplaces/acme/plugins/tools/skills/review/review.md");
    fs::write(
        &skill,
        "---\ndescription: d\nclaude:\n  frob: 1\n---\n\nX.\n",
    )
    .unwrap();

    let build = run(&["build", "--yes"], &vault);
    assert_eq!(build.code, 0, "{}", build.stderr);
    assert!(build.stderr.contains("`claude.frob`"), "{}", build.stderr);
    let text = fs::read_to_string(
        vault.join("dist/acme/.claude-plugin/packs/tools/skills/review/SKILL.md"),
    )
    .unwrap();
    assert!(!text.contains("frob"), "{text}");

    fs::write(
        &skill,
        "---\ndescription: d\nclaude:\n  metadata: 1\n---\n\nX.\n",
    )
    .unwrap();
    let wrong = run(&["build", "--yes"], &vault);
    assert_ne!(wrong.code, 0, "{}", wrong.stderr);
}

/// A prefixed key is an unknown key (section 6 rule 1): on a skill it warns
/// and ships under `metadata:`.
#[test]
fn a_prefixed_key_on_a_skill_warns_and_ships_under_metadata() {
    let (_dir, vault) = fixture();
    fs::write(
        vault.join("marketplaces/acme/plugins/tools/skills/review/review.md"),
        "---\ndescription: d\nsw-version: 2.0.0\n---\n\nX.\n",
    )
    .unwrap();
    let build = run(&["build", "--yes"], &vault);
    assert_eq!(build.code, 0, "{}", build.stderr);
    assert!(
        build.stderr.contains("unknown-key: `sw-version`"),
        "{}",
        build.stderr
    );
    let text = fs::read_to_string(
        vault.join("dist/acme/.claude-plugin/packs/tools/skills/review/SKILL.md"),
    )
    .unwrap();
    let metadata = text
        .split("metadata:")
        .nth(1)
        .unwrap_or_else(|| panic!("{text}"));
    assert!(metadata.contains("sw-version: 2.0.0"), "{text}");
}

/// On a container a prefixed key, here an `sw:` block, warns and is ignored.
#[test]
fn an_sw_block_on_a_plugin_warns_and_is_ignored() {
    let (_dir, vault) = fixture();
    fs::write(
        vault.join("marketplaces/acme/plugins/tools/tools.md"),
        "---\ndescription: A plugin.\nversion: 1.0.0\nsw:\n  version: 9.9.9\n---\n",
    )
    .unwrap();
    let build = run(&["build", "--yes"], &vault);
    assert_eq!(build.code, 0, "{}", build.stderr);
    assert!(
        build.stderr.contains("unknown-key: `sw`"),
        "{}",
        build.stderr
    );
    let manifest = fs::read_to_string(
        vault.join("dist/acme/.claude-plugin/packs/tools/.claude-plugin/plugin.json"),
    )
    .unwrap();
    assert!(manifest.contains("\"1.0.0\""), "{manifest}");
    assert!(!manifest.contains("9.9.9"), "{manifest}");
}

/// `owner` is not inherited: a plugin with no `author` never gets the
/// marketplace's `owner`.
#[test]
fn a_plugin_author_never_falls_back_to_the_owner() {
    let (_dir, vault) = fixture();
    fs::write(
        vault.join("marketplaces/acme/acme.md"),
        "---\nowner: {name: Own}\n---\n",
    )
    .unwrap();
    let build = run(&["build", "--yes"], &vault);
    assert_eq!(build.code, 0, "{}", build.stderr);
    let market = vault.join("dist/acme");
    let manifest =
        fs::read_to_string(market.join(".claude-plugin/packs/tools/.claude-plugin/plugin.json"))
            .unwrap();
    assert!(!manifest.contains("Own"), "{manifest}");
    let catalog = fs::read_to_string(market.join(".claude-plugin/marketplace.json")).unwrap();
    assert!(catalog.contains("Own"), "{catalog}");
}

/// `cursor.naming` is a key Cursor's block reads on a marketplace and a
/// plugin; Claude's and Codex's blocks do not read `naming`.
#[test]
fn check_accepts_cursor_naming() {
    let checked = |harness: &str| {
        let (_dir, vault) = fixture_with(&format!("---\n{harness}:\n  naming: prefixed\n---\n"));
        fs::write(
            vault.join("marketplaces/acme/plugins/tools/tools.md"),
            format!("---\nversion: 1.0.0\n{harness}:\n  naming: bare\n---\n"),
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
