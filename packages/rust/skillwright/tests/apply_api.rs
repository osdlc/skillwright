//! `skillwright::apply` from outside the crate: a `Vault` from its
//! constructor, and a planned op previewed, then applied.

use std::fs;

use skillwright::apply::{self, Vault};
use tradecraft_core::{Namespace, NativeVfs, NewNote, plan_create_note};

#[test]
fn a_vault_from_new_previews_and_applies_a_planned_create() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("skillwright.md"), "---\nname: home\n---\n").unwrap();
    fs::create_dir_all(root.join("skills")).unwrap();

    let note = NewNote::new("lint", &[], "\n# Lint\n");
    let op = plan_create_note(&NativeVfs, root, &Namespace::bare(), &note).unwrap();
    let vault = Vault::new(root, &NativeVfs);

    let preview = apply::preview(vault, &op).unwrap();
    assert!(preview.preview);
    assert!(!root.join("skills/lint.md").exists());

    let applied = apply::apply(vault, &op).unwrap();
    assert!(!applied.preview);
    assert!(root.join("skills/lint.md").exists());
}
