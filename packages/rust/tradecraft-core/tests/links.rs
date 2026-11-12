//! Links and calls (spec section 4): scope, containment, Markdown links,
//! plain links, calls, hand-written calls, code and leftovers; and embeds
//! (section 5, D8). All built through the core's public API. Every vault has marketplace
//! `acme` and plugin `tools`.

use std::path::Path;

use tradecraft_core::{CoreError, DiscoveryWarning, ExportMode, MemVfs, Vfs, load_vault};

/// `build_vault` over the one marketplace a request covers: its report.
fn export(
    vfs: &dyn tradecraft_core::Vfs,
    request: &tradecraft_core::VaultRequest,
) -> Result<tradecraft_core::ExportReport, tradecraft_core::CoreError> {
    let mut report = tradecraft_core::build_vault(vfs, request)?;
    assert!(report.refused.is_empty(), "{:?}", report.refused);
    Ok(report.marketplaces.remove(0).report)
}

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

/// Builds `harnesses`, or what the root file names when empty.
fn build_for(vfs: &dyn Vfs, harnesses: &[&str]) -> Result<Vec<DiscoveryWarning>, CoreError> {
    let loaded = load_vault(vfs, Path::new("/v"), None)?;
    let mut request = loaded.vault_request(&loaded.marketplaces[0], ExportMode::Build);
    if !harnesses.is_empty() {
        request.harnesses = harnesses.iter().map(|h| (*h).to_owned()).collect();
    }
    export(vfs, &request).map(|report| report.warnings)
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

/// A vault on disk at `<tmp>/v`, as [`vault`] lays it out, for symlinks.
#[cfg(unix)]
fn disk(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    put_disk(&tmp, "skillwright.md", "---\n---\n");
    put_disk(&tmp, "marketplaces/acme/acme.md", "---\n---\n");
    put_disk(
        &tmp,
        &format!("{PLUGIN}/tools.md"),
        "---\nversion: 1.0.0\n---\n",
    );
    for (path, text) in files {
        put_disk(&tmp, &format!("{PLUGIN}/{path}"), text);
    }
    tmp
}

/// Makes `link`, a path in plugin `tools`, a symlink to `target`.
#[cfg(unix)]
fn symlink(tmp: &tempfile::TempDir, link: &str, target: &str) {
    let link = tmp.path().join("v").join(PLUGIN).join(link);
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(target, link).unwrap();
}

/// Writes `text` at `rel` from the vault root on disk.
#[cfg(unix)]
fn put_disk(tmp: &tempfile::TempDir, rel: &str, text: &str) {
    let path = tmp.path().join("v").join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The refusal of a build of the vault on disk.
#[cfg(unix)]
fn disk_refusal(tmp: &tempfile::TempDir) -> String {
    let vfs = tradecraft_core::NativeVfs;
    let root = tmp.path().join("v");
    let result = load_vault(&vfs, &root, None).and_then(|loaded| {
        let request = loaded.vault_request(&loaded.marketplaces[0], ExportMode::Build);
        export(&vfs, &request).map(|_| ())
    });
    match result {
        Ok(()) => panic!("the build succeeded"),
        Err(err) => err.to_string(),
    }
}

/// What a refusal says after the link as written: the reason and the rule.
fn after_link<'e>(err: &'e str, written: &str) -> &'e str {
    let quoted = format!("`{written}`");
    err.split_once(&quoted)
        .unwrap_or_else(|| panic!("no {quoted}: {err}"))
        .1
}

/// The reason token of a refusal.
fn token(err: &str) -> &str {
    err.split(':').nth(1).unwrap_or_default().trim()
}

/// Containment rule 2: a symlink whose real path leaves the scope fails as a
/// link spelling that target does, naming the symlink and its target.
#[cfg(unix)]
fn refused_as_spelled(
    files: &[(&str, &str)],
    setup: impl Fn(&tempfile::TempDir),
    link: &str,
    target: &str,
    spelled: &str,
) -> String {
    let tmp = disk(files);
    setup(&tmp);
    symlink(&tmp, link, target);
    let by_symlink = disk_refusal(&tmp);
    names(
        &by_symlink,
        &format!("{PLUGIN}/{link}"),
        target,
        "section 4",
    );

    let tmp = disk(&[(AUTHOR, &skill(&format!("See {spelled}.")))]);
    setup(&tmp);
    let by_link = disk_refusal(&tmp);
    names(&by_link, AUTHOR, spelled, "section 4");
    assert_eq!(
        token(&by_symlink),
        token(&by_link),
        "{by_symlink}\n{by_link}"
    );
    assert_eq!(
        after_link(&by_symlink, target),
        after_link(&by_link, spelled),
        "{by_symlink}\n{by_link}"
    );
    by_symlink
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_marketplace_fails_as_a_link_spelling_its_target() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tmp.path().join("secret.md");
    std::fs::write(&outside, "Secret.\n").unwrap();
    let outside = outside.to_str().unwrap().to_owned();
    let err = refused_as_spelled(
        &[(AUTHOR, &skill("See [[secret]]."))],
        |_| {},
        "skills/author/references/secret.md",
        &outside,
        "[[../../../../../../../secret.md]]",
    );
    assert!(err.contains("link-out-of-scope"), "{err}");
    assert!(err.contains("containment rule 1"), "{err}");
}

/// Plugin `other` with `references/x.md`, on disk.
#[cfg(unix)]
fn other_plugin_on_disk(tmp: &tempfile::TempDir) {
    put_disk(
        tmp,
        "marketplaces/acme/plugins/other/other.md",
        "---\n---\n",
    );
    put_disk(
        tmp,
        "marketplaces/acme/plugins/other/references/x.md",
        "X.\n",
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_into_another_plugin_fails_as_a_link_spelling_its_target() {
    // Linked or only shipped, as a file or through a folder.
    for (body, link, target) in [
        (
            "See [[x]].",
            "skills/author/references/x.md",
            "../../../../other/references/x.md",
        ),
        (
            "Body.",
            "skills/author/references/x.md",
            "../../../../other/references/x.md",
        ),
        (
            "Body.",
            "skills/author/references/ext",
            "../../../../other/references",
        ),
    ] {
        let err = refused_as_spelled(
            &[(AUTHOR, &skill(body))],
            other_plugin_on_disk,
            link,
            target,
            "[[../../../other/references/x.md]]",
        );
        assert!(err.contains("another plugin"), "{err}");
        assert!(err.contains("scope of a link"), "{err}");
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_into_the_components_docs_fails_as_a_link_spelling_its_target() {
    let err = refused_as_spelled(
        &[
            (AUTHOR, &skill("See [[references/x]].")),
            ("skills/author/docs/x.md", "For authors.\n"),
        ],
        |tmp| {
            put_disk(
                tmp,
                &format!("{PLUGIN}/skills/author/docs/x.md"),
                "For authors.\n",
            )
        },
        "skills/author/references/x.md",
        "../docs/x.md",
        "[[docs/x]]",
    );
    assert!(err.contains("excluded-link"), "{err}");
    assert!(err.contains("rule 10"), "{err}");
}

/// A symlink inside a never-ship folder is not followed: nothing ships it,
/// so one leading out of the vault, or nowhere, does not refuse the vault.
#[cfg(unix)]
#[test]
fn a_symlink_in_a_never_ship_folder_is_not_followed() {
    let tmp = disk(&[(AUTHOR, &skill("Body."))]);
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("x.md"), "x\n").unwrap();
    let outside = outside.to_str().unwrap().to_owned();
    let gone = tmp.path().join("gone").to_str().unwrap().to_owned();
    symlink(&tmp, "src/lib", &outside);
    symlink(&tmp, "tests/gone", &gone);
    symlink(&tmp, "skills/author/docs/shared", &outside);
    symlink(&tmp, "skills/author/fixtures/gone.md", &gone);

    let vfs = tradecraft_core::NativeVfs;
    let root = tmp.path().join("v");
    let loaded = load_vault(&vfs, &root, None).unwrap();
    let request = loaded.vault_request(&loaded.marketplaces[0], ExportMode::Build);
    export(&vfs, &request).unwrap();
    let pack = root.join(CLAUDE);
    assert!(pack.join("skills/author/SKILL.md").is_file());
    assert!(!pack.join("src").exists());
    assert!(!pack.join("skills/author/docs").exists());
}

/// A link from a shipped file into a symlinked never-ship folder is still
/// refused as reaching a file that never ships (section 4 rule 10).
#[cfg(unix)]
#[test]
fn a_link_into_a_symlinked_never_ship_folder_is_refused() {
    let tmp = disk(&[(AUTHOR, &skill("See [x](../../src/lib/x.md)."))]);
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("x.md"), "x\n").unwrap();
    symlink(&tmp, "src/lib", outside.to_str().unwrap());
    let err = disk_refusal(&tmp);
    assert!(err.contains("excluded-link"), "{err}");
    assert!(err.contains("rule 10"), "{err}");
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

/// The one warning with `reason`.
fn warned<'w>(warnings: &'w [DiscoveryWarning], reason: &str) -> &'w DiscoveryWarning {
    let found: Vec<&DiscoveryWarning> = warnings.iter().filter(|w| w.reason == reason).collect();
    match found[..] {
        [one] => one,
        _ => panic!("not one `{reason}` warning: {warnings:?}"),
    }
}

#[test]
fn a_name_matching_a_file_only_by_case_warns_naming_link_file_and_note() {
    let vfs = vault(&[
        (AUTHOR, &skill("Follow [[Guide]].")),
        ("references/guide.md", "Shared.\n"),
    ]);
    let warnings = build(&vfs).unwrap();
    assert!(
        author(&vfs).contains("[Guide](../../references/guide.md)"),
        "{}",
        author(&vfs)
    );
    let warning = warned(&warnings, "link-case");
    assert!(warning.path.ends_with(AUTHOR), "{warning:?}");
    assert!(warning.detail.contains("[[Guide]]"), "{warning:?}");
    assert!(
        warning
            .detail
            .contains(&format!("{PLUGIN}/references/guide.md")),
        "{warning:?}"
    );
}

#[test]
fn a_name_matching_a_component_only_by_case_warns() {
    let vfs = vault(&[
        (AUTHOR, &skill("Then run [[Setup]].")),
        ("skills/setup/setup.md", &skill("Setup.")),
    ]);
    let warnings = build(&vfs).unwrap();
    let warning = warned(&warnings, "link-case");
    assert!(warning.detail.contains("[[Setup]]"), "{warning:?}");
    assert!(
        warning
            .detail
            .contains(&format!("{PLUGIN}/skills/setup/setup.md")),
        "{warning:?}"
    );
}

#[test]
fn a_name_spelled_as_its_file_does_not_warn() {
    let vfs = vault(&[
        (AUTHOR, &skill("Follow [[guide]] and [[Style.md]].")),
        ("references/guide.md", "Shared.\n"),
        ("references/Style.md", "Style.\n"),
    ]);
    let warnings = build(&vfs).unwrap();
    assert!(
        warnings.iter().all(|w| w.reason != "link-case"),
        "{warnings:?}"
    );
}

#[test]
fn names_that_differ_only_in_case_are_ambiguous() {
    let vfs = vault(&[
        (AUTHOR, &skill("Follow [[guide]].")),
        ("references/guide.md", "Lower.\n"),
        ("references/Guide.md", "Upper.\n"),
    ]);
    let err = refusal(&vfs);
    names(&err, AUTHOR, "[[guide]]", "section 4 rule 3");
    assert!(err.contains("references/guide.md"), "{err}");
    assert!(err.contains("references/Guide.md"), "{err}");
}

#[test]
fn a_name_with_an_extension_matches_only_that_file() {
    let vfs = vault(&[
        (AUTHOR, &skill("Run [[run.sh]].")),
        ("scripts/run.sh", "#!/bin/sh\n"),
        ("references/run.sh.md", "About run.sh.\n"),
    ]);
    build(&vfs).unwrap();
    assert!(
        author(&vfs).contains("[run.sh](../../scripts/run.sh)"),
        "{}",
        author(&vfs)
    );

    for link in ["[[run.sh]]", "[[../../references/run.sh]]"] {
        let vfs = vault(&[
            (AUTHOR, &skill(&format!("Run {link}."))),
            ("references/run.sh.md", "About run.sh.\n"),
        ]);
        names(&refusal(&vfs), AUTHOR, link, "section 4 rule 11");
    }
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
fn a_name_matching_a_shared_file_and_a_component_is_an_error_naming_both() {
    let vfs = vault(&[
        (AUTHOR, &skill("See [[x]].")),
        ("references/x.md", "Shared.\n"),
        ("skills/x/x.md", &skill("X.")),
    ]);
    let err = refusal(&vfs);
    names(&err, AUTHOR, "[[x]]", "section 4 rule 3");
    assert!(err.contains(&format!("{PLUGIN}/references/x.md")), "{err}");
    assert!(err.contains(&format!("{PLUGIN}/skills/x/x.md")), "{err}");
}

#[test]
fn a_name_only_another_plugin_holds_is_a_link_to_nothing() {
    let vfs = vault(&[(AUTHOR, &skill("See [[x]]."))]);
    put(
        &vfs,
        "marketplaces/acme/plugins/other/other.md",
        "---\nversion: 1.0.0\n---\n",
    );
    put(
        &vfs,
        "marketplaces/acme/plugins/other/skills/x/x.md",
        &skill("X."),
    );
    put(
        &vfs,
        "marketplaces/acme/plugins/other/references/x.md",
        "X.\n",
    );
    names(&refusal(&vfs), AUTHOR, "[[x]]", "section 4 rule 11");
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
        "section 4, scope of a link",
    );
    assert!(err.contains("another plugin"), "{err}");
}

#[test]
fn a_link_into_docs_is_refused_with_one_error_in_every_form() {
    let mut errors = Vec::new();
    for (link, written) in [
        ("[[docs/x]]", "[[docs/x]]"),
        ("[t](docs/x.md)", "[t](docs/x.md)"),
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
fn a_name_never_finds_a_file_in_docs_when_another_matches() {
    for (other, link) in [
        (
            "skills/author/references/notes.md",
            "[notes](references/notes.md)",
        ),
        ("references/notes.md", "[notes](../../references/notes.md)"),
    ] {
        for docs in ["skills/author/docs/notes.md", "docs/notes.md"] {
            let vfs = vault(&[
                (AUTHOR, &skill("See [[notes]].")),
                (other, "Ships.\n"),
                (docs, "For authors.\n"),
            ]);
            build(&vfs).unwrap_or_else(|err| panic!("{other} and {docs}: {err}"));
            assert!(author(&vfs).contains(link), "{}", author(&vfs));
        }
    }
}

#[test]
fn a_name_only_docs_holds_is_refused_as_a_link_into_docs() {
    for docs in ["skills/author/docs/notes.md", "docs/notes.md"] {
        let vfs = vault(&[(AUTHOR, &skill("See [[notes]].")), (docs, "For authors.\n")]);
        let err = refusal(&vfs);
        names(&err, AUTHOR, "[[notes]]", "section 4 rule 10");
        assert!(err.contains(&format!("{PLUGIN}/{docs}")), "{err}");
        assert!(err.contains("never ships"), "{err}");
    }
}

#[test]
fn a_link_out_of_the_marketplace_is_refused() {
    let vfs = vault(&[(AUTHOR, &skill("See [t](../../../../../../x.md)."))]);
    put(&vfs, "x.md", "Outside.\n");
    names(
        &refusal(&vfs),
        AUTHOR,
        "`[t](../../../../../../x.md)`",
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
    names(
        &refusal(&vfs),
        AUTHOR,
        "`[x](references/missing.md)`",
        "section 4 rule 11",
    );
}

#[test]
fn an_absolute_or_file_markdown_link_is_refused() {
    for link in ["/abs/x.md", "file:x.md"] {
        let vfs = vault(&[(AUTHOR, &skill(&format!("Read [x]({link}).")))]);
        names(
            &refusal(&vfs),
            AUTHOR,
            &format!("`[x]({link})`"),
            "section 4",
        );
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
fn a_link_to_a_command_is_a_file_link_with_a_warning_on_claude_and_fails_elsewhere() {
    let files = [
        (AUTHOR, skill("See [[deploy]].")),
        ("commands/deploy/deploy.md", skill("Deploy.")),
    ];
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let vfs = vault(&files);
    let warnings = build_for(&vfs, &["claude"]).unwrap();
    assert!(author(&vfs).contains("[deploy](../../commands/deploy.md)"));
    let warning = warnings
        .iter()
        .find(|w| w.detail.contains("Did you mean to call /[[deploy]]?"))
        .unwrap_or_else(|| panic!("no warning: {warnings:?}"));
    assert!(warning.path.ends_with(AUTHOR), "{warning:?}");
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
fn a_link_to_nothing_is_an_error() {
    let vfs = vault(&[(AUTHOR, &skill("See [[nothing]]."))]);
    names(&refusal(&vfs), AUTHOR, "[[nothing]]", "section 4 rule 11");
}

// --- Calls ------------------------------------------------------------------

/// `author`'s `SKILL.md` in each pack, under the default naming.
fn authors(vfs: &dyn Vfs) -> [(&'static str, String); 4] {
    [
        ("claude", author_in(vfs, CLAUDE, "author")),
        ("codex", author_in(vfs, CODEX, "author")),
        ("cursor", author_in(vfs, CURSOR, "tools-author")),
        ("pi", author_in(vfs, PI, "tools-author")),
    ]
}

/// The call string for `setup` in each pack, under the default naming.
const SETUP_CALLS: [&str; 4] = [
    "/tools:setup",
    "$tools:setup",
    "/tools-setup",
    "/skill:tools-setup",
];

/// `author` with `body`, the skill `setup` and the command `deploy`.
fn with_setup(body: &str) -> MemVfs {
    vault(&[
        (AUTHOR, &skill(body)),
        ("skills/setup/setup.md", &skill("## Usage\n\nSet up.")),
        ("commands/deploy/deploy.md", &skill("Deploy.")),
    ])
}

/// Writes the plugin note with `extra` in its frontmatter.
fn plugin_note(vfs: &MemVfs, extra: &str) {
    put(
        vfs,
        &format!("{PLUGIN}/tools.md"),
        &format!("---\nversion: 1.0.0\n{extra}---\n"),
    );
}

const BARE: &str = "cursor:\n  naming: bare\npi:\n  naming: bare\n";

#[test]
fn a_call_is_never_a_link_and_never_warns() {
    let vfs = with_setup("Run /[[setup]].");
    let warnings = build(&vfs).unwrap();
    let claude = author(&vfs);
    assert!(claude.contains("Run `/tools:setup`."), "{claude}");
    assert!(!claude.contains("SKILL.md"), "{claude}");
    assert!(
        !warnings.iter().any(|w| w.detail.contains("Did you mean")),
        "{warnings:?}"
    );
}

#[test]
fn a_call_renders_each_harness_s_string() {
    let vfs = with_setup("Run /[[setup]].");
    build(&vfs).unwrap();
    for ((harness, text), call) in authors(&vfs).iter().zip(SETUP_CALLS) {
        assert!(
            text.contains(&format!("Run `{call}`.")),
            "{harness}: {text}"
        );
    }
}

#[test]
fn a_call_under_bare_naming_drops_the_plugin_on_cursor_and_pi() {
    let vfs = with_setup("Run /[[setup]].");
    plugin_note(&vfs, BARE);
    build(&vfs).unwrap();
    for (pack, call) in [
        (CLAUDE, "/tools:setup"),
        (CODEX, "$tools:setup"),
        (CURSOR, "/setup"),
        (PI, "/skill:setup"),
    ] {
        let text = author_in(&vfs, pack, "author");
        assert!(text.contains(&format!("Run `{call}`.")), "{pack}: {text}");
    }
}

#[test]
fn a_call_with_words_keeps_them() {
    let vfs = with_setup("Run /[[setup|words]].");
    build(&vfs).unwrap();
    for ((harness, text), call) in authors(&vfs).iter().zip(SETUP_CALLS) {
        assert!(
            text.contains(&format!("Run words (`{call}`).")),
            "{harness}: {text}"
        );
    }
}

#[test]
fn a_call_to_a_command_renders_on_claude_and_fails_elsewhere() {
    let vfs = with_setup("Run /[[deploy]].");
    build_for(&vfs, &["claude"]).unwrap();
    assert!(author(&vfs).contains("Run `/tools:deploy`."));
    for harness in ["codex", "cursor", "pi"] {
        let vfs = with_setup("Run /[[deploy]].");
        names(
            &refusal_for(&vfs, &[harness]),
            AUTHOR,
            "/[[deploy]]",
            "rule 8",
        );
    }
}

#[test]
fn a_call_to_an_agent_fails_the_build() {
    for harness in ["claude", "codex", "cursor", "pi"] {
        let vfs = vault(&[
            (AUTHOR, &skill("Ask /[[helper]].")),
            ("agents/helper/helper.md", &skill("Help.")),
        ]);
        names(
            &refusal_for(&vfs, &[harness]),
            AUTHOR,
            "/[[helper]]",
            "rule 8",
        );
    }
}

#[test]
fn a_call_with_a_fragment_is_an_error() {
    let vfs = with_setup("Run /[[setup#Usage]].");
    names(&refusal(&vfs), AUTHOR, "/[[setup#Usage]]", "rule 8");
}

/// Adds plugin `name` of marketplace `acme`, holding skill `skill`.
fn other_plugin(vfs: &MemVfs, name: &str, skill_name: &str) {
    let base = format!("marketplaces/acme/plugins/{name}");
    put(
        vfs,
        &format!("{base}/{name}.md"),
        "---\nversion: 1.0.0\n---\n",
    );
    put(
        vfs,
        &format!("{base}/skills/{skill_name}/{skill_name}.md"),
        &skill("Review."),
    );
}

#[test]
fn a_call_finds_a_skill_in_another_plugin() {
    let vfs = vault(&[(AUTHOR, &skill("Run /[[review]]."))]);
    other_plugin(&vfs, "other", "review");
    build(&vfs).unwrap();
    assert!(author(&vfs).contains("Run `/other:review`."));
    assert!(author_in(&vfs, CURSOR, "tools-author").contains("Run `/other-review`."));
}

#[test]
fn a_name_two_plugins_share_is_an_error_naming_both() {
    let vfs = vault(&[
        (AUTHOR, &skill("Run /[[review]].")),
        ("skills/review/review.md", &skill("Review.")),
    ]);
    other_plugin(&vfs, "other", "review");
    let err = refusal(&vfs);
    names(&err, AUTHOR, "/[[review]]", "rule 8");
    assert!(err.contains("`other`"), "{err}");
    assert!(err.contains("`tools`"), "{err}");
}

#[test]
fn a_name_only_another_marketplace_holds_is_an_error() {
    let vfs = vault(&[(AUTHOR, &skill("Run /[[review]]."))]);
    put(&vfs, "marketplaces/beta/beta.md", "---\n---\n");
    put(
        &vfs,
        "marketplaces/beta/plugins/x/x.md",
        "---\nversion: 1.0.0\n---\n",
    );
    put(
        &vfs,
        "marketplaces/beta/plugins/x/skills/review/review.md",
        &skill("Review."),
    );
    names(&refusal(&vfs), AUTHOR, "/[[review]]", "rule 8");
}

/// One test per place a `/` starts a call: `$text` holds `/[[setup]]`.
macro_rules! call_position {
    ($($name:ident: $text:expr,)*) => {$(
        #[test]
        fn $name() {
            let vfs = with_setup($text);
            build(&vfs).unwrap();
            let expected = $text.replace("/[[setup]]", "`/tools:setup`");
            let claude = author(&vfs);
            assert!(claude.contains(&expected), "{claude}");
        }
    )*};
}

call_position! {
    a_call_starts_a_line: "/[[setup]] first.",
    a_call_follows_a_space: "Run /[[setup]].",
    a_call_follows_a_tab: "Run\t/[[setup]].",
    a_call_follows_a_parenthesis: "Run (/[[setup]]).",
    a_call_follows_a_double_quote: "Run \"/[[setup]]\".",
    a_call_follows_a_single_quote: "Run '/[[setup]]'.",
    a_call_follows_an_asterisk: "Run */[[setup]]*.",
    a_call_follows_a_greater_than_sign: ">/[[setup]]",
    a_call_follows_an_underscore: "Run _/[[setup]]_.",
    a_call_follows_a_pipe: "| a |/[[setup]]|",
    a_call_follows_a_bracket: "Run [/[[setup]]].",
    a_call_follows_an_equals_sign: "Run --with=/[[setup]].",
}

#[test]
fn a_slash_inside_a_path_or_after_a_link_is_text() {
    let vfs = with_setup("See dir/[[setup]] and [[author]]/[[setup]].");
    build(&vfs).unwrap();
    assert!(
        author(&vfs).contains(
            "See dir/[setup](../setup/SKILL.md) and [author](SKILL.md)/[setup](../setup/SKILL.md)."
        ),
        "{}",
        author(&vfs)
    );
}

#[test]
fn a_call_in_a_code_span_ships_as_written() {
    let vfs = with_setup("Write `/[[setup]]` or `/[[nothing]]`.");
    build(&vfs).unwrap();
    assert!(author(&vfs).contains("Write `/[[setup]]` or `/[[nothing]]`."));
}

#[test]
fn an_escaped_call_ships_as_the_notation() {
    let vfs = with_setup("Write \\/[[nothing]] to call.");
    build(&vfs).unwrap();
    assert!(author(&vfs).contains("Write /[[nothing]] to call."));
}

/// Appendix A12.
#[test]
fn a_call_in_a_container_note_does_nothing() {
    let vfs = with_setup("Body.");
    put(
        &vfs,
        &format!("{PLUGIN}/tools.md"),
        "---\nversion: 1.0.0\n---\n\nRun `/[[setup]]` first, then /[[nothing]] and [[nothing]].\n",
    );
    build(&vfs).unwrap();
}

// --- Hand-written calls -----------------------------------------------------

/// The build fails, saying to write `/[[call]]` for `written`.
fn hand_written(body: &str, written: &str, call: &str) {
    let vfs = with_setup(body);
    let err = refusal(&vfs);
    names(&err, AUTHOR, written, "rule 8");
    assert!(err.contains(&format!("write `/[[{call}]]`")), "{err}");
}

/// The build succeeds and `author` holds `body` as written.
fn ships(vfs: &MemVfs, body: &str) {
    build(vfs).unwrap();
    assert!(
        author_in(vfs, CLAUDE, "author").contains(body),
        "{}",
        author(vfs)
    );
}

#[test]
fn a_hand_written_claude_call_is_an_error() {
    hand_written("Run /tools:setup now.", "/tools:setup", "setup");
}

#[test]
fn a_hand_written_codex_call_is_an_error() {
    hand_written("Run $tools:setup now.", "$tools:setup", "setup");
}

#[test]
fn a_hand_written_cursor_call_is_an_error() {
    hand_written("Run /tools-setup now.", "/tools-setup", "setup");
}

#[test]
fn a_hand_written_pi_call_is_an_error() {
    hand_written("Run /skill:tools-setup now.", "/skill:tools-setup", "setup");
}

#[test]
fn a_hand_written_command_call_is_an_error() {
    hand_written("Run /tools:deploy now.", "/tools:deploy", "deploy");
}

#[test]
fn a_bare_hand_written_call_ships_as_written() {
    let body = "Run /setup or /skill:setup now.";
    ships(&with_setup(body), body);
}

#[test]
fn a_hand_written_call_must_end_at_the_name() {
    let body = "Run /tools:setup-x now.";
    ships(&with_setup(body), body);
}

#[test]
fn a_hand_written_call_before_a_dot_or_comma_is_an_error() {
    hand_written("Read /tools:setup.md now.", "/tools:setup", "setup");
    hand_written("Run /tools:setup, then go.", "/tools:setup", "setup");
}

#[test]
fn a_command_in_a_form_whose_pack_has_no_commands_ships_as_written() {
    let body = "Run $tools:deploy or /tools-deploy now.";
    ships(&with_setup(body), body);
}

#[test]
fn a_cursor_form_under_bare_naming_ships_as_written() {
    let body = "Run /tools-setup now.";
    let vfs = with_setup(body);
    plugin_note(&vfs, "cursor:\n  naming: bare\n");
    ships(&vfs, body);
}

#[test]
fn a_form_of_a_harness_not_built_ships_as_written() {
    let body = "Run /tools-setup now.";
    let vfs = with_setup(body);
    put(
        &vfs,
        "skillwright.md",
        "---\nbuild:\n  harnesses: [claude]\n---\n",
    );
    ships(&vfs, body);
}

#[test]
fn a_hand_written_call_in_code_ships_as_written() {
    let body =
        "Run `/tools:setup` or `$tools:setup`.\n\n```sh\n/tools-setup\n/skill:tools-setup\n```";
    ships(&with_setup(body), body);
}

#[test]
fn a_backslash_does_not_escape_a_hand_written_call() {
    hand_written("Run \\/tools:setup now.", "/tools:setup", "setup");
}

#[test]
fn a_sigil_inside_a_word_or_path_is_not_a_call() {
    let body = "See a/tools:setup and x$tools:setup.";
    ships(&with_setup(body), body);
}

// --- Code and leftovers -----------------------------------------------------

#[test]
fn links_calls_and_embeds_in_code_ship_as_written() {
    let body = "See `[[x]]`, `/[[x]]` and `![[x]]`.\n\n```md\n[[x]] /[[x]] ![[x]]\n```";
    ships(&with_setup(body), body);
}

#[test]
fn an_escape_ships_without_its_backslash() {
    let vfs = with_setup("Write \\[[x]] for a link and \\/[[x]] for a call.");
    ships(&vfs, "Write [[x]] for a link and /[[x]] for a call.");
}

#[test]
fn an_escaped_embed_ships_as_the_notation() {
    ships(
        &with_setup("Write \\![[x]] to embed."),
        "Write ![[x]] to embed.",
    );
}

#[test]
fn private_use_characters_ship_unchanged() {
    // Icon fonts, such as Nerd Fonts, put glyphs here.
    let body = "Icons \u{e000} \u{e001} \u{e002} and \\[[x]].";
    ships(
        &with_setup(body),
        "Icons \u{e000} \u{e001} \u{e002} and [[x]].",
    );
}

#[test]
fn a_noncharacter_in_a_note_is_refused() {
    let vfs = with_setup("Odd \u{fdd0} text.");
    let err = refusal(&vfs);
    assert!(err.contains("noncharacter"), "{err}");
    assert!(err.contains(AUTHOR), "{err}");
    assert!(err.contains("U+FDD0"), "{err}");
}

#[test]
fn a_leftover_in_prose_is_refused() {
    let vfs = with_setup("See [[ this.");
    let err = refusal(&vfs);
    assert!(err.contains("unresolved-syntax"), "{err}");
    names(&err, AUTHOR, "[[ this.", "section 4 rule 13");
}

#[test]
fn a_leftover_in_code_or_after_a_backslash_is_not() {
    let vfs = with_setup("See `[[ this` and \\[[ that.");
    ships(&vfs, "See `[[ this` and [[ that.");
}

// --- Embeds (section 5) -----------------------------------------------------

const GUIDE: &str = "skills/author/references/guide.md";

/// `author` holding `body`, `setup` and `deploy`, plus `files`.
fn with_files(body: &str, files: &[(&str, &str)]) -> MemVfs {
    let vfs = with_setup(body);
    for (path, text) in files {
        put(&vfs, &format!("{PLUGIN}/{path}"), text);
    }
    vfs
}

#[test]
fn the_worked_example_ships_as_shown() {
    let vfs = vault(&[
        (
            "skills/review/review.md",
            &skill("## Steps\n\n![[guide#Usage]]\n\nThen run this:\n\n![[run.sh]]"),
        ),
        (
            "skills/review/references/guide.md",
            "# Guide\n\n## Usage\n\nRun the check before each commit.\n\n## History\n\nOld notes.\n",
        ),
        (
            "skills/review/scripts/run.sh",
            "#!/bin/sh\ncargo test --workspace\n",
        ),
    ]);
    build(&vfs).unwrap();
    let shipped = author_in(&vfs, CLAUDE, "review");
    assert!(
        shipped.contains(
            "## Steps\n\n## Usage\n\nRun the check before each commit.\n\nThen run this:\n\n\
             ```bash\n#!/bin/sh\ncargo test --workspace\n```\n"
        ),
        "{shipped}"
    );
    assert!(!shipped.contains("History"), "{shipped}");
}

#[test]
fn a_heading_embed_inlines_the_section_with_its_subsections() {
    let vfs = with_files(
        "![[guide#Usage]]",
        &[(
            GUIDE,
            "## Usage\n\nU.\n\n### Detail\n\nD.\n\n## History\n\nH.\n",
        )],
    );
    ships(&vfs, "## Usage\n\nU.\n\n### Detail\n\nD.\n");
    assert!(!author(&vfs).contains("History"), "{}", author(&vfs));
}

#[test]
fn a_block_embed_inlines_the_block_without_its_marker() {
    let vfs = with_files(
        "Before.\n\n![[guide#^b1]]\n\nAfter.",
        &[(GUIDE, "One.\n\nTwo. ^b1\n\nThree.\n")],
    );
    ships(&vfs, "Before.\n\nTwo.\n\nAfter.");
}

#[test]
fn a_missing_heading_is_fragment_unresolved() {
    let vfs = with_files("![[guide#Nope]]", &[(GUIDE, "## Usage\n\nU.\n")]);
    let err = refusal(&vfs);
    assert!(err.contains("fragment-unresolved"), "{err}");
    assert!(err.contains("references/guide.md"), "{err}");
    assert!(err.contains("Nope"), "{err}");
}

#[test]
fn a_nested_embeds_link_is_written_from_the_outer_file() {
    let vfs = with_files(
        "![[outer]]",
        &[
            ("skills/author/references/outer.md", "![[inner]]\n"),
            (
                "skills/author/references/deep/inner.md",
                "See [[deep/far]].\n",
            ),
            ("skills/author/references/deep/deep/far.md", "Far.\n"),
        ],
    );
    ships(&vfs, "See [deep/far](references/deep/deep/far.md).");
}

#[test]
fn a_cycle_prints_its_loop() {
    let vfs = with_files(
        "![[a]]",
        &[
            ("skills/author/references/a.md", "![[b#Two]]\n"),
            ("skills/author/references/b.md", "## Two\n\n![[a]]\n"),
        ],
    );
    let err = refusal(&vfs);
    let refs = format!("{PLUGIN}/skills/author/references");
    assert!(err.contains("cycle"), "{err}");
    assert!(
        err.contains(&format!("{refs}/a.md -> {refs}/b.md#Two -> {refs}/a.md")),
        "{err}"
    );
}

#[test]
fn a_note_that_embeds_itself_is_a_cycle() {
    let vfs = vault(&[(AUTHOR, &skill("![[author]]"))]);
    let err = refusal(&vfs);
    let note = format!("{PLUGIN}/{AUTHOR}");
    assert!(err.contains(&format!("{note} -> {note}")), "{err}");
}

#[test]
fn two_sections_of_one_note_are_not_a_cycle() {
    let vfs = with_files(
        "![[guide#A]]",
        &[(GUIDE, "## A\n\n![[guide#B]]\n\n## B\n\nBee.\n")],
    );
    ships(&vfs, "## A\n\n## B\n\nBee.");
}

#[test]
fn a_call_in_embedded_text_renders_as_in_place() {
    let vfs = with_files("![[guide]]", &[(GUIDE, "Run /[[setup]].\n")]);
    ships(&vfs, "Run `/tools:setup`.");
    let cursor = author_in(&vfs, CURSOR, "tools-author");
    assert!(cursor.contains("Run `/tools-setup`."), "{cursor}");
}

#[test]
fn an_embedded_notes_frontmatter_is_dropped() {
    let vfs = with_files("![[guide]]", &[(GUIDE, "---\ntitle: G\n---\nBody.\n")]);
    ships(&vfs, "Body.");
    assert!(!author(&vfs).contains("title: G"), "{}", author(&vfs));
}

#[test]
fn malformed_frontmatter_in_an_embedded_note_names_both_notes() {
    for guide in ["---\ntitle: G\nBody.\n", "---\ntitle: [\n---\nBody.\n"] {
        let vfs = with_files("![[guide]]", &[(GUIDE, guide)]);
        let err = refusal(&vfs);
        names(&err, AUTHOR, "![[guide]]", "section 5 rule 4");
        assert!(err.contains(GUIDE), "{err}");
    }
}

#[test]
fn an_embed_of_a_missing_file_names_the_embed() {
    let vfs = with_setup("![[nope]]");
    names(&refusal(&vfs), AUTHOR, "![[nope]]", "section 5 rule 3");
}

#[test]
fn inlined_text_is_trimmed_of_blank_lines() {
    let vfs = with_files(
        "Before.\n\n![[guide]]\n\nAfter.",
        &[(GUIDE, "\n\n\nBody.\n\n\n")],
    );
    ships(&vfs, "Before.\n\nBody.\n\nAfter.");
}

#[test]
fn an_image_embed_is_an_image_link() {
    let vfs = with_files("![[logo.png]]", &[("skills/author/assets/logo.png", "png")]);
    ships(&vfs, "![logo.png](assets/logo.png)");
}

#[test]
fn an_image_embed_from_the_plugins_shared_assets_links_to_the_pack_root() {
    let vfs = with_files("![[logo.png]]", &[("assets/logo.png", "png")]);
    ships(&vfs, "![logo.png](../../assets/logo.png)");
}

#[test]
fn a_fragment_on_an_image_embed_is_refused() {
    let vfs = with_files(
        "![[logo.png#x]]",
        &[("skills/author/assets/logo.png", "png")],
    );
    let err = refusal(&vfs);
    assert!(err.contains("embed-fragment"), "{err}");
    names(&err, AUTHOR, "![[logo.png#x]]", "section 5 rule 13");
}

#[test]
fn an_embed_into_another_plugin_is_refused_as_a_link_is() {
    let vfs = vault(&[(AUTHOR, &skill("![[../../../other/references/x.md]]"))]);
    put(
        &vfs,
        "marketplaces/acme/plugins/other/other.md",
        "---\nversion: 1.0.0\n---\n",
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
        "![[../../../other/references/x.md]]",
        "section 4, scope of a link",
    );
    assert!(err.contains("link-out-of-scope"), "{err}");
    assert!(err.contains("another plugin"), "{err}");
}

#[test]
fn an_embed_out_of_the_marketplace_is_refused_as_a_link_is() {
    let vfs = vault(&[(AUTHOR, &skill("![[../../../../../../x.md]]"))]);
    put(&vfs, "x.md", "Outside.\n");
    let err = refusal(&vfs);
    names(
        &err,
        AUTHOR,
        "![[../../../../../../x.md]]",
        "section 4 containment rule 1",
    );
    assert!(err.contains("link-out-of-scope"), "{err}");
}

#[test]
fn a_name_in_text_embedded_from_another_component_resolves_in_that_component() {
    let vfs = with_files(
        "![[../setup/references/part]]",
        &[
            ("skills/setup/references/part.md", "See [[notes]].\n"),
            ("skills/setup/references/notes.md", "Notes.\n"),
        ],
    );
    ships(&vfs, "See [notes](../setup/references/notes.md).");
    let cursor = author_in(&vfs, CURSOR, "tools-author");
    assert!(
        cursor.contains("See [notes](../tools-setup/references/notes.md)."),
        "{cursor}"
    );
}

#[test]
fn one_note_embedded_into_two_files_links_from_each() {
    let vfs = with_files(
        "![[snippet]]",
        &[
            ("references/snippet.md", "See [[style]].\n"),
            ("references/style.md", "Style.\n"),
            (GUIDE, "![[snippet]]\n"),
        ],
    );
    ships(&vfs, "See [style](../../references/style.md).");
    let guide = read(&vfs, &format!("{CLAUDE}/{GUIDE}"));
    assert!(
        guide.contains("See [style](../../../references/style.md)."),
        "{guide}"
    );
}

#[test]
fn a_heading_path_names_the_heading_under_its_parents() {
    let vfs = with_files(
        "![[guide#Two#Usage]]",
        &[(
            GUIDE,
            "## One\n\n### Usage\n\nFirst.\n\n## Two\n\n### Usage\n\nSecond.\n",
        )],
    );
    ships(&vfs, "### Usage\n\nSecond.");
    assert!(!author(&vfs).contains("First."), "{}", author(&vfs));
}

#[test]
fn a_selector_holding_a_hash_names_a_heading_holding_one() {
    let vfs = with_files(
        "![[guide#C# notes]]",
        &[(GUIDE, "## C\n\nNot this.\n\n## C# notes\n\nThis.\n")],
    );
    ships(&vfs, "## C# notes\n\nThis.");
    assert!(!author(&vfs).contains("Not this."), "{}", author(&vfs));
}

#[test]
fn the_first_heading_that_matches_wins() {
    let vfs = with_files(
        "![[guide#Usage]]",
        &[(GUIDE, "## Usage\n\nFirst.\n\n## Usage\n\nSecond.\n")],
    );
    ships(&vfs, "## Usage\n\nFirst.");
    assert!(!author(&vfs).contains("Second."), "{}", author(&vfs));
}

#[test]
fn a_block_marker_ending_a_list_items_line_labels_that_item() {
    let vfs = with_files(
        "Before.\n\n![[guide#^two]]\n\nAfter.",
        &[(GUIDE, "- one\n- two ^two\n- three\n")],
    );
    ships(&vfs, "Before.\n\n- two\n\nAfter.");
    let text = author(&vfs);
    assert!(!text.contains("- one"), "{text}");
    assert!(!text.contains("- three"), "{text}");
}

#[test]
fn a_block_marker_alone_on_its_line_labels_the_block_above() {
    let vfs = with_files(
        "Before.\n\n![[guide#^steps]]\n\nAfter.",
        &[(GUIDE, "Intro.\n\n- one\n- two\n\n^steps\n\nOutro.\n")],
    );
    ships(&vfs, "Before.\n\n- one\n- two\n\nAfter.");
    let text = author(&vfs);
    assert!(!text.contains("^steps"), "{text}");
    assert!(!text.contains("Intro."), "{text}");
}

// --- Files inlined as fenced blocks (D8) ------------------------------------

const RUN: &str = "skills/author/scripts/run.sh";

#[test]
fn a_script_ships_as_a_fenced_block_named_by_its_extension() {
    let vfs = with_files(
        "![[run.sh]]\n\n![[data.xyz]]",
        &[
            (RUN, "echo hi\n"),
            ("skills/author/scripts/data.xyz", "raw\n"),
        ],
    );
    ships(&vfs, "```bash\necho hi\n```\n\n```\nraw\n```");
}

#[test]
fn a_python_file_is_fenced_as_python() {
    let vfs = with_files(
        "![[run.py]]",
        &[("skills/author/scripts/run.py", "print(1)\n")],
    );
    ships(&vfs, "```python\nprint(1)\n```");
}

#[test]
fn a_file_holding_a_fence_and_brackets_ships_byte_for_byte() {
    let text = "```md\n[[x]] /[[y]] ![[z]]\n```\n";
    let vfs = with_files(
        "![[notes.txt]]",
        &[("skills/author/scripts/notes.txt", text)],
    );
    ships(&vfs, &format!("````\n{text}````"));
}

#[test]
fn a_fence_indented_three_spaces_counts_and_four_does_not() {
    let three = "   ```\n[[x]]\n   ```\n";
    let four = "    ````\n";
    let vfs = with_files(
        "![[three.txt]]\n\n![[four.txt]]",
        &[
            ("skills/author/scripts/three.txt", three),
            ("skills/author/scripts/four.txt", four),
        ],
    );
    ships(&vfs, &format!("````\n{three}````\n\n```\n{four}```"));
}

#[test]
fn an_embed_in_a_list_item_is_indented_to_the_item() {
    let vfs = with_files(
        "1. Run this:\n\n   ![[run.sh]]\n2. Then:\n- ![[run.sh]]",
        &[(RUN, "echo hi\n\necho bye\n")],
    );
    ships(
        &vfs,
        "1. Run this:\n\n   ```bash\n   echo hi\n\n   echo bye\n   ```\n2. Then:\n\
         - ```bash\n  echo hi\n\n  echo bye\n  ```",
    );
}

#[test]
fn a_binary_file_is_refused() {
    let vfs = with_files("![[blob.bin]]", &[("skills/author/scripts/blob.bin", "")]);
    vfs.write_file(
        &Path::new("/v")
            .join(PLUGIN)
            .join("skills/author/scripts/blob.bin"),
        &[0xff, 0xfe, 0x00],
    )
    .unwrap();
    let err = refusal(&vfs);
    names(&err, AUTHOR, "![[blob.bin]]", "section 5 rule 13");
}

#[test]
fn a_fragment_on_a_file_is_refused() {
    let vfs = with_files("![[run.sh#main]]", &[(RUN, "echo hi\n")]);
    names(
        &refusal(&vfs),
        AUTHOR,
        "![[run.sh#main]]",
        "section 5 rule 13",
    );
}

#[test]
fn a_plain_link_to_a_file_stays_a_link() {
    let vfs = with_files("See [[run.sh]].", &[(RUN, "echo hi\n")]);
    ships(&vfs, "See [run.sh](scripts/run.sh).");
}
