//! Exit codes.
//!
//! | Code | Means |
//! |---|---|
//! | 0 | The run completed and every outcome was written or unchanged |
//! | 1 | `check` found drift, and only that |
//! | 2 | Refused: a vault error, or anything else the run refused |
//! | 3 | Usage error |
//! | 4 | No vault root at or above the working directory, and no `--vault` |
//!
//! Code 2 also covers a refusal that is not about the vault: `init` over an
//! existing file, a `bump` row it could not write, a failed write, and a
//! declined prompt. Each leaves the work undone, which 0 would hide and 1
//! would mistake for drift.
//!
//! The helpers below each write their message to the caller's error
//! sink — never stdout — and return the matching code, so a command's
//! `return refused(err, ...)` is both the message and the exit in one
//! expression.

use std::io::Write;

/// The run completed and nothing was refused.
pub const EXIT_OK: i32 = 0;
/// `check` found drift.
pub const EXIT_DRIFT: i32 = 1;
/// The vault, or something the run had to do, was refused.
pub const EXIT_REFUSED: i32 = 2;
/// Usage error.
pub const EXIT_USAGE: i32 = 3;
/// No vault root at or above the working directory, and no `--vault`.
pub const EXIT_NO_CONFIG: i32 = 4;

/// A refusal: the message, and [`EXIT_REFUSED`].
pub fn refused(err: &mut dyn Write, message: &str) -> i32 {
    let _ = writeln!(err, "{message}");
    EXIT_REFUSED
}

/// `<what> failed: <reason>`, and [`EXIT_REFUSED`]. `reason` is what
/// [`tradecraft_core::CoreError::reason`] gives, or the CLI's own words.
pub fn failed(err: &mut dyn Write, what: &str, reason: &str) -> i32 {
    refused(err, &format!("{what} failed: {reason}"))
}

/// An unknown command or flag, a missing argument, or a write off a terminal
/// without `--yes`.
pub fn usage_error(err: &mut dyn Write, message: &str) -> i32 {
    let _ = writeln!(err, "usage error: {message}");
    EXIT_USAGE
}

/// No vault root found, and no `--vault` given.
pub fn no_config(err: &mut dyn Write, message: &str) -> i32 {
    let _ = writeln!(err, "{message}");
    EXIT_NO_CONFIG
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(f: impl FnOnce(&mut dyn Write) -> i32) -> (i32, String) {
        let mut buf: Vec<u8> = Vec::new();
        let code = f(&mut buf);
        (code, String::from_utf8(buf).unwrap())
    }

    #[test]
    fn the_codes_are_fixed() {
        assert_eq!(
            [
                EXIT_OK,
                EXIT_DRIFT,
                EXIT_REFUSED,
                EXIT_USAGE,
                EXIT_NO_CONFIG
            ],
            [0, 1, 2, 3, 4]
        );
    }

    #[test]
    fn refused_writes_the_message_and_returns_2() {
        let (code, out) = written(|w| refused(w, "nothing to do"));
        assert_eq!(code, EXIT_REFUSED);
        assert_eq!(out, "nothing to do\n");
    }

    #[test]
    fn failed_names_what_failed_and_returns_2() {
        let (code, out) = written(|w| failed(w, "build", "layout: no root"));
        assert_eq!(code, EXIT_REFUSED);
        assert_eq!(out, "build failed: layout: no root\n");
    }

    #[test]
    fn usage_error_prefixes_and_returns_3() {
        let (code, out) = written(|w| usage_error(w, "unknown flag --bogus"));
        assert_eq!(code, EXIT_USAGE);
        assert_eq!(out, "usage error: unknown flag --bogus\n");
    }

    #[test]
    fn no_config_writes_the_message_and_returns_4() {
        let (code, out) = written(|w| no_config(w, "no vault"));
        assert_eq!(code, EXIT_NO_CONFIG);
        assert_eq!(out, "no vault\n");
    }
}
