//! Frontmatter field edits that keep every byte they do not touch: what
//! `bump` uses to read and write a note's `version`.

use yaml_splice::edit_frontmatter;
use yaml_splice::parse::parse_mapping;

use crate::error::CoreError;

/// One frontmatter key set to a value, or removed when `value` is `None`.
#[derive(Debug, Clone, PartialEq)]
struct FieldEdit {
    key: String,
    value: Option<yaml_splice::Value>,
}

/// The note's `version`, or `None` when its frontmatter has none.
///
/// # Errors
///
/// * `refused: frontmatter-read` when the frontmatter is not a YAML mapping.
/// * `refused: malformed` when the `version` is not a string, as the build
///   refuses it.
pub fn read_version(source: &str) -> Result<Option<String>, CoreError> {
    use yaml_splice::Value;
    let Some(mapping) = frontmatter_mapping(source)? else {
        return Ok(None);
    };
    // Only the kind matters to the refusal, so a list or mapping stands in
    // empty.
    let found = match mapping.get("version") {
        None => return Ok(None),
        Some(Value::Str(version)) => return Ok(Some(version.clone())),
        Some(Value::Null) => serde_json::Value::Null,
        Some(Value::Bool(b)) => serde_json::Value::Bool(*b),
        Some(Value::Int(_) | Value::Float(_)) => serde_json::Value::from(0),
        Some(Value::Seq(_)) => serde_json::Value::Array(Vec::new()),
        Some(Value::Map(_)) => serde_json::Value::Object(serde_json::Map::new()),
    };
    let refusal = crate::keys::check_shape(crate::EntityKind::Plugin, "version", &found)
        .err()
        .map_or_else(
            || "`version` must be a string".to_owned(),
            |r| r.sentence("version"),
        );
    Err(CoreError::refused("malformed", refusal))
}

/// The note with its `version` set to `version`, every other byte kept.
///
/// # Errors
///
/// A refusal when the frontmatter cannot be read or edited.
pub fn set_version(source: &str, version: &str) -> Result<String, CoreError> {
    let edit = FieldEdit {
        key: "version".to_owned(),
        value: Some(yaml_splice::Value::Str(version.to_owned())),
    };
    compose_note(
        Some(source),
        &[edit],
        frontmatter_lossless::split(source).body,
    )
}

/// The complete note text a request would produce.
///
/// `existing` is the file as it sits on disk for a rewrite, `None` for a new
/// note. A note with no frontmatter block, or an empty one, starts from an
/// empty mapping.
fn compose_note(
    existing: Option<&str>,
    fields: &[FieldEdit],
    body: &str,
) -> Result<String, CoreError> {
    let split = existing.map(frontmatter_lossless::split);
    let mut shape = split.as_ref().map_or_else(
        || frontmatter_lossless::new_shape(frontmatter_lossless::NewShapeOptions::default()),
        |s| s.shape,
    );
    let yaml = match split.as_ref().and_then(|s| s.yaml) {
        Some(yaml) if !yaml.trim().is_empty() => yaml.to_owned(),
        // An adjacent-fence empty block (`---\n---`) records `close_sep:
        // false`; filled, it needs the separator back, or the closing fence
        // lands on the end of the last key.
        _ => {
            shape.close_sep = true;
            String::new()
        }
    };
    let outcome = edit_frontmatter(&yaml, &splice_edits(fields)).map_err(|e| {
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

/// [`FieldEdit`] in the editor's own vocabulary.
fn splice_edits(edits: &[FieldEdit]) -> Vec<yaml_splice::FieldEdit> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use yaml_splice::Value;

    fn set(key: &str, value: &str) -> FieldEdit {
        FieldEdit {
            key: key.to_owned(),
            value: Some(Value::Str(value.to_owned())),
        }
    }

    #[test]
    fn a_note_without_frontmatter_gets_no_name() {
        let out = compose_note(None, &[set("version", "1.0.0")], "Body.\n").unwrap();
        assert_eq!(out, "---\nversion: 1.0.0\n---\nBody.\n");
        let out = compose_note(Some("Body.\n"), &[set("version", "1.0.0")], "Body.\n").unwrap();
        assert!(!out.contains("name"), "{out}");
        assert!(out.contains("version: 1.0.0"), "{out}");
    }

    /// The version is read as the note wrote it, and only a string is a
    /// version, as the build reads it.
    #[test]
    fn only_a_string_reads_as_a_version() {
        let read = |yaml: &str| read_version(&format!("---\n{yaml}\n---\n"));
        assert_eq!(read("version: '1.10'").unwrap().as_deref(), Some("1.10"));
        assert_eq!(read("version: 1.2.3").unwrap().as_deref(), Some("1.2.3"));
        assert_eq!(read("description: d").unwrap(), None);
        for (yaml, found) in [
            ("version:", "nothing"),
            ("version: 1.10", "a number — quote it"),
            ("version: 2", "a number — quote it"),
            ("version: [1]", "a list"),
        ] {
            let err = read(yaml).unwrap_err().to_string();
            assert!(
                err.contains(&format!("`version` must be a string, not {found}")),
                "{yaml}: {err}"
            );
        }
    }

    #[test]
    fn setting_the_version_of_an_empty_block_leaves_only_the_version() {
        let out = set_version("---\n---\n\n# acme\n", "1.0.1").unwrap();
        assert_eq!(out, "---\nversion: 1.0.1\n---\n\n# acme\n");
    }

    #[test]
    fn set_version_keeps_every_other_byte() {
        let source = "---\ndescription: d # keep\nversion: 1.0.0\ntail: x\n---\nbody\n";
        let out = set_version(source, "1.0.1").unwrap();
        assert_eq!(
            out,
            "---\ndescription: d # keep\nversion: 1.0.1\ntail: x\n---\nbody\n"
        );
    }

    #[test]
    fn read_version_reads_the_plain_key_only() {
        assert_eq!(
            read_version("---\nversion: 1.2.3\n---\n")
                .unwrap()
                .as_deref(),
            Some("1.2.3")
        );
        assert_eq!(read_version("---\nsw-version: 1.2.3\n---\n").unwrap(), None);
        assert_eq!(
            read_version("---\nsw:\n  version: 1.2.3\n---\n").unwrap(),
            None
        );
        assert_eq!(read_version("Body.\n").unwrap(), None);
    }
}
