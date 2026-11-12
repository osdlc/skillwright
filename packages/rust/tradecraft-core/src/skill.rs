//! Small readers every note path shares: the frontmatter split, a UTF-8
//! read and a forward-slashed path.
//!
//! [`split_frontmatter`] wraps `frontmatter_lossless::split_lenient`, the
//! BOM/CRLF-tolerant fence split.

use std::path::{Component, Path};

use crate::error::CoreError;
use crate::vfs::Vfs;

/// Splits a `---`-fenced YAML frontmatter block off the front of a document.
///
/// Returns `(frontmatter, body)`. Tolerant by design: a UTF-8 BOM, CRLF line
/// endings, and trailing whitespace on the fence lines all parse. A document
/// with no frontmatter, or an unterminated opening fence, degrades to
/// `("", whole_document)` rather than erroring — a malformed skill must still
/// be readable, because reading it is how the user fixes it.
pub fn split_frontmatter(source: &str) -> (&str, &str) {
    frontmatter_lossless::split_lenient(source)
        .map(|ls| (ls.yaml, ls.body))
        .unwrap_or(("", source))
}

/// Reads a markdown file as UTF-8. Bytes that are not UTF-8 are
/// [`CoreError::unreadable`].
pub(crate) fn read_utf8(vfs: &dyn Vfs, path: &Path) -> Result<String, CoreError> {
    let bytes = vfs.read_file(path)?;
    String::from_utf8(bytes).map_err(|e| CoreError::unreadable(path, format!("not UTF-8: {e}")))
}

/// Renders a relative path with forward slashes, so ids are stable across
/// platforms and safe to use as wire identifiers.
pub(crate) fn forward_slashed(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_plain_frontmatter() {
        let (fm, body) = split_frontmatter("---\nname: a\n---\n# Title\n");
        assert_eq!(fm, "name: a");
        assert_eq!(body, "# Title\n");
    }

    #[test]
    fn tolerates_bom_and_crlf() {
        let (fm, body) = split_frontmatter("\u{feff}---\r\nname: a\r\n---\r\nbody\r\n");
        assert_eq!(fm, "name: a");
        assert_eq!(body, "body\r\n");
    }

    #[test]
    fn document_without_frontmatter_is_all_body() {
        let (fm, body) = split_frontmatter("# Just a heading\n");
        assert_eq!(fm, "");
        assert_eq!(body, "# Just a heading\n");
    }

    #[test]
    fn unterminated_fence_degrades_to_body_rather_than_erroring() {
        // A half-written skill must stay readable — reading it is how it gets fixed.
        let source = "---\nname: a\nstill going\n";
        let (fm, body) = split_frontmatter(source);
        assert_eq!(fm, "");
        assert_eq!(body, source);
    }
}
