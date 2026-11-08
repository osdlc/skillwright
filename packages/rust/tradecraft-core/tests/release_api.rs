//! The frontmatter surface end to end: bare keys, unknown keys, harness
//! blocks, links and inheritance, read from the bytes a build writes.

use std::path::{Path, PathBuf};

use tradecraft_core::{
    ExportReport, ExportRequest, MarketplaceConfig, MemVfs, Namespace, PublishReport,
    PublishRequest, UNKNOWN_KEY_REASON, Vfs, discover_components, export_skills, inherit_targets,
    parse_root_file, publish_skill,
};

const MARKET: &str = "/vault/marketplaces/house/skillwright.md";
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
    let mut request = ExportRequest::new("/vault", "/market", MarketplaceConfig::default());
    request.marketplace = Some("house".to_owned());
    request
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
        &{
            let mut value = PublishRequest::new(path, "/dest", target);
            value.skills_root = Some(PathBuf::from("/vault"));
            value.config = config;
            value
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
fn a_notes_own_key_beats_an_inherited_block_and_its_own_block_beats_both() {
    let own = "team: mine\nsecret: 1\nmetadata:\n  owner: me\n";

    // Inherited from the plugin, on Codex, whose manifests read no `metadata`.
    let vfs = vault(
        "",
        "codex:\n  metadata:\n    team: core\n    squad: blue\n",
        own,
    );
    let report = publish(&vfs, REVIEW, "codex");
    let skill = published(&vfs, &report, "SKILL.md");
    assert!(skill.contains("owner: me"), "{skill}");
    assert!(skill.contains("secret: 1"), "{skill}");
    assert!(skill.contains("team: mine"), "{skill}");
    assert!(skill.contains("squad: blue"), "{skill}");
    assert!(!skill.contains("team: core"), "{skill}");

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

    // Both blocks, on Codex: the note's own block keeps the inherited keys it
    // does not set.
    let vfs = vault(
        "",
        "codex:\n  metadata:\n    team: core\n    squad: blue\n",
        "codex:\n  metadata:\n    team: mine\n",
    );
    let report = publish(&vfs, REVIEW, "codex");
    let skill = published(&vfs, &report, "SKILL.md");
    assert!(skill.contains("team: mine"), "{skill}");
    assert!(skill.contains("squad: blue"), "{skill}");
}

#[test]
fn an_inherited_block_never_overrules_a_key_the_note_sets() {
    let block = "claude:\n  model: opus\n  allowed-tools: Read\n  \
                 disable-model-invocation: true\n";
    let own = "model: haiku\nallowed-tools:\n  - Grep\ndisable-model-invocation: false\n";
    let vfs = vault("", block, own);
    let report = publish(&vfs, REVIEW, "claude");
    let skill = published(&vfs, &report, "SKILL.md");
    assert!(skill.contains("model: haiku"), "{skill}");
    assert!(skill.contains("Grep"), "{skill}");
    assert!(!skill.contains("opus"), "{skill}");
    assert!(!skill.contains("Read"), "{skill}");
    assert!(!skill.contains("disable-model-invocation: true"), "{skill}");

    // The same on a build, and on a command.
    build(&vfs);
    let built = text_at(&vfs, CLAUDE_REVIEW);
    assert!(built.contains("model: haiku"), "{built}");
    write(&vfs, COMMAND, &note("ship", "model: haiku\n"));
    let report = publish(&vfs, COMMAND, "claude");
    let command = published(&vfs, &report, "ship.md");
    assert!(command.contains("model: haiku"), "{command}");

    // A note that says nothing takes the block, and a blank key says nothing.
    let vfs = vault("", block, "model:\n");
    let report = publish(&vfs, REVIEW, "claude");
    let skill = published(&vfs, &report, "SKILL.md");
    assert!(skill.contains("model: opus"), "{skill}");
    assert!(skill.contains("disable-model-invocation: true"), "{skill}");
}

/// A marketplace beside vault-level members, as `build` reads them.
fn beside_a_marketplace() -> MemVfs {
    let vfs = MemVfs::new();
    write(
        &vfs,
        "/vault/marketplaces/m/skillwright.md",
        &note(
            "m",
            "license: ISC\ntargets:\n  - claude\nclaude:\n  model: opus\n",
        ),
    );
    write(&vfs, "/vault/plugins/top/skills/t.md", &note("t", ""));
    write(&vfs, "/vault/skills/loose.md", &note("loose", ""));
    vfs
}

#[test]
fn a_vault_level_note_takes_the_one_marketplace_on_publish() {
    let vfs = beside_a_marketplace();
    for path in ["/vault/plugins/top/skills/t.md", "/vault/skills/loose.md"] {
        let report = publish(&vfs, path, "claude");
        let skill = published(&vfs, &report, "SKILL.md");
        assert!(skill.contains("license: ISC"), "{skill}");
        assert!(skill.contains("model: opus"), "{skill}");
        assert!(publish(&vfs, path, "codex").files.is_empty(), "{path}");
    }
}

#[test]
fn a_vault_level_pack_takes_the_one_marketplace_on_publish() {
    let vfs = beside_a_marketplace();
    write(&vfs, "/vault/plugins/top/top.md", &note("top", ""));
    let report = publish(&vfs, "/vault/plugins/top/top.md", "claude");
    let manifest = published(&vfs, &report, "plugin.json");
    assert!(manifest.contains("\"license\": \"ISC\""), "{manifest}");
    assert!(
        publish(&vfs, "/vault/plugins/top/top.md", "codex")
            .files
            .is_empty()
    );
}

/// `publish` of a vault-level note agrees with what `build` counts: the first
/// two cases hold one marketplace, the last two hold two.
#[test]
fn a_note_counts_the_marketplaces_a_build_counts() {
    fn targets_codex(vfs: &MemVfs) -> bool {
        !publish(vfs, "/vault/skills/loose.md", "codex")
            .files
            .is_empty()
    }
    let empty = beside_a_marketplace();
    write(
        &empty,
        "/vault/marketplaces/ghost/skills/empty.md",
        "Plain.\n",
    );
    assert!(!targets_codex(&empty), "a note with no frontmatter");

    let excluded = beside_a_marketplace();
    write(
        &excluded,
        "/vault/skillwright.md",
        "---\nname: v\nbuild:\n  exclude: marketplaces/ghost\n---\n",
    );
    write(
        &excluded,
        "/vault/marketplaces/ghost/skills/x.md",
        &note("x", ""),
    );
    assert!(!targets_codex(&excluded), "an excluded folder");

    let upper = beside_a_marketplace();
    write(
        &upper,
        "/vault/marketplaces/ghost/skills/UP.MD",
        &note("up", ""),
    );
    assert!(targets_codex(&upper), "a note with an upper-case extension");

    let hidden = beside_a_marketplace();
    write(
        &hidden,
        "/vault/marketplaces/.hidden/skills/h.md",
        &note("h", ""),
    );
    assert!(targets_codex(&hidden), "a dot folder");
}

#[test]
fn a_folder_with_no_component_is_not_a_second_marketplace_on_publish() {
    let vfs = beside_a_marketplace();
    write(&vfs, "/vault/marketplaces/n/README.md", "Plain text.\n");
    write(&vfs, "/vault/marketplaces/n/notes/idea.md", "Plain text.\n");
    let report = publish(&vfs, "/vault/skills/loose.md", "claude");
    let skill = published(&vfs, &report, "SKILL.md");
    assert!(skill.contains("license: ISC"), "{skill}");
}

#[test]
fn a_vault_level_note_stays_on_the_root_file_beside_several_marketplaces() {
    let vfs = beside_a_marketplace();
    write(&vfs, "/vault/marketplaces/n/skillwright.md", &note("n", ""));
    let report = publish_as(
        &vfs,
        "/vault/skills/loose.md",
        "claude",
        Some(root_config("license: MIT\n")),
        &Namespace::default(),
    );
    let skill = published(&vfs, &report, "SKILL.md");
    assert!(skill.contains("license: MIT"), "{skill}");
    assert!(!skill.contains("opus"), "{skill}");
}

#[test]
fn a_roster_shows_the_targets_a_build_uses() {
    let vfs = beside_a_marketplace();
    let ns = Namespace::default();
    let mut scan = discover_components(&vfs, Path::new("/vault"), None, &ns).unwrap();
    inherit_targets(&mut scan, &MarketplaceConfig::default());
    let targets = |name: &str| {
        let row = scan.components.iter().find(|c| c.name == name).unwrap();
        row.targets.clone()
    };
    let claude = Some(vec!["claude".to_owned()]);
    assert_eq!(targets("t"), claude);
    assert_eq!(targets("loose"), claude);
}

#[test]
fn a_marketplace_is_matched_by_folder_and_published_under_its_name() {
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
    assert_eq!(market["name"], "acme-house");
    assert!(text_at(&vfs, CLAUDE_REVIEW).contains("license: ISC"));
    assert!(!vfs.exists(Path::new(
        "/market/.agents/packs/tools/skills/review/SKILL.md"
    )));
}

#[test]
fn a_plugin_name_that_differs_from_its_folder_is_refused() {
    let vfs = vault("", "", "");
    write(&vfs, TOOLS, &note("renamed", "version: 1.0.0\n"));
    let err = export_skills(&vfs, &request(), &Namespace::default()).unwrap_err();
    assert!(err.to_string().contains("differs from its folder"), "{err}");
}

#[test]
fn a_plain_note_with_a_containers_shape_inside_a_bundle_is_a_resource() {
    let vfs = vault("", "", "");
    let bundle = "/vault/marketplaces/house/plugins/tools/skills/foo";
    write(&vfs, &format!("{bundle}/foo.md"), &note("foo", ""));
    for nested in ["plugins/bar/bar.md", "marketplaces/bar/skillwright.md"] {
        write(&vfs, &format!("{bundle}/{nested}"), "Plain text.\n");
    }
    write(
        &vfs,
        "/vault/skills/bundle/plugins/deep/deep.md",
        "Plain text.\n",
    );
    write(&vfs, "/vault/skills/bundle/bundle.md", &note("bundle", ""));
    build(&vfs);
    assert!(vfs.exists(Path::new(
        "/market/.claude-plugin/packs/tools/skills/foo/plugins/bar/bar.md"
    )));
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
    assert!(parse_root_file("---\nowner:\nauthor:\n---\n").is_ok());
}

#[test]
fn claudes_arguments_are_not_an_unknown_key_under_a_namespace() {
    for names in ["arguments: file mode\n", "arguments:\n  - file\n"] {
        let vfs = vault("", "", names);
        let report = export_skills(&vfs, &request(), &Namespace::sw()).unwrap();
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| w.detail.contains("arguments")),
            "{:?}",
            report.warnings
        );
    }
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
    let err = export_skills(&vfs, &request(), &Namespace::default()).unwrap_err();
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
        &{
            let mut value = PublishRequest::new(TOOLS, "/dest", "claude");
            value.skills_root = Some(PathBuf::from("/vault"));
            value
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
    named.config = config;
    named.config.name = Some("house".to_owned());
    named.config.owner = Some(serde_json::json!("acme"));
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
fn packs_landing_on_a_source_folder_is_refused() {
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
        let err = build_to(&vfs, "/vault", &root).unwrap_err();
        assert!(err.contains("source folder"), "{packs}: {err}");
        assert!(vfs.exists(Path::new("/vault/plugins/rev/skills/a.md")));
    }
    // Through a nested output too: it is where the folder lands that counts.
    let root = "---\nname: m\nclaude: {packs: rev/skills}\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL]);
    let err = build_to(&vfs, "/vault/plugins", root).unwrap_err();
    assert!(err.contains("refused"), "{err}");
}

#[test]
fn packs_named_like_a_source_folder_is_allowed_where_it_lands_clear_of_it() {
    let root = "---\nname: m\nclaude: {packs: plugins}\n---\n";
    for out in ["/vault/dist", "/market"] {
        let vfs = root_vault(root, &[TOOL_SKILL]);
        build_to(&vfs, out, root).unwrap();
        assert!(
            vfs.exists(Path::new(&format!("{out}/plugins/rev"))),
            "{out}"
        );
        assert!(vfs.exists(Path::new("/vault/plugins/rev/skills/a.md")));
        // A rebuild reads the vault's `plugins/`, not the built one.
        build_to(&vfs, out, root).unwrap();
    }
}

#[test]
fn a_marketplace_notes_packs_meets_every_rail() {
    let market = "marketplaces/house/skillwright.md";
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
    for (packs, refusal) in [(".pi", "path-collision"), ("marketplaces", "source folder")] {
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
    let scope = {
        let mut value = tradecraft_core::VaultScope::new(Path::new("/vault"));
        value.marketplace = Some("house");
        value
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
        &{
            let mut value =
                PublishRequest::new("/vault/plugins/p/skills/hid.draft.md", "/dest", "claude");
            value.skills_root = Some(PathBuf::from("/vault"));
            value
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

const USE_IT: &str = "---\ndescription: Use it.\n---\n\nbody\n";

#[test]
fn a_link_to_a_component_named_for_a_never_ship_folder_builds() {
    let root = "---\nname: m\n---\n";
    let vfs = root_vault(
        root,
        &[
            (
                "plugins/rev/skills/a.md",
                "---\ndescription: Use a.\n---\n\n[[guide]] and [[plugins/rev/skills/docs/ref]]\n",
            ),
            (
                "plugins/rev/skills/b/b.md",
                "---\ndescription: Use b.\n---\n\n[t](../tests/ref.md)\n",
            ),
            ("plugins/docs/skills/guide.md", USE_IT),
            ("plugins/rev/skills/docs/ref.md", "# Ref\n"),
            ("plugins/rev/skills/docs/docs.md", USE_IT),
            ("plugins/rev/skills/tests/ref.md", "# Ref\n"),
            ("plugins/rev/skills/tests/tests.md", USE_IT),
        ],
    );
    build_to(&vfs, "/market", root).unwrap();
}

#[test]
fn a_stray_in_a_component_named_for_a_never_ship_folder_is_still_a_stray() {
    let root = "---\nname: m\n---\n";
    let vfs = root_vault(
        root,
        &[
            TOOL_SKILL,
            ("plugins/tests/z.md", USE_IT),
            ("plugins/docs/misplaced/y.md", USE_IT),
            ("plugins/rev/skills/tests/t.md", USE_IT),
        ],
    );
    let report = discover_components(&vfs, Path::new("/vault"), None, &Namespace::bare()).unwrap();
    let skipped = serde_json::to_string(&report.skipped).unwrap();
    for stray in ["plugins/tests/z.md", "misplaced/y.md"] {
        assert!(skipped.contains(stray), "{stray}: {skipped}");
    }
    // No skill claims `tests` here, so it is a folder that never ships.
    assert!(!skipped.contains("skills/tests/t.md"), "{skipped}");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.reason == "never-ships" && w.path == "plugins/rev/skills/tests/t.md"),
        "{:?}",
        report.warnings
    );
}

#[test]
fn a_build_names_a_described_note_in_a_never_ship_folder() {
    let root = "---\nname: m\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL, ("plugins/rev/docs/x.md", USE_IT)]);
    let report = build_to(&vfs, "/out", root).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.reason == "never-ships" && w.path == "plugins/rev/docs/x.md"),
        "{:?}",
        report.warnings
    );
}

#[test]
fn a_described_note_in_a_never_ship_folder_warns_that_it_will_not_ship() {
    let root = "---\nname: m\n---\n";
    let vfs = root_vault(
        root,
        &[
            TOOL_SKILL,
            ("plugins/rev/docs/x.md", USE_IT),
            ("docs/plugins/q/skills/w.md", USE_IT),
            ("plugins/rev/docs/plain.md", "# Just a page\n"),
            ("docs/outside.md", USE_IT),
        ],
    );
    let report = discover_components(&vfs, Path::new("/vault"), None, &Namespace::bare()).unwrap();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let warned: Vec<&str> = report
        .warnings
        .iter()
        .filter(|warning| warning.reason == "never-ships")
        .map(|warning| warning.path.as_str())
        .collect();
    assert_eq!(
        warned,
        ["docs/plugins/q/skills/w.md", "plugins/rev/docs/x.md"]
    );
}

#[test]
fn a_nested_note_under_a_deep_glob_names_the_pattern() {
    let root = "---\nname: m\nbuild:\n  patterns:\n    skills: notes/**\n---\n";
    let vfs = root_vault(root, &[("notes/d/y.md", USE_IT)]);
    let report = discover_components(&vfs, Path::new("/vault"), None, &Namespace::bare()).unwrap();
    let skipped = serde_json::to_string(&report.skipped).unwrap();
    assert!(
        skipped.contains("`build.patterns` reads it as `skills/d/y.md`"),
        "{skipped}"
    );
}

#[test]
fn an_os_file_in_a_moved_packs_folder_does_not_block_a_rebuild() {
    let root = "---\nname: m\nclaude: {packs: built}\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL]);
    build_to(&vfs, "/vault", root).unwrap();
    for junk in [".DS_Store", "Thumbs.db", "rev/.DS_Store", "gone/.DS_Store"] {
        write(&vfs, &format!("/vault/built/{junk}"), "x");
        let report = build_to(&vfs, "/vault", root).unwrap();
        assert!(!report.changed, "{junk}");
    }
}

#[test]
fn a_user_file_in_a_moved_packs_folder_is_named() {
    let root = "---\nname: m\nclaude: {packs: built}\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL]);
    build_to(&vfs, "/vault", root).unwrap();
    write(&vfs, "/vault/built/mine.txt", "mine");
    let err = build_to(&vfs, "/vault", root).unwrap_err();
    assert!(err.contains("foreign-tree"), "{err}");
    assert!(
        err.contains("`built/mine.txt` is not part of a pack"),
        "{err}"
    );
    assert_eq!(text_at(&vfs, "/vault/built/mine.txt"), "mine");
}

#[test]
fn a_packs_inside_the_stage_folder_is_refused() {
    for packs in [".skillwright-stage", ".skillwright-stage/x"] {
        let root = format!("---\nname: m\nclaude: {{packs: {packs}}}\n---\n");
        let vfs = root_vault(&root, &[TOOL_SKILL]);
        let err = build_to(&vfs, "/market", &root).unwrap_err();
        assert!(err.contains("path-collision"), "{packs}: {err}");
    }
}

#[test]
fn packs_differing_in_unicode_case_or_form_collide() {
    for (a, b) in [("\u{c9}crit", "\u{e9}crit"), ("caf\u{e9}", "cafe\u{301}")] {
        let root = format!("---\nname: m\nclaude: {{packs: {a}}}\ncodex: {{packs: {b}}}\n---\n");
        let vfs = root_vault(&root, &[TOOL_SKILL]);
        let err = build_to(&vfs, "/market", &root).unwrap_err();
        assert!(err.contains("path-collision"), "{a}: {err}");
    }
}

/// A workspace at `/ws` whose root file is `workspace`, with the member `a`.
fn workspace(workspace: &str, member: &str) -> (MemVfs, ExportRequest) {
    let vfs = MemVfs::new();
    write(&vfs, "/ws/skillwright.md", workspace);
    write(&vfs, "/ws/marketplaces/a/skillwright.md", member);
    write(
        &vfs,
        "/ws/marketplaces/a/plugins/rev/skills/keep.md",
        USE_IT,
    );
    write(
        &vfs,
        "/ws/marketplaces/a/plugins/rev/skills/h.draft.md",
        USE_IT,
    );
    let mut build = request();
    build.vault_root = PathBuf::from("/ws/marketplaces/a");
    build.marketplace = None;
    build.config = tradecraft_core::settings::root_file_config(member, "a").unwrap();
    (vfs, build)
}

#[test]
fn a_workspace_exclusion_reaches_its_members() {
    let shared = "---\nworkspace: true\nbuild:\n  exclude: '**/*.draft.md'\n---\n";
    let (vfs, build) = workspace(shared, "---\nname: a\n---\n");
    export_skills(&vfs, &build, &Namespace::bare()).unwrap();
    let built = "/market/.claude-plugin/packs/rev/skills";
    assert!(vfs.exists(Path::new(&format!("{built}/keep/SKILL.md"))));
    assert!(!vfs.exists(Path::new(&format!("{built}/h-draft/SKILL.md"))));
    assert!(!vfs.exists(Path::new(&format!("{built}/h.draft/SKILL.md"))));
}

#[test]
fn a_members_own_exclusion_replaces_the_workspaces() {
    let shared = "---\nworkspace: true\nbuild:\n  exclude: '**/*.draft.md'\n---\n";
    let member = "---\nname: a\nbuild:\n  exclude: '**/keep.md'\n---\n";
    let (vfs, build) = workspace(shared, member);
    export_skills(&vfs, &build, &Namespace::bare()).unwrap();
    let built = "/market/.claude-plugin/packs/rev/skills/keep/SKILL.md";
    assert!(!vfs.exists(Path::new(built)));
}

#[test]
fn a_malformed_workspace_file_refuses_a_member_build() {
    let (vfs, build) = workspace("---\nworkspace: [\n---\n", "---\nname: a\n---\n");
    let err = export_skills(&vfs, &build, &Namespace::bare()).unwrap_err();
    assert!(err.to_string().contains("malformed-root-file"), "{err}");
}

#[test]
fn a_pattern_under_a_marketplace_ships_only_from_that_marketplace() {
    let root = "---\nbuild:\n  patterns:\n    plugins: marketplaces/a/packs/*\n---\n";
    let vfs = root_vault(
        root,
        &[
            (
                "marketplaces/a/skillwright.md",
                "---\nname: a\ntargets: [claude-code]\n---\n",
            ),
            ("marketplaces/a/packs/rev/skills/s.md", USE_IT),
            ("marketplaces/b/skillwright.md", "---\nname: b\n---\n"),
            ("marketplaces/b/plugins/own/skills/t.md", USE_IT),
        ],
    );
    let mut build = request();
    build.marketplace = Some("b".to_owned());
    build.config = tradecraft_core::settings::root_file_config(root, "house").unwrap();
    export_skills(&vfs, &build, &Namespace::bare()).unwrap();
    assert!(vfs.exists(Path::new(
        "/market/.claude-plugin/packs/own/.claude-plugin/plugin.json"
    )));
    assert!(!vfs.exists(Path::new("/market/.claude-plugin/packs/rev")));

    build.marketplace = Some("a".to_owned());
    build.out_root = PathBuf::from("/market-a");
    export_skills(&vfs, &build, &Namespace::bare()).unwrap();
    let manifest = "/market-a/.claude-plugin/packs/rev/.claude-plugin/plugin.json";
    assert!(vfs.exists(Path::new(manifest)));
    // The marketplace note's `targets` reach the plugin.
    let codex = "/market-a/.agents/plugins/marketplace.json";
    assert!(!vfs.exists(Path::new(codex)) || !text_at(&vfs, codex).contains("rev"));
}

#[test]
fn a_named_publish_of_a_note_in_a_never_ship_folder_is_refused() {
    let vfs = root_vault(
        "---\nname: m\n---\n",
        &[
            ("docs/y.md", USE_IT),
            ("plugins/rev/docs/x.md", USE_IT),
            ("plugins/docs/skills/g.md", USE_IT),
        ],
    );
    for note in ["/vault/docs/y.md", "/vault/plugins/rev/docs/x.md"] {
        let request = {
            let mut value = PublishRequest::new(note, "/dest", "claude-code");
            value.skills_root = Some(PathBuf::from("/vault"));
            value
        };
        let err = publish_skill(&vfs, &request, &Namespace::default()).unwrap_err();
        assert!(err.to_string().contains("excluded-note"), "{note}: {err}");
    }
    publish(&vfs, "/vault/plugins/docs/skills/g.md", "claude-code");
}

#[test]
fn an_output_inside_a_pattern_plugin_folder_is_refused() {
    let root = "---\nname: m\nbuild:\n  patterns:\n    plugins: packs/*\n---\n";
    let vfs = root_vault(root, &[("packs/rev/skills/a.md", USE_IT)]);
    let err = build_to(&vfs, "/vault/packs/rev", root).unwrap_err();
    assert!(err.contains("unsafe-root"), "{err}");
}

#[test]
fn an_unterminated_root_file_refuses_a_build() {
    let root = "---\nbuild:\n  exclude: '**/*.draft.md'\n";
    let vfs = root_vault(root, &[TOOL_SKILL]);
    let mut build = request();
    build.marketplace = None;
    let err = export_skills(&vfs, &build, &Namespace::bare()).unwrap_err();
    assert!(err.to_string().contains("malformed-root-file"), "{err}");
}

fn view_of<'v>(vfs: &'v dyn Vfs, config: &MarketplaceConfig) -> tradecraft_core::SourceView<'v> {
    let scope = tradecraft_core::VaultScope::new(Path::new("/vault"));
    tradecraft_core::source_view(
        tradecraft_core::SourceView::over(vfs),
        scope,
        Some(Path::new("/vault")),
        config,
        &Namespace::bare(),
    )
}

#[test]
fn a_moved_folder_holding_real_notes_is_not_hidden() {
    let root = "---\nname: m\nclaude: {packs: x/guides}\nbuild:\n  patterns:\n    skills: '*/guides/*'\n---\n";
    let vfs = root_vault(root, &[TOOL_SKILL, ("x/guides/g.md", USE_IT)]);
    let config = tradecraft_core::settings::root_file_config(root, "house").unwrap();
    let view = view_of(&vfs, &config);
    assert!(view.exists(Path::new("/vault/x/guides/g.md")));
}

#[test]
fn several_marketplaces_each_hide_their_own_moved_folder() {
    let root = "---\nname: m\n---\n";
    let vfs = root_vault(
        root,
        &[
            (
                "marketplaces/a/skillwright.md",
                "---\nname: a\nclaude: {packs: mva}\n---\n",
            ),
            (
                "marketplaces/b/skillwright.md",
                "---\nname: b\nclaude: {packs: mvb}\n---\n",
            ),
            ("marketplaces/a/plugins/p/skills/s.md", USE_IT),
            ("mva/p/.claude-plugin/plugin.json", "{}"),
            ("mva/p/skills/s/SKILL.md", USE_IT),
            ("mvb/p/.claude-plugin/plugin.json", "{}"),
        ],
    );
    let config = tradecraft_core::settings::root_file_config(root, "house").unwrap();
    let view = view_of(&vfs, &config);
    assert!(!view.exists(Path::new("/vault/mva/p/skills/s/SKILL.md")));
    assert!(!view.exists(Path::new("/vault/mvb/p/.claude-plugin/plugin.json")));
}

/// `load_marketplace` on a member named after a kind folder loads that
/// member, not the workspace above it, and `export_skills` builds it.
#[test]
fn load_marketplace_loads_a_member_named_after_a_kind_folder() {
    for name in ["skills", "agents", "plugins", "skillwright"] {
        let vfs = MemVfs::new();
        write(&vfs, "/w/skillwright.md", "---\nworkspace: true\n---\n");
        let member = format!("/w/marketplaces/{name}");
        write(
            &vfs,
            &format!("{member}/skillwright.md"),
            &format!("---\nname: {name}\nbuild:\n  out: dist\n---\n"),
        );
        write(
            &vfs,
            &format!("{member}/plugins/p/skills/a.md"),
            "---\ndescription: A.\n---\n\nX.\n",
        );
        let loaded =
            tradecraft_core::load_marketplace(&vfs, Path::new(&member), Path::new("/home"))
                .unwrap()
                .unwrap();
        assert_eq!(loaded.dir, PathBuf::from(&member), "{name}");
        assert!(!loaded.root.workspace, "{name}");
        let mut req = request();
        req.vault_root = loaded.dir.clone();
        req.out_root = loaded.out_root.clone().unwrap();
        req.config = loaded.inputs.config.clone();
        req.marketplace = Some(name.to_owned());
        export_skills(&vfs, &req, &loaded.namespace).unwrap();
        assert!(
            vfs.exists(&PathBuf::from(format!(
                "{member}/dist/.claude-plugin/marketplace.json"
            ))),
            "{name}"
        );
    }
}

/// A root file that does not parse is refused with one token, whichever
/// entry point reads it: `load_marketplace` and a build agree.
#[test]
fn a_malformed_root_file_is_refused_with_one_token_everywhere() {
    for (root, member) in [
        ("---\nname: [\n---\n", None),
        ("---\nworkspace: [\n---\n", Some("---\nname: a\n---\n")),
    ] {
        let vfs = MemVfs::new();
        let start = match member {
            None => {
                write(&vfs, "/vault/skillwright.md", root);
                "/vault"
            }
            Some(member) => {
                write(&vfs, "/ws/skillwright.md", root);
                write(&vfs, "/ws/marketplaces/a/skillwright.md", member);
                "/ws/marketplaces/a"
            }
        };
        let err = tradecraft_core::load_marketplace(&vfs, Path::new(start), Path::new("/home"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("malformed-root-file: "), "{start}: {err}");
    }
}
