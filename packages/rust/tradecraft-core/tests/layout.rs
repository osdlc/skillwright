//! The v2 vault layout (spec sections 1-3) and its errors (section 11), at
//! the core's public API.

use std::path::Path;

use tradecraft_core::vfs::VfsError;
use tradecraft_core::{
    BuildScope, CoreError, EntityKind, ExportMode, MemVfs, NativeVfs, VaultRequest, Vfs,
    build_vault, check_vault_root, discover_components, find_vault_root, load_vault, read_layout,
};

/// `build_vault` over the one marketplace a request covers: its report.
fn export(
    vfs: &dyn tradecraft_core::Vfs,
    request: &tradecraft_core::VaultRequest,
) -> Result<tradecraft_core::ExportReport, tradecraft_core::CoreError> {
    let mut report = tradecraft_core::build_vault(vfs, request)?;
    assert!(report.refused.is_empty(), "{:?}", report.refused);
    Ok(report.marketplaces.remove(0).report)
}

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
                c.position.kind(),
                EntityKind::Skill | EntityKind::Agent | EntityKind::Command
            )
        })
        .map(|c| {
            let at = &c.position;
            format!(
                "{}/{}/{:?}/{}",
                at.marketplace(),
                at.plugin().unwrap_or("-"),
                at.kind(),
                at.name()
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
    let loaded = load_vault(&vfs, Path::new("/v"), &BuildScope::Vault).unwrap();
    let request = VaultRequest::for_marketplace(&loaded.root, &loaded.marketplaces[0].name);
    export(&vfs, &request).unwrap();
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
    // Appendix A2: skipped with no warning, and it does not ship.
    let (warnings, shipped) = build(&vault_of(&files));
    assert!(
        !warnings.iter().any(|w| w.contains("eslintrc")),
        "{warnings:?}"
    );
    assert!(
        shipped.iter().any(|f| f.ends_with("assets/logo.png")),
        "{shipped:?}"
    );
    assert!(
        !shipped.iter().any(|f| f.contains("eslintrc")),
        "{shipped:?}"
    );
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

/// Below a shared or never-ship folder any folders are allowed (section 3
/// rule 17), structure names included: they are ordinary folders there.
#[test]
fn structure_named_folders_below_shared_and_never_ship_folders_are_ordinary() {
    let mut inside = Vec::new();
    for top in [
        "references",
        "scripts",
        "assets",
        "src",
        "docs",
        "tests",
        "fixtures",
        "skills/review/references",
        "skills/review/src",
        "commands/run/docs",
    ] {
        for name in [
            "skills",
            "agents",
            "commands",
            "hooks",
            "plugins",
            "marketplaces",
        ] {
            inside.push(format!("{top}/{name}/x.md"));
        }
    }
    let mut owned: Vec<(&str, &str)> = vec![
        ("skills/review/review.md", SKILL),
        ("commands/run/run.md", SKILL),
    ];
    owned.extend(inside.iter().map(|path| (path.as_str(), "x\n")));
    let vfs = vault_of(&plugin("acme", "tools", &owned));
    let layout = read_layout(&vfs, Path::new("/v")).unwrap();
    for path in &inside {
        assert!(
            layout.files.iter().any(|f| f.rel.ends_with(path.as_str())),
            "{path}"
        );
    }
    let (_, written) = build(&vfs);
    let pack = "/v/dist/acme/.claude-plugin/packs/tools";
    for shipped in [
        "references/skills/x.md",
        "scripts/hooks/x.md",
        "assets/marketplaces/x.md",
        "skills/review/references/plugins/x.md",
    ] {
        let path = format!("{pack}/{shipped}");
        assert!(written.contains(&path), "{path}: {written:?}");
    }
    assert!(!written.iter().any(|path| path.contains("/src/")));
}

/// A test vault kept in a plugin's `tests/fixtures/` is not part of the
/// vault: its notes do not build, and its root file and `marketplaces/` do
/// not refuse the vault.
#[test]
fn a_vault_in_a_plugin_fixtures_folder_is_not_read() {
    let fixture = "tests/fixtures/sample";
    let owned = [
        ("skills/review/review.md", SKILL.to_owned()),
        (
            &format!("{fixture}/skillwright.md")[..],
            "---\nworkspace: x\n---\n".to_owned(),
        ),
        (
            &format!("{fixture}/marketplaces/inner/inner.md")[..],
            NOTE.to_owned(),
        ),
        (
            &format!("{fixture}/marketplaces/inner/plugins/p/p.md")[..],
            NOTE.to_owned(),
        ),
        (
            &format!("{fixture}/marketplaces/inner/plugins/p/skills/s/s.md")[..],
            "no frontmatter, no description\n".to_owned(),
        ),
    ]
    .map(|(path, text)| (path.to_owned(), text));
    let borrowed: Vec<(&str, &str)> = owned
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let vfs = vault_of(&plugin("acme", "tools", &borrowed));
    assert_eq!(listed(&vfs), ["acme/tools/Skill/review"]);
    let (warnings, written) = build(&vfs);
    assert!(
        !warnings.iter().any(|w| w.contains("fixtures")),
        "{warnings:?}"
    );
    assert!(
        !written
            .iter()
            .any(|path| path.contains("inner") || path.contains("fixtures"))
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
    for name in ["a--b", "-x", "x-", long.as_str()] {
        for (files, path) in named_everywhere(name) {
            let said = refusal(&vault_of(&files));
            assert!(said.contains("name rule"), "{name}: {said}");
            assert!(said.contains(&path), "{name}: {said}");
        }
    }
}

#[test]
fn the_six_structure_names_are_refused_for_every_name() {
    for name in [
        "marketplaces",
        "plugins",
        "skills",
        "agents",
        "commands",
        "hooks",
    ] {
        for (files, path) in named_everywhere(name) {
            let said = refusal(&vault_of(&files));
            assert!(said.contains("name rule"), "{name}: {said}");
            assert!(said.contains(&path), "{name}: {said}");
        }
    }
}

/// Three vaults that each give `name` to one thing (a skill, a plugin, a
/// marketplace), each with the folder path the refusal must name.
fn named_everywhere(name: &str) -> Vec<(Vec<(String, String)>, String)> {
    vec![
        (
            plugin(
                "acme",
                "tools",
                &[(&format!("skills/{name}/{name}.md"), SKILL)],
            ),
            format!("marketplaces/acme/plugins/tools/skills/{name}/"),
        ),
        (
            plugin("acme", name, &[("skills/review/review.md", SKILL)]),
            format!("marketplaces/acme/plugins/{name}/"),
        ),
        (
            plugin(name, "tools", &[("skills/review/review.md", SKILL)]),
            format!("marketplaces/{name}/"),
        ),
    ]
}

#[test]
fn node_modules_is_skipped_in_a_plugin_and_a_skill() {
    let files = plugin(
        "acme",
        "tools",
        &[
            ("skills/review/review.md", SKILL),
            ("node_modules/x.js", "x"),
            ("skills/review/node_modules/x.js", "x"),
        ],
    );
    let vfs = vault_of(&files);
    let layout = read_layout(&vfs, Path::new("/v")).unwrap();
    assert!(
        !layout.files.iter().any(|f| f.rel.contains("node_modules")),
        "{:?}",
        layout.files.iter().map(|f| &f.rel).collect::<Vec<_>>()
    );
    let (warnings, shipped) = build(&vfs);
    assert!(
        !warnings.iter().any(|w| w.contains("node_modules")),
        "{warnings:?}"
    );
    assert!(
        shipped
            .iter()
            .any(|f| f.ends_with("skills/review/SKILL.md")),
        "{shipped:?}"
    );
    assert!(
        !shipped.iter().any(|f| f.contains("node_modules")),
        "{shipped:?}"
    );
}

/// Builds the first marketplace of the vault at `/v`. Returns each warning
/// as `path: reason: detail`, and every file written under `dist/`.
fn build(vfs: &MemVfs) -> (Vec<String>, Vec<String>) {
    let loaded = load_vault(vfs, Path::new("/v"), &BuildScope::Vault).unwrap();
    let request = VaultRequest::for_marketplace(&loaded.root, &loaded.marketplaces[0].name);
    let report = export(vfs, &request).unwrap();
    let warnings = report
        .warnings
        .iter()
        .map(|w| format!("{}: {}: {}", w.path, w.reason, w.detail))
        .collect();
    (warnings, files_under(vfs, Path::new("/v/dist")))
}

/// Every file under `dir`, as a full path.
fn files_under(vfs: &MemVfs, dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in vfs.read_dir(dir).unwrap_or_default() {
        let path = dir.join(&entry.name);
        if entry.is_dir {
            out.extend(files_under(vfs, &path));
        } else {
            out.push(path.to_string_lossy().into_owned());
        }
    }
    out
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
    let err = load_vault(&vfs, Path::new("/v"), &BuildScope::Vault).unwrap_err();
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
    let err = load_vault(&vfs, Path::new("/v"), &BuildScope::Vault).unwrap_err();
    assert_eq!(err.to_string(), said);
}

#[test]
fn case_7_build_assets_and_build_launcher_path_are_refused() {
    let mut files = plugin("acme", "tools", &[("skills/review/review.md", SKILL)]);
    files.push((
        "skillwright.md".to_owned(),
        "---\nbuild:\n  assets: [{from: bin, to: bin, plugin: tools}]\n  launcher_path: \
         bin/tool\n---\n"
            .to_owned(),
    ));
    let said = refusal(&vault_of(&files));
    assert!(said.contains("removed"), "{said}");
    for key in ["build.assets", "build.launcher_path"] {
        assert!(said.contains(&format!("`{key}`")), "{key}: {said}");
    }
}

#[test]
fn case_7_packs_in_a_root_file_block_is_refused() {
    let mut files = plugin("acme", "tools", &[("skills/review/review.md", SKILL)]);
    files.push((
        "skillwright.md".to_owned(),
        "---\nclaude:\n  packs: out/claude\n---\n".to_owned(),
    ));
    let said = refusal(&vault_of(&files));
    assert!(said.contains("removed"), "{said}");
    assert!(said.contains("`claude.packs`"), "{said}");
}

#[test]
fn case_7_packs_in_a_marketplace_or_plugin_block_is_refused() {
    for (note, at) in [
        ("marketplaces/acme/acme.md", "codex"),
        ("marketplaces/acme/plugins/tools/tools.md", "pi"),
    ] {
        let mut files = plugin("acme", "tools", &[("skills/review/review.md", SKILL)]);
        let text = format!("---\n{at}:\n  packs: elsewhere\n---\n");
        files.retain(|(path, _)| path != note);
        files.push((note.to_owned(), text));
        let mut request = VaultRequest::new("/v");
        request.options.mode = ExportMode::Check;
        // A check lists the refusal in its report.
        let report = build_vault(&vault_of(&files), &request).unwrap();
        let said = &report.refused[0].detail;
        assert!(said.contains(&format!("`{at}.packs`")), "{note}: {said}");
        assert!(said.contains("removed"), "{note}: {said}");
        assert!(said.contains(note), "{note}: {said}");
    }
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
    assert!(matches!(err, CoreError::Refused { .. }), "{err}");
    assert!(
        err.to_string().contains("a folder the walk is inside"),
        "{err}"
    );
    std::fs::remove_file(tools.join("references/loop")).unwrap();

    // A target outside the marketplace folder is refused.
    std::fs::write(tmp.join("outside.md"), "x").unwrap();
    std::os::unix::fs::symlink(tmp.join("outside.md"), tools.join("references/out.md")).unwrap();
    let err = read_layout(&vfs, &tmp).unwrap_err();
    assert!(err.to_string().contains("link-out-of-scope"), "{err}");
    assert!(err.to_string().contains("marketplaces/acme/"), "{err}");
    std::fs::remove_dir_all(&tmp).unwrap();
}

/// Two sibling folder links that point at each other: `a/x -> b` and
/// `b/y -> a`. Neither target holds its own link, but `a/x/y` is `a` again,
/// so the walk would never end.
#[cfg(unix)]
#[test]
fn sibling_folder_links_that_lead_back_are_refused() {
    let tmp = std::env::temp_dir().join(format!("tc-layout-siblings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let tools = tmp.join("marketplaces/acme/plugins/tools");
    let refs = tools.join("references");
    std::fs::create_dir_all(refs.join("a")).unwrap();
    std::fs::create_dir_all(refs.join("b")).unwrap();
    std::fs::write(tmp.join("skillwright.md"), ROOT).unwrap();
    std::fs::write(tmp.join("marketplaces/acme/acme.md"), NOTE).unwrap();
    std::fs::write(tools.join("tools.md"), NOTE).unwrap();
    std::fs::write(refs.join("a/one.md"), "x").unwrap();
    std::fs::write(refs.join("b/two.md"), "x").unwrap();
    std::os::unix::fs::symlink(refs.join("b"), refs.join("a/x")).unwrap();
    std::os::unix::fs::symlink(refs.join("a"), refs.join("b/y")).unwrap();

    let err = read_layout(&tradecraft_core::NativeVfs, &tmp).unwrap_err();
    std::fs::remove_dir_all(&tmp).unwrap();
    assert!(matches!(err, CoreError::Refused { .. }), "{err}");
    assert!(
        err.to_string().contains("a folder the walk is inside"),
        "{err}"
    );
}

/// The host on disk, except that it cannot resolve the path ending in the
/// given suffix.
#[cfg(unix)]
struct DeniedMarket(&'static str);

#[cfg(unix)]
impl Vfs for DeniedMarket {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        NativeVfs.read_file(path)
    }
    fn read_dir(&self, path: &Path) -> Result<Vec<tradecraft_core::DirEntry>, VfsError> {
        NativeVfs.read_dir(path)
    }
    fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<(), VfsError> {
        NativeVfs.write_file(path, bytes)
    }
    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        NativeVfs.create_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        NativeVfs.exists(path)
    }
    fn metadata(&self, path: &Path) -> Result<tradecraft_core::vfs::Metadata, VfsError> {
        NativeVfs.metadata(path)
    }
    fn is_executable(&self, path: &Path) -> bool {
        NativeVfs.is_executable(path)
    }
    fn set_executable(&self, path: &Path, yes: bool) -> Result<(), VfsError> {
        NativeVfs.set_executable(path, yes)
    }
    fn rename(&self, from: &Path, to: &Path) -> Result<(), VfsError> {
        NativeVfs.rename(from, to)
    }
    fn remove(&self, path: &Path) -> Result<(), VfsError> {
        NativeVfs.remove(path)
    }
    fn is_symlink(&self, path: &Path) -> bool {
        NativeVfs.is_symlink(path)
    }
    fn sees_links(&self) -> bool {
        true
    }
    fn read_link(&self, path: &Path) -> Result<std::path::PathBuf, VfsError> {
        NativeVfs.read_link(path)
    }
    fn canonicalize(&self, path: &Path) -> Result<std::path::PathBuf, VfsError> {
        if path.ends_with(self.0) {
            let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
            return Err(VfsError::io(path, denied));
        }
        NativeVfs.canonicalize(path)
    }
}

/// A host that cannot resolve the folder a symlink must stay inside refuses
/// the vault as an unsafe path; it does not fail with a bare host error.
#[cfg(unix)]
#[test]
fn a_symlink_whose_scope_cannot_be_resolved_is_refused() {
    let tmp = std::env::temp_dir().join(format!("tc-layout-denied-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let tools = tmp.join("marketplaces/acme/plugins/tools");
    std::fs::create_dir_all(tools.join("references")).unwrap();
    std::fs::write(tmp.join("skillwright.md"), ROOT).unwrap();
    std::fs::write(tmp.join("marketplaces/acme/acme.md"), NOTE).unwrap();
    std::fs::write(tools.join("tools.md"), NOTE).unwrap();
    std::fs::write(tools.join("references/x.md"), "x").unwrap();
    std::os::unix::fs::symlink(tools.join("references/x.md"), tools.join("references/y.md"))
        .unwrap();

    let err = read_layout(&DeniedMarket("marketplaces/acme"), &tmp).unwrap_err();
    std::fs::remove_dir_all(&tmp).unwrap();
    assert!(
        err.to_string().starts_with("refused: unsafe-path: "),
        "{err}"
    );
}

/// The walk-ancestor check refuses the same way when a folder the walk is
/// inside cannot be resolved.
#[cfg(unix)]
#[test]
fn a_walk_ancestor_that_cannot_be_resolved_is_refused() {
    let tmp = std::env::temp_dir().join(format!("tc-layout-denied-up-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let tools = tmp.join("marketplaces/acme/plugins/tools");
    std::fs::create_dir_all(tools.join("references")).unwrap();
    std::fs::create_dir_all(tools.join("other")).unwrap();
    std::fs::write(tmp.join("skillwright.md"), ROOT).unwrap();
    std::fs::write(tmp.join("marketplaces/acme/acme.md"), NOTE).unwrap();
    std::fs::write(tools.join("tools.md"), NOTE).unwrap();
    std::os::unix::fs::symlink(tools.join("other"), tools.join("references/y")).unwrap();

    let err = read_layout(&DeniedMarket("plugins/tools/references"), &tmp).unwrap_err();
    std::fs::remove_dir_all(&tmp).unwrap();
    assert!(
        err.to_string().starts_with("refused: unsafe-path: "),
        "{err}"
    );
}

/// Root entries other than `skillwright.md`, `marketplaces/` and `dist/` are
/// left alone and not read (spec section 3), so a symlink there, even a
/// dangling one, does not refuse the vault.
#[cfg(unix)]
#[test]
fn a_symlink_at_the_vault_root_is_left_alone() {
    let tmp = std::env::temp_dir().join(format!("tc-layout-root-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let tools = tmp.join("marketplaces/acme/plugins/tools");
    std::fs::create_dir_all(tools.join("skills/review")).unwrap();
    std::fs::write(tmp.join("skillwright.md"), ROOT).unwrap();
    std::fs::write(tmp.join("marketplaces/acme/acme.md"), NOTE).unwrap();
    std::fs::write(tools.join("tools.md"), NOTE).unwrap();
    std::fs::write(tools.join("skills/review/review.md"), SKILL).unwrap();
    std::fs::write(tmp.join("CLAUDE.md"), "x").unwrap();
    std::os::unix::fs::symlink(tmp.join("CLAUDE.md"), tmp.join("AGENTS.md")).unwrap();
    std::os::unix::fs::symlink(std::env::temp_dir(), tmp.join("notes")).unwrap();
    std::os::unix::fs::symlink(tmp.join("missing"), tmp.join("broken")).unwrap();

    let result = read_layout(&tradecraft_core::NativeVfs, &tmp);
    std::fs::remove_dir_all(&tmp).unwrap();
    let layout = result.unwrap();
    assert!(
        layout
            .files
            .iter()
            .all(|f| f.rel.starts_with("marketplaces/"))
    );
}

/// `skillwright.md`, `marketplaces/` and `dist/` may not be symlinks (spec
/// section 3), wherever the link leads, and the refusal says so.
#[cfg(unix)]
#[test]
fn a_symlinked_root_file_marketplaces_or_dist_is_refused() {
    for name in ["skillwright.md", "marketplaces", "dist"] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("v");
        let shared = tmp.path().join("shared");
        // The linked entry is written under `shared/`, the rest in the vault.
        let at = |rel: &str| {
            let top = rel.split('/').next().unwrap();
            let path = if top == name { &shared } else { &root }.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            path
        };
        let tools = "marketplaces/acme/plugins/tools";
        std::fs::write(at("skillwright.md"), ROOT).unwrap();
        std::fs::write(at("marketplaces/acme/acme.md"), NOTE).unwrap();
        std::fs::write(at(&format!("{tools}/tools.md")), NOTE).unwrap();
        std::fs::write(at(&format!("{tools}/skills/review/review.md")), SKILL).unwrap();
        std::fs::create_dir_all(at("dist/x").parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(shared.join(name), root.join(name)).unwrap();

        let err = read_layout(&tradecraft_core::NativeVfs, &root)
            .unwrap_err()
            .to_string();
        assert!(err.contains("refused: unsafe-path: "), "{name}: {err}");
        assert!(
            err.contains(&format!("`{name}` may not be a symlink")),
            "{name}: {err}"
        );
        assert!(!err.contains("marketplace folder (`"), "{name}: {err}");
    }
}

/// A symlinked folder inside a skill that the layout walk accepts builds,
/// and its files ship under the skill.
#[cfg(unix)]
#[test]
fn a_symlinked_folder_in_a_skill_builds() {
    let tmp = std::env::temp_dir().join(format!("tc-layout-skill-link-{}", std::process::id()));
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
    let result = load_vault(&vfs, &tmp, &BuildScope::Vault).and_then(|loaded| {
        let request = VaultRequest::for_marketplace(&loaded.root, &loaded.marketplaces[0].name);
        export(&vfs, &request)
    });
    let shipped = tmp.join("dist/acme/.claude-plugin/packs/tools/skills/review/references/x.md");
    let exists = shipped.exists();
    std::fs::remove_dir_all(&tmp).unwrap();
    result.unwrap();
    assert!(exists, "no {}", shipped.display());
}

#[test]
fn a_stray_file_in_plugins_is_refused() {
    let mut files = plugin("acme", "tools", &[("skills/review/review.md", SKILL)]);
    files.push((
        "marketplaces/acme/plugins/README.md".to_owned(),
        "# x\n".to_owned(),
    ));
    let said = refusal(&vault_of(&files));
    assert!(
        said.contains("marketplaces/acme/plugins/README.md"),
        "{said}"
    );
}

#[test]
fn a_stray_file_in_a_marketplace_folder_is_refused() {
    let mut files = plugin("acme", "tools", &[("skills/review/review.md", SKILL)]);
    files.push(("marketplaces/acme/notes.md".to_owned(), "# x\n".to_owned()));
    let said = refusal(&vault_of(&files));
    assert!(said.contains("marketplaces/acme/notes.md"), "{said}");
}

#[test]
fn case_6_a_flat_agent_or_command_note() {
    let files = plugin(
        "acme",
        "tools",
        &[
            ("skills/review/review.md", SKILL),
            ("agents/a.md", SKILL),
            ("commands/c.md", SKILL),
        ],
    );
    let said = refusal(&vault_of(&files));
    assert!(said.contains("no flat notes"), "{said}");
    for path in ["agents/a.md", "commands/c.md"] {
        assert!(
            said.contains(&format!("marketplaces/acme/plugins/tools/{path}")),
            "{path}: {said}"
        );
    }
}

#[test]
fn shared_folders_are_refused_in_a_command_or_agent_folder() {
    for kind in ["commands", "agents"] {
        let ok = plugin(
            "acme",
            "tools",
            &[
                (&format!("{kind}/ship/ship.md"), SKILL),
                (&format!("{kind}/ship/docs/x.md"), "x"),
                (&format!("{kind}/ship/tests/x.md"), "x"),
                (&format!("{kind}/ship/fixtures/x.md"), "x"),
            ],
        );
        assert!(
            read_layout(&vault_of(&ok), Path::new("/v")).is_ok(),
            "{kind}"
        );
        let files = plugin(
            "acme",
            "tools",
            &[
                (&format!("{kind}/ship/ship.md"), SKILL),
                (&format!("{kind}/ship/references/x.md"), "x"),
                (&format!("{kind}/ship/scripts/x.sh"), "x"),
                (&format!("{kind}/ship/assets/x.png"), "x"),
            ],
        );
        let said = refusal(&vault_of(&files));
        for folder in ["references", "scripts", "assets"] {
            assert!(
                said.contains(&format!(
                    "marketplaces/acme/plugins/tools/{kind}/ship/{folder}"
                )),
                "{kind}/{folder}: {said}"
            );
        }
    }
}
