//! What a command is given instead of reaching for the process: a working
//! directory and two streams.
//!
//! Every verb takes one, so every verb is testable by calling it. The tests
//! in this crate drive [`crate::cli::run_with`] in-process rather than
//! spawning a binary.

use std::io::Write;
use std::path::{Path, PathBuf};

/// What a command is given instead of the process.
pub struct CommandContext<'a> {
    /// The working directory relative paths resolve against.
    pub cwd: PathBuf,
    /// The report. The CLI spec reserves it for that alone.
    pub out: &'a mut dyn Write,
    /// Progress, tallies, prompts, and every message an exit code carries.
    pub err: &'a mut dyn Write,
}

impl<'a> CommandContext<'a> {
    /// A context over `cwd` and the two streams.
    pub fn new(cwd: impl AsRef<Path>, out: &'a mut dyn Write, err: &'a mut dyn Write) -> Self {
        Self {
            cwd: cwd.as_ref().to_path_buf(),
            out,
            err,
        }
    }
}
