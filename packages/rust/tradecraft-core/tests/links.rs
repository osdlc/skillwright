//! Links (spec section 4): scope, containment, Markdown links and plain
//! links, built through the core's public API. Every vault has marketplace
//! `acme` and plugin `tools`.

use std::path::Path;

use tradecraft_core::{
    CoreError, DiscoveryWarning, ExportMode, MemVfs, Vfs, export_skills, load_vault,
};

const SKILL: &str = "---\ndescription: Use it.\n---\n\n";

/// The packs of plugin `tools`, by harness.
const CLAUDE: &str = "dist/acme/.claude-plugin/packs/tools";
const CODEX: &str = "dist/acme/.agents/packs/tools";
const CURSOR: &str = "dist/acme/.cursor-plugin/packs/tools";
const PI: &str = "dist/acme/.pi/packs/tools";

const PLUGIN: &str = "marketplaces/acme/plugins/tools";

/// A vault at `/v` with plugin `tools`, plus `files` under the plugin folder.
/// A file ending in `.md` under a kind folder gets a description.
fn vault(files: &[(&str, &str)]) -> MemVfs {
    let vfs = MemVfs::new();
    put(&vfs, "skillwright.md", "---\n---\n");
    put(&vfs, "marketplaces/acme/acme.md", "---\n---\n");
    put(
        &vfs,
        &format!("{PLUGIN}/tools.md"),
        "---\nversion: 1.0.0\n---\n",
    );
    for (path, text) in files {
        put(&vfs, &format!("{PLUGIN}/{path}"), text);
    }
    vfs
}

/// A skill note with `body`.
fn skill(body: &str) -> String {
    format!("{SKILL}{body}\n")
}

fn put(vfs: &MemVfs, rel: &str, text: &str) {
    let path = Path::new("/v").join(rel);
    vfs.create_dir_all(path.parent().unwrap()).unwrap();
    vfs.write_file(&path, text.as_bytes()).unwrap();
}

/// Builds every harness and returns the warnings.
fn build(vfs: &dyn Vfs) -> Result<Vec<DiscoveryWarning>, CoreError> {
    build_for(vfs, &[])
}

/// Builds `harnesses`, every one when empty.
fn build_for(vfs: &dyn Vfs, harnesses: &[&str]) -> Result<Vec<DiscoveryWarning>, CoreError> {
    let loaded = load_vault(vfs, Path::new("/v"), None)?;
    let mut request = loaded.export_request(&loaded.marketplaces[0], ExportMode::Build);
    request.harnesses = harnesses.iter().map(|h| (*h).to_owned()).collect();
    export_skills(vfs, &request).map(|report| report.warnings)
}

fn refusal(vfs: &dyn Vfs) -> String {
    refusal_for(vfs, &[])
}

fn refusal_for(vfs: &dyn Vfs, harnesses: &[&str]) -> String {
    match build_for(vfs, harnesses) {
        Ok(_) => panic!("the build succeeded"),
        Err(err) => err.to_string(),
    }
}

/// Rule 14: an error names the linking file, the link as written and the rule.
fn names(message: &str, file: &str, link: &str, rule: &str) {
    assert!(message.contains(file), "no file `{file}`: {message}");
    assert!(message.contains(link), "no link `{link}`: {message}");
    assert!(message.contains(rule), "no rule `{rule}`: {message}");
}

fn read(vfs: &dyn Vfs, rel: &str) -> String {
    String::from_utf8(vfs.read_file(&Path::new("/v").join(rel)).unwrap()).unwrap()
}

/// `author`'s `SKILL.md` in `pack`, where the skill folder is `folder`.
fn author_in(vfs: &dyn Vfs, pack: &str, folder: &str) -> String {
    read(vfs, &format!("{pack}/skills/{folder}/SKILL.md"))
}

fn author(vfs: &dyn Vfs) -> String {
    author_in(vfs, CLAUDE, "author")
}

const AUTHOR: &str = "skills/author/author.md";

// --- Scope ------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_symlink_in_a_skill_folder_out_of_the_marketplace_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("v");
    let plugin = root.join(PLUGIN);
    std::fs::create_dir_all(plugin.join("skills/author/references")).unwrap();
    std::fs::write(root.join("skillwright.md"), "---\n---\n").unwrap();
    std::fs::write(root.join("marketplaces/acme/acme.md"), "---\n---\n").unwrap();
    std::fs::write(plugin.join("tools.md"), "---\n---\n").unwrap();
    std::fs::write(plugin.join(AUTHOR), skill("See [[secret]].")).unwrap();
    std::fs::write(tmp.path().join("secret.md"), "Secret.\n").unwrap();
    let link = plugin.join("skills/author/references/secret.md");
    std::os::unix::fs::symlink(tmp.path().join("secret.md"), &link).unwrap();
    let vfs = tradecraft_core::NativeVfs;
    let err = load_vault(&vfs, &root, None).unwrap_err().to_string();
    assert!(err.contains("unsafe-path"), "{err}");
    assert!(err.contains("secret.md"), "{err}");
}

#[test]
fn a_shared_reference_renders_from_the_pack_root() {
    let vfs = vault(&[
        (AUTHOR, &skill("Follow [[style]].")),
        ("references/style.md", "Style.\n"),
    ]);
    build(&vfs).unwrap();
    assert!(
        author(&vfs).contains("[style](../../references/style.md)"),
        "{}",
        author(&vfs)
    );
}

#[test]
fn a_name_prefers_the_components_own_folder() {
    let vfs = vault(&[
        (AUTHOR, &skill("Follow [[guide]].")),
        ("skills/author/references/guide.md", "Own.\n"),
        ("references/guide.md", "Shared.\n"),
    ]);
    build(&vfs).unwrap();
    assert!(author(&vfs).contains("[guide](references/guide.md)"));
}

#[test]
fn a_shared_reference_starts_at_step_two_and_links_from_where_it_lands() {
    let vfs = vault(&[
        (AUTHOR, &skill("Body.")),
        ("skills/author/references/guide.md", "Own.\n"),
        ("references/guide.md", "Shared.\n"),
        ("references/style.md", "See [[guide]].\n"),
    ]);
    build(&vfs).unwrap();
    for pack in [CLAUDE, CODEX, CURSOR, PI] {
        assert_eq!(
            read(&vfs, &format!("{pack}/references/style.md")),
            "See [guide](guide.md).\n"
        );
    }
}

#[test]
fn a_name_matches_at_any_depth() {
    let vfs = vault(&[
        (AUTHOR, &skill("Use [[layout]].")),
        ("skills/author/references/design/layout.md", "Layout.\n"),
    ]);
    build(&vfs).unwrap();
    assert!(author(&vfs).contains("[layout](references/design/layout.md)"));
}

#[test]
fn a_name_matching_twice_is_an_error_naming_both() {
    let vfs = vault(&[
        (AUTHOR, &skill("Use [[layout]].")),
        ("skills/author/references/design/layout.md", "Layout.\n"),
        ("skills/author/references/old/layout.md", "Old.\n"),
    ]);
    let err = refusal(&vfs);
    names(&err, AUTHOR, "[[layout]]", "rule 3");
    assert!(err.contains("references/design/layout.md"), "{err}");
    assert!(err.contains("references/old/layout.md"), "{err}");
}

#[test]
fn a_name_inside_another_component_is_a_link_to_nothing() {
    let vfs = vault(&[
        (AUTHOR, &skill("See [[notes]].")),
        ("skills/setup/setup.md", &skill("Setup.")),
        ("skills/setup/references/notes.md", "Notes.\n"),
    ]);
    names(&refusal(&vfs), AUTHOR, "[[notes]]", "rule 11");
}

#[test]
fn a_path_into_another_component_works() {
    let vfs = vault(&[
        (
            AUTHOR,
            &skill("See [[../setup/references/notes.md|notes]]."),
        ),
        ("skills/setup/setup.md", &skill("Setup.")),
        ("skills/setup/references/notes.md", "Notes.\n"),
    ]);
    build(&vfs).unwrap();
    assert!(author(&vfs).contains("[notes](../setup/references/notes.md)"));
    assert!(
        author_in(&vfs, CURSOR, "tools-author")
            .contains("[notes](../tools-setup/references/notes.md)")
    );
}

#[test]
fn a_link_into_another_plugin_is_refused() {
    let vfs = vault(&[(AUTHOR, &skill("See [[../../../other/references/x.md]]."))]);
    put(
        &vfs,
        "marketplaces/acme/plugins/other/other.md",
        "---\n---\n",
    );
    put(
        &vfs,
        "marketplaces/acme/plugins/other/references/x.md",
        "X.\n",
    );
    let err = refusal(&vfs);
    names(
        &err,
        AUTHOR,
        "[[../../../other/references/x.md]]",
        "another plugin",
    );
}

#[test]
fn a_link_into_docs_is_refused_with_one_error_in_every_form() {
    // A Markdown link is quoted by its destination.
    let mut errors = Vec::new();
    for (link, written) in [
        ("[[docs/x]]", "[[docs/x]]"),
        ("[t](docs/x.md)", "docs/x.md"),
        ("![[docs/x]]", "![[docs/x]]"),
    ] {
        let vfs = vault(&[
            (AUTHOR, &skill(&format!("See {link}."))),
            ("skills/author/docs/x.md", "For authors.\n"),
        ]);
        let err = refusal(&vfs);
        names(&err, AUTHOR, written, "rule 10");
        assert!(err.contains("never ships"), "{err}");
        errors.push(err.replace(&format!("`{written}`"), "`LINK`"));
    }
    errors.dedup();
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_link_out_of_the_marketplace_is_refused() {
    let vfs = vault(&[(AUTHOR, &skill("See [t](../../../../../../x.md)."))]);
    put(&vfs, "x.md", "Outside.\n");
    names(
        &refusal(&vfs),
        AUTHOR,
        "../../../../../../x.md",
        "containment rule 1",
    );
}

// --- Markdown links ---------------------------------------------------------

#[test]
fn a_relative_markdown_link_is_checked_and_rewritten() {
    let vfs = vault(&[
        (
            AUTHOR,
            &skill("Read [x](references/x.md) and [s](../setup/setup.md#Usage)."),
        ),
        ("skills/author/references/x.md", "X.\n"),
        ("skills/setup/setup.md", &skill("## Usage")),
    ]);
    build(&vfs).unwrap();
    let claude = author(&vfs);
    assert!(claude.contains("[x](references/x.md)"), "{claude}");
    assert!(claude.contains("[s](../setup/SKILL.md#Usage)"), "{claude}");
    let cursor = author_in(&vfs, CURSOR, "tools-author");
    assert!(
        cursor.contains("[s](../tools-setup/SKILL.md#Usage)"),
        "{cursor}"
    );
}

#[test]
fn a_markdown_link_to_a_missing_file_is_an_error_naming_it() {
    let vfs = vault(&[(AUTHOR, &skill("Read [x](references/missing.md)."))]);
    names(&refusal(&vfs), AUTHOR, "references/missing.md", "rule 11");
}

#[test]
fn an_absolute_or_file_markdown_link_is_refused() {
    for link in ["/abs/x.md", "file:x.md"] {
        let vfs = vault(&[(AUTHOR, &skill(&format!("Read [x]({link}).")))]);
        names(&refusal(&vfs), AUTHOR, link, "section 4");
    }
}

#[test]
fn an_external_or_same_file_link_ships_as_written() {
    let body = "See [t](https://x.io), <https://x.io>, [m](mailto:a@b) and [u](#usage).";
    let vfs = vault(&[(AUTHOR, &skill(body))]);
    build(&vfs).unwrap();
    assert!(author(&vfs).contains(body));
}

// --- Plain links ------------------------------------------------------------

#[test]
fn a_link_to_another_skill_is_a_file_link_with_a_warning() {
    let vfs = vault(&[
        (AUTHOR, &skill("Then run [[setup]].")),
        ("skills/setup/setup.md", &skill("Setup.")),
    ]);
    let warnings = build(&vfs).unwrap();
    assert!(author(&vfs).contains("[setup](../setup/SKILL.md)"));
    assert!(author_in(&vfs, CODEX, "author").contains("[setup](../setup/SKILL.md)"));
    for pack in [CURSOR, PI] {
        assert!(author_in(&vfs, pack, "tools-author").contains("[setup](../tools-setup/SKILL.md)"));
    }
    let warning = warnings
        .iter()
        .find(|w| w.detail.contains("Did you mean to call /[[setup]]?"))
        .unwrap_or_else(|| panic!("no warning: {warnings:?}"));
    assert!(warning.path.ends_with(AUTHOR), "{warning:?}");
    assert!(warning.detail.contains("[[setup]]"), "{warning:?}");
}

#[test]
fn a_link_keeps_its_fragment() {
    let vfs = vault(&[
        (AUTHOR, &skill("See [[setup#Usage]].")),
        ("skills/setup/setup.md", &skill("## Usage")),
    ]);
    build(&vfs).unwrap();
    assert!(author(&vfs).contains("[setup](../setup/SKILL.md#Usage)"));
}

#[test]
fn a_skills_link_to_its_own_note_does_not_warn() {
    let vfs = vault(&[(AUTHOR, &skill("Part of [[author]]."))]);
    let warnings = build(&vfs).unwrap();
    assert!(author(&vfs).contains("[author](SKILL.md)"));
    assert!(
        !warnings.iter().any(|w| w.detail.contains("Did you mean")),
        "{warnings:?}"
    );
}

#[test]
fn a_link_to_a_command_is_a_file_link_on_claude_and_fails_elsewhere() {
    let files = [
        (AUTHOR, skill("See [[deploy]].")),
        ("commands/deploy/deploy.md", skill("Deploy.")),
    ];
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let vfs = vault(&files);
    build_for(&vfs, &["claude"]).unwrap();
    assert!(author(&vfs).contains("[deploy](../../commands/deploy.md)"));
    for harness in ["codex", "cursor", "pi"] {
        let vfs = vault(&files);
        names(
            &refusal_for(&vfs, &[harness]),
            AUTHOR,
            "[[deploy]]",
            "rule 6",
        );
    }
}

#[test]
fn a_link_to_an_agent_fails_the_build() {
    let vfs = vault(&[
        (AUTHOR, &skill("Ask [[helper]].")),
        ("agents/helper/helper.md", &skill("Help.")),
    ]);
    names(
        &refusal_for(&vfs, &["claude"]),
        AUTHOR,
        "[[helper]]",
        "rule 6",
    );
}

// --- Missing targets --------------------------------------------------------

#[test]
fn a_link_or_embed_to_nothing_is_an_error() {
    for link in ["[[nothing]]", "![[nothing]]"] {
        let vfs = vault(&[(AUTHOR, &skill(&format!("See {link}.")))]);
        names(&refusal(&vfs), AUTHOR, link, "rule 11");
    }
}
