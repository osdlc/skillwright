//! What a user meets following the docs, run through the binary: unknown
//! keys, workspaces, and the verbs that must agree with a build.

use std::path::{Path, PathBuf};
use std::process::Command;

const SKILL: &str = "---\ndescription: Does a thing.\n---\nBody.\n";
const PLUGIN: &str = "---\ndescription: A plugin.\nversion: 1.2.3\n---\n";
const ROOT: &str = "---\nname: v\nbuild:\n  out: dist\n---\n";

struct Ran {
    code: i32,
    text: String,
}

/// A vault in `<temp>/vault`.
struct Fixture(tempfile::TempDir);

impl Fixture {
    fn path(&self) -> PathBuf {
        self.0.path().join("vault")
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
fn a_prefixed_key_on_a_bare_vault_is_an_unknown_key() {
    for key in ["sw-version: 1.0.0", "tc-license: MIT"] {
        let note = format!("---\ndescription: D.\n{key}\n---\nBody.\n");
        let dir = vault(&[("skillwright.md", ROOT), ("skills/s.md", &note)]);
        let built = sw(&dir.path(), &["build", "--yes"]);
        assert_eq!(built.code, 0, "{}", built.text);
        assert!(built.text.contains("unknown-key"), "{}", built.text);
    }
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

fn member(name: &str, extra: &str) -> String {
    format!("---\nname: {name}\nbuild:\n  out: built\n{extra}---\n")
}

/// A workspace glob written for the old, member-relative reading no longer
/// excludes, and a member's own build says how to rewrite it.
#[test]
fn a_member_build_names_a_workspace_glob_written_the_old_way() {
    let shared = "---\nworkspace: true\nbuild:\n  exclude: ['skills/a.md']\n---\n";
    let dir = vault(&[
        ("skillwright.md", shared),
        ("marketplaces/one/skillwright.md", &member("one", "")),
        ("marketplaces/one/skills/a.md", SKILL),
    ]);
    let built = sw(&dir.path().join("marketplaces/one"), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    assert!(
        built.text.contains("write `marketplaces/one/skills/a.md`"),
        "{}",
        built.text
    );
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

/// Issue #123: a plugin's Cursor display name reaches its catalog entry as
/// well as its `plugin.json`.
#[test]
fn cursor_catalog_entry_has_the_display_name() {
    let plugin = "---\ndescription: P.\nversion: 1.0.0\ncursor:\n  displayName: \"Ticket workflow (t)\"\n---\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/t/t.md", plugin),
        ("plugins/t/skills/s.md", SKILL),
    ]);
    let ran = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(ran.code, 0, "{}", ran.text);
    let json =
        |tail: &str| -> serde_json::Value { serde_json::from_str(&built(&dir, tail)).unwrap() };
    let manifest = json(".cursor-plugin/packs/t/.cursor-plugin/plugin.json");
    assert_eq!(manifest["displayName"], "Ticket workflow (t)");
    let catalog = json(".cursor-plugin/marketplace.json");
    let entry = catalog["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == "t")
        .unwrap();
    assert_eq!(entry["displayName"], "Ticket workflow (t)", "{catalog}");
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
fn a_known_block_key_with_a_wrong_value_is_refused() {
    let skill = |block: &str| format!("---\ndescription: S.\n{block}\n---\nBody.\n");
    let plugin = |block: &str| format!("---\ndescription: P.\nversion: 1.0.0\n{block}\n---\n");
    let cases = [
        (
            PLUGIN.to_owned(),
            skill("claude: {allowed-tools: 5}"),
            "`claude.allowed-tools` must be a string or a list of strings, not a number",
        ),
        (
            PLUGIN.to_owned(),
            skill("claude: {model: [1]}"),
            "`claude.model` must be a string, not a list",
        ),
        (
            plugin("codex: {policy: 5}"),
            SKILL.to_owned(),
            "`codex.policy` of plugin `p` must be a mapping, not a number",
        ),
        (
            plugin("claude: {model: [1]}"),
            SKILL.to_owned(),
            "`claude.model` of plugin `p` must be a string, not a list",
        ),
    ];
    for (plugin, skill, said) in &cases {
        let dir = vault(&[
            ("skillwright.md", ROOT),
            ("plugins/p/p.md", plugin),
            ("plugins/p/skills/s.md", skill),
        ]);
        for verb in [&["check"][..], &["build", "--yes"][..]] {
            let ran = sw(&dir.path(), verb);
            assert_eq!(ran.code, 2, "{verb:?}: {}", ran.text);
            assert!(ran.text.contains(said), "{verb:?}: {}", ran.text);
        }
        assert!(!dir.path().join("dist").exists());
    }
}

/// The text of the built file under `dist` whose path ends with `tail`.
fn built(dir: &Fixture, tail: &str) -> String {
    let file = files_under(&dir.path().join("dist"))
        .into_iter()
        .find(|path| path.to_string_lossy().ends_with(tail))
        .unwrap_or_else(|| panic!("no built {tail}"));
    std::fs::read_to_string(file).unwrap()
}

#[test]
fn a_known_block_key_with_a_right_value_ships() {
    let root = "---\nname: v\nclaude: {model: opus}\ncodex: {interface: {displayName: Vee}}\n\
                pi: {naming: bare}\nbuild:\n  out: dist\n---\n";
    let skill = "---\ndescription: S.\nclaude:\n  allowed-tools: [Read, Grep]\n  \
                 argument-hint: [issue-number]\n  user-invocable: off\n  metadata: {team: {lead: Ada}}\n---\nB.\n";
    let plugin = "---\ndescription: P.\nversion: 1.0.0\n\
                  codex: {policy: {installation: AVAILABLE}, interface: {displayName: Pea}}\n---\n";
    let dir = vault(&[
        ("skillwright.md", root),
        ("plugins/p/p.md", plugin),
        ("plugins/p/skills/s.md", skill),
    ]);
    let ran = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(ran.code, 0, "{}", ran.text);

    let claude = built(&dir, ".claude-plugin/packs/p/skills/s/SKILL.md");
    for line in [
        "allowed-tools:",
        "- Read",
        "argument-hint: \"[issue-number]\"",
        "user-invocable: false",
        "model: opus",
        "lead: Ada",
    ] {
        assert!(claude.contains(line), "{line}: {claude}");
    }
    let marketplace = built(&dir, ".agents/plugins/marketplace.json");
    assert!(
        marketplace.contains("\"displayName\": \"Vee\""),
        "{marketplace}"
    );
    assert!(
        marketplace.contains("\"installation\": \"AVAILABLE\""),
        "{marketplace}"
    );
    let manifest = built(&dir, "p/.codex-plugin/plugin.json");
    assert!(manifest.contains("\"displayName\": \"Pea\""), "{manifest}");
    assert!(
        files_under(&dir.path().join("dist"))
            .iter()
            .any(|path| path.ends_with("p/skills/s/SKILL.md")
                && path.to_string_lossy().contains(".pi")),
        "pi skill is bare"
    );
}

#[test]
fn a_block_key_the_container_does_not_read_warns_whatever_it_holds() {
    for block in [
        "claude: {owner: Ada}",
        "codex: {plugins: 5}",
        "claude: {forceRemoveDeletedPlugins: yes}",
    ] {
        let plugin = format!("---\ndescription: P.\nversion: 1.0.0\n{block}\n---\n");
        let dir = vault(&[
            ("skillwright.md", ROOT),
            ("plugins/p/p.md", &plugin),
            ("plugins/p/skills/s.md", SKILL),
        ]);
        let ran = sw(&dir.path(), &["build", "--yes"]);
        assert_eq!(ran.code, 0, "{block}: {}", ran.text);
        assert!(ran.text.contains("unknown-key"), "{block}: {}", ran.text);
        assert!(!ran.text.contains("must be"), "{block}: {}", ran.text);
    }
}

#[test]
fn a_bare_spec_key_takes_the_check_its_block_does() {
    let skill = |line: &str| format!("---\ndescription: S.\n{line}\n---\nB.\n");
    for (line, said) in [
        ("model: [1]", "`model` must be a string, not a list"),
        (
            "allowed-tools: 5",
            "`allowed-tools` must be a string or a list of strings, not a number",
        ),
    ] {
        let dir = vault(&[
            ("skillwright.md", ROOT),
            ("plugins/p/p.md", PLUGIN),
            ("plugins/p/skills/s.md", &skill(line)),
        ]);
        let ran = sw(&dir.path(), &["build", "--yes"]);
        assert_eq!(ran.code, 2, "{line}: {}", ran.text);
        assert!(ran.text.contains(said), "{line}: {}", ran.text);
    }
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", PLUGIN),
        (
            "plugins/p/skills/s.md",
            &skill("argument-hint: [issue-number]"),
        ),
    ]);
    let ran = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(ran.code, 0, "{}", ran.text);
    let claude = built(&dir, ".claude-plugin/packs/p/skills/s/SKILL.md");
    assert!(
        claude.contains("argument-hint: \"[issue-number]\""),
        "{claude}"
    );
}

#[test]
fn a_key_set_to_nothing_is_dropped_bare_and_in_a_block() {
    let plugin = "---\ndescription: P.\nversion: 1.0.0\nclaude:\n  homepage:\n---\n";
    let skill = "---\ndescription: S.\nmodel:\nclaude:\n  model:\n---\nB.\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", plugin),
        ("plugins/p/skills/s.md", skill),
    ]);
    let ran = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(ran.code, 0, "{}", ran.text);
    let claude = built(&dir, ".claude-plugin/packs/p/skills/s/SKILL.md");
    assert!(!claude.contains("model"), "{claude}");
    let manifest = built(&dir, ".claude-plugin/packs/p/.claude-plugin/plugin.json");
    assert!(!manifest.contains("homepage"), "{manifest}");
}

#[test]
fn a_word_flag_ships_as_a_boolean_to_every_harness() {
    let skill = "---\ndescription: S.\nuser-invocable: \"0\"\ncursor:\n  \
                 disable-model-invocation: yes\n---\nB.\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", PLUGIN),
        ("plugins/p/skills/s.md", skill),
    ]);
    let ran = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(ran.code, 0, "{}", ran.text);
    let cursor = built(&dir, ".cursor-plugin/packs/p/skills/s/SKILL.md");
    assert!(
        cursor.contains("disable-model-invocation: true"),
        "{cursor}"
    );
    let claude = built(&dir, ".claude-plugin/packs/p/skills/s/SKILL.md");
    assert!(claude.contains("user-invocable: false"), "{claude}");
}

#[test]
fn every_block_is_checked_whichever_harnesses_the_build_selects() {
    let root = "---\nname: v\nbuild:\n  out: dist\n  harnesses: [claude]\n---\n";
    let skill = "---\ndescription: S.\ncodex: {allowed-tools: 5}\n---\nB.\n";
    let plugin = "---\ndescription: P.\nversion: 1.0.0\ncodex: {policy: 5}\n---\n";
    for (plugin, skill, said) in [
        (PLUGIN, skill, "`codex.allowed-tools` must be a string"),
        (
            plugin,
            SKILL,
            "`codex.policy` of plugin `p` must be a mapping",
        ),
    ] {
        let dir = vault(&[
            ("skillwright.md", root),
            ("plugins/p/p.md", plugin),
            ("plugins/p/skills/s.md", skill),
        ]);
        let ran = sw(&dir.path(), &["build", "--yes"]);
        assert_eq!(ran.code, 2, "{said}: {}", ran.text);
        assert!(ran.text.contains(said), "{said}: {}", ran.text);
    }
}

#[test]
fn a_build_names_each_reason_when_the_skipped_notes_differ() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", PLUGIN),
        (
            "plugins/p/skills/a.md",
            "---\ndescription: S.\nmodel: [1]\n---\nB.\n",
        ),
        (
            "plugins/p/skills/b.md",
            "---\ndescription: [unclosed\n---\nB.\n",
        ),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 2, "{}", built.text);
    assert!(
        built.text.contains(
            "unreadable-note: the vault holds 2 note(s) this scan could not read or refused"
        ),
        "{}",
        built.text
    );
    assert!(
        built.text.contains("plugins/p/skills/a.md (malformed: "),
        "{}",
        built.text
    );
}

#[test]
fn an_agent_with_a_wrong_kind_stops_a_build() {
    let agent = "---\ndescription: A.\ntemperature: \"0.2\"\n---\nB.\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/skills/s.md", SKILL),
        ("plugins/p/agents/x.md", agent),
    ]);
    for verb in [&["check"][..], &["build", "--yes"][..]] {
        let ran = sw(&dir.path(), verb);
        assert_eq!(ran.code, 2, "{verb:?}: {}", ran.text);
        assert!(
            ran.text
                .contains("`temperature` must be a number, not a string"),
            "{verb:?}: {}",
            ran.text
        );
    }
}

#[test]
fn a_root_block_drops_nothing_and_reads_a_quoted_flag() {
    let root = "---\nname: v\nclaude:\n  homepage:\n  user-invocable: \"0\"\n\
                build:\n  out: dist\n---\n";
    let dir = vault(&[
        ("skillwright.md", root),
        ("plugins/p/p.md", PLUGIN),
        ("plugins/p/skills/s.md", SKILL),
    ]);
    let ran = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(ran.code, 0, "{}", ran.text);
    for file in files_under(&dir.path().join("dist/.claude-plugin")) {
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("homepage"), "{}: {text}", file.display());
    }
    let claude = built(&dir, ".claude-plugin/packs/p/skills/s/SKILL.md");
    assert!(claude.contains("user-invocable: false"), "{claude}");
}

#[test]
fn a_block_key_no_harness_reads_warns_when_set_to_nothing() {
    let skill = "---\ndescription: S.\nclaude:\n  nonsense:\n---\nB.\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", PLUGIN),
        ("plugins/p/skills/s.md", skill),
    ]);
    let ran = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(ran.code, 0, "{}", ran.text);
    assert!(ran.text.contains("`claude.nonsense`"), "{}", ran.text);
    let claude = built(&dir, ".claude-plugin/packs/p/skills/s/SKILL.md");
    assert!(!claude.contains("nonsense"), "{claude}");
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

/// Two bundles that each keep `references/contract.md`, each linking it bare:
/// each skill ships its own copy.
#[test]
fn a_bare_link_resolves_to_the_match_in_its_own_bundle() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p1/p1.md", PLUGIN),
        ("plugins/p2/p2.md", PLUGIN),
        (
            "plugins/p1/skills/doc/doc.md",
            "---\ndescription: D.\n---\nSee [[contract]].\n",
        ),
        ("plugins/p1/skills/doc/references/contract.md", "# One\n"),
        (
            "plugins/p2/skills/doc/doc.md",
            "---\ndescription: D.\n---\nSee [[contract]].\n",
        ),
        ("plugins/p2/skills/doc/references/contract.md", "# Two\n"),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    for (plugin, heading) in [("p1", "# One\n"), ("p2", "# Two\n")] {
        let pack = dir
            .path()
            .join(format!("dist/.claude-plugin/packs/{plugin}/skills/doc"));
        let body = std::fs::read_to_string(pack.join("SKILL.md")).unwrap();
        assert!(
            body.contains("See [contract.md](references/contract.md)."),
            "{body}"
        );
        let copy = std::fs::read_to_string(pack.join("references/contract.md")).unwrap();
        assert_eq!(copy, heading);
    }
}

/// `\/[[skill]]` shows the call's notation in every harness, in a code span
/// as in prose, while `/[[skill]]` in the same span is the call.
#[test]
fn an_escaped_call_in_a_code_span_ships_as_the_notation() {
    let dir = vault(&[
        (
            "skillwright.md",
            "---\nname: v\nbuild:\n  out: dist\n  harnesses: [claude, codex, cursor, pi]\n---\n",
        ),
        ("plugins/tools/tools.md", PLUGIN),
        (
            "plugins/tools/skills/create-pr.md",
            "---\ndescription: D.\n---\nTo call a skill, write `\\/[[create-pr]]`: `/[[create-pr]]`.\n",
        ),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    for (skill, call) in [
        (
            ".claude-plugin/packs/tools/skills/create-pr",
            "/tools:create-pr",
        ),
        (".agents/packs/tools/skills/create-pr", "$tools:create-pr"),
        (".cursor-plugin/packs/tools/skills/create-pr", "/create-pr"),
        (
            ".pi/packs/tools/skills/tools-create-pr",
            "/skill:tools-create-pr",
        ),
    ] {
        let body =
            std::fs::read_to_string(dir.path().join("dist").join(skill).join("SKILL.md")).unwrap();
        assert!(
            body.contains(&format!(
                "To call a skill, write `/[[create-pr]]`: `{call}`."
            )),
            "{skill}: {body}"
        );
    }
}

/// The manifest fields a plugin note states reach each harness that has the
/// field: Codex's `repository` and `homepage`, Cursor's `displayName`, and
/// Pi's `keywords` with `pi-package`.
#[test]
fn plugin_fields_reach_the_codex_cursor_and_pi_manifests() {
    let root = "---\nname: v\nrepository: https://example.com/org/repo\n\
                homepage: https://example.com/docs\nbuild:\n  out: dist\n\
                \x20 harnesses: [codex, cursor, pi]\n---\n";
    let plugin = "---\ndescription: T.\nversion: 1.0.0\nkeywords: [vertex, sdlc]\n\
                  display_name: Ticket workflow\n---\n";
    let dir = vault(&[
        ("skillwright.md", root),
        ("plugins/t/t.md", plugin),
        ("plugins/t/skills/s.md", SKILL),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let json = |path: &str| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("dist").join(path)).unwrap())
            .unwrap()
    };

    let codex = json(".agents/packs/t/.codex-plugin/plugin.json");
    assert_eq!(codex["repository"], "https://example.com/org/repo");
    assert_eq!(codex["homepage"], "https://example.com/docs");
    assert_eq!(codex["interface"]["displayName"], "Ticket workflow");

    let cursor = json(".cursor-plugin/packs/t/.cursor-plugin/plugin.json");
    assert_eq!(cursor["displayName"], "Ticket workflow");

    let pi = json(".pi/packs/t/package.json");
    assert_eq!(
        pi["keywords"],
        serde_json::json!(["vertex", "sdlc", "pi-package"])
    );

    // A `cursor.displayName` wins for Cursor; a stated `pi-package` is not
    // repeated.
    std::fs::write(
        dir.path().join("plugins/t/t.md"),
        "---\ndescription: T.\nversion: 1.0.0\nkeywords: [pi-package, sdlc]\n\
         display_name: Ticket workflow\ncursor:\n  displayName: Ticket workflow (t)\n---\n",
    )
    .unwrap();
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let cursor = json(".cursor-plugin/packs/t/.cursor-plugin/plugin.json");
    assert_eq!(cursor["displayName"], "Ticket workflow (t)");
    let pi = json(".pi/packs/t/package.json");
    assert_eq!(pi["keywords"], serde_json::json!(["pi-package", "sdlc"]));

    // A `pi.keywords` block key replaces the whole list, as any block key does.
    std::fs::write(
        dir.path().join("plugins/t/t.md"),
        "---\ndescription: T.\nversion: 1.0.0\nkeywords: [sdlc]\npi:\n  keywords: [own]\n---\n",
    )
    .unwrap();
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let pi = json(".pi/packs/t/package.json");
    assert_eq!(pi["keywords"], serde_json::json!(["own"]));
}

#[test]
fn a_skill_and_its_reference_name_the_skill_they_belong_to() {
    let note = "---\ndescription: Work.\n---\nPart of [[work]].\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/t/t.md", PLUGIN),
        ("plugins/t/skills/work/work.md", note),
        (
            "plugins/t/skills/work/references/bug-route.md",
            "Part of [[work]].\n",
        ),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let read = |rel: &str| std::fs::read_to_string(dir.path().join(rel)).unwrap();
    for (skill, named) in [
        ("dist/.claude-plugin/packs/t/skills/work", "t:work"),
        ("dist/.pi/packs/t/skills/t-work", "t-work"),
    ] {
        let want = format!("Part of `{named}`.");
        let entry = read(&format!("{skill}/SKILL.md"));
        assert!(entry.contains(&want), "{skill}: {entry}");
        let reference = read(&format!("{skill}/references/bug-route.md"));
        assert_eq!(reference, format!("{want}\n"), "{skill}");
    }
}

#[test]
fn a_bundle_resource_with_yaml_lists_in_its_frontmatter_builds() {
    let data = "---\ntags: [[a]]\n---\nData.\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/t/t.md", PLUGIN),
        ("plugins/t/skills/work/work.md", SKILL),
        ("plugins/t/skills/work/references/data.md", data),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let shipped = std::fs::read_to_string(
        dir.path()
            .join("dist/.claude-plugin/packs/t/skills/work/references/data.md"),
    )
    .unwrap();
    assert_eq!(shipped, data);
}
