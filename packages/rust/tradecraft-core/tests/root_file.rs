//! The root file's key set, the key table's refusals, the `name` key and the
//! four `targets` values (spec sections 2, 3 rule 4, 6 and 9 rule 12, and
//! section 11 cases 2 and 7), at the core's public API.

use std::path::Path;

use tradecraft_core::{ExportMode, MemVfs, Vfs, load_vault, parse_root_file, read_layout};

/// `build_vault` over the one marketplace a request covers: its report.
fn export(
    vfs: &dyn tradecraft_core::Vfs,
    request: &tradecraft_core::VaultRequest,
) -> Result<tradecraft_core::ExportReport, tradecraft_core::CoreError> {
    let mut report = tradecraft_core::build_vault(vfs, request)?;
    assert!(report.refused.is_empty(), "{:?}", report.refused);
    Ok(report.marketplaces.remove(0).report)
}

const SKILL: &str = "---\ndescription: Use it.\n---\n\nBody.\n";

/// A vault at `/v`: marketplace `acme`, plugin `tools`, skill `review`, with
/// `files` laid over it.
fn vault(files: &[(&str, &str)]) -> MemVfs {
    let vfs = MemVfs::new();
    for (path, text) in [
        ("skillwright.md", "---\n---\n"),
        ("marketplaces/acme/acme.md", "---\n---\n"),
        ("marketplaces/acme/plugins/tools/tools.md", "---\n---\n"),
        (
            "marketplaces/acme/plugins/tools/skills/review/review.md",
            SKILL,
        ),
    ]
    .into_iter()
    .chain(files.iter().copied())
    {
        let path = Path::new("/v").join(path);
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        vfs.write_file(&path, text.as_bytes()).unwrap();
    }
    vfs
}

/// The root file holding `frontmatter`.
fn root(frontmatter: &str) -> MemVfs {
    vault(&[("skillwright.md", &format!("---\n{frontmatter}\n---\n"))])
}

/// The layout refusal for `vfs`, as text. `load_vault` stops on it too.
fn layout_refusal(vfs: &MemVfs) -> String {
    let said = read_layout(vfs, Path::new("/v")).unwrap_err().to_string();
    assert!(said.contains("refused: layout"), "{said}");
    let loaded = load_vault(vfs, Path::new("/v"), None).unwrap_err();
    assert_eq!(loaded.to_string(), said);
    said
}

/// A build of every marketplace in `vfs`, its error as text.
fn build_error(vfs: &MemVfs) -> String {
    let loaded = match load_vault(vfs, Path::new("/v"), None) {
        Ok(loaded) => loaded,
        Err(err) => return err.to_string(),
    };
    for marketplace in &loaded.marketplaces {
        let request = loaded.vault_request(marketplace, ExportMode::Build);
        if let Err(err) = export(vfs, &request) {
            return err.to_string();
        }
    }
    panic!("the build passed");
}

#[test]
fn a_key_that_is_not_inherited_is_refused_naming_the_notes_that_accept_it() {
    for (line, accepts, refuses) in [
        ("owner: Ada", &["`<m>.md`"][..], &["`<p>.md`"][..]),
        ("keywords: [a]", &["`<p>.md`"], &["`<m>.md`"]),
        ("category: tools", &["`<p>.md`"], &["`<m>.md`"]),
        ("version: '1.0.0'", &["`<m>.md`", "`<p>.md`"], &[]),
        ("description: Mine.", &["`<m>.md`", "`<p>.md`"], &[]),
        ("display_name: Acme", &["`<m>.md`", "`<p>.md`"], &[]),
    ] {
        let key = line.split(':').next().unwrap();
        let said = layout_refusal(&root(line));
        let item = said
            .lines()
            .find(|l| l.contains(&format!("`{key}`")))
            .unwrap_or_else(|| panic!("{line}: {said}"));
        for file in accepts {
            assert!(item.contains(file), "{line}: {item}");
        }
        for file in refuses {
            assert!(!item.contains(file), "{line}: {item}");
        }
        let err = parse_root_file(&format!("---\n{line}\n---\n")).unwrap_err();
        assert!(err.to_string().contains(&format!("`{key}`")), "{err}");
    }
}

#[test]
fn case_2_workspace_and_owner_in_the_root_file_are_refused() {
    let said = layout_refusal(&root("workspace: true\nowner: Ada"));
    assert!(said.contains("`workspace`"), "{said}");
    assert!(said.contains("`owner`"), "{said}");
    parse_root_file("---\nworkspace: true\n---\n").unwrap_err();
}

#[test]
fn cases_2_and_7_come_out_in_one_message() {
    let said = layout_refusal(&root("owner: Ada\nbuild:\n  namespace: sw"));
    assert!(said.contains("`owner`"), "{said}");
    assert!(said.contains("`build.namespace`"), "{said}");
    assert_eq!(said.matches("The expected layout").count(), 1, "{said}");
}

#[test]
fn the_inherited_keys_are_read_from_the_root_file() {
    let text = "---\nauthor: Ada\nlicense: MIT\nrepository: https://x.example/r\n\
                homepage: https://x.example\nclaude: {}\n\
                codex: {}\ncursor: {naming: bare}\npi: {}\n\
                build:\n  harnesses: [claude]\n  bump: {patch: increment}\n---\n";
    let root = parse_root_file(text).unwrap();
    assert!(root.warnings.is_empty(), "{:?}", root.warnings);
    read_layout(&vault(&[("skillwright.md", text)]), Path::new("/v")).unwrap();
}

#[test]
fn an_unknown_root_key_and_an_unknown_build_key_warn() {
    let text = "---\nflavour: mint\nbuild:\n  colour: red\n---\n";
    let root = parse_root_file(text).unwrap();
    assert_eq!(root.warnings.len(), 2, "{:?}", root.warnings);
    assert!(
        root.warnings[0].contains("`flavour`"),
        "{:?}",
        root.warnings
    );
    assert!(
        root.warnings[1].contains("`build.colour`"),
        "{:?}",
        root.warnings
    );
    load_vault(&vault(&[("skillwright.md", text)]), Path::new("/v"), None).unwrap();
}

#[test]
fn build_outside_the_root_file_is_refused() {
    for note in [
        "marketplaces/acme/acme.md",
        "marketplaces/acme/plugins/tools/tools.md",
        "marketplaces/acme/plugins/tools/skills/review/review.md",
    ] {
        let vfs = vault(&[(
            note,
            "---\ndescription: Use it.\nbuild:\n  harnesses: [claude]\n---\n",
        )]);
        let said = build_error(&vfs);
        assert!(said.contains(note), "{note}: {said}");
        assert!(said.contains("`build`"), "{note}: {said}");
    }
}

#[test]
fn a_name_key_is_refused_on_every_note_saying_to_delete_it() {
    let said = layout_refusal(&root("name: acme"));
    assert!(said.contains("`name`"), "{said}");
    assert!(said.contains("delete it"), "{said}");
    let err = parse_root_file("---\nname: acme\n---\n").unwrap_err();
    assert!(err.to_string().contains("delete it"), "{err}");

    for (note, name) in [
        ("marketplaces/acme/acme.md", "acme"),
        ("marketplaces/acme/plugins/tools/tools.md", "tools"),
        (
            "marketplaces/acme/plugins/tools/skills/review/review.md",
            "review",
        ),
    ] {
        let vfs = vault(&[(
            note,
            &format!("---\nname: {name}\ndescription: Use it.\n---\n"),
        )]);
        let said = build_error(&vfs);
        assert!(said.contains(note), "{note}: {said}");
        assert!(said.contains("`name`"), "{note}: {said}");
        assert!(said.contains("delete it"), "{note}: {said}");
    }
}

/// Spec section 3 rule 4: `name` is refused in a harness block too, on every
/// note and in the root file.
#[test]
fn a_name_key_in_a_harness_block_is_refused_saying_to_delete_it() {
    let err = parse_root_file("---\nclaude:\n  name: acme\n---\n").unwrap_err();
    assert!(err.to_string().contains("claude.name"), "{err}");
    assert!(err.to_string().contains("delete it"), "{err}");

    for note in [
        "marketplaces/acme/acme.md",
        "marketplaces/acme/plugins/tools/tools.md",
        "marketplaces/acme/plugins/tools/skills/review/review.md",
    ] {
        let vfs = vault(&[(
            note,
            "---\ndescription: Use it.\ncodex:\n  name: other\n---\n",
        )]);
        let said = build_error(&vfs);
        assert!(said.contains(note), "{note}: {said}");
        assert!(said.contains("`codex.name`"), "{note}: {said}");
        assert!(said.contains("delete it"), "{note}: {said}");
    }
}

#[test]
fn a_skill_with_malformed_frontmatter_is_an_error_naming_it() {
    let note = "marketplaces/acme/plugins/tools/skills/review/review.md";
    for text in [
        "---\ndescription: [unclosed\n---\n",
        "---\ndescription: Use it.\n",
    ] {
        let said = build_error(&vault(&[(note, text)]));
        assert!(said.contains(note), "{text}: {said}");
    }
}

#[test]
fn a_skill_with_no_description_is_an_error_even_when_its_plugin_has_one() {
    let note = "marketplaces/acme/plugins/tools/skills/review/review.md";
    let vfs = vault(&[
        (
            "marketplaces/acme/plugins/tools/tools.md",
            "---\ndescription: The tools.\n---\n",
        ),
        (note, "---\n---\n\nBody.\n"),
    ]);
    let said = build_error(&vfs);
    assert!(said.contains("missing-description"), "{said}");
    assert!(said.contains(note), "{said}");
}

/// The four names every refusal of a harness name lists.
fn names_the_four(said: &str) {
    for name in ["claude", "codex", "cursor", "pi"] {
        assert!(said.contains(name), "{name}: {said}");
    }
}

#[test]
fn the_known_harnesses_are_the_four() {
    assert_eq!(
        tradecraft_core::KNOWN_TARGETS,
        ["claude", "codex", "cursor", "pi"]
    );
    for alias in ["claude-code", "codex-cli", "opencode", "amp"] {
        assert!(!tradecraft_core::KNOWN_TARGETS.contains(&alias), "{alias}");
    }
}

/// `targets` is removed at every level: the root file, a marketplace note,
/// a plugin note and a component note each refuse it, saying what to use.
#[test]
fn targets_is_refused_at_every_level() {
    let err = parse_root_file("---\ntargets: [claude]\n---\n").unwrap_err();
    let said = err.to_string();
    assert!(said.contains("`targets`"), "{said}");
    assert!(
        said.contains("`build.harnesses`") && said.contains("`--harness`"),
        "{said}"
    );
    for note in [
        "marketplaces/acme/acme.md",
        "marketplaces/acme/plugins/tools/tools.md",
        "marketplaces/acme/plugins/tools/skills/review/review.md",
    ] {
        let said = build_error(&vault(&[(
            note,
            "---\ndescription: Use it.\ntargets: [claude]\n---\n",
        )]));
        assert!(said.contains(note), "{note}: {said}");
        assert!(
            said.contains("`targets` is a removed key"),
            "{note}: {said}"
        );
        assert!(said.contains("`--harness`"), "{note}: {said}");
    }
}

#[test]
fn build_harnesses_accept_only_the_four_harness_names() {
    let err = parse_root_file("---\nbuild:\n  harnesses: [amp]\n---\n").unwrap_err();
    assert!(err.to_string().contains("amp"), "{err}");
    names_the_four(&err.to_string());
}

#[test]
fn build_harnesses_picks_the_trees_written() {
    let vfs = root("build:\n  harnesses: [claude, pi]");
    let loaded = load_vault(&vfs, Path::new("/v"), None).unwrap();
    let request = loaded.vault_request(&loaded.marketplaces[0], ExportMode::Build);
    export(&vfs, &request).unwrap();
    let mut trees: Vec<String> = vfs
        .read_dir(Path::new("/v/dist/acme"))
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    trees.sort();
    assert_eq!(trees, [".claude-plugin", ".pi"]);
}

#[test]
fn a_bad_marketplace_note_fails_the_build_naming_it() {
    let note = "marketplaces/acme/acme.md";
    let said = build_error(&vault(&[(note, "---\nowner: [x]\n---\n")]));
    assert!(said.contains(note), "{said}");
}
