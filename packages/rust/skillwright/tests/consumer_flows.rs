//! What a user meets following the docs, run through the binary: unknown
//! keys, harness blocks, and what a build ships.

use std::path::{Path, PathBuf};
use std::process::Command;

const SKILL: &str = "---\ndescription: Does a thing.\n---\nBody.\n";
const PLUGIN: &str = "---\ndescription: A plugin.\nversion: 1.2.3\n---\n";
const ROOT: &str = "---\n---\n";
const MARKET: &str = "---\ndescription: A marketplace.\n---\n";

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

    /// The built tree of marketplace `v`.
    fn dist(&self) -> PathBuf {
        self.path().join("dist/v")
    }
}

/// A vault holding `files`, each path relative to the root.
fn raw(files: &[(&str, &str)]) -> Fixture {
    let dir = Fixture(tempfile::tempdir().unwrap());
    for (path, text) in files {
        write(&dir.path().join(path), text);
    }
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A vault with root file `root` and one marketplace, `v`. Each path in
/// `plugins` is relative to `marketplaces/v/plugins/`.
fn vault(root: &str, plugins: &[(&str, &str)]) -> Fixture {
    let dir = raw(&[("skillwright.md", root), ("marketplaces/v/v.md", MARKET)]);
    for (path, text) in plugins {
        write(&dir.path().join("marketplaces/v/plugins").join(path), text);
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
        let dir = vault(ROOT, &[("p/p.md", PLUGIN), ("p/skills/s/s.md", &note)]);
        let built = sw(&dir.path(), &["build", "--yes"]);
        assert_eq!(built.code, 0, "{}", built.text);
        assert!(built.text.contains("unknown-key"), "{}", built.text);
    }
}

#[test]
fn a_namespace_keeps_the_prefix_and_names_the_right_spelling() {
    let root = "---\nbuild:\n  namespace: sw\n---\n";
    let plugin = "---\ndescription: P.\nsw-version: 2.0.0\nsw-display-name: X\n---\n";
    let dir = vault(root, &[("p/p.md", plugin), ("p/skills/s/s.md", SKILL)]);
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
fn a_never_ship_folder_in_a_skill_folder_does_not_ship() {
    let dir = vault(
        ROOT,
        &[
            ("p/p.md", PLUGIN),
            ("p/skills/b/b.md", SKILL),
            ("p/skills/b/docs/inner.md", SKILL),
        ],
    );
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let skill = dir.dist().join(".claude-plugin/packs/p/skills/b");
    assert!(skill.join("SKILL.md").is_file());
    assert!(!skill.join("docs").exists());
}

#[test]
fn manifests_keep_to_what_each_harness_documents() {
    let root = "---\nauthor: {name: A, url: 'https://a.example'}\n---\n";
    let dir = vault(root, &[("p/p.md", PLUGIN), ("p/skills/s/s.md", SKILL)]);
    write(
        &dir.path().join("marketplaces/v/v.md"),
        "---\ndescription: M.\nowner: {name: O, url: 'https://o.example'}\n---\n",
    );
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let read = |path: &str| std::fs::read_to_string(dir.dist().join(path)).unwrap();
    for path in [
        ".cursor-plugin/marketplace.json",
        ".cursor-plugin/packs/p/.cursor-plugin/plugin.json",
    ] {
        assert!(!read(path).contains("example"), "{path}: {}", read(path));
    }
    assert!(read(".claude-plugin/marketplace.json").contains("o.example"));
    assert!(read(".cursor-plugin/marketplace.json").contains("1.2.3"));
    assert!(!read(".claude-plugin/marketplace.json").contains("1.2.3"));
    assert!(read(".claude-plugin/packs/p/.claude-plugin/plugin.json").contains("1.2.3"));
}

/// Issue #123: a plugin's Cursor display name reaches its catalog entry as
/// well as its `plugin.json`.
#[test]
fn cursor_catalog_entry_has_the_display_name() {
    let plugin = "---\ndescription: P.\nversion: 1.0.0\ncursor:\n  displayName: \"Ticket workflow (t)\"\n---\n";
    let dir = vault(ROOT, &[("t/t.md", plugin), ("t/skills/s/s.md", SKILL)]);
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
fn block_metadata_on_a_container_must_be_a_mapping() {
    let root = "---\nclaude: {metadata: 5}\n---\n";
    let plugin = "---\ndescription: P.\nclaude: {metadata: 5}\n---\n";
    for (root, plugin) in [(root, PLUGIN), (ROOT, plugin)] {
        let dir = vault(root, &[("p/p.md", plugin), ("p/skills/s/s.md", SKILL)]);
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
        let dir = vault(ROOT, &[("p/p.md", plugin), ("p/skills/s/s.md", skill)]);
        for verb in [&["check"][..], &["build", "--yes"][..]] {
            let ran = sw(&dir.path(), verb);
            assert_eq!(ran.code, 2, "{verb:?}: {}", ran.text);
            assert!(ran.text.contains(said), "{verb:?}: {}", ran.text);
        }
        assert!(!dir.path().join("dist").exists());
    }
}

/// The text of the built file under `dist/v` whose path ends with `tail`.
fn built(dir: &Fixture, tail: &str) -> String {
    let file = files_under(&dir.dist())
        .into_iter()
        .find(|path| path.to_string_lossy().ends_with(tail))
        .unwrap_or_else(|| panic!("no built {tail}"));
    std::fs::read_to_string(file).unwrap()
}

#[test]
fn a_known_block_key_with_a_right_value_ships() {
    let root = "---\nclaude: {model: opus}\ncodex: {interface: {displayName: Vee}}\n\
                pi: {naming: bare}\n---\n";
    let skill = "---\ndescription: S.\nclaude:\n  allowed-tools: [Read, Grep]\n  \
                 argument-hint: [issue-number]\n  user-invocable: off\n  metadata: {team: {lead: Ada}}\n---\nB.\n";
    let plugin = "---\ndescription: P.\nversion: 1.0.0\n\
                  codex: {policy: {installation: AVAILABLE}, interface: {displayName: Pea}}\n---\n";
    let dir = vault(root, &[("p/p.md", plugin), ("p/skills/s/s.md", skill)]);
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
        files_under(&dir.dist())
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
        let dir = vault(ROOT, &[("p/p.md", &plugin), ("p/skills/s/s.md", SKILL)]);
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
        let dir = vault(
            ROOT,
            &[("p/p.md", PLUGIN), ("p/skills/s/s.md", &skill(line))],
        );
        let ran = sw(&dir.path(), &["build", "--yes"]);
        assert_eq!(ran.code, 2, "{line}: {}", ran.text);
        assert!(ran.text.contains(said), "{line}: {}", ran.text);
    }
    let dir = vault(
        ROOT,
        &[
            ("p/p.md", PLUGIN),
            ("p/skills/s/s.md", &skill("argument-hint: [issue-number]")),
        ],
    );
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
    let dir = vault(ROOT, &[("p/p.md", plugin), ("p/skills/s/s.md", skill)]);
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
    let dir = vault(ROOT, &[("p/p.md", PLUGIN), ("p/skills/s/s.md", skill)]);
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
    let root = "---\nbuild:\n  harnesses: [claude]\n---\n";
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
        let dir = vault(root, &[("p/p.md", plugin), ("p/skills/s/s.md", skill)]);
        let ran = sw(&dir.path(), &["build", "--yes"]);
        assert_eq!(ran.code, 2, "{said}: {}", ran.text);
        assert!(ran.text.contains(said), "{said}: {}", ran.text);
    }
}

#[test]
fn a_build_names_each_reason_when_the_skipped_notes_differ() {
    let dir = vault(
        ROOT,
        &[
            ("p/p.md", PLUGIN),
            (
                "p/skills/a/a.md",
                "---\ndescription: S.\nmodel: [1]\n---\nB.\n",
            ),
            ("p/skills/b/b.md", "---\ndescription: [unclosed\n---\nB.\n"),
        ],
    );
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
        built
            .text
            .contains("marketplaces/v/plugins/p/skills/a/a.md (malformed: "),
        "{}",
        built.text
    );
}

#[test]
fn an_agent_with_a_wrong_kind_stops_a_build() {
    let agent = "---\ndescription: A.\ntemperature: \"0.2\"\n---\nB.\n";
    let dir = vault(
        ROOT,
        &[
            ("p/p.md", PLUGIN),
            ("p/skills/s/s.md", SKILL),
            ("p/agents/x/x.md", agent),
        ],
    );
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
    let root = "---\nclaude:\n  homepage:\n  user-invocable: \"0\"\n---\n";
    let dir = vault(root, &[("p/p.md", PLUGIN), ("p/skills/s/s.md", SKILL)]);
    let ran = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(ran.code, 0, "{}", ran.text);
    for file in files_under(&dir.dist().join(".claude-plugin")) {
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("homepage"), "{}: {text}", file.display());
    }
    let claude = built(&dir, ".claude-plugin/packs/p/skills/s/SKILL.md");
    assert!(claude.contains("user-invocable: false"), "{claude}");
}

#[test]
fn a_block_key_no_harness_reads_warns_when_set_to_nothing() {
    let skill = "---\ndescription: S.\nclaude:\n  nonsense:\n---\nB.\n";
    let dir = vault(ROOT, &[("p/p.md", PLUGIN), ("p/skills/s/s.md", skill)]);
    let ran = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(ran.code, 0, "{}", ran.text);
    assert!(ran.text.contains("`claude.nonsense`"), "{}", ran.text);
    let claude = built(&dir, ".claude-plugin/packs/p/skills/s/SKILL.md");
    assert!(!claude.contains("nonsense"), "{claude}");
}

#[test]
fn a_link_into_a_plugin_s_assets_ships() {
    let linking = "---\ndescription: D.\n---\nSee [guide](../../assets/docs/guide.md).\n";
    let dir = vault(
        ROOT,
        &[
            ("p/p.md", PLUGIN),
            ("p/assets/docs/guide.md", "# Guide\n"),
            ("p/skills/s/s.md", linking),
        ],
    );
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    assert!(
        dir.dist()
            .join(".claude-plugin/packs/p/docs/guide.md")
            .is_file()
    );
}

/// Plugin `rev` in marketplace `rev` is just a plugin: with no `rev.md`, the
/// error names the plugin's folder, not the marketplace's.
#[test]
fn a_plugin_named_like_its_marketplace_needs_its_own_note() {
    let dir = raw(&[
        ("skillwright.md", ROOT),
        ("marketplaces/rev/rev.md", MARKET),
        ("marketplaces/rev/plugins/rev/skills/a/a.md", SKILL),
    ]);
    let note = "marketplaces/rev/plugins/rev/rev.md";
    let bumped = sw(&dir.path(), &["bump", "rev", "--patch", "--yes"]);
    assert_eq!(bumped.code, 2, "{}", bumped.text);
    assert!(bumped.text.contains(note), "{}", bumped.text);
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 2, "{}", built.text);
    assert!(built.text.contains(note), "{}", built.text);
}

/// Two bundles that each keep `references/contract.md`, each linking it bare:
/// each skill ships its own copy.
#[test]
fn a_bare_link_resolves_to_the_match_in_its_own_bundle() {
    let doc = "---\ndescription: D.\n---\nSee [[contract]].\n";
    let dir = vault(
        ROOT,
        &[
            ("p1/p1.md", PLUGIN),
            ("p2/p2.md", PLUGIN),
            ("p1/skills/doc/doc.md", doc),
            ("p1/skills/doc/references/contract.md", "# One\n"),
            ("p2/skills/doc/doc.md", doc),
            ("p2/skills/doc/references/contract.md", "# Two\n"),
        ],
    );
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    for (plugin, heading) in [("p1", "# One\n"), ("p2", "# Two\n")] {
        let pack = dir
            .dist()
            .join(format!(".claude-plugin/packs/{plugin}/skills/doc"));
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
    let dir = vault(
        "---\nbuild:\n  harnesses: [claude, codex, cursor, pi]\n---\n",
        &[
            ("tools/tools.md", PLUGIN),
            (
                "tools/skills/create-pr/create-pr.md",
                "---\ndescription: D.\n---\nTo call a skill, write `\\/[[create-pr]]`: `/[[create-pr]]`.\n",
            ),
        ],
    );
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
        let body = std::fs::read_to_string(dir.dist().join(skill).join("SKILL.md")).unwrap();
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
    let root = "---\nrepository: https://example.com/org/repo\n\
                homepage: https://example.com/docs\nbuild:\n\
                \x20 harnesses: [codex, cursor, pi]\n---\n";
    let plugin = "---\ndescription: T.\nversion: 1.0.0\nkeywords: [vertex, sdlc]\n\
                  display_name: Ticket workflow\n---\n";
    let dir = vault(root, &[("t/t.md", plugin), ("t/skills/s/s.md", SKILL)]);
    let note = dir.path().join("marketplaces/v/plugins/t/t.md");
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let json = |path: &str| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(dir.dist().join(path)).unwrap()).unwrap()
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
    write(
        &note,
        "---\ndescription: T.\nversion: 1.0.0\nkeywords: [pi-package, sdlc]\n\
         display_name: Ticket workflow\ncursor:\n  displayName: Ticket workflow (t)\n---\n",
    );
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let cursor = json(".cursor-plugin/packs/t/.cursor-plugin/plugin.json");
    assert_eq!(cursor["displayName"], "Ticket workflow (t)");
    let pi = json(".pi/packs/t/package.json");
    assert_eq!(pi["keywords"], serde_json::json!(["pi-package", "sdlc"]));

    // A `pi.keywords` block key replaces the whole list, as any block key does.
    write(
        &note,
        "---\ndescription: T.\nversion: 1.0.0\nkeywords: [sdlc]\npi:\n  keywords: [own]\n---\n",
    );
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let pi = json(".pi/packs/t/package.json");
    assert_eq!(pi["keywords"], serde_json::json!(["own"]));
}

#[test]
fn a_skill_and_its_reference_name_the_skill_they_belong_to() {
    let note = "---\ndescription: Work.\n---\nPart of [[work]].\n";
    let dir = vault(
        ROOT,
        &[
            ("t/t.md", PLUGIN),
            ("t/skills/work/work.md", note),
            (
                "t/skills/work/references/bug-route.md",
                "Part of [[work]].\n",
            ),
        ],
    );
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let read = |rel: &str| std::fs::read_to_string(dir.dist().join(rel)).unwrap();
    for (skill, named) in [
        (".claude-plugin/packs/t/skills/work", "t:work"),
        (".pi/packs/t/skills/t-work", "t-work"),
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
    let dir = vault(
        ROOT,
        &[
            ("t/t.md", PLUGIN),
            ("t/skills/work/work.md", SKILL),
            ("t/skills/work/references/data.md", data),
        ],
    );
    let built = sw(&dir.path(), &["build", "--yes"]);
    assert_eq!(built.code, 0, "{}", built.text);
    let shipped = std::fs::read_to_string(
        dir.dist()
            .join(".claude-plugin/packs/t/skills/work/references/data.md"),
    )
    .unwrap();
    assert_eq!(shipped, data);
}
