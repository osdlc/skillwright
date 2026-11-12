//! Frontmatter field edits that keep every byte they do not touch: what
//! `bump` uses to read and write a note's `version`.

use yaml_splice::edit_frontmatter;
use yaml_splice::parse::parse_mapping;

use crate::error::CoreError;

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

/// The note with its `version` set to `version`, every other byte kept. A
/// note with no frontmatter block, or an empty one, starts from an empty
/// mapping. The block edited is the one the build reads (the lenient split),
/// so a note never gains a second block.
///
/// # Errors
///
/// A refusal when the frontmatter cannot be read or edited, and
/// `refused: frontmatter-edit` for an empty block only the lenient split
/// reads, which cannot be filled in place.
pub fn set_version(source: &str, version: &str) -> Result<String, CoreError> {
    let split = frontmatter_lossless::split(source);
    if let Some(lenient) = frontmatter_lossless::split_lenient(source) {
        // `lenient.yaml` is a slice of `source`. Widen it over a CRLF `\r`
        // to match the strict split's range.
        let start = lenient.yaml.as_ptr() as usize - source.as_ptr() as usize;
        let mut end = start + lenient.yaml.len();
        if source[end..].starts_with('\r') {
            end += 1;
        }
        if split.yaml.is_none() || split.yaml_range != (start..end) {
            if lenient.yaml.trim().is_empty() {
                return Err(CoreError::refused(
                    "frontmatter-edit",
                    "the frontmatter block is empty and its fences are not plain `---` \
                     lines at the top of the note, so `version` cannot be added in place. \
                     Write each fence as `---` alone on its line, at the top",
                ));
            }
            let block = edit_version(&source[start..end], version)?;
            return Ok(format!("{}{block}{}", &source[..start], &source[end..]));
        }
    }
    let mut shape = split.shape;
    let yaml = match split.yaml {
        Some(yaml) if !yaml.trim().is_empty() => yaml,
        // An adjacent-fence empty block (`---\n---`) records `close_sep:
        // false`; filled, it needs the separator back, or the closing fence
        // lands on the end of the last key.
        _ => {
            shape.close_sep = true;
            ""
        }
    };
    let block = edit_version(yaml, version)?;
    Ok(frontmatter_lossless::compose(
        Some(&block),
        split.body,
        shape,
    ))
}

/// `yaml` with its `version` set, every other byte kept.
fn edit_version(yaml: &str, version: &str) -> Result<String, CoreError> {
    let edit = yaml_splice::FieldEdit::Set {
        key: "version".to_owned(),
        value: yaml_splice::Value::Str(version.to_owned()),
    };
    let outcome = edit_frontmatter(yaml, &[edit]).map_err(|e| {
        CoreError::refused(
            "frontmatter-edit",
            format!("frontmatter could not be edited: {e}"),
        )
    })?;
    String::from_utf8(outcome.bytes).map_err(|e| {
        CoreError::refused(
            "frontmatter-encoding",
            format!("edited frontmatter is not UTF-8: {e}"),
        )
    })
}

/// The frontmatter of `source` as the build reads it, as a mapping, or
/// `None` for no block.
fn frontmatter_mapping(source: &str) -> Result<Option<yaml_splice::Mapping>, CoreError> {
    let yaml = crate::skill::split_frontmatter(source).0;
    if yaml.trim().is_empty() {
        return Ok(None);
    }
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

    #[test]
    fn a_note_without_frontmatter_gets_a_version_block() {
        let out = set_version("Body.\n", "1.0.0").unwrap();
        assert_eq!(out, "---\nversion: 1.0.0\n---\nBody.\n");
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

    /// Fences only the build's lenient split accepts: the version is read
    /// and set inside the block the build reads, never in a second block.
    #[test]
    fn lenient_fences_are_read_and_edited_in_place() {
        for (source, edited) in [
            // Trailing space on the opening fence.
            (
                "--- \nversion: 1.0.0\n---\nb\n",
                "--- \nversion: 1.0.1\n---\nb\n",
            ),
            // Trailing tab on the closing fence.
            (
                "---\nversion: 1.0.0\n---\t\nb\n",
                "---\nversion: 1.0.1\n---\t\nb\n",
            ),
            // A blank line before the opening fence.
            (
                "\n---\nversion: 1.0.0\n---\nb\n",
                "\n---\nversion: 1.0.1\n---\nb\n",
            ),
            // CRLF fences with trailing tabs and a leading blank line.
            (
                "\r\n---\t\r\nversion: 1.0.0\r\n---\t\r\nb\r\n",
                "\r\n---\t\r\nversion: 1.0.1\r\n---\t\r\nb\r\n",
            ),
            // A `--- ` line closes the block before a later strict `---`.
            (
                "---\nversion: 1.0.0\n--- \nversion: 9.9.9\n---\nb\n",
                "---\nversion: 1.0.1\n--- \nversion: 9.9.9\n---\nb\n",
            ),
        ] {
            assert_eq!(
                read_version(source).unwrap().as_deref(),
                Some("1.0.0"),
                "{source:?}"
            );
            assert_eq!(set_version(source, "1.0.1").unwrap(), edited, "{source:?}");
        }
    }

    /// A lenient block without a version gets the key added inside it.
    #[test]
    fn a_lenient_block_without_a_version_gains_one_inside_it() {
        let out = set_version("--- \ndescription: d\n---\nb\n", "1.0.0").unwrap();
        assert_eq!(out, "--- \ndescription: d\nversion: 1.0.0\n---\nb\n");
    }

    /// An empty block only the lenient split reads cannot be filled in
    /// place, so it is refused rather than given a second block.
    #[test]
    fn an_empty_lenient_block_is_refused() {
        for source in ["--- \n---\nb\n", "\n---\n---\nb\n"] {
            assert_eq!(read_version(source).unwrap(), None, "{source:?}");
            let err = set_version(source, "1.0.0").unwrap_err().to_string();
            assert!(err.contains("frontmatter-edit"), "{source:?}: {err}");
        }
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
