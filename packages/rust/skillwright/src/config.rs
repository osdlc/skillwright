//! Configuration: the root file, `skillwright.md`, and the paths flags name.
//!
//! The root file sits at the vault root beside `marketplaces/`. It holds the
//! shared defaults and, under `build:`, how skillwright runs. The core finds
//! and parses it ([`tradecraft_core::load_vault`]); this module resolves flag
//! paths and prints the file's warnings. It never writes the file.

use std::path::{Path, PathBuf};

pub use tradecraft_core::paths::{normalize_join, relative_path};
pub use tradecraft_core::{BuildSettings, ROOT_FILE, RootFile, resolve_path};

/// Resolve a flag's path as the shell would: absolute as given, `~` or a
/// `~/` prefix against the home directory, anything else relative to
/// `base`. The rule `--vault` and `--changed-since` follow, with `base`
/// the working directory.
pub fn resolve_against_cwd(path: &str, base: &Path) -> PathBuf {
    resolve_path(path, base, &home_dir())
}

/// The user's home directory, read directly from `$HOME`. Falls back to `/`
/// rather than panicking when unset.
pub(crate) fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Print each warning the root file at `path` carries to `err`.
pub fn print_warnings(root: &RootFile, path: &Path, err: &mut dyn std::io::Write) {
    for warning in &root.warnings {
        let _ = writeln!(err, "warning: {}: {warning}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only `~` and `~/` name the home directory. `~alice/notes` and
    /// `~notes` are ordinary relative paths.
    #[test]
    fn a_tilde_that_is_not_followed_by_a_separator_is_a_relative_path() {
        let home = home_dir();
        assert_eq!(
            resolve_against_cwd("~alice/notes", Path::new("/work")),
            PathBuf::from("/work/~alice/notes")
        );
        assert_eq!(
            resolve_against_cwd("~notes", Path::new("/proj")),
            PathBuf::from("/proj/~notes")
        );
        assert_eq!(resolve_against_cwd("~", Path::new("/work")), home);
        assert_eq!(
            resolve_against_cwd("~/notes", Path::new("/work")),
            home.join("notes")
        );
    }
}
