//! What a user meets following the docs, run through the binary: leftovers of
//! the old shape, workspaces, and the verbs that must agree with a build.

use std::path::{Path, PathBuf};
use std::process::Command;

const SKILL: &str = "---\ndescription: Does a thing.\n---\nBody.\n";
const PLUGIN: &str = "---\ndescription: A plugin.\nversion: 1.2.3\n---\n";
const ROOT: &str = "---\nname: v\nbuild:\n  out: dist\n---\n";

struct Ran {
    code: i32,
    text: String,
}

/// A vault in `<temp>/vault`, with a home beside it.
struct Fixture(tempfile::TempDir);

impl Fixture {
    fn path(&self) -> PathBuf {
        self.0.path().join("vault")
    }

    fn home(&self) -> PathBuf {
        self.0.path().join("home")
    }
}

fn vault(files: &[(&str, &str)]) -> Fixture {
    let dir = Fixture(tempfile::tempdir().unwrap());
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

/// Runs in `cwd`, a folder of a fixture's vault, with `$HOME` beside the vault.
fn sw(cwd: &Path, args: &[&str]) -> Ran {
    let vault = cwd
        .ancestors()
        .find(|folder| folder.ends_with("vault"))
        .unwrap();
    let home = vault.with_file_name("home");
    std::fs::create_dir_all(&home).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_skillwright"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", &home)
        .output()
        .unwrap();
    Ran {
        code: output.status.code().unwrap_or(-1),
        text: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

#[test]
fn an_old_prefixed_key_on_a_bare_vault_is_refused_naming_its_new_home() {
    for (key, home) in [
        ("sw-version: 1.0.0", "`version`"),
        ("sw-targets: [claude]", "`targets`"),
        ("sw-source: x", "`adopted.source`"),
        ("sw-display-name: X", "`display_name`"),
        ("sw:\n  version: 1.0.0", "`version`"),
    ] {
        let note = format!("---\ndescription: D.\n{key}\n---\nBody.\n");
        let dir = vault(&[("skillwright.md", ROOT), ("skills/s.md", &note)]);
        let built = sw(&dir.path(), &["build", "--yes"]);
        assert_ne!(built.code, 0, "{key}: {}", built.text);
        for said in [home, "`metadata:`", "docs/guide/migrating.md"] {
            assert!(built.text.contains(said), "{key}: {}", built.text);
        }
        let moved = key.starts_with("sw-source");
        assert_eq!(
            built.text.contains("build.namespace"),
            !moved,
            "{}",
            built.text
        );
        let listed = sw(&dir.path(), &["list"]);
        assert!(listed.text.contains("removed-key"), "{}", listed.text);
    }
}

#[test]
fn a_prefixed_key_that_matches_nothing_stays_a_warning() {
    for key in ["sw-nonsense: 1", "tc-license: MIT"] {
        let note = format!("---\ndescription: D.\n{key}\n---\nBody.\n");
        let dir = vault(&[("skillwright.md", ROOT), ("skills/s.md", &note)]);
        let built = sw(&dir.path(), &["build", "--yes"]);
        assert_eq!(built.code, 0, "{}", built.text);
        assert!(built.text.contains("unknown-key"), "{}", built.text);
    }
}

#[test]
fn fix_adopt_hashes_says_when_there_is_nothing_to_do() {
    let dir = vault(&[("skillwright.md", ROOT), ("skills/s.md", SKILL)]);
    let ran = sw(&dir.path(), &["check", "--fix-adopt-hashes", "--yes"]);
    assert_eq!(ran.code, 0, "{}", ran.text);
    assert!(
        ran.text.contains("no adopt hash to refresh"),
        "{}",
        ran.text
    );
}

#[test]
fn a_namespace_keeps_the_prefix_and_names_the_right_spelling() {
    let root = "---\nname: v\nbuild:\n  out: dist\n  namespace: sw\n---\n";
    let plugin = "---\ndescription: P.\nsw-version: 2.0.0\nsw-display-name: X\n---\n";
    let dir = vault(&[
        ("skillwright.md", root),
        ("plugins/p/p.md", plugin),
        ("plugins/p/skills/s.md", SKILL),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    assert!(
        built
            .text
            .contains("`sw-display-name` is `sw-display_name`"),
        "{}",
        built.text
    );
}

#[test]
fn a_leftover_old_config_says_it_is_no_longer_read() {
    for old in ["skillwright.yaml", "tradecraft.yaml"] {
        let alone = vault(&[(old, "vault: .\n")]);
        let ran = sw(&alone.path(), &["list"]);
        let beside = vault(&[(old, "vault: .\n"), ("skillwright.md", ROOT)]);
        let listed = sw(&beside.path(), &["list"]);
        for text in [ran.text, listed.text] {
            assert!(text.contains("no longer read"), "{text}");
            assert!(text.contains("docs/guide/migrating.md"), "{text}");
        }
    }
}

#[test]
fn publish_writes_each_skill_once_and_no_manifest() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", PLUGIN),
        ("plugins/p/skills/s.md", SKILL),
        ("skills/t.md", SKILL),
    ]);
    let published = sw(&dir.path(), &["publish", "--yes"]);
    assert_eq!(published.code, 0, "{}", published.text);
    let root = dir.home().join(".claude/skills");
    let mut tree: Vec<String> = files_under(&root)
        .iter()
        .map(|path| path.strip_prefix(&root).unwrap().display().to_string())
        .collect();
    tree.sort();
    assert_eq!(tree, ["s/SKILL.md", "t/SKILL.md"]);
    assert!(!dir.home().join(".pi/agent/skills/p").exists());
}

#[test]
fn status_check_passes_a_kind_a_harness_cannot_carry() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", PLUGIN),
        ("plugins/p/skills/s.md", SKILL),
        ("plugins/p/agents/a.md", SKILL),
    ]);
    assert_eq!(sw(&dir.path(), &["publish", "--yes"]).code, 0);
    let status = sw(&dir.path(), &["status", "--check"]);
    assert_eq!(status.code, 0, "{}", status.text);
    assert!(status.text.contains("no-renderer"), "{}", status.text);
}

#[test]
fn folders_in_and_beside_a_skill_folder_do_what_the_spec_says() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", PLUGIN),
        ("plugins/p/skills/docs/z.md", SKILL),
        ("plugins/p/skills/b/b.md", SKILL),
        ("plugins/p/skills/b/peer.md", SKILL),
        ("plugins/p/skills/b/docs/inner.md", SKILL),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    assert!(built.text.contains("never-ships"), "{}", built.text);
    let skill = dir.path().join("dist/.claude-plugin/packs/p/skills/b");
    assert!(skill.join("peer.md").is_file());
    assert!(!skill.join("docs").exists());
}

#[test]
fn a_note_with_a_typo_for_every_component_key_is_named() {
    let note = "---\ndesciption: D.\n---\nBody.\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("skills/s.md", note),
        ("skills/t.md", SKILL),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert!(built.text.contains("no-component-keys"), "{}", built.text);
    let listed = sw(&dir.path(), &["list"]);
    assert!(listed.text.contains("no-component-keys"), "{}", listed.text);
}

const WORKSPACE: &str = "---\nworkspace: true\n---\n";

fn member(name: &str, extra: &str) -> String {
    format!("---\nname: {name}\nbuild:\n  out: built\n{extra}---\n")
}

#[test]
fn list_at_a_workspace_root_reads_each_member_as_a_build_does() {
    let one = member(
        "one",
        "  exclude: skills/secret.md\n  patterns:\n    skills: notes/*\n",
    );
    let two = member("two", "  namespace: acme\n");
    let dir = vault(&[
        ("skillwright.md", WORKSPACE),
        ("marketplaces/one/skillwright.md", &one),
        ("marketplaces/one/skills/secret.md", SKILL),
        ("marketplaces/one/skills/open.md", SKILL),
        ("marketplaces/one/notes/n.md", SKILL),
        ("marketplaces/two/skillwright.md", &two),
        (
            "marketplaces/two/skills/k.md",
            "---\ndescription: D.\nacme-targets: [claude]\n---\nB.\n",
        ),
    ]);
    let listed = sw(&dir.path(), &["list"]);
    assert_eq!(listed.code, 0, "{}", listed.text);
    assert!(!listed.text.contains("secret"), "{}", listed.text);
    assert!(!listed.text.contains("skipped"), "{}", listed.text);
    assert!(!listed.text.contains("unknown-key"), "{}", listed.text);
    assert!(
        listed.text.contains("marketplaces/one/notes/n.md"),
        "{}",
        listed.text
    );
}

#[test]
fn list_at_a_workspace_root_reads_inherited_patterns() {
    let shared = "---\nworkspace: true\nbuild:\n  patterns:\n    skills: notes/*\n---\n";
    let dir = vault(&[
        ("skillwright.md", shared),
        ("marketplaces/one/skillwright.md", &member("one", "")),
        ("marketplaces/one/notes/n.md", SKILL),
    ]);
    let listed = sw(&dir.path(), &["list"]);
    assert!(!listed.text.contains("skipped"), "{}", listed.text);
    assert!(listed.text.contains("notes/n.md"), "{}", listed.text);
}

#[test]
fn a_workspace_warns_about_what_builds_nowhere() {
    let shared = "---\nworkspace: true\nbuild:\n  out: dist\n---\n";
    let dir = vault(&[
        ("skillwright.md", shared),
        ("marketplaces/one/skillwright.md", &member("one", "")),
        ("marketplaces/one/skills/s.md", SKILL),
        ("skills/top.md", SKILL),
        ("plugins/p/skills/q.md", SKILL),
    ]);
    for args in [&["build", "--yes"][..], &["list"][..]] {
        let ran = sw(&dir.path(), args);
        assert_eq!(ran.code, 0, "{}", ran.text);
        assert_eq!(
            ran.text.matches("outside-marketplaces").count(),
            1,
            "{}",
            ran.text
        );
        assert!(ran.text.contains("`plugins/`, `skills/`"), "{}", ran.text);
        assert!(ran.text.contains("workspace-out"), "{}", ran.text);
    }
}

#[test]
fn an_inherited_glob_warns_only_when_no_member_matches_it() {
    let shared =
        "---\nworkspace: true\nbuild:\n  exclude: ['skills/a.md', 'skills/gone.md']\n---\n";
    let dir = vault(&[
        ("skillwright.md", shared),
        ("marketplaces/one/skillwright.md", &member("one", "")),
        ("marketplaces/one/skills/a.md", SKILL),
        ("marketplaces/one/skills/s.md", SKILL),
        ("marketplaces/two/skillwright.md", &member("two", "")),
        ("marketplaces/two/skills/s.md", SKILL),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    assert_eq!(
        built.text.matches("`skills/gone.md`").count(),
        2,
        "{}",
        built.text
    );
    assert!(!built.text.contains("`skills/a.md`"), "{}", built.text);
}

#[test]
fn a_missing_asset_says_where_it_was_read_from() {
    let shared =
        "---\nworkspace: true\nbuild:\n  assets:\n    - {from: shared/N, to: N, plugin: p}\n---\n";
    let dir = vault(&[
        ("skillwright.md", shared),
        ("shared/N", "x\n"),
        ("marketplaces/m/skillwright.md", &member("m", "")),
        ("marketplaces/m/plugins/p/skills/s.md", SKILL),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_ne!(built.code, 0, "{}", built.text);
    assert!(built.text.contains("missing-asset"), "{}", built.text);
    assert!(
        built.text.contains("marketplace's own folder"),
        "{}",
        built.text
    );
}

#[test]
fn manifests_keep_to_what_each_harness_documents() {
    let root = "---\nname: v\nowner: {name: O, url: 'https://o.example'}\n\
                author: {name: A, url: 'https://a.example'}\nbuild:\n  out: dist\n---\n";
    let dir = vault(&[
        ("skillwright.md", root),
        ("plugins/p/p.md", PLUGIN),
        ("plugins/p/skills/s.md", SKILL),
    ]);
    assert_eq!(sw(&dir.path(), &["build", "--yes"]).code, 0);
    let read = |path: &str| std::fs::read_to_string(dir.path().join("dist").join(path)).unwrap();
    for path in [
        ".cursor-plugin/marketplace.json",
        ".cursor-plugin/packs/p/.cursor-plugin/plugin.json",
    ] {
        assert!(!read(path).contains("example"), "{path}: {}", read(path));
    }
    assert!(read(".cursor-plugin/marketplace.json").contains("1.2.3"));
    assert!(!read(".claude-plugin/marketplace.json").contains("1.2.3"));
    assert!(read(".claude-plugin/packs/p/.claude-plugin/plugin.json").contains("1.2.3"));
}

#[test]
fn a_marketplace_named_apart_from_its_folder_keeps_its_packs() {
    let note = "---\nname: bar\nclaude: {packs: alt}\n---\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("marketplaces/foo/skillwright.md", note),
        ("marketplaces/foo/plugins/p/skills/s.md", SKILL),
    ]);
    for args in [
        &["build", "--yes"][..],
        &["build", "--yes"][..],
        &["build", "--yes", "--marketplace", "foo"][..],
        &["check"][..],
    ] {
        let ran = sw(&dir.path(), args);
        assert_eq!(ran.code, 0, "{args:?}: {}", ran.text);
    }
    assert!(dir.path().join("dist/alt/p").is_dir());
}

#[test]
fn a_marketplace_renames_and_builds() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("marketplaces/foo/skillwright.md", "---\nname: foo\n---\n"),
        ("marketplaces/foo/plugins/p/skills/s.md", SKILL),
    ]);
    let renamed = sw(&dir.path(), &["rename", "foo", "bar", "--yes"]);
    assert_eq!(renamed.code, 0, "{}", renamed.text);
    let note = std::fs::read_to_string(dir.path().join("marketplaces/bar/skillwright.md")).unwrap();
    assert!(note.contains("name: bar"), "{note}");
    assert!(!dir.path().join("marketplaces/foo").exists());
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
}

#[test]
fn convert_writes_the_marketplace_form_and_reads_it_back() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", PLUGIN),
        ("plugins/p/skills/s.md", SKILL),
        ("skills/z.md", "---\nname: z\ndescription: D.\n---\nB.\n"),
    ]);
    let converted = sw(
        &dir.path(),
        &["convert", "z", "--kind", "marketplace", "--yes"],
    );
    assert_eq!(converted.code, 0, "{}", converted.text);
    assert!(dir.path().join("marketplaces/z/skillwright.md").is_file());
    assert!(!dir.path().join("marketplaces/z/z.md").exists());
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);

    let back = sw(&dir.path(), &["convert", "z", "--kind", "plugin", "--yes"]);
    assert_eq!(back.code, 0, "{}", back.text);
    assert!(dir.path().join("plugins/z/z.md").is_file());
    assert!(!dir.path().join("marketplaces/z").exists());
}

#[test]
fn a_malformed_workspace_file_refuses_every_verb_in_a_member() {
    let dir = vault(&[
        ("skillwright.md", "---\nworkspace: [\n---\n"),
        ("marketplaces/m/skillwright.md", &member("m", "")),
        ("marketplaces/m/skills/secret.md", SKILL),
    ]);
    let inside = dir.path().join("marketplaces/m");
    for args in [
        &["list"][..],
        &["status"][..],
        &["publish", "--yes"][..],
        &["publish", "skills/secret.md", "--yes"][..],
        &["build", "--yes"][..],
    ] {
        let ran = sw(&inside, args);
        assert_ne!(ran.code, 0, "{args:?}: {}", ran.text);
        assert!(ran.text.contains("skillwright.md"), "{}", ran.text);
    }
    assert!(!dir.home().join(".claude/skills/secret").exists());
}

#[test]
fn the_old_marketplace_note_names_the_new_path() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("marketplaces/m/m.md", "---\nlicense: GPL-3.0\n---\n"),
        ("marketplaces/m/plugins/p/skills/s.md", SKILL),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_ne!(built.code, 0, "{}", built.text);
    assert!(
        built.text.contains("marketplaces/m/skillwright.md"),
        "{}",
        built.text
    );
}

#[test]
fn new_marketplace_runs_only_at_the_workspace_root() {
    let dir = vault(&[
        ("skillwright.md", WORKSPACE),
        ("marketplaces/one/skillwright.md", &member("one", "")),
    ]);
    let inside = dir.path().join("marketplaces/one");
    let ran = sw(
        &inside,
        &["new", "marketplace", "three", "--description", "D", "--yes"],
    );
    assert_ne!(ran.code, 0, "{}", ran.text);
    assert!(!inside.join("marketplaces").exists());
}

#[test]
fn the_version_hint_names_the_plugin_s_real_folder() {
    let root = "---\nname: v\nbuild:\n  out: dist\n  patterns:\n    plugins: packs/*\n---\n";
    let dir = vault(&[("skillwright.md", root), ("packs/rev/skills/s.md", SKILL)]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert!(built.text.contains("packs/rev/rev.md"), "{}", built.text);
    let bumped = sw(&dir.path(), &["bump", "rev", "--yes"]);
    assert!(bumped.text.contains("packs/rev/rev.md"), "{}", bumped.text);
}

#[test]
fn block_metadata_on_a_container_must_be_a_mapping() {
    let root = "---\nname: v\nclaude: {metadata: 5}\nbuild:\n  out: dist\n---\n";
    let plugin = "---\ndescription: P.\nclaude: {metadata: 5}\n---\n";
    for (root, plugin) in [(root, PLUGIN), (ROOT, plugin)] {
        let dir = vault(&[
            ("skillwright.md", root),
            ("plugins/p/p.md", plugin),
            ("plugins/p/skills/s.md", SKILL),
        ]);
        let built = sw(&dir.path(), &["build", "--yes"]);
        assert_ne!(built.code, 0, "{}", built.text);
        assert!(built.text.contains("must be a mapping"), "{}", built.text);
    }
}

#[test]
fn a_link_into_a_plugin_s_assets_ships() {
    let linking = "---\ndescription: D.\n---\nSee [guide](../assets/docs/guide.md).\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", PLUGIN),
        ("plugins/p/assets/docs/guide.md", "# Guide\n"),
        ("plugins/p/skills/s.md", linking),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    assert!(
        dir.path()
            .join("dist/.claude-plugin/packs/p/docs/guide.md")
            .is_file()
    );
}

#[test]
fn publish_refuses_two_plugins_writing_one_skill_folder() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/skills/review.md", SKILL),
        ("plugins/q/skills/review.md", SKILL),
    ]);
    let dest = dir.home().join("dest");
    let to = dest.to_str().unwrap();
    let flags = ["--to", to, "--target", "claude", "--yes"];

    let bare = sw(&dir.path(), &[&["publish"], &flags[..]].concat());
    assert_eq!(bare.code, 2, "{}", bare.text);
    assert!(bare.text.contains("path-collision"), "{}", bare.text);
    assert!(
        bare.text.contains("plugins/p/skills/review.md"),
        "{}",
        bare.text
    );
    assert!(
        bare.text.contains("plugins/q/skills/review.md"),
        "{}",
        bare.text
    );
    assert!(!dest.exists(), "nothing is written");

    let both = [
        "publish",
        "plugins/p/skills/review.md",
        "plugins/q/skills/review.md",
    ];
    let named = sw(&dir.path(), &[&both[..], &flags[..]].concat());
    assert_eq!(named.code, 2, "{}", named.text);

    let one = sw(&dir.path(), &[&both[..2], &flags[..]].concat());
    assert_eq!(one.code, 0, "{}", one.text);
    assert!(
        dest.join(".claude/skills/review/SKILL.md").exists(),
        "{}",
        one.text
    );
}

#[test]
fn publish_allows_one_skill_name_on_two_harnesses() {
    let aimed = |to: &str| format!("---\ndescription: D.\ntargets: [{to}]\n---\nBody.\n");
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/skills/review.md", &aimed("claude")),
        ("plugins/q/skills/review.md", &aimed("codex")),
    ]);
    let dest = dir.home().join("dest");
    let flags = [
        "--to",
        dest.to_str().unwrap(),
        "--target",
        "claude",
        "--yes",
    ];
    let ran = sw(&dir.path(), &[&["publish"], &flags[..]].concat());
    assert_eq!(ran.code, 0, "{}", ran.text);
}

#[test]
fn publish_of_a_plugin_note_refuses_block_metadata_that_is_not_a_mapping() {
    let note = "---\ndescription: P.\nversion: 1.0.0\nclaude:\n  metadata: 5\n---\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", note),
        ("plugins/p/skills/a.md", SKILL),
    ]);
    let dest = dir.home().join("dest");
    let ran = sw(
        &dir.path(),
        &[
            "publish",
            "plugins/p/p.md",
            "--to",
            dest.to_str().unwrap(),
            "--target",
            "claude",
            "--yes",
        ],
    );
    assert_ne!(ran.code, 0, "{}", ran.text);
    assert!(
        ran.text.contains("`claude.metadata` must be a mapping"),
        "{}",
        ran.text
    );
}

#[test]
fn a_hint_for_a_plugin_named_like_its_marketplace_gives_the_plugin_folder() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("marketplaces/rev/skillwright.md", "---\nname: rev\n---\n"),
        ("marketplaces/rev/plugins/rev/skills/a.md", SKILL),
    ]);
    let bumped = sw(&dir.path(), &["bump", "rev", "--patch", "--yes"]);
    let hint = "add `marketplaces/rev/plugins/rev/rev.md`";
    assert!(bumped.text.contains(hint), "{}", bumped.text);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert!(
        built.text.contains("marketplaces/rev/plugins/rev/rev.md"),
        "{}",
        built.text
    );
}

#[test]
fn a_leftover_old_note_in_a_workspace_member_is_refused() {
    let dir = vault(&[
        ("skillwright.md", WORKSPACE),
        ("marketplaces/foo/skillwright.md", &member("foo", "")),
        ("marketplaces/foo/foo.md", "---\nlicense: GPL-3.0\n---\n"),
        ("marketplaces/foo/plugins/p/skills/a.md", SKILL),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 2, "{}", built.text);
    assert!(
        built.text.contains("marketplaces/foo/skillwright.md"),
        "{}",
        built.text
    );
}

#[test]
fn an_old_spelling_of_a_moved_key_is_not_told_to_set_a_namespace() {
    let note = "---\ndescription: D.\nsw-adopted-hash: abc\n---\nBody.\n";
    let dir = vault(&[("skillwright.md", ROOT), ("skills/s.md", note)]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 2, "{}", built.text);
    assert!(built.text.contains("`adopted.hash`"), "{}", built.text);
    assert!(built.text.contains("`metadata:`"), "{}", built.text);
    assert!(!built.text.contains("build.namespace"), "{}", built.text);
}

#[test]
fn a_tc_kind_note_reads_as_a_skill_with_a_warning() {
    let note = "---\ndescription: D.\ntc-kind: plugin\n---\nBody.\n";
    let dir = vault(&[("skillwright.md", ROOT), ("skills/s.md", note)]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    assert!(built.text.contains("unknown-key"), "{}", built.text);
    let listed = sw(&dir.path(), &["list"]);
    assert!(listed.text.contains("skill"), "{}", listed.text);
}

#[test]
fn a_moved_out_reports_nothing_and_a_moved_packs_names_the_earlier_build() {
    let out_moved = vault(&[("skillwright.md", ROOT), ("plugins/p/skills/a.md", SKILL)]);
    assert_eq!(sw(&out_moved.path(), &["build", "--yes"]).code, 0);
    std::fs::write(
        out_moved.path().join("skillwright.md"),
        ROOT.replace("dist", "dist2"),
    )
    .unwrap();
    let built = sw(&out_moved.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    assert!(!built.text.contains("stale-output"), "{}", built.text);

    let packs =
        |to: &str| format!("---\nname: v\nclaude:\n  packs: {to}\nbuild:\n  out: dist\n---\n");
    let dir = vault(&[
        ("skillwright.md", &packs("alt")),
        ("plugins/p/skills/a.md", SKILL),
    ]);
    assert_eq!(sw(&dir.path(), &["build", "--yes"]).code, 0);
    std::fs::write(dir.path().join("skillwright.md"), packs("alt2")).unwrap();
    let stopped = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(stopped.code, 2, "{}", stopped.text);
    assert!(
        stopped.text.contains("written by an earlier build"),
        "{}",
        stopped.text
    );
    assert!(
        stopped.text.contains("delete the old folder"),
        "{}",
        stopped.text
    );
}

#[test]
fn rename_at_a_workspace_root_says_how_to_rename_a_marketplace() {
    let dir = vault(&[
        ("skillwright.md", WORKSPACE),
        ("marketplaces/foo/skillwright.md", &member("foo", "")),
        ("marketplaces/foo/plugins/p/skills/a.md", SKILL),
    ]);
    let ran = sw(&dir.path(), &["rename", "foo", "bar", "--yes"]);
    assert_eq!(ran.code, 2, "{}", ran.text);
    assert!(ran.text.contains("rename its folder"), "{}", ran.text);
}
