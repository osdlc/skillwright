//! The crate's error type.
//!
//! Deliberately small and host-neutral: each host (the `skillwright` CLI,
//! the `tradecraft-wasm` binding) maps it into its own output, so this crate
//! never learns which host it runs in. The `Display` strings reach users, so
//! the test at the bottom pins them verbatim.

use std::path::{Path, PathBuf};

/// Something went wrong locating or reading a skill corpus.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CoreError {
    /// A content root does not exist. Returned before anything is read.
    #[error("corpus root not found: {0}")]
    RootNotFound(PathBuf),

    /// A path escaped its corpus root. Always a bug or an attack; never shown
    /// as a routine condition.
    #[error("path escapes its corpus root: {0}")]
    PathEscapesRoot(PathBuf),

    /// Filesystem failure reading a corpus. Carries the path, because a bare
    /// `io::Error` ("permission denied") is unactionable.
    #[error("reading {path}: {source}")]
    Io {
        /// The path that could not be read.
        path: PathBuf,
        #[source]
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// A guard said no: a build refusal such as an unreadable root file or an
    /// output path that overlaps the vault. Always the guard working, never a
    /// bug.
    #[error("refused: {0}")]
    Refused(String),

    /// Rendering a skill for a runtime target failed.
    #[error("projection: {0}")]
    Projection(String),

    /// The marketplace commit failed part-way through its root swaps. The
    /// message says what failed and what the rollback restored — `dist/<m>/`
    /// is back to its pre-build state unless the message says otherwise. Its
    /// own variant because no other fits: it is not a refusal and not a plain
    /// read failure, and a host has to be able to
    /// tell "nothing happened" from "something happened and was undone".
    #[error("swap failed: {0}")]
    SwapFailed(String),
}

/// A failed [`crate::vfs::Vfs`] call is a read failure against its path:
/// `reading <path>: <cause>`, as it was when the trait returned this type.
impl From<crate::vfs::VfsError> for CoreError {
    fn from(err: crate::vfs::VfsError) -> Self {
        let path = err.path().to_path_buf();
        Self::io(path, err.into_io_error())
    }
}

impl CoreError {
    /// The variant's name, such as `"Refused"`. The wasm binding sets it as
    /// the thrown `Error`'s `.name`. The match is exhaustive on purpose: a new
    /// variant fails to compile here until it is given a name.
    pub fn variant_name(&self) -> &'static str {
        match self {
            CoreError::RootNotFound(_) => "RootNotFound",
            CoreError::PathEscapesRoot(_) => "PathEscapesRoot",
            CoreError::Io { .. } => "Io",
            CoreError::Refused(_) => "Refused",
            CoreError::Projection(_) => "Projection",
            CoreError::SwapFailed(_) => "SwapFailed",
        }
    }

    /// Attach a path to an [`std::io::Error`]. Threading the path through
    /// every `?` is the whole reason this helper exists.
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    /// A *content* problem in a file we read: frontmatter that will not parse,
    /// or bytes that are not UTF-8. A value of the wrong kind or shape in a
    /// note that parsed is [`Self::wrong_kind`].
    ///
    /// [`CoreError::Io`] rather than a variant of its own, because it is the
    /// one variant that already carries a path and its `Display` — `reading
    /// <path>: <what>` — is exactly the sentence a user needs. A caller
    /// holding a path it believed was a skill has to be told *which* one was
    /// not.
    pub fn malformed(path: impl AsRef<Path>, message: impl Into<String>) -> Self {
        Self::io(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, message.into()),
        )
    }

    /// A note that parsed holds a value of the wrong kind:
    /// `refused: malformed: <path>: <message>`. Not [`Self::malformed`],
    /// whose `reading <path>` says the file could not be read.
    pub fn wrong_kind(path: impl AsRef<Path>, message: impl Into<String>) -> Self {
        Self::refused(
            "malformed",
            format!("{}: {}", path.as_ref().display(), message.into()),
        )
    }

    /// A refusal message split into the token it leads with and the prose
    /// after it, or `None` when it leads with nothing shaped like a token:
    /// some refusals carry prose alone.
    pub(crate) fn split_refusal(message: &str) -> Option<(&str, &str)> {
        message.split_once(": ").filter(|(token, _)| {
            !token.is_empty()
                && token
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
    }

    /// A refusal prefixed with a reason token — `unknown-kind`, `mixed-frontmatter-shape`,
    /// and the rest.
    ///
    /// The token leads the message so a report, a test, or a later UI can find
    /// it without a second field on the variant; the prose after it names the
    /// path and the fix. Every refusal in this crate goes through here, so a
    /// report never has to render one as bare prose.
    pub fn refused(token: &str, message: impl Into<String>) -> Self {
        Self::Refused(format!("{token}: {}", message.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_strings_are_stable() {
        // These reach users verbatim. Changing one is a user-visible change.
        assert_eq!(
            CoreError::RootNotFound(PathBuf::from("/nope")).to_string(),
            "corpus root not found: /nope"
        );
        assert_eq!(
            CoreError::PathEscapesRoot(PathBuf::from("/etc/passwd")).to_string(),
            "path escapes its corpus root: /etc/passwd"
        );
    }

    #[test]
    fn a_refusal_leads_with_its_reason_token() {
        // The token is how a report, a test, or a UI identifies the refusal;
        // everything after it is prose for a human.
        assert_eq!(
            CoreError::refused("mixed-frontmatter-shape", "/vault/a.md: keep one shape")
                .to_string(),
            "refused: mixed-frontmatter-shape: /vault/a.md: keep one shape"
        );
    }

    #[test]
    fn a_wrong_kind_carries_the_malformed_token() {
        let err = CoreError::wrong_kind("/vault/a.md", "`model` must be a string, not a list");
        assert_eq!(
            err.to_string(),
            "refused: malformed: /vault/a.md: `model` must be a string, not a list"
        );
        let message = err.to_string();
        let (token, _) = CoreError::split_refusal(message.strip_prefix("refused: ").unwrap())
            .expect("a token leads the refusal");
        assert_eq!(token, "malformed");
    }

    #[test]
    fn malformed_reads_as_a_read_failure_against_the_path() {
        assert_eq!(
            CoreError::malformed("/vault/a.md", "`sw-kind` must be a string, not a list")
                .to_string(),
            "reading /vault/a.md: `sw-kind` must be a string, not a list"
        );
    }

    #[test]
    fn io_helper_keeps_the_path_in_the_message() {
        let err = CoreError::io(
            "/tmp/SKILL.md",
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
        );
        assert_eq!(err.to_string(), "reading /tmp/SKILL.md: permission denied");
    }
}
