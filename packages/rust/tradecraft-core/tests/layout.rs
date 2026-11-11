//! The v2 vault layout (spec sections 1-3) and its errors (section 11), at
//! the core's public API.

use std::path::Path;

use tradecraft_core::{
    CoreError, EntityKind, ExportMode, MemVfs, Vfs, check_vault_root, discover_components,
    export_skills, find_vault_root, load_vault, read_layout,
};

const ROOT: &str = "---\n---\n";
const NOTE: &str = "---\n---\n";
const SKILL: &str = "---\ndescription: Use it.\n---\n\nBody.\n";

/// A vault at `/v` holding `files`, each a root-relative path and its text.
/// The root file and `marketplaces/` are added unless `files` sets them.
fn vault(files: &[(&str, &str)]) -> MemVfs {
    let vfs = MemVfs::new();
    vfs.create_dir_all(Path::new("/v/marketplaces")).unwrap();
    put(&vfs, "skillwright.md", ROOT);
    for (path, text) in files {
        put(&vfs, path, text);
    }
    vfs
}

fn put(vfs: &MemVfs, rel: &str, text: &str) {
    let path = Path::new("/v").join(rel);
    vfs.create_dir_all(path.parent().unwrap()).unwrap();
    vfs.write_file(&path, text.as_bytes()).unwrap();
}

/// The notes of marketplace `m` and plugin `p`, with `files` under the
/// plugin folder.
fn plugin(m: &str, p: &str, files: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut out = vec![
        (format!("marketplaces/{m}/{m}.md"), NOTE.to_owned()),
        (
            format!("marketplaces/{m}/plugins/{p}/{p}.md"),
            NOTE.to_owned(),
        ),
    ];
    for (path, text) in files {
        out.push((
            format!("marketplaces/{m}/plugins/{p}/{path}"),
            (*text).to_owned(),
        ));
    }
    out
}

fn vault_of(files: &[(String, String)]) -> MemVfs {
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    vault(&borrowed)
}

/// The layout refusal for `vfs`, as text.
fn refusal(vfs: &MemVfs) -> String {
    let err = read_layout(vfs, Path::new("/v")).unwrap_err();
    let said = err.to_string();
    assert!(said.contains("refused: layout"), "{said}");
    assert_expected_layout(&said);
    said
}

fn assert_expected_layout(said: &str) {
    assert!(said.contains("The expected layout:"), "{said}");
    assert!(said.contains("skills/<s>/<s>.md"), "{said}");
}

/// Each component as `marketplace/plugin/kind/name`.
fn listed(vfs: &MemVfs) -> Vec<String> {
    let report = discover_components(vfs, Path::new("/v")).unwrap();
    let mut out: Vec<String> = report
        .components
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                EntityKind::Skill | EntityKind::Agent | EntityKind::Command
            )
        })
        .map(|c| {
            format!(
                "{}/{}/{:?}/{}",
                c.marketplace.as_deref().unwrap_or("-"),
                c.plugin.as_deref().unwrap_or("-"),
                c.kind,
                c.name
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn a_vault_lists_every_component_with_its_marketplace_and_plugin() {
    let mut files = plugin(
        "acme",
        "tools",
        &[
            ("skills/review/review.md", SKILL),
            ("agents/triage/triage.md", SKILL),
            ("commands/ship/ship.md", SKILL),
        ],
    );
    files.extend(plugin("beta", "kit", &[("skills/lint/lint.md", SKILL)]));
    let vfs = vault_of(&files);
    assert_eq!(
        listed(&vfs),
        [
            "acme/tools/Agent/triage",
            "acme/tools/Command/ship",
            "acme/tools/Skill/review",
            "beta/kit/Skill/lint",
        ]
    );
}

#[test]
fn files_beside_the_root_file_are_not_read_and_a_build_leaves_them() {
    let mut files = plugin("acme", "tools", &[("skills/review/review.md", SKILL)]);
    for (path, text) in [
        ("README.md", "# Read me\n"),
        ("notes/plain.md", "---\nname: [broken\n"),
        ("notes/marketplaces/x/x.md", "---\nname: [broken\n"),
        ("notes/marketplaces/x/plugins/y/skills/z.md", "loose\n"),
    ] {
        files.push((path.to_owned(), text.to_owned()));
    }
    // The root file's body is not read: a broken note in it changes nothing.
    files.push((
        "skillwright.md".to_owned(),
        "---\n---\n\n[[nowhere]] and ---\nname: [broken\n".to_owned(),
    ));
    let vfs = vault_of(&files);
    assert_eq!(listed(&vfs), ["acme/tools/Skill/review"]);

    let before: Vec<Vec<u8>> = ["README.md", "notes/plain.md", "notes/marketplaces/x/x.md"]
        .iter()
        .map(|rel| vfs.read_file(&Path::new("/v").join(rel)).unwrap())
        .collect();
    let loaded = load_vault(&vfs, Path::new("/v"), None, Path::new("/home")).unwrap();
    let request = loaded.export_request(&loaded.marketplaces[0], ExportMode::Build);
    export_skills(&vfs, &request).unwrap();
    assert!(vfs.exists(Path::new("/v/dist/acme")));
    let after: Vec<Vec<u8>> = ["README.md", "notes/plain.md", "notes/marketplaces/x/x.md"]
        .iter()
        .map(|rel| vfs.read_file(&Path::new("/v").join(rel)).unwrap())
        .collect();
    assert_eq!(before, after);
}

#[test]
fn a_root_without_marketplaces_is_refused() {
    let vfs = MemVfs::new();
    vfs.create_dir_all(Path::new("/v")).unwrap();
    vfs.write_file(Path::new("/v/skillwright.md"), ROOT.as_bytes())
        .unwrap();
    let said = check_vault_root(&vfs, Path::new("/v"))
        .unwrap_err()
        .to_string();
    assert!(said.contains("refused: layout"), "{said}");
    assert!(said.contains("no `marketplaces/`"), "{said}");
    assert_expected_layout(&said);
    assert!(read_layout(&vfs, Path::new("/v")).is_err());
}

#[test]
fn a_skill_named_skillwright_is_not_taken_for_a_root() {
    let files = plugin(
        "acme",
        "tools",
        &[("skills/skillwright/skillwright.md", SKILL)],
    );
    let vfs = vault_of(&files);
    let start = Path::new("/v/marketplaces/acme/plugins/tools/skills/skillwright");
    assert_eq!(find_vault_root(&vfs, start).unwrap(), Path::new("/v"));
    assert_eq!(listed(&vfs), ["acme/tools/Skill/skillwright"]);
}

#[test]
fn a_component_named_skills_is_refused() {
    let files = plugin("acme", "tools", &[("skills/skills/skills.md", SKILL)]);
    let said = refusal(&vault_of(&files));
    assert!(said.contains("name rule"), "{said}");
    assert!(
        said.contains("marketplaces/acme/plugins/tools/skills/skills/"),
        "{said}"
    );
}

#[test]
fn a_loose_file_in_a_skill_folder_is_refused_naming_the_allowed_entries() {
    let files = plugin(
        "acme",
        "tools",
        &[
            ("skills/review/review.md", SKILL),
            ("skills/review/notes.md", "loose\n"),
        ],
    );
    let said = refusal(&vault_of(&files));
    assert!(
        said.contains("marketplaces/acme/plugins/tools/skills/review/notes.md"),
        "{said}"
    );
    assert!(said.contains("references"), "{said}");
}

#[test]
fn a_readme_in_marketplaces_is_refused() {
    let said = refusal(&vault(&[("marketplaces/README.md", "# x\n")]));
    assert!(said.contains("marketplaces/README.md"), "{said}");
}

#[test]
fn src_is_refused_in_a_command_or_agent_and_accepted_in_a_skill_or_plugin() {
    let ok = plugin(
        "acme",
        "tools",
        &[
            ("src/lib.rs", "x"),
            ("skills/review/review.md", SKILL),
            ("skills/review/src/main.rs", "x"),
        ],
    );
    assert!(read_layout(&vault_of(&ok), Path::new("/v")).is_ok());
    for kind in ["commands", "agents"] {
        let files = plugin(
            "acme",
            "tools",
            &[
                (&format!("{kind}/ship/ship.md"), SKILL),
                (&format!("{kind}/ship/src/main.rs"), "x"),
            ],
        );
        let said = refusal(&vault_of(&files));
        assert!(
            said.contains(&format!("marketplaces/acme/plugins/tools/{kind}/ship/src")),
            "{said}"
        );
    }
}

#[test]
fn an_mcp_json_is_an_error_and_a_dot_file_in_assets_is_skipped() {
    let files = plugin(
        "acme",
        "tools",
        &[("skills/review/review.md", SKILL), (".mcp.json", "{}")],
    );
    let said = refusal(&vault_of(&files));
    assert!(
        said.contains("marketplaces/acme/plugins/tools/.mcp.json"),
        "{said}"
    );

    let files = plugin(
        "acme",
        "tools",
        &[
            ("skills/review/review.md", SKILL),
            ("assets/logo.png", "png"),
            ("assets/.eslintrc.json", "{}"),
        ],
    );
    let layout = read_layout(&vault_of(&files), Path::new("/v")).unwrap();
    let names: Vec<String> = layout.files.iter().map(|f| f.rel.clone()).collect();
    assert!(
        names.iter().any(|f| f.ends_with("assets/logo.png")),
        "{names:?}"
    );
    assert!(!names.iter().any(|f| f.contains(".eslintrc")), "{names:?}");
}

#[test]
fn obsidian_settings_are_never_read() {
    let mut files = plugin("acme", "tools", &[("skills/review/review.md", SKILL)]);
    files.push((
        ".obsidian/app.json".to_owned(),
        r#"{"userIgnoreFilters": ["marketplaces/acme/plugins/tools/skills/review/"]}"#.to_owned(),
    ));
    assert_eq!(listed(&vault_of(&files)), ["acme/tools/Skill/review"]);
}

#[test]
fn docs_under_references_is_accepted() {
    let files = plugin(
        "acme",
        "tools",
        &[
            ("skills/review/review.md", SKILL),
            ("skills/review/references/docs/x.md", "x"),
            ("skills/review/docs/x.md", "x"),
        ],
    );
    let layout = read_layout(&vault_of(&files), Path::new("/v")).unwrap();
    assert!(
        layout
            .files
            .iter()
            .any(|f| f.rel.ends_with("skills/review/references/docs/x.md"))
    );
}

#[test]
fn names_follow_the_name_rule() {
    let ok = plugin(
        "acme",
        "v2-tools",
        &[("skills/v2-review/v2-review.md", SKILL)],
    );
    assert_eq!(listed(&vault_of(&ok)), ["acme/v2-tools/Skill/v2-review"]);

    let long = "a".repeat(65);
    for name in ["a--b", long.as_str()] {
        let files = plugin(
            "acme",
            "tools",
            &[(&format!("skills/{name}/{name}.md"), SKILL)],
        );
        let said = refusal(&vault_of(&files));
        assert!(said.contains(&format!("skills/{name}/")), "{said}");
    }
}

#[test]
fn one_skill_copied_into_two_marketplaces_is_two_components() {
    let mut files = plugin("acme", "tools", &[("skills/review/review.md", SKILL)]);
    files.extend(plugin(
        "beta",
        "tools",
        &[("skills/review/review.md", SKILL)],
    ));
    assert_eq!(
        listed(&vault_of(&files)),
        ["acme/tools/Skill/review", "beta/tools/Skill/review"]
    );
}

#[test]
fn a_skill_and_a_command_of_one_name_in_one_plugin_are_refused() {
    let files = plugin(
        "acme",
        "tools",
        &[
            ("skills/deploy/deploy.md", SKILL),
            ("commands/deploy/deploy.md", SKILL),
        ],
    );
    let vfs = vault_of(&files);
    let said = discover_components(&vfs, Path::new("/v"))
        .unwrap_err()
        .to_string();
    assert!(said.contains("skills/deploy/deploy.md"), "{said}");
    assert!(said.contains("commands/deploy/deploy.md"), "{said}");

    let mut files = plugin("acme", "tools", &[("skills/deploy/deploy.md", SKILL)]);
    files.extend(plugin(
        "acme",
        "ops",
        &[("commands/deploy/deploy.md", SKILL)],
    ));
    assert_eq!(
        listed(&vault_of(&files)),
        ["acme/ops/Command/deploy", "acme/tools/Skill/deploy"]
    );
}

#[test]
fn hooks_holds_only_hooks_json() {
    let ok = plugin(
        "acme",
        "tools",
        &[
            ("skills/review/review.md", SKILL),
            ("hooks/hooks.json", "{}"),
        ],
    );
    assert!(read_layout(&vault_of(&ok), Path::new("/v")).is_ok());
    let files = plugin(
        "acme",
        "tools",
        &[
            ("hooks/hooks.json", "{}"),
            ("hooks/lint/lint.md", SKILL),
            ("hooks/notes.md", "x"),
        ],
    );
    let said = refusal(&vault_of(&files));
    assert!(
        said.contains("marketplaces/acme/plugins/tools/hooks/lint"),
        "{said}"
    );
    assert!(
        said.contains("marketplaces/acme/plugins/tools/hooks/notes.md"),
        "{said}"
    );
}

// Section 11, one test per case.

#[test]
fn case_1_no_root_names_a_skillwright_md_passed_over() {
    let vfs = MemVfs::new();
    vfs.create_dir_all(Path::new("/w/skills/skillwright"))
        .unwrap();
    vfs.write_file(
        Path::new("/w/skills/skillwright/skillwright.md"),
        SKILL.as_bytes(),
    )
    .unwrap();
    let err = find_vault_root(&vfs, Path::new("/w/skills/skillwright")).unwrap_err();
    let said = err.to_string();
    assert!(said.contains("refused: layout"), "{said}");
    assert!(
        said.contains("/w/skills/skillwright/skillwright.md"),
        "{said}"
    );
    assert_expected_layout(&said);
}

#[test]
fn case_3_a_marketplace_or_plugin_with_no_note() {
    let vfs = vault(&[
        ("marketplaces/acme/skillwright.md", NOTE),
        ("marketplaces/beta/beta.md", NOTE),
        (
            "marketplaces/beta/plugins/tools/skills/review/review.md",
            SKILL,
        ),
    ]);
    let said = refusal(&vfs);
    assert!(said.contains("marketplaces/acme/acme.md"), "{said}");
    assert!(said.contains("now `acme.md`"), "{said}");
    assert!(
        said.contains("marketplaces/beta/plugins/tools/tools.md"),
        "{said}"
    );
}

#[test]
fn case_4_kind_folders_out_of_place() {
    let vfs = vault(&[
        ("skills/x/x.md", SKILL),
        ("agents/a.md", SKILL),
        ("commands/c.md", SKILL),
        ("hooks/hooks.json", "{}"),
        ("plugins/p/p.md", NOTE),
        ("marketplaces/acme/acme.md", NOTE),
        ("marketplaces/acme/skills/y/y.md", SKILL),
    ]);
    let said = refusal(&vfs);
    for path in [
        "skills/",
        "agents/",
        "commands/",
        "hooks/",
        "plugins/",
        "marketplaces/acme/skills/",
    ] {
        assert!(said.contains(&format!("  - {path}")), "{path}: {said}");
    }
}

#[test]
fn case_5_marketplaces_inside_marketplaces() {
    let files = plugin(
        "acme",
        "tools",
        &[
            ("skills/review/review.md", SKILL),
            ("marketplaces/inner/inner.md", NOTE),
        ],
    );
    let said = refusal(&vault_of(&files));
    assert!(
        said.contains("marketplaces/acme/plugins/tools/marketplaces"),
        "{said}"
    );
    assert!(said.contains("inside `marketplaces/`"), "{said}");
}

#[test]
fn case_6_a_flat_note_and_a_skill_md() {
    let files = plugin(
        "acme",
        "tools",
        &[("skills/x.md", SKILL), ("skills/y/SKILL.md", SKILL)],
    );
    let said = refusal(&vault_of(&files));
    assert!(said.contains("no flat notes"), "{said}");
    assert!(
        said.contains("marketplaces/acme/plugins/tools/skills/x.md"),
        "{said}"
    );
    assert!(
        said.contains("marketplaces/acme/plugins/tools/skills/y/SKILL.md"),
        "{said}"
    );
}

#[test]
fn case_7_removed_build_keys_are_reported_together() {
    let mut files = plugin("acme", "tools", &[("skills/review/review.md", SKILL)]);
    files.push((
        "skillwright.md".to_owned(),
        "---\nbuild:\n  patterns: {skills: a/*}\n  exclude: x\n  out: dist\n---\n".to_owned(),
    ));
    let said = refusal(&vault_of(&files));
    for key in ["build.patterns", "build.exclude", "build.out"] {
        assert!(said.contains(&format!("`{key}`")), "{key}: {said}");
    }
    // `load_vault` stops on the same report.
    let vfs = vault_of(&files);
    let err = load_vault(&vfs, Path::new("/v"), None, Path::new("/home")).unwrap_err();
    assert_eq!(err.to_string(), said);
}

#[test]
fn case_7_build_namespace_is_refused() {
    let mut files = plugin("acme", "tools", &[("skills/review/review.md", SKILL)]);
    files.push((
        "skillwright.md".to_owned(),
        "---\nbuild:\n  namespace: sw\n---\n".to_owned(),
    ));
    let said = refusal(&vault_of(&files));
    assert!(said.contains("`build.namespace`"), "{said}");
    let vfs = vault_of(&files);
    let err = load_vault(&vfs, Path::new("/v"), None, Path::new("/home")).unwrap_err();
    assert_eq!(err.to_string(), said);
}

#[test]
fn case_7_a_removed_key_and_a_layout_error_are_reported_in_one_run() {
    let mut files = plugin("acme", "tools", &[("skills/x.md", SKILL)]);
    files.push((
        "skillwright.md".to_owned(),
        "---\nbuild:\n  out: dist\n---\n".to_owned(),
    ));
    let said = refusal(&vault_of(&files));
    assert!(said.contains("`build.out`"), "{said}");
    assert!(said.contains("skills/x.md"), "{said}");
}

#[test]
fn case_8_entries_a_folder_does_not_allow() {
    let files = plugin(
        "acme",
        "tools",
        &[
            ("bin/tool", "#!/bin/sh\n"),
            ("skills/review/review.md", SKILL),
            ("skills/review/notes.md", "x"),
        ],
    );
    let said = refusal(&vault_of(&files));
    assert!(
        said.contains("marketplaces/acme/plugins/tools/bin"),
        "{said}"
    );
    assert!(
        said.contains("marketplaces/acme/plugins/tools/skills/review/notes.md"),
        "{said}"
    );
}

#[test]
fn two_flat_notes_are_one_case_listing_both() {
    let files = plugin(
        "acme",
        "tools",
        &[("skills/a.md", SKILL), ("skills/b.md", SKILL)],
    );
    let said = refusal(&vault_of(&files));
    assert_eq!(said.matches("no flat notes").count(), 1, "{said}");
    assert_eq!(said.matches("The expected layout").count(), 1, "{said}");
    assert!(said.contains("skills/a.md"), "{said}");
    assert!(said.contains("skills/b.md"), "{said}");
}

#[cfg(unix)]
#[test]
fn a_symlink_is_read_when_its_target_stays_in_the_marketplace() {
    let tmp = std::env::temp_dir().join(format!("tc-layout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let tools = tmp.join("marketplaces/acme/plugins/tools");
    std::fs::create_dir_all(tools.join("skills/review")).unwrap();
    std::fs::create_dir_all(tools.join("references")).unwrap();
    std::fs::write(tmp.join("skillwright.md"), ROOT).unwrap();
    std::fs::write(tmp.join("marketplaces/acme/acme.md"), NOTE).unwrap();
    std::fs::write(tools.join("tools.md"), NOTE).unwrap();
    std::fs::write(tools.join("skills/review/review.md"), SKILL).unwrap();
    std::fs::write(tools.join("references/x.md"), "x").unwrap();
    std::os::unix::fs::symlink(
        tools.join("references"),
        tools.join("skills/review/references"),
    )
    .unwrap();
    let vfs = tradecraft_core::NativeVfs;
    let layout = read_layout(&vfs, &tmp).unwrap();
    let place = layout
        .files
        .iter()
        .find(|f| f.rel.ends_with("skills/review/references/x.md"))
        .map(|f| f.place.clone());
    assert_eq!(
        place,
        Some(tradecraft_core::Place::Shared {
            skill: Some("review".to_owned()),
            dir: tradecraft_core::SharedDir::References,
            path: "references/x.md".to_owned(),
        })
    );

    // A folder link that holds itself would never end.
    std::os::unix::fs::symlink(&tools, tools.join("references/loop")).unwrap();
    let err = read_layout(&vfs, &tmp).unwrap_err();
    assert!(matches!(err, CoreError::Refused(_)), "{err}");
    assert!(err.to_string().contains("a folder that holds it"), "{err}");
    std::fs::remove_file(tools.join("references/loop")).unwrap();

    // A target outside the marketplace folder is refused.
    std::fs::write(tmp.join("outside.md"), "x").unwrap();
    std::os::unix::fs::symlink(tmp.join("outside.md"), tools.join("references/out.md")).unwrap();
    let err = read_layout(&vfs, &tmp).unwrap_err();
    assert!(err.to_string().contains("unsafe-path"), "{err}");
    assert!(err.to_string().contains("marketplaces/acme/"), "{err}");
    std::fs::remove_dir_all(&tmp).unwrap();
}
