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
