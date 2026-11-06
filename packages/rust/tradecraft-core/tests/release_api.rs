//! The frontmatter surface end to end: bare keys, unknown keys, harness
//! blocks, links and inheritance, read from the bytes a build writes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tradecraft_core::{
    ExportMode, ExportReport, ExportRequest, MarketplaceConfig, MemVfs, Namespace, Vfs,
    export_skills,
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
