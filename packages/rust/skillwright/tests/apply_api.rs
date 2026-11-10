//! `skillwright::apply` from outside the crate: a `Vault` from its
//! constructor, and a planned op previewed, then applied.

use std::fs;

use skillwright::apply::{self, Applied, ApplyError, Vault};
use tradecraft_core::{Namespace, NativeVfs, NewNote, plan_create_note};

#[test]
fn a_vault_from_new_previews_and_applies_a_planned_create() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("skillwright.md"), "---\nname: home\n---\n").unwrap();
    fs::create_dir_all(root.join("skills")).unwrap();

    let mut note = NewNote::new("lint");
    note.body = "\n# Lint\n";
    let op = plan_create_note(&NativeVfs, root, &Namespace::bare(), &note).unwrap();
    let vault = Vault::new(root, &NativeVfs);

    let preview = apply::preview(vault, &op).unwrap();
    assert!(preview.preview);
    assert!(!root.join("skills/lint.md").exists());

    let applied = apply::apply(vault, &op).unwrap();
    assert!(!applied.preview);
    assert!(root.join("skills/lint.md").exists());
    for entry in &applied.entries {
        let word = match &entry.applied {
            Applied::Created => "created",
            Applied::Scaffolded => "scaffolded",
            _ => "other",
        };
        assert_eq!(word, "created", "{}", entry.path);
    }

    let again = apply::apply(vault, &op).unwrap_err();
    let why = match &again {
        ApplyError::Refused(why) => why.as_str(),
        _ => "",
    };
    assert!(!why.is_empty(), "{again:?}");
}

/// A vault at a temp folder holding `files`.
fn vault_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, text) in files {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    dir
}

fn move_op(from: &str, to: &str) -> tradecraft_core::AuthoringOp {
    tradecraft_core::AuthoringOp::Move {
        from: from.to_owned(),
        to: to.to_owned(),
        bundle: true,
    }
}

/// Moving a marketplace's entry onto a `skillwright.md` that is not a
/// marketplace entry moves the folder as it is. A link from outside stays as
/// written, as for any folder the relinker cannot name.
#[test]
fn moving_a_marketplace_entry_onto_a_non_marketplace_path_moves_the_folder() {
    let link = "[a](marketplaces/m1/plugins/p/skills/a.md)\n";
    let dir = vault_with(&[
        ("skillwright.md", "---\nname: home\n---\n"),
        ("marketplaces/m1/skillwright.md", "---\nname: m1\n---\n"),
        (
            "marketplaces/m1/plugins/p/skills/a.md",
            "---\nname: a\n---\n",
        ),
        ("notes.md", link),
    ]);
    let root = dir.path();
    let op = move_op("marketplaces/m1/skillwright.md", "plugins/x/skillwright.md");
    apply::apply(Vault::new(root, &NativeVfs), &op).unwrap();
    assert!(root.join("plugins/x/skillwright.md").is_file());
    assert!(root.join("plugins/x/plugins/p/skills/a.md").is_file());
    assert!(!root.join("marketplaces/m1").exists());
    assert_eq!(fs::read_to_string(root.join("notes.md")).unwrap(), link);
}

/// A marketplace below the top is not a marketplace entry to `apply`: its
/// move takes the note route, and the relinker refuses a bundle whose entry
/// is not `<dir>/<dir>.md`.
#[test]
fn a_nested_marketplace_entry_moves_as_a_note() {
    let dir = vault_with(&[
        ("skillwright.md", "---\nname: home\n---\n"),
        ("marketplaces/a/skillwright.md", "---\nname: a\n---\n"),
        (
            "marketplaces/a/marketplaces/b/skillwright.md",
            "---\nname: b\n---\n",
        ),
        (
            "marketplaces/a/marketplaces/b/plugins/p/skills/s.md",
            "---\nname: s\n---\n",
        ),
    ]);
    let root = dir.path();
    let op = move_op(
        "marketplaces/a/marketplaces/b/skillwright.md",
        "marketplaces/a/marketplaces/c/skillwright.md",
    );
    let err = apply::apply(Vault::new(root, &NativeVfs), &op).unwrap_err();
    let why = match &err {
        ApplyError::Refused(why) => why.as_str(),
        _ => "",
    };
    assert!(why.contains("is not a bundle entry note"), "{err:?}");
    assert!(
        root.join("marketplaces/a/marketplaces/b/skillwright.md")
            .is_file()
    );
    assert!(!root.join("marketplaces/a/marketplaces/c").exists());
}
