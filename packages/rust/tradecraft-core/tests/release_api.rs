//! The frontmatter surface end to end: bare keys, unknown keys, harness
//! blocks, links and inheritance, read from the bytes a build writes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tradecraft_core::{
    ExportMode, ExportReport, ExportRequest, MarketplaceConfig, MemVfs, Namespace, PublishMode,
    PublishReport, PublishRequest, UNKNOWN_KEY_REASON, Vfs, export_skills, parse_root_file,
    publish_skill,
};

const MARKET: &str = "/vault/marketplaces/house/house.md";
const TOOLS: &str = "/vault/marketplaces/house/plugins/tools/tools.md";
const REVIEW: &str = "/vault/marketplaces/house/plugins/tools/skills/review.md";
const OTHER: &str = "/vault/marketplaces/house/plugins/other/other.md";
const DRAFT: &str = "/vault/marketplaces/house/plugins/other/skills/draft.md";

const CLAUDE_REVIEW: &str = "/market/.claude-plugin/packs/tools/skills/review/SKILL.md";

fn write(vfs: &dyn Vfs, path: &str, content: &str) {
    let path = Path::new(path);
    vfs.create_dir_all(path.parent().unwrap()).unwrap();
    vfs.write_file(path, content.as_bytes()).unwrap();
}

fn text_at(vfs: &dyn Vfs, path: &str) -> String {
    String::from_utf8(vfs.read_file(Path::new(path)).unwrap()).unwrap()
}

fn json_at(vfs: &dyn Vfs, path: &str) -> serde_json::Value {
    serde_json::from_str(&text_at(vfs, path)).unwrap()
}

fn note(name: &str, extra: &str) -> String {
    format!("---\nname: {name}\ndescription: Use {name}.\n{extra}---\n\n# {name}\n")
}

/// A marketplace of two plugins, one skill each, with `market`, `tools` and
/// `review` spliced into those notes' frontmatter.
fn vault(market: &str, tools: &str, review: &str) -> MemVfs {
    let vfs = MemVfs::new();
    write(
        &vfs,
        MARKET,
        &note("house", &format!("owner: acme\n{market}")),
    );
    write(&vfs, TOOLS, &note("tools", tools));
    write(&vfs, REVIEW, &note("review", review));
    write(&vfs, OTHER, &note("other", "license: Apache-2.0\n"));
    write(&vfs, DRAFT, &note("draft", ""));
    vfs
}

fn request() -> ExportRequest {
    ExportRequest {
        vault_root: PathBuf::from("/vault"),
        skills_root: None,
        marketplace: Some("house".to_owned()),
        out_root: PathBuf::from("/market"),
        harnesses: BTreeSet::new(),
        mode: ExportMode::Build,
        config: MarketplaceConfig {
            name: None,
            display_name: None,
            owner: None,
            description: None,
            author: None,
            license: None,
            version: None,
            targets: None,
            repository: None,
            homepage: None,
            blocks: Default::default(),
            plugins: BTreeMap::new(),
            require_descriptions: false,
            launcher_path: None,
        },
        assets: Vec::new(),
        hooks: Vec::new(),
    }
}

fn build(vfs: &dyn Vfs) -> ExportReport {
    export_skills(vfs, &request(), &Namespace::bare()).unwrap()
}

fn entry<'a>(manifest: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    manifest["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|plugin| plugin["name"] == name)
        .unwrap()
}

#[test]
fn a_bare_note_needs_no_prefix_and_none_of_its_keys_reach_a_harness() {
    let vfs = vault("", "", "targets:\n  - claude\n");
    build(&vfs);
    let skill = text_at(&vfs, CLAUDE_REVIEW);
    assert!(!skill.contains("targets"), "{skill}");
    assert!(!vfs.exists(Path::new(
        "/market/.agents/packs/tools/skills/review/SKILL.md"
    )));
}

#[test]
fn an_unknown_key_warns_and_travels_under_metadata_as_yaml() {
    let vfs = vault("", "", "team:\n  lead: kelly\n");
    let report = build(&vfs);

    let warning = report
        .warnings
        .iter()
        .find(|w| w.reason == "unknown-key")
        .expect("the key is reported");
    assert!(warning.detail.contains("`team`"), "{}", warning.detail);

    let skill = text_at(&vfs, CLAUDE_REVIEW);
    assert!(
        skill.contains("metadata:\n  team:\n    lead: kelly\n"),
        "{skill}"
    );
}

#[test]
fn an_unknown_key_on_a_container_warns_and_goes_nowhere() {
    let vfs = vault("", "flavour: mild\n", "");
    let report = build(&vfs);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.reason == "unknown-key" && w.detail.contains("`flavour`")),
        "{:?}",
        report.warnings
    );
    assert!(
        !text_at(
            &vfs,
            "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json"
        )
        .contains("flavour")
    );
}

#[test]
fn a_removed_key_is_refused_naming_the_folder_rule() {
    let vfs = vault("", "", "kind: skill\n");
    let err = export_skills(&vfs, &request(), &Namespace::bare())
        .unwrap_err()
        .to_string();
    assert!(err.contains("removed-key"), "{err}");
    assert!(err.contains("`kind`"), "{err}");
}

#[test]
fn repository_and_homepage_reach_every_manifest_with_the_field() {
    let vfs = vault(
        "repository: https://example.com/house\nhomepage: https://example.com\n",
        "",
        "",
    );
    build(&vfs);

    let claude = json_at(
        &vfs,
        "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
    );
    assert_eq!(claude["repository"], "https://example.com/house");
    assert_eq!(claude["homepage"], "https://example.com");

    let pi = json_at(&vfs, "/market/.pi/packs/tools/package.json");
    assert_eq!(pi["repository"], "https://example.com/house");
    assert_eq!(pi["homepage"], "https://example.com");
}

#[test]
fn a_plugins_own_link_outranks_the_one_it_would_inherit() {
    let vfs = vault(
        "repository: https://example.com/house\n",
        "repository: https://example.com/tools\n",
        "",
    );
    build(&vfs);
    let tools = json_at(
        &vfs,
        "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
    );
    assert_eq!(tools["repository"], "https://example.com/tools");
    let other = json_at(
        &vfs,
        "/market/.claude-plugin/packs/other/.claude-plugin/plugin.json",
    );
    assert_eq!(other["repository"], "https://example.com/house");
}

#[test]
fn a_license_inherits_down_to_the_skill_and_stops_at_a_plugins_own() {
    let vfs = vault("license: MIT\n", "", "");
    build(&vfs);
    assert!(text_at(&vfs, CLAUDE_REVIEW).contains("license: MIT"));
    let draft = text_at(
        &vfs,
        "/market/.claude-plugin/packs/other/skills/draft/SKILL.md",
    );
    assert!(draft.contains("license: Apache-2.0"), "{draft}");
}

#[test]
fn a_skills_own_license_is_left_alone() {
    let vfs = vault("license: MIT\n", "", "license: ISC\n");
    build(&vfs);
    assert!(text_at(&vfs, CLAUDE_REVIEW).contains("license: ISC"));
}

#[test]
fn targets_inherit_and_a_nearer_list_replaces_them() {
    let vfs = vault(
        "targets:\n  - claude\n",
        "targets:\n  - claude\n  - codex\n",
        "",
    );
    build(&vfs);
    assert!(vfs.exists(Path::new(
        "/market/.agents/packs/tools/skills/review/SKILL.md"
    )));
    assert!(!vfs.exists(Path::new(
        "/market/.agents/packs/other/skills/draft/SKILL.md"
    )));
}

#[test]
fn the_root_files_targets_reach_a_plugin_that_sets_none() {
    let vfs = vault("", "targets:\n  - codex\n", "");
    let mut narrowed = request();
    narrowed.config.targets = Some(vec!["claude".to_owned()]);
    export_skills(&vfs, &narrowed, &Namespace::bare()).unwrap();
    assert!(vfs.exists(Path::new(
        "/market/.agents/packs/tools/skills/review/SKILL.md"
    )));
    assert!(!vfs.exists(Path::new(
        "/market/.agents/packs/other/skills/draft/SKILL.md"
    )));
}

#[test]
fn a_harness_block_reaches_that_harness_alone() {
    let vfs = vault("", "", "claude:\n  permissionMode: plan\n");
    build(&vfs);
    assert!(text_at(&vfs, CLAUDE_REVIEW).contains("permissionMode: plan"));
    let codex = text_at(&vfs, "/market/.agents/packs/tools/skills/review/SKILL.md");
    assert!(!codex.contains("permissionMode"), "{codex}");
}

#[test]
fn a_plugin_block_merges_over_the_generated_entry_and_manifest() {
    let vfs = vault(
        "",
        "codex:\n  policy:\n    installation: INSTALLED_BY_DEFAULT\n  category: Writing\n",
        "",
    );
    build(&vfs);
    let market = json_at(&vfs, "/market/.agents/plugins/marketplace.json");
    let tools = entry(&market, "tools");
    assert_eq!(tools["policy"]["installation"], "INSTALLED_BY_DEFAULT");
    assert_eq!(tools["category"], "Writing");
    assert_ne!(entry(&market, "other")["category"], "Writing");
}

#[test]
fn a_marketplace_block_inherits_key_by_key_and_keeps_its_own_top_keys() {
    let vfs = vault(
        "codex:\n  interface:\n    displayName: The House\n  category: Shared\n",
        "codex:\n  category: Mine\n",
        "",
    );
    build(&vfs);
    let market = json_at(&vfs, "/market/.agents/plugins/marketplace.json");
    assert_eq!(market["interface"]["displayName"], "The House");
    assert_eq!(entry(&market, "tools")["category"], "Mine");
    assert_eq!(entry(&market, "other")["category"], "Shared");
    let pack = text_at(
        &vfs,
        "/market/.agents/packs/other/.codex-plugin/plugin.json",
    );
    assert!(!pack.contains("The House"), "{pack}");
}

#[test]
fn a_block_key_a_harness_does_not_read_on_a_container_warns() {
    let vfs = vault("", "codex:\n  nonsense: 1\n", "");
    let report = build(&vfs);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.detail.contains("codex.nonsense")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn under_a_namespace_the_prefixed_forms_are_ours_and_the_bare_ones_are_not() {
    let vfs = MemVfs::new();
    write(&vfs, MARKET, &note("house", "sw-owner: acme\n"));
    write(&vfs, TOOLS, &note("tools", "sw-license: MIT\n"));
    write(
        &vfs,
        REVIEW,
        &note(
            "review",
            "sw-claude:\n  permissionMode: plan\nclaude: theirs\n",
        ),
    );
    export_skills(&vfs, &request(), &Namespace::sw()).unwrap();
    let skill = text_at(&vfs, CLAUDE_REVIEW);
    assert!(skill.contains("permissionMode: plan"), "{skill}");
    assert!(skill.contains("claude: theirs"), "{skill}");
    assert!(!skill.contains("sw-claude"), "{skill}");
}

const COMMAND: &str = "/vault/marketplaces/house/plugins/tools/commands/ship.md";
const AGENT: &str = "/vault/marketplaces/house/plugins/tools/agents/helper.md";

fn publish_as(
    vfs: &dyn Vfs,
    path: &str,
    target: &str,
    config: Option<MarketplaceConfig>,
    ns: &Namespace,
) -> PublishReport {
    publish_skill(
        vfs,
        &PublishRequest {
            skill_path: PathBuf::from(path),
            destination_root: PathBuf::from("/dest"),
            target: target.to_owned(),
            mode: PublishMode::Write,
            skills_root: Some(PathBuf::from("/vault")),
            config,
            published_at: None,
            overwrite: Vec::new(),
        },
        ns,
    )
    .unwrap()
}

fn publish(vfs: &dyn Vfs, path: &str, target: &str) -> PublishReport {
    publish_as(vfs, path, target, None, &Namespace::default())
}

fn root_config(frontmatter: &str) -> MarketplaceConfig {
    parse_root_file(&format!("---\n{frontmatter}---\n"))
        .unwrap()
        .to_config("house")
}

/// The one file a report wrote whose path ends with `suffix`, under `/dest`.
fn published(vfs: &dyn Vfs, report: &PublishReport, suffix: &str) -> String {
    let file = report
        .files
        .iter()
        .find(|file| file.path.ends_with(suffix))
        .unwrap_or_else(|| panic!("no {suffix} in {:?}", report.files));
    text_at(vfs, &format!("/dest/{}", file.path))
}

#[test]
fn a_container_block_never_sets_a_members_name_or_description() {
    let vfs = vault(
        "",
        "claude:\n  name: renamed\n  description: Claude's plugin blurb.\n  model: opus\n",
        "",
    );
    write(&vfs, COMMAND, &note("ship", ""));
    build(&vfs);

    let skill = text_at(&vfs, CLAUDE_REVIEW);
    assert!(skill.contains("name: review"), "{skill}");
    assert!(skill.contains("description: Use review."), "{skill}");
    assert!(!skill.contains("Claude's plugin blurb"), "{skill}");
    let command = text_at(&vfs, "/market/.claude-plugin/packs/tools/commands/ship.md");
    assert!(!command.contains("Claude's plugin blurb"), "{command}");
    assert!(!command.contains("renamed"), "{command}");

    // The manifest it was written for still takes it, and a key only a member
    // reads still travels.
    let manifest = json_at(
        &vfs,
        "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
    );
    assert_eq!(manifest["description"], "Claude's plugin blurb.");
    assert!(skill.contains("model: opus"), "{skill}");
}

#[test]
fn a_notes_own_block_can_still_set_its_name_and_description() {
    let vfs = vault("", "", "claude:\n  description: Only for Claude.\n");
    build(&vfs);
    let skill = text_at(&vfs, CLAUDE_REVIEW);
    assert!(skill.contains("description: Only for Claude."), "{skill}");
}

#[test]
fn a_marketplace_block_never_sets_a_plugins_identity() {
    let vfs = vault(
        "cursor:\n  description: The marketplace blurb.\ncodex:\n  version: 9.9.9\n",
        "version: 1.0.0\n",
        "",
    );
    build(&vfs);
    let cursor = json_at(
        &vfs,
        "/market/.cursor-plugin/packs/tools/.cursor-plugin/plugin.json",
    );
    assert_eq!(cursor["description"], "Use tools.");
    let codex = json_at(
        &vfs,
        "/market/.agents/packs/tools/.codex-plugin/plugin.json",
    );
    assert_eq!(codex["version"], "1.0.0");
}

#[test]
fn an_inherited_block_does_not_refuse_an_agent() {
    let vfs = vault("", "claude:\n  model: opus\n", "");
    write(&vfs, AGENT, &note("helper", ""));
    let report = publish(&vfs, AGENT, "claude");
    let agent = published(&vfs, &report, "helper.md");
    assert!(agent.contains("name: helper"), "{agent}");
    assert!(!agent.contains("opus"), "{agent}");
}

#[test]
fn a_block_s_metadata_merges_over_the_notes_own_keys() {
    let own = "team: mine\nsecret: 1\nmetadata:\n  owner: me\n";

    // Inherited from the plugin, on Codex, whose manifests read no `metadata`.
    let vfs = vault("", "codex:\n  metadata:\n    team: core\n", own);
    let report = publish(&vfs, REVIEW, "codex");
    let skill = published(&vfs, &report, "SKILL.md");
    assert!(skill.contains("owner: me"), "{skill}");
    assert!(skill.contains("secret: 1"), "{skill}");
    assert!(skill.contains("team: core"), "{skill}");
    assert!(!skill.contains("team: mine"), "{skill}");

    // The note's own block, on Claude.
    let vfs = vault(
        "",
        "",
        &format!("{own}claude:\n  metadata:\n    team: core\n"),
    );
    let report = publish(&vfs, REVIEW, "claude");
    let skill = published(&vfs, &report, "SKILL.md");
    assert!(skill.contains("owner: me"), "{skill}");
    assert!(skill.contains("secret: 1"), "{skill}");
    assert!(skill.contains("team: core"), "{skill}");
}

#[test]
fn a_single_note_publish_honours_inherited_targets() {
    let vfs = vault("", "targets: [claude]\n", "");
    let report = publish(&vfs, REVIEW, "codex");
    assert!(report.files.is_empty(), "{:?}", report.files);
    assert!(report.not_published.is_some());
    assert!(!publish(&vfs, REVIEW, "claude").files.is_empty());

    // The root file is the last rung.
    let vfs = vault("", "", "");
    let config = root_config("targets: [claude]\n");
    let report = publish_as(&vfs, REVIEW, "codex", Some(config), &Namespace::default());
    assert!(report.not_published.is_some());
}

/// A note's published text without the generated-by line, which only a build
/// writes.
fn without_marker(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with("<!-- Generated by skillwright"))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn one_note_gives_the_same_frontmatter_on_every_path() {
    let vfs = vault(
        "license: MIT\n",
        "license: Apache-2.0\nclaude:\n  model: opus\n",
        "",
    );
    build(&vfs);
    let built = without_marker(&text_at(&vfs, CLAUDE_REVIEW));
    assert!(built.contains("license: Apache-2.0"), "{built}");

    let single = publish(&vfs, REVIEW, "claude");
    let text = published(&vfs, &single, "review/SKILL.md");
    assert_eq!(without_marker(&text), built);

    let vfs = vault(
        "license: MIT\n",
        "license: Apache-2.0\nclaude:\n  model: opus\n",
        "",
    );
    let pack = publish(&vfs, TOOLS, "claude");
    let text = published(&vfs, &pack, "review/SKILL.md");
    assert_eq!(without_marker(&text), built);
}

#[test]
fn a_pack_manifest_carries_what_a_built_one_does() {
    let vfs = vault(
        "repository: https://example.com/house\n",
        "claude:\n  icon: ./i.png\n",
        "",
    );
    build(&vfs);
    let built = json_at(
        &vfs,
        "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
    );
    let pack = publish(&vfs, TOOLS, "claude");
    let manifest: serde_json::Value =
        serde_json::from_str(&published(&vfs, &pack, ".claude-plugin/plugin.json")).unwrap();
    assert_eq!(manifest["repository"], "https://example.com/house");
    assert_eq!(manifest["icon"], "./i.png");
    assert_eq!(manifest, built);
}

#[test]
fn a_pack_honours_targets_its_members_inherit() {
    let vfs = vault("", "", "");
    let config = root_config("targets: [claude]\n");
    let report = publish_as(&vfs, TOOLS, "codex", Some(config), &Namespace::default());
    assert!(report.not_published.is_some(), "{:?}", report.files);
}

#[test]
fn a_plugin_note_with_neither_name_nor_description_is_still_read() {
    let vfs = vault("license: MIT\n", "", "");
    write(
        &vfs,
        OTHER,
        "---\ntargets: []\nversion: 5.0.0\nlicense: GPL-3.0\n---\n",
    );
    write(
        &vfs,
        TOOLS,
        "---\nversion: 5.0.0\nlicense: GPL-3.0\nhomepage: https://example.com\n---\n",
    );
    let report = build(&vfs);
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| w.path.ends_with("tools/tools.md")),
        "{:?}",
        report.warnings
    );
    let manifest = json_at(
        &vfs,
        "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
    );
    assert_eq!(manifest["version"], "5.0.0");
    assert_eq!(manifest["license"], "GPL-3.0");
    assert_eq!(manifest["homepage"], "https://example.com");
    // It describes itself as a plugin with no note would.
    assert_eq!(manifest["description"], "Skills: review.");
    // `targets: []` means nowhere.
    assert!(!vfs.exists(Path::new("/market/.claude-plugin/packs/other")));

    let pack = publish(&vfs, TOOLS, "claude");
    let published: serde_json::Value =
        serde_json::from_str(&published(&vfs, &pack, ".claude-plugin/plugin.json")).unwrap();
    assert_eq!(published, manifest);
}

#[test]
fn a_marketplace_note_outranks_the_root_file_for_every_inherited_key() {
    let vfs = vault(
        "license: ISC\ntargets: [codex]\nrepository: https://note.example\n",
        "",
        "",
    );
    let mut request = request();
    request.config =
        root_config("license: MIT\ntargets: [claude]\nrepository: https://root.example\n");
    request.config.name = None;
    export_skills(&vfs, &request, &Namespace::default()).unwrap();
    assert!(!vfs.exists(Path::new(CLAUDE_REVIEW)));
    let manifest = json_at(
        &vfs,
        "/market/.agents/packs/tools/.codex-plugin/plugin.json",
    );
    assert_eq!(manifest["license"], "ISC");
}

#[test]
fn under_a_namespace_a_bare_key_of_ours_is_warned_about() {
    let vfs = MemVfs::new();
    write(
        &vfs,
        REVIEW,
        &note(
            "review",
            "targets: [nowhere]\nclaude:\n  model: opus\nsw-custom: 1\n",
        ),
    );
    let report = publish_as(&vfs, REVIEW, "claude", None, &Namespace::sw());
    let [warning] = report.warnings.as_slice() else {
        panic!("{:?}", report.warnings);
    };
    assert!(warning.detail.contains("`targets`"), "{warning:?}");
    assert!(warning.detail.contains("`claude`"), "{warning:?}");
    assert!(
        warning.detail.contains("`sw-custom` dropped"),
        "{warning:?}"
    );
}

#[test]
fn a_publish_reports_unknown_keys_and_a_pack_its_members_too() {
    let vfs = vault("", "", "team: core\n");
    let report = publish(&vfs, REVIEW, "claude");
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert_eq!(report.warnings[0].reason, UNKNOWN_KEY_REASON);
    assert!(
        report.warnings[0]
            .detail
            .contains("`team` is not a key skillwright reads on a skill"),
        "{:?}",
        report.warnings
    );
    let report = publish(&vfs, TOOLS, "claude");
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
}

#[test]
fn the_warning_says_what_happens_to_each_kind() {
    let vfs = vault("", "", "");
    write(&vfs, AGENT, &note("helper", "team: core\nmood: calm\n"));
    write(
        &vfs,
        "/vault/marketplaces/house/plugins/tools/hooks/guard.md",
        "---\nname: guard\nevent: PreToolUse\ncommand: run\nmystery: 1\n---\n",
    );
    let report = build(&vfs);
    let detail = |suffix: &str| {
        report
            .warnings
            .iter()
            .find(|w| w.path.ends_with(suffix) && w.reason == UNKNOWN_KEY_REASON)
            .map(|w| w.detail.clone())
            .unwrap_or_else(|| panic!("{:?}", report.warnings))
    };
    let agent = detail("helper.md");
    assert!(agent.contains("are not keys"), "{agent}");
    assert!(agent.contains("on an agent"), "{agent}");
    assert!(
        agent.contains("when the agent publishes on its own"),
        "{agent}"
    );
    assert!(detail("guard.md").ends_with("; ignored"), "{report:?}");
}

#[test]
fn an_author_of_the_wrong_shape_is_refused() {
    let vfs = vault("", "author: 5\n", "");
    let err = export_skills(&vfs, &request(), &Namespace::default()).unwrap_err();
    assert!(err.to_string().contains("`author`"), "{err}");

    let err = parse_root_file("---\nowner: 5\n---\n").unwrap_err();
    assert_eq!(err.key, "owner");
}

#[test]
fn a_bare_arguments_string_is_claudes_names() {
    let vfs = vault("", "", "arguments: file mode\n");
    build(&vfs);
    let skill = text_at(&vfs, CLAUDE_REVIEW);
    assert!(skill.contains("arguments:"), "{skill}");
    assert!(skill.contains("file"), "{skill}");
    assert!(skill.contains("mode"), "{skill}");
}

#[test]
fn an_empty_plugin_names_targets_in_the_vaults_spelling() {
    let vfs = vault("", "targets: [claude]\n", "");
    vfs.remove(Path::new(REVIEW)).unwrap();
    let err = publish_skill(
        &vfs,
        &PublishRequest {
            skill_path: PathBuf::from(TOOLS),
            destination_root: PathBuf::from("/dest"),
            target: "claude".to_owned(),
            mode: PublishMode::Write,
            skills_root: Some(PathBuf::from("/vault")),
            config: None,
            published_at: None,
            overwrite: Vec::new(),
        },
        &Namespace::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("by `targets`"), "{err}");
}

#[test]
fn claude_and_cursor_entries_list_category_and_tags() {
    let vfs = vault("", "category: Testing\nkeywords: [lint, ci]\n", "");
    build(&vfs);
    for manifest in [
        "/market/.claude-plugin/marketplace.json",
        "/market/.cursor-plugin/marketplace.json",
    ] {
        let manifest = json_at(&vfs, manifest);
        let tools = entry(&manifest, "tools");
        assert_eq!(tools["category"], "Testing");
        assert_eq!(tools["tags"], serde_json::json!(["lint", "ci"]));
        // A plugin stating neither adds neither.
        let other = entry(&manifest, "other");
        assert!(other.get("category").is_none(), "{other}");
        assert!(other.get("tags").is_none(), "{other}");
    }
}
