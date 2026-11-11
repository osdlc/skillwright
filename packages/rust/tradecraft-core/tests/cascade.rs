//! The cascade from the root file down to a component, harness blocks and
//! versions (spec section 7 and section 8 rules 1-4), at the core's public
//! API.

use std::path::Path;

use serde_json::Value;
use tradecraft_core::{DiscoveryWarning, ExportMode, MemVfs, Vfs, load_vault};

const ROOT: &str = "skillwright.md";
const MARKET: &str = "marketplaces/acme/acme.md";
const PLUGIN: &str = "marketplaces/acme/plugins/tools/tools.md";
const SKILL: &str = "marketplaces/acme/plugins/tools/skills/review/review.md";

/// The four levels, farthest first.
const LEVELS: [&str; 4] = [ROOT, MARKET, PLUGIN, SKILL];

/// Each note's frontmatter lines, by level, before a test adds its own.
fn base(level: &str) -> &'static str {
    match level {
        SKILL => "description: Use it.\n",
        PLUGIN => "version: '1.0.0'\n",
        _ => "",
    }
}

/// A vault at `/v`: marketplace `acme`, plugin `tools`, skill `review`. Each
/// note's frontmatter is its base lines plus what `extra` gives it.
fn vault(extra: &[(&str, &str)]) -> MemVfs {
    let vfs = MemVfs::new();
    for level in LEVELS {
        let mut front = base(level).to_owned();
        for (_, lines) in extra.iter().filter(|(at, _)| *at == level) {
            front.push_str(lines);
            front.push('\n');
        }
        let path = Path::new("/v").join(level);
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        vfs.write_file(&path, format!("---\n{front}---\n\nBody.\n").as_bytes())
            .unwrap();
    }
    vfs
}

/// Builds every marketplace in `vfs` into `/v/dist`, its warnings.
fn build(vfs: &MemVfs) -> Vec<DiscoveryWarning> {
    let loaded = load_vault(vfs, Path::new("/v"), None).unwrap();
    let mut warnings = Vec::new();
    for marketplace in &loaded.marketplaces {
        let request = loaded.export_request(marketplace, ExportMode::Build);
        let report = tradecraft_core::export_skills(vfs, &request).unwrap();
        warnings.extend(report.warnings);
    }
    warnings
}

/// The build's error for `vfs`, as text.
fn build_error(vfs: &MemVfs) -> String {
    let loaded = match load_vault(vfs, Path::new("/v"), None) {
        Ok(loaded) => loaded,
        Err(err) => return err.to_string(),
    };
    for marketplace in &loaded.marketplaces {
        let request = loaded.export_request(marketplace, ExportMode::Build);
        if let Err(err) = tradecraft_core::export_skills(vfs, &request) {
            return err.to_string();
        }
    }
    panic!("the build passed");
}

fn read(vfs: &MemVfs, path: &str) -> Option<String> {
    let bytes = vfs.read_file(&Path::new("/v/dist/acme").join(path)).ok()?;
    Some(String::from_utf8(bytes).unwrap())
}

fn json(vfs: &MemVfs, path: &str) -> Value {
    serde_json::from_str(&read(vfs, path).unwrap_or_else(|| panic!("no {path}"))).unwrap()
}

/// Where each harness writes plugin `tools`'s manifest.
fn plugin_manifest(harness: &str) -> &'static str {
    match harness {
        "claude" => ".claude-plugin/packs/tools/.claude-plugin/plugin.json",
        "codex" => ".agents/packs/tools/.codex-plugin/plugin.json",
        "cursor" => ".cursor-plugin/packs/tools/.cursor-plugin/plugin.json",
        "pi" => ".pi/packs/tools/package.json",
        _ => unreachable!(),
    }
}

/// Where each harness writes skill `review`.
fn skill_file(harness: &str) -> &'static str {
    match harness {
        "claude" => ".claude-plugin/packs/tools/skills/review/SKILL.md",
        "codex" => ".agents/packs/tools/skills/review/SKILL.md",
        "cursor" => ".cursor-plugin/packs/tools/skills/tools-review/SKILL.md",
        "pi" => ".pi/packs/tools/skills/tools-review/SKILL.md",
        _ => unreachable!(),
    }
}

/// Skill `review`'s frontmatter on `harness`, as JSON.
fn skill_front(vfs: &MemVfs, harness: &str) -> Value {
    let text = read(vfs, skill_file(harness)).unwrap_or_else(|| panic!("no skill on {harness}"));
    let front = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front, _)| front)
        .unwrap();
    serde_yaml::from_str(front).unwrap()
}

/// One inherited key: the levels that accept it, the line that sets it to the
/// level's own value, and what the build made of it.
struct Case {
    key: &'static str,
    levels: &'static [&'static str],
    line: fn(usize) -> String,
    read: fn(&MemVfs) -> String,
}

const CONTAINER_LEVELS: &[&str] = &[ROOT, MARKET, PLUGIN];
const EVERY_LEVEL: &[&str] = &[ROOT, MARKET, PLUGIN, SKILL];

fn str_at(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_owned()
}

/// Which harnesses got skill `review`.
fn harnesses_with_the_skill(vfs: &MemVfs) -> String {
    ["claude", "codex", "cursor", "pi"]
        .into_iter()
        .filter(|harness| read(vfs, skill_file(harness)).is_some())
        .collect::<Vec<_>>()
        .join(",")
}

const HARNESS_BY_LEVEL: [&str; 4] = ["claude", "codex", "cursor", "pi"];

fn cases() -> Vec<Case> {
    vec![
        Case {
            key: "author",
            levels: CONTAINER_LEVELS,
            line: |i| format!("author: a{i}"),
            read: |vfs| {
                ["claude", "codex", "cursor", "pi"]
                    .map(|h| str_at(&json(vfs, plugin_manifest(h)), "/author/name"))
                    .join(",")
            },
        },
        Case {
            key: "license (plugin)",
            levels: CONTAINER_LEVELS,
            line: |i| format!("license: L{i}"),
            read: |vfs| str_at(&json(vfs, plugin_manifest("claude")), "/license"),
        },
        Case {
            key: "license (skill)",
            levels: EVERY_LEVEL,
            line: |i| format!("license: L{i}"),
            read: |vfs| {
                ["claude", "codex", "cursor", "pi"]
                    .map(|h| str_at(&skill_front(vfs, h), "/license"))
                    .join(",")
            },
        },
        Case {
            key: "repository",
            levels: CONTAINER_LEVELS,
            line: |i| format!("repository: https://r{i}.example"),
            read: |vfs| str_at(&json(vfs, plugin_manifest("claude")), "/repository"),
        },
        Case {
            key: "homepage",
            levels: CONTAINER_LEVELS,
            line: |i| format!("homepage: https://h{i}.example"),
            read: |vfs| str_at(&json(vfs, plugin_manifest("codex")), "/homepage"),
        },
        Case {
            key: "targets",
            levels: EVERY_LEVEL,
            line: |i| format!("targets: [{}]", HARNESS_BY_LEVEL[i]),
            read: harnesses_with_the_skill,
        },
        Case {
            key: "claude.model (skill)",
            levels: EVERY_LEVEL,
            line: |i| format!("claude: {{model: m{i}}}"),
            read: |vfs| str_at(&skill_front(vfs, "claude"), "/model"),
        },
        Case {
            key: "claude.author (plugin)",
            levels: CONTAINER_LEVELS,
            line: |i| format!("claude: {{author: {{name: b{i}}}}}"),
            read: |vfs| str_at(&json(vfs, plugin_manifest("claude")), "/author/name"),
        },
        Case {
            key: "codex.keywords (plugin)",
            levels: CONTAINER_LEVELS,
            line: |i| format!("codex: {{keywords: [k{i}]}}"),
            read: |vfs| str_at(&json(vfs, plugin_manifest("codex")), "/keywords/0"),
        },
    ]
}

/// What the build reads when level `k` and every level above it set the key,
/// and when only level `k` and the root do.
fn expected(case: &Case, k: usize) -> String {
    let level = case.levels[k];
    let i = LEVELS.iter().position(|l| *l == level).unwrap();
    match case.key {
        "author" => vec![format!("a{i}"); 4].join(","),
        "license (skill)" => vec![format!("L{i}"); 4].join(","),
        "license (plugin)" => format!("L{i}"),
        "repository" => format!("https://r{i}.example"),
        "homepage" => format!("https://h{i}.example"),
        "targets" => HARNESS_BY_LEVEL[i].to_owned(),
        "claude.model (skill)" => format!("m{i}"),
        "claude.author (plugin)" => format!("b{i}"),
        "codex.keywords (plugin)" => format!("k{i}"),
        _ => unreachable!(),
    }
}

/// Spec section 7 rules 1-3: each inherited key comes from the nearest of
/// component, plugin, marketplace and root that sets it.
#[test]
fn each_inherited_key_comes_from_the_nearest_level_that_sets_it() {
    for case in cases() {
        for k in 0..case.levels.len() {
            let level_index = |level: &str| LEVELS.iter().position(|l| *l == level).unwrap();
            // Every level from the root down to `k`.
            let all: Vec<(&str, String)> = case.levels[..=k]
                .iter()
                .map(|level| (*level, (case.line)(level_index(level))))
                .collect();
            // Only the root and `k`, skipping the levels between.
            let ends: Vec<(&str, String)> = [case.levels[0], case.levels[k]]
                .into_iter()
                .map(|level| (level, (case.line)(level_index(level))))
                .collect();
            for set in [all, ends] {
                let extra: Vec<(&str, &str)> =
                    set.iter().map(|(at, line)| (*at, line.as_str())).collect();
                let mut extra = extra;
                extra.dedup();
                let vfs = vault(&extra);
                build(&vfs);
                assert_eq!(
                    (case.read)(&vfs),
                    expected(&case, k),
                    "{} set at {:?}",
                    case.key,
                    extra
                );
            }
        }
    }
}

/// Harness blocks inherit key by key: a nearer block that sets one key
/// leaves the farther block's other keys in place.
#[test]
fn harness_blocks_inherit_key_by_key() {
    let vfs = vault(&[
        (ROOT, "claude: {model: root, context: fork}"),
        (PLUGIN, "claude: {model: plugin}"),
    ]);
    build(&vfs);
    let front = skill_front(&vfs, "claude");
    assert_eq!(str_at(&front, "/model"), "plugin");
    assert_eq!(str_at(&front, "/context"), "fork");
}

/// Section 7 rule 5: a plugin's `claude.author` beats its plain `author` for
/// Claude Code only. Codex, Cursor and Pi get the plain one.
#[test]
fn a_plugin_s_claude_author_beats_its_plain_author_for_claude_only() {
    let vfs = vault(&[(PLUGIN, "author: Ada\nclaude: {author: Bob}")]);
    build(&vfs);
    let author = |h| str_at(&json(&vfs, plugin_manifest(h)), "/author/name");
    assert_eq!(author("claude"), "Bob");
    for harness in ["codex", "cursor", "pi"] {
        assert_eq!(author(harness), "Ada", "{harness}");
    }
}

/// Section 7 rule 5, step 3: a container's plain key beats a farther
/// container's block.
#[test]
fn a_nearer_plain_key_beats_a_farther_block() {
    for (far, near) in [(ROOT, MARKET), (ROOT, PLUGIN), (MARKET, PLUGIN)] {
        let vfs = vault(&[
            (
                far,
                "claude: {author: {name: Far}, homepage: https://far.example}",
            ),
            (near, "author: Near\nhomepage: https://near.example"),
        ]);
        build(&vfs);
        let manifest = json(&vfs, plugin_manifest("claude"));
        assert_eq!(str_at(&manifest, "/author/name"), "Near", "{far} {near}");
        assert_eq!(
            str_at(&manifest, "/homepage"),
            "https://near.example",
            "{far} {near}"
        );
    }
    // A farther block still beats a farther plain key on its own level.
    let vfs = vault(&[(ROOT, "author: Plain\nclaude: {author: Block}")]);
    build(&vfs);
    let author = |h| str_at(&json(&vfs, plugin_manifest(h)), "/author/name");
    assert_eq!(author("claude"), "Block");
    assert_eq!(author("codex"), "Plain");
}

/// Section 7 rule 4: a plugin's `claude.description` describes the plugin,
/// never its skills.
#[test]
fn a_plugin_s_block_description_does_not_set_its_skills_description() {
    let vfs = vault(&[(PLUGIN, "description: Tools.\nclaude: {description: x}")]);
    build(&vfs);
    assert_eq!(
        str_at(&json(&vfs, plugin_manifest("claude")), "/description"),
        "x"
    );
    assert_eq!(
        str_at(&skill_front(&vfs, "claude"), "/description"),
        "Use it."
    );
}

/// Section 7 rule 4: a block key a skill's kind does not accept stays with
/// the plugin.
#[test]
fn a_plugin_block_key_a_skill_does_not_accept_stays_with_the_plugin() {
    let vfs = vault(&[(
        PLUGIN,
        "claude: {keywords: [k], homepage: https://h.example}",
    )]);
    let warnings = build(&vfs);
    assert!(
        warnings.iter().all(|w| w.reason != "unknown-key"),
        "{warnings:?}"
    );
    let manifest = json(&vfs, plugin_manifest("claude"));
    assert_eq!(manifest["keywords"], serde_json::json!(["k"]));
    assert_eq!(str_at(&manifest, "/homepage"), "https://h.example");
    let front = skill_front(&vfs, "claude");
    assert!(front.get("keywords").is_none(), "{front}");
    assert!(front.get("homepage").is_none(), "{front}");
    assert!(front.get("metadata").is_none(), "{front}");
}

/// Section 7 rule 4: a marketplace block's identity keys and the keys of its
/// own marketplace manifest stay with it, though a plugin accepts them.
#[test]
fn a_marketplace_s_identity_and_manifest_keys_do_not_pass_down() {
    let vfs = vault(&[(
        MARKET,
        "claude: {metadata: {from: market}, version: '9.9.9'}\n\
         codex: {interface: {displayName: Acme}}",
    )]);
    let warnings = build(&vfs);
    assert!(
        warnings.iter().all(|w| w.reason != "unknown-key"),
        "{warnings:?}"
    );
    let manifest = |harness| {
        json(
            &vfs,
            tradecraft_core::marketplace_manifest_path(harness).unwrap(),
        )
    };
    let claude = manifest("claude");
    assert_eq!(str_at(&claude, "/metadata/from"), "market", "{claude}");
    assert_eq!(str_at(&claude, "/version"), "9.9.9", "{claude}");
    let codex = manifest("codex");
    assert_eq!(str_at(&codex, "/interface/displayName"), "Acme", "{codex}");

    let plugin = json(&vfs, plugin_manifest("claude"));
    assert_eq!(str_at(&plugin, "/version"), "1.0.0", "{plugin}");
    assert!(plugin.pointer("/metadata/from").is_none(), "{plugin}");
    let entry = &claude["plugins"][0];
    assert!(entry.pointer("/metadata/from").is_none(), "{entry}");
    assert_ne!(str_at(entry, "/version"), "9.9.9", "{entry}");
    let plugin = json(&vfs, plugin_manifest("codex"));
    assert!(
        plugin.pointer("/interface/displayName") != Some(&Value::from("Acme")),
        "{plugin}"
    );
    let front = skill_front(&vfs, "claude");
    assert!(front.pointer("/metadata/from").is_none(), "{front}");
}

/// Section 8 rule 2: an unknown key in a block warns and is left out of that
/// harness's output, on a component and on a container.
#[test]
fn an_unknown_block_key_warns_and_is_left_out() {
    for (note, written) in [
        (SKILL, skill_file("claude")),
        (PLUGIN, plugin_manifest("claude")),
        (MARKET, ".claude-plugin/marketplace.json"),
        (ROOT, plugin_manifest("claude")),
    ] {
        let vfs = vault(&[(note, "claude: {flavour: mint}")]);
        let warnings = build(&vfs);
        if note != ROOT {
            assert!(
                warnings
                    .iter()
                    .any(|w| w.path == note && w.detail.contains("claude.flavour")),
                "{note}: {warnings:?}"
            );
        }
        let text = read(&vfs, written).unwrap();
        assert!(!text.contains("flavour"), "{note}: {text}");
        assert!(!text.contains("mint"), "{note}: {text}");
    }
    // The root file's own reader warns about it.
    let root = tradecraft_core::parse_root_file("---\nclaude: {flavour: mint}\n---\n").unwrap();
    assert!(
        root.warnings.iter().any(|w| w.contains("claude.flavour")),
        "{:?}",
        root.warnings
    );
}

/// Section 8 rule 2: a known block key with a wrong value is refused, on a
/// component, a container and the root file.
#[test]
fn a_known_block_key_with_a_wrong_value_is_refused() {
    for (note, line, key) in [
        (SKILL, "claude: {metadata: 3}", "claude.metadata"),
        (PLUGIN, "claude: {author: 5}", "claude.author"),
        (MARKET, "claude: {author: 5}", "claude.author"),
        (ROOT, "claude: {author: 5}", "claude.author"),
    ] {
        let said = build_error(&vault(&[(note, line)]));
        assert!(said.contains(key), "{note}: {said}");
    }
}

/// An unknown plain key on a container warns and is ignored (section 6
/// rule 3).
#[test]
fn an_unknown_key_on_a_container_warns_and_is_ignored() {
    for note in [MARKET, PLUGIN] {
        let vfs = vault(&[(note, "flavour: mint")]);
        let warnings = build(&vfs);
        assert!(
            warnings
                .iter()
                .any(|w| w.path == note && w.detail.contains("flavour")),
            "{note}: {warnings:?}"
        );
        for path in [plugin_manifest("claude"), ".claude-plugin/marketplace.json"] {
            assert!(
                !read(&vfs, path).unwrap().contains("mint"),
                "{note}: {path}"
            );
        }
        assert!(
            skill_front(&vfs, "claude").get("metadata").is_none(),
            "{note}"
        );
    }
}

/// Section 7 rule 6: a plugin with no `version` builds as 0.1.0 and warns.
#[test]
fn a_plugin_with_no_version_builds_as_0_1_0_with_a_warning() {
    let vfs = MemVfs::new();
    for (path, text) in [
        (ROOT, "---\n---\n"),
        (MARKET, "---\n---\n"),
        (PLUGIN, "---\n---\n"),
        (SKILL, "---\ndescription: Use it.\n---\n"),
    ] {
        let path = Path::new("/v").join(path);
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        vfs.write_file(&path, text.as_bytes()).unwrap();
    }
    let warnings = build(&vfs);
    assert!(
        warnings
            .iter()
            .any(|w| w.path == PLUGIN && w.reason == "missing-version"),
        "{warnings:?}"
    );
    for harness in ["claude", "codex", "cursor", "pi"] {
        assert_eq!(
            str_at(&json(&vfs, plugin_manifest(harness)), "/version"),
            "0.1.0",
            "{harness}"
        );
    }
}

/// Section 7 rule 6: a marketplace with no `version` writes none, and does
/// not warn. With one, it writes it where a manifest has the field.
#[test]
fn a_marketplace_with_no_version_writes_none_and_does_not_warn() {
    let manifests = [
        ".claude-plugin/marketplace.json",
        ".agents/plugins/marketplace.json",
        ".cursor-plugin/marketplace.json",
    ];
    let vfs = vault(&[]);
    let warnings = build(&vfs);
    assert!(warnings.iter().all(|w| w.path != MARKET), "{warnings:?}");
    for path in manifests {
        let manifest = json(&vfs, path);
        assert!(manifest.get("version").is_none(), "{path}: {manifest}");
        assert!(
            manifest.pointer("/metadata/version").is_none(),
            "{path}: {manifest}"
        );
    }

    let vfs = vault(&[(MARKET, "version: '2.0.0'")]);
    build(&vfs);
    let written: Vec<&str> = manifests
        .into_iter()
        .filter(|path| read(&vfs, path).unwrap().contains("2.0.0"))
        .collect();
    assert!(!written.is_empty());
}

/// Section 7 rule 3: inheritance reaches a command. The plugin's `claude:`
/// keys reach Claude Code's `commands/<c>.md`. `author` does not: a command
/// has no such key, and a plugin block's `author` stays with the plugin
/// (rule 4).
#[test]
fn a_plugin_s_block_keys_reach_its_commands() {
    let vfs = vault(&[(
        PLUGIN,
        "author: Ada\nclaude: {author: Bob, model: m1, context: fork}",
    )]);
    let path = Path::new("/v/marketplaces/acme/plugins/tools/commands/deploy/deploy.md");
    vfs.create_dir_all(path.parent().unwrap()).unwrap();
    vfs.write_file(path, b"---\ndescription: Deploy.\n---\n\nBody.\n")
        .unwrap();
    build(&vfs);
    let text = read(&vfs, ".claude-plugin/packs/tools/commands/deploy.md").unwrap();
    let front: Value = serde_yaml::from_str(
        text.strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .unwrap()
            .0,
    )
    .unwrap();
    assert_eq!(str_at(&front, "/model"), "m1", "{text}");
    assert_eq!(str_at(&front, "/context"), "fork", "{text}");
    assert!(front.get("author").is_none(), "{text}");
    assert_eq!(
        str_at(&json(&vfs, plugin_manifest("claude")), "/author/name"),
        "Bob"
    );
}

/// Section 8 rule 3: a block's `metadata` merges with the note's own. Both
/// keys appear, and the block wins a clash, on its harness only.
#[test]
fn a_block_s_metadata_merges_with_the_note_s_own() {
    let vfs = vault(&[(
        SKILL,
        "metadata: {a: note, c: note}\nclaude: {metadata: {b: block, c: block}}",
    )]);
    build(&vfs);
    let claude = skill_front(&vfs, "claude");
    assert_eq!(
        claude["metadata"],
        serde_json::json!({"a": "note", "b": "block", "c": "block"})
    );
    let codex = skill_front(&vfs, "codex");
    assert_eq!(
        codex["metadata"],
        serde_json::json!({"a": "note", "c": "note"})
    );
}
