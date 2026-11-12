//! What every verb but `init` asks before it does anything: where the vault
//! root is, what its root file says, and which marketplaces the run covers.
//!
//! The root is the folder holding both `skillwright.md` and `marketplaces/`
//! (spec section 1). `--vault` names it; otherwise the walk up from the
//! working directory finds it. The core checks the layout and reads the
//! notes ([`tradecraft_core::load_vault`]); this module turns its errors into
//! exit codes.

use std::io::Write;
use std::path::Path;

use tradecraft_core::{
    BuildScope, LoadedVault, NativeVfs, check_vault_root, find_vault_root, load_vault,
};

use crate::config::{print_warnings, resolve_against_cwd};
use crate::exit::{no_config, refused};

/// The globals every verb but `init` declares, as far as this module cares.
#[derive(Debug, Clone, Default)]
pub struct CommonFlags {
    /// `--vault`.
    pub vault: Option<String>,
}

/// Find the vault root and load it, writing any failure
/// to `err` and handing back the exit code it earned. `marketplace` limits
/// the run's marketplaces to that one.
///
/// No root at or above `cwd` is exit 4, nothing supplied. A `--vault` that is
/// not a root, a layout error, a bad root file and an unknown marketplace are
/// run-level blockers, exit 2.
pub fn resolve_run(
    flags: &CommonFlags,
    marketplace: Option<&str>,
    cwd: &Path,
    err: &mut dyn Write,
) -> Result<LoadedVault, i32> {
    let vault = match &flags.vault {
        Some(flag) => {
            let dir = resolve_against_cwd(flag, cwd);
            if !dir.is_dir() {
                return Err(refused(err, &format!("vault not found: {}", dir.display())));
            }
            if let Err(problem) = check_vault_root(&NativeVfs, &dir) {
                return Err(refused(err, &problem.to_string()));
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
) -> Result<LoadedVault, i32> {
    let scope = BuildScope::from(marketplace.map(str::to_owned));
    let loaded = load_vault(&NativeVfs, vault, &scope)
        .map_err(|problem| refused(err, &problem.to_string()))?;
    print_warnings(&loaded.settings, &loaded.root_file, err);
    Ok(loaded)
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
        let loaded = resolve_run(&CommonFlags::default(), None, &cwd, &mut err).unwrap();
        assert_eq!(loaded.root, dir.path());
        let names: Vec<&str> = loaded
            .marketplaces
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(names, ["acme"]);
    }
}
