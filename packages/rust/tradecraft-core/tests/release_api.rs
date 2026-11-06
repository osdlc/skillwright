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

/// A vault with a root file, and `files` as (path, text) under `/vault`.
fn root_vault(root_file: &str, files: &[(&str, &str)]) -> MemVfs {
    let vfs = MemVfs::new();
    write(&vfs, "/vault/skillwright.md", root_file);
    for (path, text) in files {
        write(&vfs, &format!("/vault/{path}"), text);
    }
    vfs
}

/// A build of a vault with no `marketplaces/` folder, named `house`.
fn build_root(vfs: &dyn Vfs, config: MarketplaceConfig) -> Result<ExportReport, String> {
    let mut named = request();
    named.marketplace = None;
    named.config = MarketplaceConfig {
        name: Some("house".to_owned()),
        owner: Some(serde_json::json!("acme")),
        ..config
    };
    export_skills(vfs, &named, &Namespace::bare()).map_err(|err| err.to_string())
}

#[test]
fn a_plugin_pattern_builds_a_folder_that_is_not_plugins() {
    let vfs = root_vault(
        "---\nname: house\nbuild:\n  patterns:\n    plugins: packs/*\n---\n",
        &[
            ("packs/tools/tools.md", &note("tools", "version: 1.2.0\n")),
            ("packs/tools/skills/review.md", &note("review", "")),
            ("packs/tools/CHANGELOG.md", "# Changelog\n\n## 1.2.0\n"),
        ],
    );
    build_root(&vfs, request().config).unwrap();
    assert!(vfs.exists(Path::new(CLAUDE_REVIEW)));
    assert!(vfs.exists(Path::new("/market/.claude-plugin/packs/tools/CHANGELOG.md")));
    let manifest = json_at(
        &vfs,
        "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
    );
    assert_eq!(manifest["version"], "1.2.0");
}

#[test]
fn a_skill_pattern_and_an_assets_pattern_map_a_vaults_own_folders() {
    let vfs = root_vault(
        "---\nname: house\nbuild:\n  patterns:\n    plugins: packs/*\n    \
         skills: packs/*/guides/*\n    assets: packs/*/static\n---\n",
        &[
            ("packs/tools/guides/review.md", &note("review", "")),
            ("packs/tools/static/tool.sh", "#!/bin/sh\n"),
        ],
    );
    build_root(&vfs, request().config).unwrap();
    assert!(vfs.exists(Path::new(CLAUDE_REVIEW)));
    assert!(vfs.exists(Path::new("/market/.claude-plugin/packs/tools/tool.sh")));
}

#[test]
fn an_excluded_note_is_not_a_component() {
    let vfs = root_vault(
        "---\nname: house\nbuild:\n  exclude: [\"**/*.draft.md\"]\n---\n",
        &[
            ("plugins/tools/skills/review.md", &note("review", "")),
            ("plugins/tools/skills/next.draft.md", &note("next", "")),
        ],
    );
    build_root(&vfs, request().config).unwrap();
    assert!(vfs.exists(Path::new(CLAUDE_REVIEW)));
    assert!(!vfs.exists(Path::new(
        "/market/.claude-plugin/packs/tools/skills/next/SKILL.md"
    )));
}

#[test]
fn an_excluded_file_in_a_bundle_does_not_ship() {
    let vfs = root_vault(
        "---\nname: house\nbuild:\n  exclude: [\"**/scratch\"]\n---\n",
        &[
            ("plugins/tools/skills/review/review.md", &note("review", "")),
            ("plugins/tools/skills/review/scripts/run.sh", "#!/bin/sh\n"),
            ("plugins/tools/skills/review/scratch/notes.txt", "todo\n"),
        ],
    );
    build_root(&vfs, request().config).unwrap();
    let pack = "/market/.claude-plugin/packs/tools/skills/review";
    assert!(vfs.exists(Path::new(&format!("{pack}/scripts/run.sh"))));
    assert!(!vfs.exists(Path::new(&format!("{pack}/scratch/notes.txt"))));
}

#[test]
fn a_body_link_into_an_excluded_folder_is_refused() {
    let body = "---\nname: review\ndescription: Use review.\n---\n\nSee [[drafts/plan]].\n";
    let vfs = root_vault(
        "---\nname: house\nbuild:\n  exclude: [drafts]\n---\n",
        &[
            ("plugins/tools/skills/review.md", body),
            ("drafts/plan.md", "# Plan\n"),
        ],
    );
    let err = build_root(&vfs, request().config).unwrap_err();
    assert!(err.contains("excluded-link"), "{err}");
    assert!(err.contains("drafts/plan.md"), "{err}");
}

#[test]
fn a_body_link_into_a_bundles_docs_is_refused() {
    let body = "---\nname: review\ndescription: Use review.\n---\n\nSee [[docs/plan]].\n";
    let vfs = root_vault(
        "---\nname: house\n---\n",
        &[
            ("plugins/tools/skills/review/review.md", body),
            ("plugins/tools/skills/review/docs/plan.md", "# Plan\n"),
        ],
    );
    let err = build_root(&vfs, request().config).unwrap_err();
    assert!(err.contains("excluded-link"), "{err}");
}

/// Two plugins, each with a `review`-calling or `review` skill.
fn pi_vault() -> MemVfs {
    let caller = "---\nname: lead\ndescription: Use lead.\n---\n\nRun /[[review]].\n";
    root_vault(
        "---\nname: house\n---\n",
        &[
            ("plugins/tools/skills/review.md", &note("review", "")),
            ("plugins/tools/skills/lead.md", caller),
        ],
    )
}

fn pi_block(key: &str, value: &str) -> MarketplaceConfig {
    let mut config = request().config;
    let block = serde_json::Map::from_iter([(key.to_owned(), serde_json::json!(value))]);
    config.blocks.insert("pi".to_owned(), block);
    config
}

#[test]
fn pi_prefixes_a_skill_with_its_plugin_unless_told_bare() {
    let vfs = pi_vault();
    build_root(&vfs, request().config).unwrap();
    assert!(vfs.exists(Path::new(
        "/market/.pi/packs/tools/skills/tools-review/SKILL.md"
    )));

    let vfs = pi_vault();
    build_root(&vfs, pi_block("naming", "bare")).unwrap();
    let pack = "/market/.pi/packs/tools/skills";
    assert!(vfs.exists(Path::new(&format!("{pack}/review/SKILL.md"))));
    let lead = text_at(&vfs, &format!("{pack}/lead/SKILL.md"));
    assert!(lead.contains("name: lead"), "{lead}");
    assert!(lead.contains("/skill:review"), "{lead}");
    assert!(!lead.contains("tools-review"), "{lead}");
}

#[test]
fn a_plugin_can_ask_for_bare_names_alone_and_a_bad_value_is_refused() {
    let vfs = pi_vault();
    write(
        &vfs,
        "/vault/plugins/tools/tools.md",
        &note("tools", "pi:\n  naming: bare\n"),
    );
    build_root(&vfs, request().config).unwrap();
    assert!(vfs.exists(Path::new("/market/.pi/packs/tools/skills/review/SKILL.md")));

    let vfs = pi_vault();
    let err = build_root(&vfs, pi_block("naming", "short")).unwrap_err();
    assert!(err.contains("unknown-naming"), "{err}");
}

#[test]
fn bare_names_that_collide_across_plugins_are_refused() {
    let vfs = pi_vault();
    write(
        &vfs,
        "/vault/plugins/other/skills/review.md",
        &note("review", ""),
    );
    let err = build_root(&vfs, pi_block("naming", "bare")).unwrap_err();
    assert!(err.contains("path-collision"), "{err}");
}

#[test]
fn a_marketplace_block_moves_a_harnesss_packs_and_its_catalog_follows() {
    let vfs = pi_vault();
    let mut config = request().config;
    config.blocks.insert(
        "codex".to_owned(),
        serde_json::Map::from_iter([("packs".to_owned(), serde_json::json!("plugins-out/codex"))]),
    );
    build_root(&vfs, config).unwrap();
    assert!(vfs.exists(Path::new(
        "/market/plugins-out/codex/tools/skills/review/SKILL.md"
    )));
    assert!(!vfs.exists(Path::new(
        "/market/.agents/packs/tools/skills/review/SKILL.md"
    )));
    let catalog = text_at(&vfs, "/market/.agents/plugins/marketplace.json");
    assert!(catalog.contains("plugins-out/codex/tools"), "{catalog}");
}

#[test]
fn a_packs_folder_must_stay_inside_the_output_and_apart_from_the_others() {
    let vfs = pi_vault();
    let err = build_root(&vfs, pi_block("packs", "../elsewhere")).unwrap_err();
    assert!(err.contains("unsafe-path"), "{err}");

    let err = build_root(&vfs, pi_block("packs", ".agents/packs")).unwrap_err();
    assert!(err.contains("path-collision"), "{err}");
}

// --- pack location, once resolved ------------------------------------------

/// [`build_root`] with the output at `out`.
fn build_to(vfs: &dyn Vfs, out: &str, root_file: &str) -> Result<ExportReport, String> {
    let mut request = request();
    request.marketplace = None;
    request.out_root = PathBuf::from(out);
    request.config = tradecraft_core::settings::root_file_config(root_file, "house").unwrap();
    export_skills(vfs, &request, &Namespace::bare()).map_err(|err| err.to_string())
}

const TOOL_SKILL: (&str, &str) = (
    "plugins/rev/skills/a.md",
    "---\ndescription: Use a.\n---\n\nbody\n",
);

#[test]
fn a_moved_folder_in_an_output_inside_the_vault_rebuilds() {
    let root = "---\nname: m\npi: {packs: moved/pi}\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL]);
    build_to(&vfs, "/vault/dist", root).unwrap();
    assert!(vfs.exists(Path::new("/vault/dist/moved/pi/rev/package.json")));
    build_to(&vfs, "/vault/dist", root).unwrap();
}

#[test]
fn a_block_from_the_request_hides_output_the_root_file_does_not_name() {
    // A workspace's `packs` reaches the build as config, not in the vault's
    // own root file.
    let vfs = root_vault("---\nname: m\n---\n", &[TOOL_SKILL]);
    let root = "---\nname: m\nclaude: {packs: wsp}\n---\n";
    build_to(&vfs, "/vault", root).unwrap();
    assert!(vfs.exists(Path::new("/vault/wsp/rev")));
    build_to(&vfs, "/vault", root).unwrap();
}

#[test]
fn a_built_copy_in_a_moved_folder_never_makes_a_link_ambiguous() {
    let root = "---\nname: m\nclaude: {packs: built/claude}\n---\n";
    let vfs = root_vault(
        root,
        &[
            (
                "plugins/rev/skills/a/a.md",
                "---\ndescription: Use a.\n---\n\nSee [[guide]].\n",
            ),
            ("plugins/rev/skills/a/refs/guide.md", "# Guide\n"),
        ],
    );
    build_to(&vfs, "/vault", root).unwrap();
    assert!(vfs.exists(Path::new("/vault/built/claude/rev/skills/a/refs/guide.md")));
    build_to(&vfs, "/vault", root).unwrap();
}

#[test]
fn packs_on_a_harness_path_is_refused_and_touches_nothing() {
    for packs in [
        ".pi",
        ".pi/skills",
        ".claude-plugin",
        ".claude-plugin/marketplace.json",
        ".agents/plugins",
        ".agents",
        ".cursor",
        ".Claude-Plugin",
    ] {
        for harness in ["pi", "claude"] {
            let root = format!("---\nname: m\n{harness}: {{packs: '{packs}'}}\n---\n");
            let vfs = root_vault(&root, &[TOOL_SKILL]);
            write(&vfs, "/vault/.pi/settings.json", "{}\n");
            let err = build_to(&vfs, "/vault", &root).unwrap_err();
            assert!(err.contains("path-collision"), "{packs}: {err}");
            assert!(
                err.contains(&format!("`{harness}.packs`")),
                "{packs}: {err}"
            );
            assert!(vfs.exists(Path::new("/vault/.pi/settings.json")), "{packs}");
        }
    }
}

#[test]
fn a_harness_may_name_its_own_default_folder() {
    let root = "---\nname: m\nclaude: {packs: .claude-plugin/packs}\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL]);
    build_to(&vfs, "/vault", root).unwrap();
}

#[test]
fn a_windows_path_in_packs_is_unsafe() {
    for packs in ["C:/x", "C:\\\\x", "c:x"] {
        let root = format!("---\nname: m\npi: {{packs: \"{packs}\"}}\n---\n");
        let vfs = root_vault(&root, &[TOOL_SKILL]);
        let err = build_to(&vfs, "/market", &root).unwrap_err();
        assert!(err.contains("unsafe-path"), "{packs}: {err}");
    }
}

#[test]
fn packs_on_a_source_folder_is_refused_when_the_output_overlaps_the_vault() {
    for packs in [
        "plugins",
        "plugins/rev",
        "skills",
        "assets",
        "notes",
        "notes/x",
    ] {
        let root = format!(
            "---\nname: m\nbuild:\n  patterns:\n    skills: notes/*\nclaude: {{packs: {packs}}}\n---\n"
        );
        let vfs = root_vault(&root, &[TOOL_SKILL]);
        for out in ["/vault", "/vault/dist"] {
            let err = build_to(&vfs, out, &root).unwrap_err();
            assert!(err.contains("source folder"), "{packs} at {out}: {err}");
        }
        assert!(vfs.exists(Path::new("/vault/plugins/rev/skills/a.md")));
    }
    // Clear of the vault, the same name is only a folder.
    let root = "---\nname: m\nclaude: {packs: plugins}\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL]);
    build_to(&vfs, "/market", root).unwrap();
    assert!(vfs.exists(Path::new("/market/plugins/rev")));
}

#[test]
fn a_marketplace_notes_packs_meets_every_rail() {
    let market = "marketplaces/house/house.md";
    let skill = "marketplaces/house/plugins/rev/skills/a.md";
    let body = "---\ndescription: Use a.\n---\n\n<!-- Generated by skillwright from x -->\n";
    let marked = (skill, body);

    // The note's value is the one the vault rail sees.
    let vfs = MemVfs::new();
    write(&vfs, "/top/v/skillwright.md", "---\nname: house\n---\n");
    write(
        &vfs,
        &format!("/top/v/{market}"),
        "---\nname: house\nclaude: {packs: v}\n---\n",
    );
    write(&vfs, &format!("/top/v/{skill}"), body);
    let mut asked = request();
    asked.vault_root = PathBuf::from("/top/v");
    asked.out_root = PathBuf::from("/top");
    let err = export_skills(&vfs, &asked, &Namespace::bare())
        .unwrap_err()
        .to_string();
    assert!(err.contains("unsafe-root"), "{err}");
    assert!(vfs.exists(Path::new("/top/v/skillwright.md")));

    // And it hides the output and meets the harness and source checks.
    for (packs, refusal) in [(".pi", "path-collision"), ("plugins", "source folder")] {
        let note = format!("---\nname: house\nclaude: {{packs: {packs}}}\n---\n");
        let vfs = root_vault("---\nname: house\n---\n", &[marked, (market, &note)]);
        let err = build_to(&vfs, "/vault", "---\nname: house\n---\n").unwrap_err();
        assert!(err.contains(refusal), "{packs}: {err}");
    }
    let note = "---\nname: house\nclaude: {packs: built}\n---\n";
    let vfs = root_vault(
        "---\nname: house\n---\n",
        &[(skill, "---\ndescription: d\n---\n\nb\n"), (market, note)],
    );
    build_to(&vfs, "/vault", "---\nname: house\n---\n").unwrap();
    assert!(vfs.exists(Path::new("/vault/built/rev")));
    build_to(&vfs, "/vault", "---\nname: house\n---\n").unwrap();
}

#[test]
fn effective_config_holds_a_notes_blocks_and_only_those() {
    let vfs = vault("claude: {packs: moved}\n", "", "");
    let scope = tradecraft_core::VaultScope {
        vault_root: Path::new("/vault"),
        skills_root: None,
        marketplace: Some("house"),
    };
    let asked = request().config;
    let config = tradecraft_core::effective_config(&vfs, scope, &asked, &Namespace::bare());
    let mut request = request();
    assert_eq!(config.name, None);
    assert_eq!(config.description, None);
    request.config = config;
    let owned = tradecraft_core::owned_paths(&request).unwrap();
    assert!(owned.contains(&("claude", "moved")), "{owned:?}");
}

#[test]
fn a_folder_of_the_users_is_not_taken_over_on_one_marker() {
    let root = "---\nname: m\npi: {packs: stuff}\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL]);
    write(&vfs, "/vault/stuff/mine.txt", "mine\n");
    write(
        &vfs,
        "/vault/stuff/deep/note.md",
        "<!-- Generated by skillwright from x; do not edit. -->\n",
    );
    let err = build_to(&vfs, "/vault", root).unwrap_err();
    assert!(err.contains("foreign-tree"), "{err}");
    assert!(vfs.exists(Path::new("/vault/stuff/mine.txt")));
}

#[test]
fn a_pack_that_was_dropped_from_a_moved_folder_still_rebuilds() {
    let root = "---\nname: m\nclaude: {packs: built}\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL]);
    write(
        &vfs,
        "/vault/plugins/old/skills/b.md",
        "---\ndescription: Use b.\n---\n\nbody\n",
    );
    build_to(&vfs, "/vault", root).unwrap();
    vfs.remove(Path::new("/vault/plugins/old")).unwrap();
    build_to(&vfs, "/vault", root).unwrap();
    assert!(!vfs.exists(Path::new("/vault/built/old")));
}

#[test]
fn output_left_at_the_default_folder_is_reported_and_kept() {
    let root = "---\nname: m\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL]);
    build_to(&vfs, "/market", root).unwrap();
    let moved = "---\nname: m\nclaude: {packs: mv}\n---\n";
    let report = build_to(&vfs, "/market", moved).unwrap();
    let stale = report
        .warnings
        .iter()
        .find(|w| w.reason == "stale-output")
        .unwrap();
    assert_eq!(stale.path, ".claude-plugin/packs");
    assert!(vfs.exists(Path::new("/market/.claude-plugin/packs/rev")));
    assert!(vfs.exists(Path::new("/market/mv/rev")));
}

#[test]
fn output_left_by_a_move_inside_the_vault_is_named_as_the_cause() {
    let first = "---\nname: m\nclaude: {packs: built/claude}\n---\n";
    let vfs = root_vault(first, &[TOOL_SKILL]);
    build_to(&vfs, "/vault", first).unwrap();
    let moved = "---\nname: m\nclaude: {packs: built/other}\n---\n";
    let err = build_to(&vfs, "/vault", moved).unwrap_err();
    assert!(err.contains("built/claude/rev/skills/a/SKILL.md"), "{err}");
    assert!(
        err.contains("no `packs` setting points at its folder"),
        "{err}"
    );
}

// --- patterns, exclusions and links ----------------------------------------

#[test]
fn a_skill_pattern_under_plugins_stays_in_its_plugin() {
    let root = "---\nname: m\nbuild:\n  patterns:\n    skills: plugins/*/guides/*\n---\n";
    let vfs = root_vault(
        root,
        &[
            TOOL_SKILL,
            (
                "plugins/rev/guides/g.md",
                "---\ndescription: Use g.\n---\n\nbody\n",
            ),
        ],
    );
    build_to(&vfs, "/market", root).unwrap();
    assert!(vfs.exists(Path::new(
        "/market/.claude-plugin/packs/rev/skills/g/SKILL.md"
    )));
    assert!(!vfs.exists(Path::new("/market/.claude-plugin/packs/m")));
}

#[test]
fn a_pattern_on_a_folder_that_never_ships_ships_nothing() {
    let root = "---\nname: m\nbuild:\n  patterns:\n    skills: [docs/*, plugins/rev/docs/*]\n---\n";
    let note = "---\ndescription: Use it.\n---\n\nbody\n";
    let vfs = root_vault(
        root,
        &[
            TOOL_SKILL,
            ("docs/d.md", note),
            ("plugins/rev/docs/pd.md", note),
        ],
    );
    let warned = parse_root_file(root).unwrap().warnings.join("\n");
    assert!(warned.contains("`docs/*`"), "{warned}");
    build_to(&vfs, "/market", root).unwrap();
    let skills = "/market/.claude-plugin/packs";
    assert!(vfs.exists(Path::new(&format!("{skills}/rev/skills/a/SKILL.md"))));
    assert!(!vfs.exists(Path::new(&format!("{skills}/rev/skills/pd"))));
    assert!(!vfs.exists(Path::new(&format!("{skills}/m"))));
}

#[test]
fn every_spelling_of_a_link_to_what_never_ships_is_refused_alike() {
    let root = "---\nname: m\nbuild:\n  exclude: private\n---\n";
    let shipped = |body: &str| format!("---\ndescription: Use it.\n---\n\n{body}\n");
    let bundle = "plugins/rev/skills/a/a.md";
    let flat = "plugins/rev/skills/flat.md";
    for (path, body, target) in [
        (flat, "[[docs/guide]]", "docs/guide.md"),
        (flat, "![[pg]]", "plugins/rev/docs/pg.md"),
        (flat, "[x](../../../docs/guide.md)", "docs/guide.md"),
        (flat, "[x](../docs/pg.md#part)", "plugins/rev/docs/pg.md"),
        (flat, "[x]: ../docs/pg.md", "plugins/rev/docs/pg.md"),
        (flat, "[x](../../../private/x.md)", "private/x.md"),
        (bundle, "[x](docs/in.md)", "plugins/rev/skills/a/docs/in.md"),
        (bundle, "[[docs/in]]", "plugins/rev/skills/a/docs/in.md"),
        (bundle, "[x](../../../../private/x.md)", "private/x.md"),
        (
            bundle,
            "![x](docs/in%20a.png)",
            "plugins/rev/skills/a/docs/in a.png",
        ),
    ] {
        let body = shipped(body);
        let vfs = root_vault(
            root,
            &[
                (path, &body),
                ("docs/guide.md", "# Guide\n"),
                ("plugins/rev/docs/pg.md", "# Page\n"),
                ("private/x.md", "# Private\n"),
                ("plugins/rev/skills/a/docs/in.md", "# In\n"),
                ("plugins/rev/skills/a/docs/in a.png", "png"),
            ],
        );
        let err = build_to(&vfs, "/market", root).unwrap_err();
        assert!(err.contains("excluded-link"), "{body}: {err}");
        assert!(err.contains(&format!("links to {target}")), "{body}: {err}");
    }
}

#[test]
fn a_link_that_is_not_one_or_that_ships_is_left_alone() {
    let root = "---\nname: m\nbuild:\n  exclude: private\n---\n";
    let body = "---\ndescription: Use it.\n---\n\n`[c](docs/in.md)` <!-- [c](docs/in.md) --> \
                [web](https://example.com/docs/in.md) [ok](refs/r.md) [gone](docs/nope.md)\n\n\
                ```\n[c](docs/in.md)\n```\n";
    let vfs = root_vault(
        root,
        &[
            ("plugins/rev/skills/a/a.md", body),
            ("plugins/rev/skills/a/refs/r.md", "# R\n"),
            ("plugins/rev/skills/a/docs/in.md", "# In\n"),
        ],
    );
    build_to(&vfs, "/market", root).unwrap();
}

#[test]
fn a_skill_or_plugin_named_for_a_never_ship_folder_links_inside_itself() {
    let root = "---\nname: m\n---\n";
    let vfs = root_vault(
        root,
        &[
            (
                "plugins/docs/skills/a.md",
                "---\ndescription: Use a.\n---\n\nSee [[b]].\n",
            ),
            (
                "plugins/docs/skills/b.md",
                "---\ndescription: Use b.\n---\n\nbody\n",
            ),
            (
                "skills/tests/tests.md",
                "---\ndescription: Use tests.\n---\n\nSee [[refs/r]].\n",
            ),
            ("skills/tests/refs/r.md", "# R\n"),
        ],
    );
    build_to(&vfs, "/market", root).unwrap();
}

#[test]
fn a_deep_skill_glob_builds_each_note_under_the_folder() {
    let root = "---\nname: m\nbuild:\n  patterns:\n    skills: notes/**\n---\n";
    let note = "---\ndescription: Use it.\n---\n\nbody\n";
    let vfs = root_vault(root, &[("notes/a.md", note), ("notes/b/b.md", note)]);
    build_to(&vfs, "/market", root).unwrap();
    for skill in ["a", "b"] {
        let built = format!("/market/.claude-plugin/packs/m/skills/{skill}/SKILL.md");
        assert!(vfs.exists(Path::new(&built)), "{skill}");
    }
}

#[test]
fn a_glob_that_matches_nothing_is_reported_by_the_build() {
    let root = "---\nname: m\nbuild:\n  exclude: nope/*\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL]);
    let report = build_to(&vfs, "/market", root).unwrap();
    assert!(
        report.warnings.iter().any(|w| w.reason == "unmatched-glob"),
        "{:?}",
        report.warnings
    );
}

#[test]
fn a_malformed_root_file_refuses_a_build_rather_than_lose_its_exclusions() {
    let vfs = root_vault("---\nbuild:\n  exclude: '../x'\n---\n", &[TOOL_SKILL]);
    let err = build_to(&vfs, "/market", "---\nname: m\n---\n").unwrap_err();
    assert!(err.contains("malformed-root-file"), "{err}");
}

#[test]
fn naming_an_excluded_note_does_not_publish_it() {
    let root = "---\nname: m\nbuild:\n  exclude: '**/*.draft.md'\n---\n";
    let note = "---\ndescription: Use it.\n---\n\nbody\n";
    let vfs = root_vault(root, &[("plugins/p/skills/hid.draft.md", note)]);
    let err = publish_skill(
        &vfs,
        &PublishRequest {
            skill_path: PathBuf::from("/vault/plugins/p/skills/hid.draft.md"),
            destination_root: PathBuf::from("/dest"),
            target: "claude".to_owned(),
            mode: PublishMode::Write,
            skills_root: Some(PathBuf::from("/vault")),
            config: None,
            published_at: None,
            overwrite: Vec::new(),
        },
        &Namespace::bare(),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("excluded-note"), "{err}");
    assert!(!vfs.exists(Path::new("/dest")));
}

#[test]
fn a_single_note_publish_finds_a_plugin_note_a_pattern_moved() {
    let root = "---\nname: m\nbuild:\n  patterns:\n    plugins: packs/*\n---\n";
    let vfs = root_vault(
        root,
        &[
            (
                "packs/rev/rev.md",
                &note("rev", "license: MIT\nclaude:\n  model: opus\n"),
            ),
            ("packs/rev/skills/a.md", &note("a", "")),
        ],
    );
    let report = publish(&vfs, "/vault/packs/rev/skills/a.md", "claude");
    let text = published(&vfs, &report, "SKILL.md");
    assert!(text.contains("license: MIT"), "{text}");
    assert!(text.contains("model: opus"), "{text}");
}
