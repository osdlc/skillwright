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
fn a_workspace_glob_warns_once_when_nothing_under_the_workspace_matches_it() {
    let shared = "---\nworkspace: true\nbuild:\n  exclude: ['marketplaces/*/skills/a.md', 'skills/gone.md']\n---\n";
    let dir = vault(&[
        ("skillwright.md", shared),
        ("marketplaces/one/skillwright.md", &member("one", "")),
        ("marketplaces/one/skills/a.md", SKILL),
        ("marketplaces/one/skills/s.md", SKILL),
        ("marketplaces/two/skillwright.md", &member("two", "")),
        ("marketplaces/two/skills/s.md", SKILL),
    ]);
    for args in [&["build", "--yes"][..], &["check"][..], &["list"][..]] {
        let ran = sw(&dir.path(), args);
        assert_eq!(ran.code, 0, "{args:?}: {}", ran.text);
        assert_eq!(
            ran.text.matches("`skills/gone.md`").count(),
            1,
            "{args:?}: {}",
            ran.text
        );
        assert!(!ran.text.contains("skills/a.md`"), "{args:?}: {}", ran.text);
    }
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

/// The secrets/drafts case: a member's exclusions add to the workspace's,
/// each read from its own folder, and build, list and publish agree.
#[test]
fn a_members_exclusions_add_to_the_workspaces_for_every_verb() {
    let shared = "---\nworkspace: true\nbuild:\n  exclude: ['**/secrets/**']\n---\n";
    let one = member("one", "  exclude: ['skills/drafts/**']\n");
    let dir = vault(&[
        ("skillwright.md", shared),
        ("marketplaces/one/skillwright.md", &one),
        ("marketplaces/one/skills/open.md", SKILL),
        ("marketplaces/one/skills/secrets/secrets.md", SKILL),
        ("marketplaces/one/skills/drafts/drafts.md", SKILL),
        ("marketplaces/two/skillwright.md", &member("two", "")),
        ("marketplaces/two/skills/drafts/drafts.md", SKILL),
        ("marketplaces/two/skills/secrets/secrets.md", SKILL),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let one_skills = dir
        .path()
        .join("marketplaces/one/built/.claude-plugin/packs");
    let two_skills = dir
        .path()
        .join("marketplaces/two/built/.claude-plugin/packs");
    let shipped = |root: &Path, name: &str| {
        files_under(root)
            .iter()
            .any(|path| path.to_string_lossy().contains(&format!("/{name}/")))
    };
    assert!(
        shipped(&one_skills, "open"),
        "{:?}",
        files_under(&one_skills)
    );
    assert!(!shipped(&one_skills, "secrets"));
    assert!(!shipped(&one_skills, "drafts"));
    // A member's glob does not reach its sibling; the workspace's does.
    assert!(
        shipped(&two_skills, "drafts"),
        "{:?}",
        files_under(&two_skills)
    );
    assert!(!shipped(&two_skills, "secrets"));

    let listed = sw(&dir.path(), &["list"]);
    assert_eq!(listed.code, 0, "{}", listed.text);
    assert!(
        listed.text.contains("marketplaces/one/skills/open.md"),
        "{}",
        listed.text
    );
    assert!(
        !listed.text.contains("marketplaces/one/skills/drafts"),
        "{}",
        listed.text
    );
    assert!(
        listed.text.contains("marketplaces/two/skills/drafts"),
        "{}",
        listed.text
    );
    assert!(!listed.text.contains("secrets"), "{}", listed.text);

    let one_dir = dir.path().join("marketplaces/one");
    for note in ["skills/secrets/secrets.md", "skills/drafts/drafts.md"] {
        let published = sw(&one_dir, &["publish", note, "--yes"]);
        assert_ne!(published.code, 0, "{note}: {}", published.text);
        assert!(
            published.text.contains("excluded-note"),
            "{note}: {}",
            published.text
        );
    }
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
fn list_skips_a_note_with_a_wrong_kind_that_a_build_refuses() {
    let skill = |line: &str| format!("---\ndescription: S.\n{line}\n---\nB.\n");
    let plugin = "---\ndescription: P.\nversion: 1.0.0\ncodex: {policy: 5}\n---\n";
    for (plugin, skill, said) in [
        (
            PLUGIN,
            skill("allowed-tools: 5"),
            "plugins/p/skills/s.md  skipped · malformed: ",
        ),
        (
            PLUGIN,
            skill("codex: {allowed-tools: 5}"),
            "plugins/p/skills/s.md  skipped · malformed: ",
        ),
        (
            plugin,
            SKILL.to_owned(),
            "plugins/p/p.md  skipped · malformed: ",
        ),
    ] {
        let dir = vault(&[
            ("skillwright.md", ROOT),
            ("plugins/p/p.md", plugin),
            ("plugins/p/skills/s.md", &skill),
        ]);
        let list = sw(&dir.path(), &["list"]);
        assert_eq!(list.code, 0, "{said}: {}", list.text);
        assert!(list.text.contains(said), "{said}: {}", list.text);
        assert!(!list.text.contains("unknown-key"), "{}", list.text);
        let built = sw(&dir.path(), &["build", "--yes"]);
        assert_eq!(built.code, 2, "{said}: {}", built.text);
        assert!(
            built
                .text
                .contains("malformed: the vault holds 1 note(s) with a value of the wrong kind"),
            "{said}: {}",
            built.text
        );
        assert!(!built.text.contains("could not read"), "{}", built.text);
    }
}

#[test]
fn list_skips_a_root_file_whose_block_holds_a_wrong_kind() {
    let root = "---\nname: v\nclaude: {model: [1]}\nbuild:\n  out: dist\n---\n";
    let dir = vault(&[
        ("skillwright.md", root),
        ("plugins/p/p.md", PLUGIN),
        ("plugins/p/skills/s.md", SKILL),
    ]);
    let list = sw(&dir.path(), &["list"]);
    assert_eq!(list.code, 0, "{}", list.text);
    assert!(
        list.text.contains("skillwright.md  skipped · malformed: "),
        "{}",
        list.text
    );
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 2, "{}", built.text);
    assert!(
        built
            .text
            .contains("malformed: `claude.model` of marketplace `v` must be a string"),
        "{}",
        built.text
    );
}

#[test]
fn publish_reports_a_wrong_kind_as_malformed_in_json() {
    let skill = "---\ndescription: S.\nmodel: [1]\n---\nB.\n";
    let dir = vault(&[("skillwright.md", ROOT), ("plugins/p/skills/s.md", skill)]);
    let ran = publish_json(&dir, "plugins/p/skills/s.md");
    assert_ne!(ran.code, 0, "{}", ran.text);
    assert!(
        ran.text.contains("\"token\": \"malformed\""),
        "{}",
        ran.text
    );
    assert!(
        ran.text.contains("`model` must be a string, not a list"),
        "{}",
        ran.text
    );
}

/// `publish <note> --target claude --json` into the fixture's home.
fn publish_json(dir: &Fixture, note: &str) -> Ran {
    let dest = dir.home().join("dest");
    sw(
        &dir.path(),
        &[
            "publish",
            note,
            "--target",
            "claude",
            "--to",
            dest.to_str().unwrap(),
            "--json",
            "--yes",
        ],
    )
}

#[test]
fn an_opt_in_key_of_the_wrong_kind_alone_is_refused() {
    for (line, said) in [
        (
            "description: 2048",
            "`description` must be a string, not a number",
        ),
        (
            "description: [a, b]",
            "`description` must be a string, not a list",
        ),
        ("name: 5", "`name` must be a string, not a number"),
    ] {
        let skill = format!("---\n{line}\n---\nB.\n");
        let dir = vault(&[
            ("skillwright.md", ROOT),
            ("plugins/p/p.md", PLUGIN),
            ("plugins/p/skills/s.md", &skill),
        ]);
        let list = sw(&dir.path(), &["list"]);
        assert!(
            list.text
                .contains("plugins/p/skills/s.md  skipped · malformed: "),
            "{line}: {}",
            list.text
        );
        assert!(list.text.contains(said), "{line}: {}", list.text);
        assert!(!list.text.contains("no-component-keys"), "{}", list.text);
        let built = sw(&dir.path(), &["build", "--yes"]);
        assert_eq!(built.code, 2, "{line}: {}", built.text);
        assert!(built.text.contains(said), "{line}: {}", built.text);
        let ran = publish_json(&dir, "plugins/p/skills/s.md");
        assert_ne!(ran.code, 0, "{line}: {}", ran.text);
        assert!(
            ran.text.contains("\"token\": \"malformed\""),
            "{line}: {}",
            ran.text
        );
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
fn a_plugin_publish_refuses_a_member_with_a_wrong_kind() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", PLUGIN),
        (
            "plugins/p/skills/s.md",
            "---\ndescription: S.\nmodel: [1]\n---\nB.\n",
        ),
    ]);
    let ran = publish_json(&dir, "plugins/p/p.md");
    assert_ne!(ran.code, 0, "{}", ran.text);
    assert!(
        ran.text.contains("\"token\": \"malformed\""),
        "{}",
        ran.text
    );
    assert!(
        ran.text
            .contains("`p` holds 1 note(s) with a value of the wrong kind"),
        "{}",
        ran.text
    );
}

#[test]
fn targets_of_the_wrong_kind_is_malformed() {
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", PLUGIN),
        (
            "plugins/p/skills/s.md",
            "---\ndescription: S.\ntargets: 5\n---\nB.\n",
        ),
    ]);
    let list = sw(&dir.path(), &["list"]);
    assert!(
        list.text
            .contains("plugins/p/skills/s.md  skipped · malformed: "),
        "{}",
        list.text
    );
    let ran = publish_json(&dir, "plugins/p/skills/s.md");
    assert_ne!(ran.code, 0, "{}", ran.text);
    assert!(
        ran.text.contains("\"token\": \"malformed\""),
        "{}",
        ran.text
    );
}

#[test]
fn publish_reports_a_plugin_block_as_malformed_in_json() {
    let plugin = "---\ndescription: P.\nversion: 1.0.0\ncodex: {policy: 5}\n---\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/p/p.md", plugin),
        ("plugins/p/skills/s.md", SKILL),
    ]);
    for note in ["plugins/p/p.md", "plugins/p/skills/s.md"] {
        let ran = publish_json(&dir, note);
        assert_ne!(ran.code, 0, "{note}: {}", ran.text);
        assert!(
            ran.text.contains("\"token\": \"malformed\""),
            "{note}: {}",
            ran.text
        );
        assert!(
            ran.text
                .contains("`codex.policy` of plugin `p` must be a mapping"),
            "{note}: {}",
            ran.text
        );
    }
}

#[test]
fn a_bad_root_block_is_blamed_on_the_marketplace_in_publish_and_status() {
    let root = "---\nname: v\nclaude: {model: [1]}\nbuild:\n  out: dist\n---\n";
    let dir = vault(&[
        ("skillwright.md", root),
        ("plugins/p/p.md", PLUGIN),
        ("plugins/p/skills/s.md", SKILL),
    ]);
    let said = "malformed: `claude.model` of marketplace `v` must be a string";
    let ran = publish_json(&dir, "plugins/p/skills/s.md");
    assert_ne!(ran.code, 0, "{}", ran.text);
    assert!(
        ran.text.contains("`claude.model` of marketplace `v`"),
        "{}",
        ran.text
    );
    assert!(!ran.text.contains("s.md: `claude.model`"), "{}", ran.text);
    let status = sw(&dir.path(), &["status"]);
    assert!(status.text.contains(said), "{}", status.text);
    assert!(
        !status.text.contains("s.md: `claude.model`"),
        "{}",
        status.text
    );
}

#[test]
fn a_bad_workspace_block_is_listed_once_naming_the_workspace_file() {
    let workspace = "---\nworkspace: true\nclaude: {model: [1]}\n---\n";
    let dir = vault(&[
        ("skillwright.md", workspace),
        ("marketplaces/one/skillwright.md", "---\nname: one\n---\n"),
        ("marketplaces/one/plugins/p/skills/s.md", SKILL),
        ("marketplaces/two/skillwright.md", "---\nname: two\n---\n"),
        ("marketplaces/two/plugins/q/skills/s.md", SKILL),
    ]);
    let list = sw(&dir.path(), &["list"]);
    assert_eq!(
        list.text.matches("skipped · malformed").count(),
        1,
        "{}",
        list.text
    );
    assert!(
        list.text
            .contains("skillwright.md  skipped · malformed: `claude.model` of the workspace file"),
        "{}",
        list.text
    );
    assert!(!list.text.contains("marketplace ``"), "{}", list.text);
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
fn an_agent_s_stray_block_is_refused_only_for_its_own_target() {
    let agent = "---\ndescription: A.\nclaude: {model: 5}\n---\nB.\n";
    let dir = vault(&[("skillwright.md", ROOT), ("plugins/p/agents/x.md", agent)]);
    let publish = |target: &str| {
        let dest = dir.home().join(target);
        let to = dest.to_str().unwrap().to_owned();
        sw(
            &dir.path(),
            &[
                "publish",
                "plugins/p/agents/x.md",
                "--target",
                target,
                "--to",
                &to,
                "--yes",
            ],
        )
    };
    let claude = publish("claude");
    assert_ne!(claude.code, 0, "{}", claude.text);
    assert!(
        claude.text.contains("unknown-override-key"),
        "{}",
        claude.text
    );
    assert!(!claude.text.contains("must be a string"), "{}", claude.text);
    let opencode = publish("opencode");
    assert_eq!(opencode.code, 0, "{}", opencode.text);
    assert!(dir.home().join("opencode/.opencode/agents/x.md").is_file());
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
fn an_agent_s_tools_take_opencode_s_mapping() {
    let agent = "---\ndescription: A.\ntools:\n  write: false\n  bash: true\n---\nB.\n";
    let dir = vault(&[("skillwright.md", ROOT), ("plugins/p/agents/x.md", agent)]);
    let publish = |target: &str| {
        let dest = dir.home().join(target);
        let to = dest.to_str().unwrap().to_owned();
        let note = "plugins/p/agents/x.md";
        sw(
            &dir.path(),
            &["publish", note, "--target", target, "--to", &to, "--yes"],
        )
    };
    let opencode = publish("opencode");
    assert_eq!(opencode.code, 0, "{}", opencode.text);
    let written =
        std::fs::read_to_string(dir.home().join("opencode/.opencode/agents/x.md")).unwrap();
    assert!(written.contains("write: false"), "{written}");
    // Claude Code reads only a list, so the mapping is refused for it alone.
    let claude = publish("claude");
    assert_ne!(claude.code, 0, "{}", claude.text);
    assert!(claude.text.contains("malformed-tools"), "{}", claude.text);

    let agent = "---\ndescription: A.\ntemperature: \"0.2\"\n---\nB.\n";
    std::fs::write(dir.path().join("plugins/p/agents/x.md"), agent).unwrap();
    let ran = publish("opencode");
    assert_ne!(ran.code, 0, "{}", ran.text);
    assert!(
        ran.text
            .contains("`temperature` must be a number, not a string"),
        "{}",
        ran.text
    );
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
        ran.text
            .contains("`claude.metadata` of plugin `p` must be a mapping"),
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

#[test]
fn a_plain_skill_mention_renders_on_bare_keys_in_build_and_publish() {
    let note = "---\ndescription: A.\n---\nSee [[b]] and [[b|the other]].\n";
    let dir = vault(&[
        ("skillwright.md", ROOT),
        ("plugins/tools/tools.md", PLUGIN),
        ("plugins/tools/skills/a.md", note),
        ("plugins/tools/skills/b.md", SKILL),
    ]);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let read = |rel: &str| std::fs::read_to_string(dir.path().join(rel)).unwrap();
    let claude = read("dist/.claude-plugin/packs/tools/skills/a/SKILL.md");
    assert!(
        claude.contains("See `tools:b` and the other (`tools:b`)."),
        "{claude}"
    );
    let pi = read("dist/.pi/packs/tools/skills/tools-a/SKILL.md");
    assert!(pi.contains("See `tools-b`"), "{pi}");
    let cursor = read("dist/.cursor-plugin/packs/tools/skills/a/SKILL.md");
    assert!(cursor.contains("See `b`"), "{cursor}");

    let dest = dir.home().join("dest");
    let ran = sw(
        &dir.path(),
        &[
            "publish",
            "plugins/tools/skills/a.md",
            "--to",
            dest.to_str().unwrap(),
            "--target",
            "claude",
            "--yes",
        ],
    );
    assert_eq!(ran.code, 0, "{}", ran.text);
    let published = std::fs::read_to_string(dest.join(".claude/skills/a/SKILL.md")).unwrap();
    assert!(published.contains("See `b`"), "{published}");
}
