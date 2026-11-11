//! Frontmatter field edits that keep every byte they do not touch: what
//! `bump` uses to write a `version`.

use yaml_splice::edit_frontmatter;
use yaml_splice::parse::parse_mapping;

use crate::error::CoreError;
use crate::namespace::Namespace;

/// One frontmatter key set to a value, or removed when `value` is `None`.
///
/// No serde: [`yaml_splice::Value`] is not serde-aware, and teaching it to be
/// would put a wire concern in a pure YAML editor.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldEdit {
    /// The frontmatter key.
    pub key: String,
    /// The new value, or `None` to remove the key.
    pub value: Option<yaml_splice::Value>,
}

impl FieldEdit {
    /// An edit that sets `key` to `value`.
    pub fn set(key: impl Into<String>, value: yaml_splice::Value) -> Self {
        Self {
            key: key.into(),
            value: Some(value),
        }
    }

    /// An edit that removes `key`.
    pub fn remove(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: None,
        }
    }
}

/// The complete note text a request would produce.
///
/// `existing` is the file as it sits on disk for a rewrite, `None` for a new
/// note. A note with no frontmatter block — or an empty one — is seeded with a
/// throwaway `name: ''` so the document root is the mapping the edit needs.
/// The seed is removed after the edits unless one of them sets `name`.
///
/// `split` and `edit_frontmatter` agree on split form — the last line
/// unterminated — so the block `edit_frontmatter` returns is exactly what
/// `compose` expects, with no newline bookkeeping of our own.
pub fn compose_note(
    existing: Option<&str>,
    fields: &[FieldEdit],
    body: &str,
) -> Result<String, CoreError> {
    let split = existing.map(frontmatter_lossless::split);
    let mut shape = split.as_ref().map_or_else(
        || frontmatter_lossless::new_shape(frontmatter_lossless::NewShapeOptions::default()),
        |s| s.shape,
    );
    let (yaml, seeded) = match split.as_ref().and_then(|s| s.yaml) {
        Some(yaml) if !yaml.trim().is_empty() => (yaml.to_owned(), false),
        _ => ("name: ''".to_owned(), true),
    };
    // An adjacent-fence empty block (`---\n---`) records `close_sep: false`,
    // because it had no separator byte to restore. Filling that block gives it
    // content, and content needs the separator back — without this the closing
    // fence is written onto the end of the last key.
    if seeded {
        shape.close_sep = true;
    }

    let mut edits = splice_edits(fields);
    if seeded && !fields.iter().any(|field| field.key == "name") {
        edits.push(yaml_splice::FieldEdit::Remove {
            key: "name".to_owned(),
        });
    }
    let outcome = edit_frontmatter(&yaml, &edits).map_err(|e| {
        CoreError::refused(
            "frontmatter-edit",
            format!("frontmatter could not be edited: {e}"),
        )
    })?;
    let block = String::from_utf8(outcome.bytes).map_err(|e| {
        CoreError::refused(
            "frontmatter-encoding",
            format!("edited frontmatter is not UTF-8: {e}"),
        )
    })?;
    Ok(frontmatter_lossless::compose(Some(&block), body, shape))
}

/// [`FieldEdit`] in the editor's own vocabulary. Total: `None` is a removal,
/// and nothing here can produce the `Rename` this module never asks for.
pub fn splice_edits(edits: &[FieldEdit]) -> Vec<yaml_splice::FieldEdit> {
    edits
        .iter()
        .map(|edit| match &edit.value {
            Some(value) => yaml_splice::FieldEdit::Set {
                key: edit.key.clone(),
                value: value.clone(),
            },
            None => yaml_splice::FieldEdit::Remove {
                key: edit.key.clone(),
            },
        })
        .collect()
}

/// The frontmatter of `source` as a mapping, or `None` for no block.
fn frontmatter_mapping(source: &str) -> Result<Option<yaml_splice::Mapping>, CoreError> {
    let yaml = match frontmatter_lossless::split(source).yaml {
        Some(yaml) if !yaml.trim().is_empty() => yaml,
        _ => return Ok(None),
    };
    parse_mapping(yaml).map(Some).map_err(|e| {
        CoreError::refused(
            "frontmatter-read",
            format!("frontmatter could not be read: {e}"),
        )
    })
}

/// One of our own fields (`version`, say) read out of `source`, in any shape
/// a note may write it: bare, the flat `sw-<field>` key, or `<field>` inside
/// the nested `sw:` block.
pub fn read_ns_field(
    source: &str,
    ns: &Namespace,
    field: &str,
) -> Result<Option<String>, CoreError> {
    let Some(mapping) = frontmatter_mapping(source)? else {
        return Ok(None);
    };
    if let Some(value) = mapping.get(&ns.key(field)) {
        return Ok(Some(value.to_string()));
    }
    Ok(mapping
        .get(ns.block_key())
        .and_then(yaml_splice::Value::as_mapping)
        .and_then(|block| block.get(field))
        .map(|value| value.to_string()))
}

/// The edits to apply to `source` that set (or, for `None`, remove) each of
/// `changes`, in the shape the note already uses.
///
/// A note with a nested `sw:` block gets one edit rewriting that block; any
/// other note gets flat keys, prefixed under a namespace. Writing a flat key beside an
/// existing block would give the note both shapes, which discovery refuses.
///
/// [`FieldEdit`] addresses top-level keys only, so the block is written back
/// whole: its keys keep their order and a new one is appended, but a comment
/// inside the block is lost (a trailing one on its last entry moves to the
/// `sw:` line), quoting on its values is dropped, and a flow-style block
/// (`sw: {…}`) comes back in block style. Every byte outside the block is
/// kept.
pub fn ns_field_edits(
    source: &str,
    ns: &Namespace,
    changes: &[(&str, Option<yaml_splice::Value>)],
) -> Result<Vec<FieldEdit>, CoreError> {
    let Some(mut block) = nested_block(source, ns)? else {
        return Ok(changes
            .iter()
            .map(|(field, value)| FieldEdit {
                key: ns.key(field),
                value: value.clone(),
            })
            .collect());
    };
    let before = block.clone();
    for (field, value) in changes {
        match value {
            Some(value) => {
                block.insert(*field, value.clone());
            }
            None => {
                block.remove(field);
            }
        }
    }
    Ok(if block == before {
        Vec::new()
    } else if block.is_empty() {
        vec![FieldEdit::remove(ns.block_key())]
    } else {
        vec![FieldEdit::set(
            ns.block_key(),
            yaml_splice::Value::Map(block),
        )]
    })
}

/// How `field` is spelled in `source`, for a message: `sw.<field>` for a
/// note with a nested `sw:` block, its flat key otherwise — the shape
/// [`ns_field_edits`] writes in.
pub fn ns_field_label(source: &str, ns: &Namespace, field: &str) -> Result<String, CoreError> {
    Ok(match nested_block(source, ns)? {
        Some(_) => format!("{}.{field}", ns.block_key()),
        None => ns.key(field),
    })
}

/// The note's nested `sw:` block, or `None` when it has none or the key
/// holds something other than a mapping.
fn nested_block(source: &str, ns: &Namespace) -> Result<Option<yaml_splice::Mapping>, CoreError> {
    Ok(
        match frontmatter_mapping(source)?.and_then(|m| m.get(ns.block_key()).cloned()) {
            Some(yaml_splice::Value::Map(block)) => Some(block),
            // `sw:` with nothing under it is an empty block, not a broken one.
            Some(yaml_splice::Value::Null) => Some(yaml_splice::Mapping::new()),
            _ => None,
        },
    )
}

/// The note as it would be written to disk, frontmatter spliced and body kept.
///
/// See [`compose_note`] for the seeding rule; this is that function with the
/// body read back off `source`.
pub fn apply_field_edits(source: &str, edits: &[FieldEdit]) -> Result<String, CoreError> {
    compose_note(
        Some(source),
        edits,
        frontmatter_lossless::split(source).body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaml_splice::Value;

    fn ns_rewrite(source: &str, changes: &[(&str, Option<Value>)]) -> (Vec<FieldEdit>, String) {
        let edits = ns_field_edits(source, &sw(), changes).unwrap();
        let out = apply_field_edits(source, &edits).unwrap();
        (edits, out)
    }

    fn str_value(s: &str) -> Option<Value> {
        Some(Value::Str(s.to_owned()))
    }

    #[test]
    fn an_edit_of_an_empty_block_leaves_no_seed() {
        let edits = [FieldEdit::set("version", Value::Str("1.0.1".to_owned()))];
        let out = apply_field_edits("---\n---\n\n# acme\n", &edits).unwrap();
        assert_eq!(out, "---\nversion: 1.0.1\n---\n\n# acme\n");
    }

    #[test]
    fn ns_field_edits_is_empty_when_the_block_already_holds_the_value() {
        let source = "---\nname: a\nsw:\n  version: 1.0.0\n---\nbody\n";
        let (edits, out) = ns_rewrite(source, &[("version", str_value("1.0.0"))]);
        assert!(edits.is_empty());
        assert_eq!(out, source);
    }

    #[test]
    fn ns_field_edits_removes_a_block_its_changes_empty() {
        let source = "---\nname: a\nsw:\n  version: 1.0.0\ntail: x\n---\nbody\n";
        let (edits, out) = ns_rewrite(source, &[("version", None)]);
        assert_eq!(edits, vec![FieldEdit::remove("sw")]);
        assert_eq!(out, "---\nname: a\ntail: x\n---\nbody\n");
    }

    #[test]
    fn ns_field_edits_fills_a_null_block() {
        let source = "---\nname: a\nsw:\ntail: x\n---\nbody\n";
        let (_, out) = ns_rewrite(source, &[("plugin", str_value("p"))]);
        assert_eq!(out, "---\nname: a\nsw:\n  plugin: p\ntail: x\n---\nbody\n");
    }

    #[test]
    fn ns_field_edits_falls_back_to_flat_keys_beside_a_block_that_is_not_a_map() {
        let source = "---\nname: a\nsw: oops\n---\nbody\n";
        let (edits, _) = ns_rewrite(source, &[("plugin", str_value("p"))]);
        assert_eq!(
            edits,
            vec![FieldEdit::set("sw-plugin", Value::Str("p".to_owned()))]
        );
    }

    /// The costs the doc names, pinned so a splicer upgrade that changes
    /// them shows up here: a flow block comes back in block style and its
    /// quoting is dropped, while bytes outside the block are kept.
    #[test]
    fn ns_field_edits_rewrites_a_flow_block_in_block_style() {
        let source =
            "---\nname: a # keep\nsw: {version: 1.0.0, plugin: \"p\"}\ntail: x\n---\nbody\n";
        let (_, out) = ns_rewrite(source, &[("version", str_value("1.0.1"))]);
        assert_eq!(
            out,
            "---\nname: a # keep\nsw:\n  version: 1.0.1\n  plugin: p\ntail: x\n---\nbody\n"
        );
    }

    #[test]
    fn ns_field_edits_drops_a_comment_inside_the_block() {
        let source = "---\nname: a\nsw:\n  # note\n  plugin: \"p\"\n  version: 1.0.0 # last\ntail: x\n---\nbody\n";
        let (_, out) = ns_rewrite(source, &[("version", str_value("1.0.1"))]);
        assert_eq!(
            out,
            "---\nname: a\nsw: # last\n  plugin: p\n  version: 1.0.1\ntail: x\n---\nbody\n"
        );
    }

    #[test]
    fn ns_field_label_spells_the_key_the_way_the_note_does() {
        let nested = "---\nname: a\nsw:\n  version: 1.0.0\n---\n";
        let flat = "---\nname: a\nsw-version: 1.0.0\n---\n";
        assert_eq!(
            ns_field_label(nested, &sw(), "version").unwrap(),
            "sw.version"
        );
        assert_eq!(
            ns_field_label(flat, &sw(), "version").unwrap(),
            "sw-version"
        );
    }

    fn sw() -> Namespace {
        Namespace::sw()
    }
}
