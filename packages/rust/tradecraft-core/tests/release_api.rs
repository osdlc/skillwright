//! The frontmatter surface end to end: bare keys, unknown keys, harness
//! blocks, links and inheritance, read from the bytes a build writes.

use std::path::{Path, PathBuf};

use tradecraft_core::{
    ExportMode, ExportReport, ExportRequest, MarketplaceConfig, MemVfs, UNKNOWN_KEY_REASON, Vfs,
    export_skills, parse_root_file,
};

const ROOT: &str = "/vault/skillwright.md";
const HOUSE: &str = "/vault/marketplaces/house";
const MARKET: &str = "/vault/marketplaces/house/house.md";
const TOOLS: &str = "/vault/marketplaces/house/plugins/tools/tools.md";
const REVIEW: &str = "/vault/marketplaces/house/plugins/tools/skills/review/review.md";
const OTHER: &str = "/vault/marketplaces/house/plugins/other/other.md";
const DRAFT: &str = "/vault/marketplaces/house/plugins/other/skills/draft/draft.md";

const CLAUDE_REVIEW: &str = "/market/.claude-plugin/packs/tools/skills/review/SKILL.md";

fn write(vfs: &MemVfs, path: &str, content: &str) {
    vfs.insert(path, content).unwrap();
}

fn text_at(vfs: &dyn Vfs, path: &str) -> String {
    String::from_utf8(vfs.read_file(Path::new(path)).unwrap()).unwrap()
}

fn json_at(vfs: &dyn Vfs, path: &str) -> serde_json::Value {
    serde_json::from_str(&text_at(vfs, path)).unwrap()
}

fn note(name: &str, extra: &str) -> String {
    format!("---\ndescription: Use {name}.\n{extra}---\n\n# {name}\n")
}

/// A marketplace of two plugins, one skill each, with `market`, `tools` and
/// `review` spliced into those notes' frontmatter.
fn vault(market: &str, tools: &str, review: &str) -> MemVfs {
    let vfs = MemVfs::new();
    write(&vfs, ROOT, "---\n---\n");
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
    let mut request = ExportRequest::new("/vault", "/market", MarketplaceConfig::default());
    request.marketplace = Some("house".to_owned());
    request
}

fn build(vfs: &dyn Vfs) -> ExportReport {
    export_skills(vfs, &request()).unwrap()
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
    export_skills(&vfs, &narrowed).unwrap();
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

const COMMAND: &str = "/vault/marketplaces/house/plugins/tools/commands/ship/ship.md";
const AGENT: &str = "/vault/marketplaces/house/plugins/tools/agents/helper/helper.md";

fn root_config(frontmatter: &str) -> MarketplaceConfig {
    parse_root_file(&format!("---\n{frontmatter}---\n"))
        .unwrap()
        .to_config()
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
fn a_marketplace_is_matched_and_published_by_its_folder() {
    let vfs = vault("", "", "");
    write(
        &vfs,
        MARKET,
        &note(
            "acme-house",
            "owner: acme\nlicense: ISC\ntargets:\n  - claude\n",
        ),
    );
    build(&vfs);
    let market = json_at(&vfs, "/market/.claude-plugin/marketplace.json");
    assert_eq!(market["name"], "house");
    assert!(text_at(&vfs, CLAUDE_REVIEW).contains("license: ISC"));
    assert!(!vfs.exists(Path::new(
        "/market/.agents/packs/tools/skills/review/SKILL.md"
    )));
}

#[test]
fn a_plugin_note_setting_name_is_refused() {
    let vfs = vault("", "", "");
    write(&vfs, TOOLS, "---\nname: tools\ndescription: Tools.\n---\n");
    let err = export_skills(&vfs, &request()).unwrap_err();
    assert!(err.to_string().contains("delete it"), "{err}");
}

#[test]
fn a_block_key_goes_where_its_harness_reads_it_or_warns() {
    // Claude's `metadata` is the pack's; Cursor's reaches the member, because
    // Cursor's pack manifest has no such key.
    let vfs = vault(
        "",
        "cursor:\n  metadata:\n    team: core\nclaude:\n  metadata:\n    team: core\n",
        "",
    );
    let report = build(&vfs);
    let cursor = text_at(
        &vfs,
        "/market/.cursor-plugin/packs/tools/skills/review/SKILL.md",
    );
    assert!(cursor.contains("team: core"), "{cursor}");
    assert!(!text_at(&vfs, CLAUDE_REVIEW).contains("team: core"));
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| w.reason == UNKNOWN_KEY_REASON),
        "{:?}",
        report.warnings
    );

    // A subagent's key stays with the container, and warns.
    let vfs = vault("", "claude:\n  permissionMode: plan\n", "");
    let report = build(&vfs);
    assert!(!text_at(&vfs, CLAUDE_REVIEW).contains("permissionMode"));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.detail.contains("claude.permissionMode")),
        "{:?}",
        report.warnings
    );

    // A note's own block is still read: the author asked for it.
    let vfs = vault("", "", "claude:\n  permissionMode: plan\n");
    build(&vfs);
    assert!(text_at(&vfs, CLAUDE_REVIEW).contains("permissionMode: plan"));
}

#[test]
fn a_root_block_key_that_reaches_no_file_warns() {
    let root = parse_root_file(
        "---\ncursor:\n  description: x\ncodex:\n  description: x\n  version: 9.9.9\n\
         pi:\n  description: x\nclaude:\n  description: x\n---\n",
    )
    .unwrap();
    let warned = |key: &str| {
        root.warnings
            .iter()
            .any(|w| w.contains(&format!("`{key}`")))
    };
    for key in [
        "cursor.description",
        "codex.description",
        "codex.version",
        "pi.description",
    ] {
        assert!(warned(key), "{key} in {:?}", root.warnings);
    }
    assert!(!warned("claude.description"), "{:?}", root.warnings);
}

#[test]
fn a_blank_author_is_unset_in_a_note_as_in_the_root_file() {
    let vfs = vault("author:\n", "author:\nowner:\n", "");
    build(&vfs);
    let manifest = json_at(
        &vfs,
        "/market/.claude-plugin/packs/tools/.claude-plugin/plugin.json",
    );
    // Unset everywhere, and `owner` is not inherited, so there is none.
    assert!(manifest.get("author").is_none(), "{manifest}");
    assert!(parse_root_file("---\nauthor:\n---\n").is_ok());
}

#[test]
fn a_dotted_key_of_the_authors_is_published_and_said_to_be() {
    let vfs = vault("", "", "my.key: 1\n");
    let report = build(&vfs);
    let warning = report
        .warnings
        .iter()
        .find(|w| w.reason == UNKNOWN_KEY_REASON)
        .unwrap();
    assert!(
        warning.detail.contains("published under `metadata:`"),
        "{}",
        warning.detail
    );
    assert!(text_at(&vfs, CLAUDE_REVIEW).contains("my.key: 1"));
}

#[test]
fn block_metadata_that_is_not_a_mapping_is_refused() {
    let vfs = vault("", "", "foo: 1\nclaude:\n  metadata: x\n");
    let err = export_skills(&vfs, &request()).unwrap_err();
    assert!(err.to_string().contains("must be a mapping"), "{err}");
}

#[test]
fn allowed_tools_reads_the_specs_string_and_claude_s_comma_form() {
    let vfs = vault("", "", "allowed-tools: Read Bash(git status)\n");
    let report = build(&vfs);
    let skill = text_at(&vfs, CLAUDE_REVIEW);
    let (frontmatter, _) = skill.split_once("\n---\n").unwrap();
    assert!(frontmatter.contains("allowed-tools:"), "{skill}");
    assert!(frontmatter.contains("Bash(git status)"), "{skill}");
    assert!(!frontmatter.contains("metadata:"), "{skill}");
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| w.reason == UNKNOWN_KEY_REASON),
        "{:?}",
        report.warnings
    );

    let vfs = vault("", "", "allowed-tools: Read, Grep\n");
    let report = build(&vfs);
    let skill = text_at(&vfs, CLAUDE_REVIEW);
    assert!(skill.contains("allowed-tools: Read Grep"), "{skill}");
    assert!(!skill.contains("metadata:"), "{skill}");
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| w.reason == UNKNOWN_KEY_REASON),
        "{:?}",
        report.warnings
    );
}

#[test]
fn a_marketplace_note_outranks_the_root_file_for_every_identity_key() {
    // `vault` gives the note `owner: acme`.
    let vfs = vault(
        "author: Note Author\ndisplay_name: Note House\nlicense: ISC\nversion: '2.0.0'\n\
         targets: [codex, cursor]\nrepository: https://note.example/repo\n\
         homepage: https://note.example\n",
        "",
        "",
    );
    let mut request = request();
    request.config = root_config(
        "author: Root Author\nlicense: MIT\ntargets: [claude]\n\
         repository: https://root.example/repo\nhomepage: https://root.example\n",
    );
    request.config.name = None;
    export_skills(&vfs, &request).unwrap();
    assert!(!vfs.exists(Path::new(CLAUDE_REVIEW)));
    let codex = json_at(
        &vfs,
        "/market/.agents/packs/tools/.codex-plugin/plugin.json",
    );
    assert_eq!(codex["license"], "ISC");
    assert_eq!(codex["author"], serde_json::json!({"name": "Note Author"}));
    let cursor = json_at(
        &vfs,
        "/market/.cursor-plugin/packs/tools/.cursor-plugin/plugin.json",
    );
    assert_eq!(cursor["repository"], "https://note.example/repo");
    assert_eq!(cursor["homepage"], "https://note.example");
    let codex_index = json_at(&vfs, "/market/.agents/plugins/marketplace.json");
    assert_eq!(codex_index["interface"]["displayName"], "Note House");
    let cursor_index = json_at(&vfs, "/market/.cursor-plugin/marketplace.json");
    assert_eq!(cursor_index["owner"], serde_json::json!({"name": "acme"}));
    assert_eq!(cursor_index["metadata"]["version"], "2.0.0");
}

#[test]
fn the_warning_says_what_happens_to_each_kind() {
    let vfs = vault("", "", "");
    write(&vfs, AGENT, &note("helper", "team: core\nmood: calm\n"));
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
}

#[test]
fn an_author_of_the_wrong_shape_is_refused() {
    let vfs = vault("", "author: 5\n", "");
    let err = export_skills(&vfs, &request()).unwrap_err();
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

/// Writes `text` at `rel` under the marketplace `house`, and a plain note for
/// the plugin `rel` sits in when it has none.
fn add(vfs: &MemVfs, rel: &str, text: &str) {
    if let Some(plugin) = rel
        .strip_prefix("plugins/")
        .and_then(|rest| rest.split('/').next())
    {
        let note = format!("{HOUSE}/plugins/{plugin}/{plugin}.md");
        if !vfs.exists(Path::new(&note)) {
            write(vfs, &note, "---\n---\n");
        }
    }
    write(vfs, &format!("{HOUSE}/{rel}"), text);
}

/// A vault of the marketplace `house` with `root_file` at the root, and
/// `files` as (path, text) under `marketplaces/house/`.
fn root_vault(root_file: &str, files: &[(&str, &str)]) -> MemVfs {
    let vfs = MemVfs::new();
    write(&vfs, ROOT, root_file);
    write(&vfs, MARKET, "---\nowner: acme\n---\n");
    for (path, text) in files {
        add(&vfs, path, text);
    }
    vfs
}

/// A build of the vault's one marketplace, named `house`.
fn build_root(vfs: &dyn Vfs, config: MarketplaceConfig) -> Result<ExportReport, String> {
    let mut named = request();
    named.marketplace = None;
    named.config = config;
    named.config.name = Some("house".to_owned());
    named.config.identity.owner = Some(serde_json::json!("acme"));
    export_skills(vfs, &named).map_err(|err| err.to_string())
}

#[test]
fn a_body_link_into_a_bundles_docs_is_refused() {
    let body = "---\ndescription: Use review.\n---\n\nSee [[docs/plan]].\n";
    let vfs = root_vault(
        "---\n---\n",
        &[
            ("plugins/tools/skills/review/review.md", body),
            ("plugins/tools/skills/review/docs/plan.md", "# Plan\n"),
        ],
    );
    let err = build_root(&vfs, request().config).unwrap_err();
    assert!(err.contains("excluded-link"), "{err}");
}

/// Two plugins, each with a `review`-calling or `review` skill. The call
/// names its path, so a second `review` does not make it ambiguous.
fn pi_vault() -> MemVfs {
    let caller = "---\ndescription: Use lead.\n---\n\nRun /[[tools/skills/review/review]].\n";
    root_vault(
        "---\n---\n",
        &[
            ("plugins/tools/skills/review/review.md", &note("review", "")),
            ("plugins/tools/skills/lead/lead.md", caller),
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
    plugin_note(&vfs, "tools", "pi:\n  naming: bare\n");
    build_root(&vfs, request().config).unwrap();
    assert!(vfs.exists(Path::new("/market/.pi/packs/tools/skills/review/SKILL.md")));

    let vfs = pi_vault();
    let err = build_root(&vfs, pi_block("naming", "short")).unwrap_err();
    assert!(err.contains("unknown-naming"), "{err}");
}

#[test]
fn bare_names_that_collide_across_plugins_are_refused() {
    let vfs = pi_vault();
    add(
        &vfs,
        "plugins/other/skills/review/review.md",
        &note("review", ""),
    );
    let err = build_root(&vfs, pi_block("naming", "bare")).unwrap_err();
    assert!(err.contains("path-collision"), "{err}");
}

#[test]
fn a_bare_pi_collision_names_the_bare_setting() {
    let vfs = pi_vault();
    add(
        &vfs,
        "plugins/other/skills/review/review.md",
        &note("review", ""),
    );
    let err = build_root(&vfs, pi_block("naming", "bare")).unwrap_err();
    assert!(err.contains("`pi.naming: bare`"), "{err}");
    assert!(!err.contains("by default"), "{err}");
}

/// Plugins `p1` and `p2`, each holding `hello`. `p2` also holds `lead`, which
/// calls and mentions its own `hello` and calls `p1`'s, each by path.
fn cursor_vault() -> MemVfs {
    let lead = "---\ndescription: Use lead.\n---\n\n\
                Run /[[p2/skills/hello/hello]]. See [[p2/skills/hello/hello]]. \
                Then /[[p1/skills/hello/hello]].\n";
    root_vault(
        "---\n---\n",
        &[
            ("plugins/p1/skills/hello/hello.md", &note("hello", "")),
            ("plugins/p2/skills/hello/hello.md", &note("hello", "")),
            ("plugins/p2/skills/lead/lead.md", lead),
        ],
    )
}

fn cursor_block(key: &str, value: &str) -> MarketplaceConfig {
    let mut config = request().config;
    let block = serde_json::Map::from_iter([(key.to_owned(), serde_json::json!(value))]);
    config.blocks.insert("cursor".to_owned(), block);
    config
}

/// `plugin`'s note in a [`root_vault`], with `extra` in its frontmatter.
fn plugin_note(vfs: &MemVfs, plugin: &str, extra: &str) {
    write(
        vfs,
        &format!("{HOUSE}/plugins/{plugin}/{plugin}.md"),
        &note(plugin, extra),
    );
}

const CURSOR_PACKS: &str = "/market/.cursor-plugin/packs";

fn cursor_skill(vfs: &dyn Vfs, plugin: &str, folder: &str) -> Option<String> {
    let path = format!("{CURSOR_PACKS}/{plugin}/skills/{folder}/SKILL.md");
    vfs.exists(Path::new(&path)).then(|| text_at(vfs, &path))
}

#[test]
fn cursor_names_are_bare_by_default() {
    let vfs = cursor_vault();
    build_root(&vfs, request().config).unwrap();
    for plugin in ["p1", "p2"] {
        let hello = cursor_skill(&vfs, plugin, "hello").expect(plugin);
        assert!(hello.contains("name: hello\n"), "{hello}");
    }
}

#[test]
fn a_marketplace_can_prefix_cursor_names() {
    let vfs = cursor_vault();
    build_root(&vfs, cursor_block("naming", "prefixed")).unwrap();
    for plugin in ["p1", "p2"] {
        let hello = cursor_skill(&vfs, plugin, &format!("{plugin}-hello")).expect(plugin);
        assert!(
            hello.contains(&format!("name: {plugin}-hello\n")),
            "{hello}"
        );
        assert!(cursor_skill(&vfs, plugin, "hello").is_none(), "{plugin}");
    }
}

#[test]
fn a_plugin_s_cursor_naming_overrides_the_marketplace_s() {
    let vfs = cursor_vault();
    plugin_note(&vfs, "p1", "cursor:\n  naming: bare\n");
    build_root(&vfs, cursor_block("naming", "prefixed")).unwrap();
    assert!(cursor_skill(&vfs, "p1", "hello").is_some());
    assert!(cursor_skill(&vfs, "p2", "p2-hello").is_some());

    let vfs = cursor_vault();
    plugin_note(&vfs, "p2", "cursor:\n  naming: prefixed\n");
    build_root(&vfs, request().config).unwrap();
    assert!(cursor_skill(&vfs, "p1", "hello").is_some());
    assert!(cursor_skill(&vfs, "p2", "p2-hello").is_some());
}

#[test]
fn a_body_names_the_prefixed_cursor_skill() {
    let vfs = cursor_vault();
    plugin_note(&vfs, "p2", "cursor:\n  naming: prefixed\n");
    build_root(&vfs, request().config).unwrap();
    let lead = cursor_skill(&vfs, "p2", "p2-lead").unwrap();
    assert!(
        lead.contains("Run `/p2-hello`. See `p2-hello`. Then `/hello`."),
        "{lead}"
    );
    let claude = text_at(&vfs, "/market/.claude-plugin/packs/p2/skills/lead/SKILL.md");
    assert!(claude.contains("Run `/p2:hello`."), "{claude}");
    let codex = text_at(&vfs, "/market/.agents/packs/p2/skills/lead/SKILL.md");
    assert!(codex.contains("p2:hello"), "{codex}");
}

#[test]
fn a_bad_cursor_naming_is_refused() {
    let vfs = cursor_vault();
    plugin_note(&vfs, "p1", "cursor:\n  naming: short\n");
    let err = build_root(&vfs, request().config).unwrap_err();
    assert!(err.contains("unknown-naming"), "{err}");
    assert!(err.contains("`cursor.naming` of plugin `p1`"), "{err}");
}

#[test]
fn cursor_naming_is_not_an_unknown_key() {
    let warned =
        |report: &ExportReport, key: &str| report.warnings.iter().any(|w| w.detail.contains(key));
    let vfs = vault(
        "cursor:\n  naming: prefixed\n",
        "cursor:\n  naming: bare\n",
        "",
    );
    let report = build(&vfs);
    assert!(!warned(&report, "cursor.naming"), "{:?}", report.warnings);

    for harness in ["claude", "codex"] {
        let vfs = vault("", &format!("{harness}:\n  naming: bare\n"), "");
        let report = build(&vfs);
        assert!(
            warned(&report, &format!("{harness}.naming")),
            "{:?}",
            report.warnings
        );
        let vfs = vault(&format!("{harness}:\n  naming: bare\n"), "", "");
        let report = build(&vfs);
        assert!(
            warned(&report, &format!("{harness}.naming")),
            "{:?}",
            report.warnings
        );
    }
}

#[test]
fn cursor_naming_never_reaches_a_manifest() {
    let vfs = cursor_vault();
    plugin_note(&vfs, "p2", "cursor:\n  naming: prefixed\n");
    build_root(&vfs, cursor_block("naming", "prefixed")).unwrap();
    let manifest = json_at(
        &vfs,
        &format!("{CURSOR_PACKS}/p2/.cursor-plugin/plugin.json"),
    );
    assert!(manifest.get("naming").is_none(), "{manifest}");
    let catalog = json_at(&vfs, "/market/.cursor-plugin/marketplace.json");
    assert!(catalog.get("naming").is_none(), "{catalog}");
    assert!(entry(&catalog, "p2").get("naming").is_none(), "{catalog}");
}

#[test]
fn a_prefixed_cursor_name_that_meets_a_bare_one_is_refused() {
    let vfs = root_vault(
        "---\n---\n",
        &[
            ("plugins/p/skills/hello/hello.md", &note("hello", "")),
            ("plugins/x/skills/p-hello/p-hello.md", &note("p-hello", "")),
        ],
    );
    plugin_note(&vfs, "p", "cursor:\n  naming: prefixed\n");
    let err = build_root(&vfs, request().config).unwrap_err();
    assert!(err.contains("path-collision"), "{err}");
    for part in [
        "plugins/p/skills/hello/hello.md",
        "plugins/x/skills/p-hello/p-hello.md",
        "`p-hello`",
        "cursor.naming",
    ] {
        assert!(err.contains(part), "{part}: {err}");
    }
}

/// Only Cursor admits these skills, so Pi's default prefix cannot be what
/// refuses them.
#[test]
fn two_prefixed_cursor_names_that_meet_are_refused() {
    let vfs = root_vault(
        "---\n---\n",
        &[
            (
                "plugins/a/skills/b-c/b-c.md",
                &note("b-c", "targets: [cursor]\n"),
            ),
            (
                "plugins/a-b/skills/c/c.md",
                &note("c", "targets: [cursor]\n"),
            ),
        ],
    );
    let err = build_root(&vfs, cursor_block("naming", "prefixed")).unwrap_err();
    assert!(err.contains("path-collision"), "{err}");
    for part in [
        "Cursor skill `a-b-c`",
        "`cursor.naming: prefixed` on the marketplace",
    ] {
        assert!(err.contains(part), "{part}: {err}");
    }
}

/// The bare plugin sorts first, so the rule must read the second plugin's
/// naming too.
#[test]
fn a_prefixed_cursor_name_after_a_bare_one_is_refused() {
    let vfs = root_vault(
        "---\n---\n",
        &[
            (
                "plugins/a/skills/z-hello/z-hello.md",
                &note("z-hello", "targets: [cursor]\n"),
            ),
            (
                "plugins/z/skills/hello/hello.md",
                &note("hello", "targets: [cursor]\n"),
            ),
        ],
    );
    plugin_note(&vfs, "z", "cursor:\n  naming: prefixed\n");
    let err = build_root(&vfs, request().config).unwrap_err();
    assert!(err.contains("path-collision"), "{err}");
    for part in [
        "Cursor skill `z-hello`",
        "`cursor.naming: prefixed` on plugin `z`",
    ] {
        assert!(err.contains(part), "{part}: {err}");
    }
}

#[test]
fn bare_cursor_duplicates_stay_allowed_beside_a_prefixed_plugin() {
    let vfs = cursor_vault();
    add(&vfs, "plugins/p3/skills/hello/hello.md", &note("hello", ""));
    plugin_note(&vfs, "p2", "cursor:\n  naming: prefixed\n");
    build_root(&vfs, request().config).unwrap();
    assert!(cursor_skill(&vfs, "p1", "hello").is_some());
    assert!(cursor_skill(&vfs, "p3", "hello").is_some());
    assert!(cursor_skill(&vfs, "p2", "p2-hello").is_some());
}

#[test]
fn a_prefixed_cursor_name_over_the_limit_refuses_the_build() {
    let plugin = "p".repeat(33);
    let skill = "s".repeat(31);
    let vfs = root_vault(
        "---\n---\n",
        &[(
            &format!("plugins/{plugin}/skills/{skill}/{skill}.md"),
            &note(&skill, "targets: [cursor]\n"),
        )],
    );
    plugin_note(&vfs, &plugin, "cursor:\n  naming: prefixed\n");
    let err = build_root(&vfs, request().config).unwrap_err();
    assert!(err.contains("published-name-invalid"), "{err}");
}

#[test]
fn an_explicit_default_naming_records_nothing() {
    let vfs = cursor_vault();
    plugin_note(
        &vfs,
        "p1",
        "pi:\n  naming: prefixed\ncursor:\n  naming: bare\n",
    );
    let mut config = request().config;
    config.name = Some("house".to_owned());
    config.identity.owner = Some(serde_json::json!("acme"));
    let market = tradecraft_core::model::load(
        &vfs,
        &tradecraft_core::model::LoadRequest::new("/vault", config),
    )
    .unwrap();
    for plugin in market.plugins.values() {
        assert!(plugin.bare_on.is_empty(), "{}", plugin.name);
        assert!(plugin.prefixed_on.is_empty(), "{}", plugin.name);
    }
}

// --- pack location, once resolved ------------------------------------------

/// A build of the vault's one marketplace into `out`, configured by
/// `root_file`.
fn build_to(vfs: &dyn Vfs, out: &str, root_file: &str) -> Result<ExportReport, String> {
    let mut request = request();
    request.marketplace = None;
    request.out_root = PathBuf::from(out);
    request.config = tradecraft_core::settings::root_file_config(root_file).unwrap();
    export_skills(vfs, &request).map_err(|err| err.to_string())
}

const TOOL_SKILL: (&str, &str) = (
    "plugins/rev/skills/a/a.md",
    "---\ndescription: Use a.\n---\n\nbody\n",
);

// --- links into folders that never ship -----------------------------------

#[test]
fn every_spelling_of_a_link_to_what_never_ships_is_refused_alike() {
    let shipped = |body: &str| format!("---\ndescription: Use it.\n---\n\n{body}\n");
    let skill = "plugins/rev/skills/a/a.md";
    let plugin_docs = "marketplaces/house/plugins/rev/docs/pg.md";
    let skill_docs = "marketplaces/house/plugins/rev/skills/a/docs";
    for (body, target) in [
        ("![[pg]]", plugin_docs.to_owned()),
        ("[[docs/pg]]", plugin_docs.to_owned()),
        ("[x](../../docs/pg.md#part)", plugin_docs.to_owned()),
        ("[x]: ../../docs/pg.md", plugin_docs.to_owned()),
        ("[x](docs/in.md)", format!("{skill_docs}/in.md")),
        ("[[docs/in]]", format!("{skill_docs}/in.md")),
        ("![x](docs/in%20a.png)", format!("{skill_docs}/in a.png")),
    ] {
        let text = shipped(body);
        let vfs = root_vault(
            "---\n---\n",
            &[
                (skill, &text),
                ("plugins/rev/docs/pg.md", "# Page\n"),
                ("plugins/rev/skills/a/docs/in.md", "# In\n"),
                ("plugins/rev/skills/a/docs/in a.png", "png"),
            ],
        );
        let err = build_to(&vfs, "/market", "---\n---\n").unwrap_err();
        assert!(err.contains("excluded-link"), "{body}: {err}");
        assert!(err.contains(&format!("links to {target}")), "{body}: {err}");
    }
}

#[test]
fn a_link_that_is_not_one_or_that_ships_is_left_alone() {
    let body = "---\ndescription: Use it.\n---\n\n`[c](docs/in.md)` <!-- [c](docs/in.md) --> \
                [web](https://example.com/docs/in.md) [ok](references/r.md) \
                [gone](docs/nope.md)\n\n```\n[c](docs/in.md)\n```\n";
    let vfs = root_vault(
        "---\n---\n",
        &[
            ("plugins/rev/skills/a/a.md", body),
            ("plugins/rev/skills/a/references/r.md", "# R\n"),
            ("plugins/rev/skills/a/docs/in.md", "# In\n"),
        ],
    );
    build_to(&vfs, "/market", "---\n---\n").unwrap();
}

#[test]
fn a_skill_or_plugin_named_for_a_never_ship_folder_links_inside_itself() {
    let vfs = root_vault(
        "---\n---\n",
        &[
            (
                "plugins/docs/skills/a/a.md",
                "---\ndescription: Use a.\n---\n\nSee [[b]].\n",
            ),
            (
                "plugins/docs/skills/b/b.md",
                "---\ndescription: Use b.\n---\n\nbody\n",
            ),
            (
                "plugins/rev/skills/tests/tests.md",
                "---\ndescription: Use tests.\n---\n\nSee [[references/r]].\n",
            ),
            ("plugins/rev/skills/tests/references/r.md", "# R\n"),
        ],
    );
    build_to(&vfs, "/market", "---\n---\n").unwrap();
}

const USE_IT: &str = "---\ndescription: Use it.\n---\n\nbody\n";

#[test]
fn a_link_to_a_component_named_for_a_never_ship_folder_builds() {
    let vfs = root_vault(
        "---\n---\n",
        &[
            (
                "plugins/rev/skills/a/a.md",
                "---\ndescription: Use a.\n---\n\n[[guide]] and \
                 [[plugins/rev/skills/docs/references/ref]]\n",
            ),
            (
                "plugins/rev/skills/b/b.md",
                "---\ndescription: Use b.\n---\n\n[t](../tests/references/ref.md)\n",
            ),
            ("plugins/docs/skills/guide/guide.md", USE_IT),
            ("plugins/rev/skills/docs/references/ref.md", "# Ref\n"),
            ("plugins/rev/skills/docs/docs.md", USE_IT),
            ("plugins/rev/skills/tests/references/ref.md", "# Ref\n"),
            ("plugins/rev/skills/tests/tests.md", USE_IT),
        ],
    );
    build_to(&vfs, "/market", "---\n---\n").unwrap();
}

// --- marketplaces and the root file ---------------------------------------

#[test]
fn a_build_of_one_marketplace_ships_only_that_marketplace() {
    let vfs = MemVfs::new();
    write(&vfs, ROOT, "---\n---\n");
    for (path, text) in [
        ("a/a.md", "---\ntargets: [claude]\n---\n"),
        ("a/plugins/rev/rev.md", "---\n---\n"),
        ("a/plugins/rev/skills/s/s.md", USE_IT),
        ("b/b.md", "---\n---\n"),
        ("b/plugins/own/own.md", "---\n---\n"),
        ("b/plugins/own/skills/t/t.md", USE_IT),
    ] {
        write(&vfs, &format!("/vault/marketplaces/{path}"), text);
    }
    let mut build = request();
    build.marketplace = Some("b".to_owned());
    export_skills(&vfs, &build).unwrap();
    assert!(vfs.exists(Path::new(
        "/market/.claude-plugin/packs/own/.claude-plugin/plugin.json"
    )));
    assert!(!vfs.exists(Path::new("/market/.claude-plugin/packs/rev")));

    build.marketplace = Some("a".to_owned());
    build.out_root = PathBuf::from("/market-a");
    export_skills(&vfs, &build).unwrap();
    let manifest = "/market-a/.claude-plugin/packs/rev/.claude-plugin/plugin.json";
    assert!(vfs.exists(Path::new(manifest)));
    // The marketplace note's `targets` reach the plugin.
    let codex = "/market-a/.agents/plugins/marketplace.json";
    assert!(!vfs.exists(Path::new(codex)) || !text_at(&vfs, codex).contains("rev"));
}

#[test]
fn an_output_inside_marketplaces_is_refused() {
    let vfs = root_vault("---\n---\n", &[TOOL_SKILL]);
    let err = build_to(&vfs, "/vault/marketplaces/house/plugins/rev", "---\n---\n").unwrap_err();
    assert!(err.contains("unsafe-root"), "{err}");
}

/// A marketplace named `skillwright` holds a note named `skillwright.md`,
/// which is not a root file: `load_vault` loads it and a build writes it to
/// `dist/skillwright/`.
#[test]
fn a_marketplace_named_skillwright_loads_and_builds() {
    let vfs = MemVfs::new();
    write(&vfs, "/w/skillwright.md", "---\n---\n");
    let market = "/w/marketplaces/skillwright";
    write(&vfs, &format!("{market}/skillwright.md"), "---\n---\n");
    write(&vfs, &format!("{market}/plugins/p/p.md"), "---\n---\n");
    write(
        &vfs,
        &format!("{market}/plugins/p/skills/a/a.md"),
        "---\ndescription: A.\n---\n\nX.\n",
    );
    let loaded =
        tradecraft_core::load_vault(&vfs, Path::new("/w"), None, Path::new("/home")).unwrap();
    let found = loaded.marketplace("skillwright").unwrap();
    assert_eq!(found.dir, PathBuf::from(market));
    assert_eq!(found.out_root, PathBuf::from("/w/dist/skillwright"));
    let request = loaded.export_request(found, ExportMode::Build);
    export_skills(&vfs, &request).unwrap();
    assert!(vfs.exists(Path::new(
        "/w/dist/skillwright/.claude-plugin/marketplace.json"
    )));
}

/// A root file that does not parse, or has no closing `---`, is refused by
/// `load_vault` with one token.
#[test]
fn a_malformed_root_file_is_refused_with_one_token() {
    for root in [
        "---\nlicense: [\n---\n",
        "---\nbuild:\n  harnesses: [claude]\n",
    ] {
        let vfs = root_vault(root, &[TOOL_SKILL]);
        let err = tradecraft_core::load_vault(&vfs, Path::new("/vault"), None, Path::new("/home"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("malformed-root-file: "), "{root}: {err}");
    }
}

/// Each harness's manifest of `plugin`, by harness.
fn plugin_manifests(vfs: &dyn Vfs, plugin: &str) -> [(&'static str, serde_json::Value); 4] {
    [
        (
            "claude",
            json_at(
                vfs,
                &format!("/market/.claude-plugin/packs/{plugin}/.claude-plugin/plugin.json"),
            ),
        ),
        (
            "codex",
            json_at(
                vfs,
                &format!("/market/.agents/packs/{plugin}/.codex-plugin/plugin.json"),
            ),
        ),
        (
            "cursor",
            json_at(
                vfs,
                &format!("/market/.cursor-plugin/packs/{plugin}/.cursor-plugin/plugin.json"),
            ),
        ),
        (
            "pi",
            json_at(vfs, &format!("/market/.pi/packs/{plugin}/package.json")),
        ),
    ]
}

/// `tools` sets nothing, so it shows every inherited field and the default
/// `version`: no plugin takes the marketplace's `version`.
#[test]
fn a_plugin_takes_exactly_the_inheritable_fields_from_its_marketplace() {
    // `vault` gives the note `owner: acme`.
    let vfs = vault(
        "author: Ada\ndisplay_name: House\nlicense: MIT\nversion: '3.0.0'\n\
         repository: https://house.example/repo\nhomepage: https://house.example\n",
        "",
        "",
    );
    write(
        &vfs,
        OTHER,
        &note(
            "other",
            "author: Bo\ndisplay_name: Other Pack\nlicense: ISC\nversion: '2.0.0'\n\
             repository: https://other.example/repo\nhomepage: https://other.example\n\
             keywords: [k]\ncategory: Testing\n",
        ),
    );
    build(&vfs);
    for (harness, manifest) in plugin_manifests(&vfs, "tools") {
        assert_eq!(manifest["author"]["name"], "Ada", "{harness}");
        assert_eq!(manifest["license"], "MIT", "{harness}");
        assert_eq!(
            manifest["repository"], "https://house.example/repo",
            "{harness}"
        );
        assert_eq!(manifest["homepage"], "https://house.example", "{harness}");
        assert_eq!(manifest["version"], "0.1.0", "{harness}");
        assert!(manifest.get("displayName").is_none(), "{harness}");
        let keywords = if harness == "pi" {
            serde_json::json!(["pi-package"])
        } else {
            serde_json::json!([])
        };
        assert_eq!(manifest["keywords"], keywords, "{harness}");
        if harness == "codex" {
            assert_eq!(manifest["interface"]["displayName"], "tools");
            assert_eq!(manifest["interface"]["category"], "Productivity");
        }
    }
    for (harness, manifest) in plugin_manifests(&vfs, "other") {
        assert_eq!(manifest["author"]["name"], "Bo", "{harness}");
        assert_eq!(manifest["license"], "ISC", "{harness}");
        assert_eq!(
            manifest["repository"], "https://other.example/repo",
            "{harness}"
        );
        assert_eq!(manifest["homepage"], "https://other.example", "{harness}");
        assert_eq!(manifest["version"], "2.0.0", "{harness}");
        let keywords = if harness == "pi" {
            serde_json::json!(["k", "pi-package"])
        } else {
            serde_json::json!(["k"])
        };
        assert_eq!(manifest["keywords"], keywords, "{harness}");
        match harness {
            "codex" => {
                assert_eq!(manifest["interface"]["displayName"], "Other Pack");
                assert_eq!(manifest["interface"]["category"], "Testing");
            }
            "cursor" => assert_eq!(manifest["displayName"], "Other Pack"),
            _ => {}
        }
    }
}
