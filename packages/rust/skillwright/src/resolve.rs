//! What every verb but `init` asks before it does anything: where the vault
//! root is, what its root file says, and which marketplaces the run covers.
//!
//! The root is the folder holding both `skillwright.md` and `marketplaces/`
//! (spec section 1). `--vault` names it; otherwise the walk up from the
//! working directory finds it. The core checks the layout and reads the
//! notes ([`tradecraft_core::load_vault`]); this module turns its errors into
//! exit codes.

use std::io::Write;
use std::path::{Path, PathBuf};

use tradecraft_core::{
    LoadedVault, NativeVfs, VaultMarketplace, check_vault_root, find_vault_root, load_vault,
};

use crate::config::{home_dir, print_warnings, resolve_against_cwd};
use crate::exit::{blocked, no_config};

/// The globals every verb but `init` declares, as far as this module cares.
#[derive(Debug, Clone, Default)]
pub struct CommonFlags {
    /// `--vault`.
    pub vault: Option<String>,
}

/// What a run resolved to.
#[derive(Debug, Clone)]
pub struct ResolvedRun {
    /// The vault as the core loaded it: the root file and the marketplaces
    /// this run covers.
    pub loaded: LoadedVault,
    /// The vault root.
    pub vault: PathBuf,
}

impl ResolvedRun {
    /// The marketplaces this run covers: every one, or the one
    /// `--marketplace` names.
    pub fn marketplaces(&self) -> &[VaultMarketplace] {
        &self.loaded.marketplaces
    }
}

/// Find the vault root and load it, writing any failure
/// to `err` and handing back the exit code it earned. `marketplace` limits
/// the run to that marketplace.
///
/// No root at or above `cwd` is exit 4, nothing supplied. A `--vault` that is
/// not a root, a layout error, a bad root file and an unknown marketplace are
/// run-level blockers, exit 2.
pub fn resolve_run(
    flags: &CommonFlags,
    marketplace: Option<&str>,
    cwd: &Path,
    err: &mut dyn Write,
) -> Result<ResolvedRun, i32> {
    let vault = match &flags.vault {
        Some(flag) => {
            let dir = resolve_against_cwd(flag, cwd);
            if !dir.is_dir() {
                return Err(blocked(err, &format!("vault not found: {}", dir.display())));
            }
            if let Err(refused) = check_vault_root(&NativeVfs, &dir) {
                return Err(blocked(err, &refused.to_string()));
            }
            dir
        }
        None => match find_vault_root(&NativeVfs, cwd) {
            Ok(dir) => dir,
            Err(refused) => return Err(no_config(err, &refused.to_string())),
        },
    };
    resolve_at(&vault, marketplace, err)
}

/// [`resolve_run`] for a root already found.
pub fn resolve_at(
    vault: &Path,
    marketplace: Option<&str>,
    err: &mut dyn Write,
) -> Result<ResolvedRun, i32> {
    let loaded = load_vault(&NativeVfs, vault, marketplace, &home_dir())
        .map_err(|refused| blocked(err, &refused.to_string()))?;
    print_warnings(&loaded.settings, &loaded.root_file, err);
    Ok(ResolvedRun {
        loaded,
        vault: vault.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit::EXIT_NO_CONFIG;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn vault(root: &Path) {
        write(root, "skillwright.md", "---\n---\n");
        write(root, "marketplaces/acme/acme.md", "---\n---\n");
        write(
            root,
            "marketplaces/acme/plugins/tools/tools.md",
            "---\nversion: 1.0.0\n---\n",
        );
    }

    #[test]
    fn a_folder_with_no_root_at_or_above_it_is_exit_4() {
        let dir = tempfile::tempdir().unwrap();
        let mut err: Vec<u8> = Vec::new();
        let code = resolve_run(&CommonFlags::default(), None, dir.path(), &mut err).unwrap_err();
        assert_eq!(code, EXIT_NO_CONFIG);
        let message = String::from_utf8(err).unwrap();
        assert!(message.contains("refused: layout"), "{message}");
        assert!(message.contains("marketplaces/"), "{message}");
    }

    #[test]
    fn the_walk_up_stops_at_the_root_and_lists_its_marketplaces() {
        let dir = tempfile::tempdir().unwrap();
        vault(dir.path());
        let cwd = dir.path().join("marketplaces/acme/plugins/tools");
        let mut err: Vec<u8> = Vec::new();
        let run = resolve_run(&CommonFlags::default(), None, &cwd, &mut err).unwrap();
        assert_eq!(run.vault, dir.path());
        let names: Vec<&str> = run.marketplaces().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["acme"]);
    }
}
