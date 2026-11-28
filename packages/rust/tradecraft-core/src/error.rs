//! The crate's error type.
//!
//! Deliberately small and host-neutral: each host (the `skillwright` CLI,
//! the `tradecraft-wasm` binding) maps it into its own output, so this crate
//! never learns which host it runs in. The `Display` strings reach users, so
//! the test at the bottom pins them verbatim.

use std::path::{Path, PathBuf};

use crate::refusal::{self, Token};

/// Something went wrong reading a vault or building from it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CoreError {
    /// A path escaped its vault root. Always a bug or an attack; never shown
    /// as a routine condition.
    #[error("path escapes its vault root: {0}")]
    PathEscapesRoot(PathBuf),

    /// Filesystem failure. Carries the path, because a bare
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
    #[error("refused: {token}: {message}")]
    Refused {
        /// The reason, for a host or a test to match on.
        token: Token,
        /// The prose after the token: the path and the fix.
        message: String,
    },

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
            CoreError::PathEscapesRoot(_) => "PathEscapesRoot",
            CoreError::Io { .. } => "Io",
            CoreError::Refused { .. } => "Refused",
            CoreError::Projection(_) => "Projection",
            CoreError::SwapFailed(_) => "SwapFailed",
        }
    }

    /// A refusal's token, or `None` for an error that is not a refusal.
    pub fn token(&self) -> Option<Token> {
        match self {
            CoreError::Refused { token, .. } => Some(*token),
            _ => None,
        }
    }

    /// What a report says after the token: a refusal's message, or the whole
    /// `Display` of any other error.
    pub fn detail(&self) -> String {
        match self {
            CoreError::Refused { message, .. } => message.clone(),
            other => other.to_string(),
        }
    }

    /// The error as a report line gives a reason: `<token>: <detail>`, or the
    /// detail alone for an error with no token. Leaves out the `refused:`
    /// prefix of `Display`; a report line shows it as its status.
    pub fn reason(&self) -> String {
        reason(self.token().map(Token::as_str), &self.detail())
    }

    /// Attach a path to an [`std::io::Error`]. Threading the path through
    /// every `?` is the whole reason this helper exists.
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    /// Refuses the note at `path` with [`refusal::UNREADABLE_NOTE`]:
    /// `refused: unreadable-note: <path>: <message>`.
    pub fn unreadable(path: impl AsRef<Path>, message: impl Into<String>) -> Self {
        Self::refused_at(refusal::UNREADABLE_NOTE, path, message)
    }

    /// A note that parsed holds a value of the wrong kind:
    /// `refused: malformed: <path>: <message>`.
    pub fn wrong_kind(path: impl AsRef<Path>, message: impl Into<String>) -> Self {
        Self::refused_at(refusal::MALFORMED, path, message)
    }

    /// A refusal about the file at `path`: `refused: <token>: <path>: <message>`.
    pub(crate) fn refused_at(
        token: Token,
        path: impl AsRef<Path>,
        message: impl Into<String>,
    ) -> Self {
        Self::refused(
            token,
            format!("{}: {}", path.as_ref().display(), message.into()),
        )
    }

    /// A refusal with its token from [`crate::refusal`]. The message names
    /// the path and the fix.
    pub fn refused(token: Token, message: impl Into<String>) -> Self {
        Self::Refused {
            token,
            message: message.into(),
        }
    }
}

/// `<token>: <detail>`, or `detail` alone with no token.
pub(crate) fn reason(token: Option<&str>, detail: &str) -> String {
    match token {
        Some(token) => format!("{token}: {detail}"),
        None => detail.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_strings_are_stable() {
        // These reach users verbatim. Changing one is a user-visible change.
        assert_eq!(
            CoreError::PathEscapesRoot(PathBuf::from("/etc/passwd")).to_string(),
            "path escapes its vault root: /etc/passwd"
        );
    }

    #[test]
    fn a_refusal_carries_its_token_beside_the_message() {
        let err = CoreError::refused(refusal::LAYOUT, "/vault: keep one shape");
        assert_eq!(err.to_string(), "refused: layout: /vault: keep one shape");
        assert_eq!(err.token(), Some(refusal::LAYOUT));
        assert_eq!(err.detail(), "/vault: keep one shape");
        assert_eq!(err.reason(), "layout: /vault: keep one shape");
    }

    #[test]
    fn a_wrong_kind_carries_the_malformed_token() {
        let err = CoreError::wrong_kind("/vault/a.md", "`model` must be a string, not a list");
        assert_eq!(
            err.to_string(),
            "refused: malformed: /vault/a.md: `model` must be a string, not a list"
        );
        assert_eq!(err.token(), Some(refusal::MALFORMED));
    }

    #[test]
    fn an_unreadable_note_is_a_refusal_naming_the_path() {
        assert_eq!(
            CoreError::unreadable("/vault/a.md", "invalid frontmatter").to_string(),
            "refused: unreadable-note: /vault/a.md: invalid frontmatter"
        );
    }

    #[test]
    fn io_helper_keeps_the_path_in_the_message() {
        let err = CoreError::io(
            "/tmp/SKILL.md",
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
        );
        assert_eq!(err.to_string(), "reading /tmp/SKILL.md: permission denied");
        assert_eq!(err.token(), None);
        assert_eq!(err.detail(), err.to_string());
        assert_eq!(err.reason(), err.to_string());
    }
}
