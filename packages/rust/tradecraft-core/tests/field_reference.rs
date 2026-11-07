//! The published field reference is the key table's own text.
//!
//! When the table changes, refresh it from `packages/rust` with
//! `TRADECRAFT_BLESS=1 cargo test -p tradecraft-core --test field_reference`.

use tradecraft_core::keys::field_reference;

const REFERENCE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/reference/fields.md"
);

#[test]
fn the_field_reference_matches_the_key_table() {
    let generated = field_reference();
    if std::env::var_os("TRADECRAFT_BLESS").is_some() {
        std::fs::write(REFERENCE, &generated).unwrap();
    }
    let committed = std::fs::read_to_string(REFERENCE).unwrap();
    assert!(
        committed == generated,
        "docs/reference/fields.md drifted from the key table; see this test's module doc"
    );
}
