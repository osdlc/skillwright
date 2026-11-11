//! Where a build's output sits against the vault it reads.
//!
//! A build reads only `marketplaces/`, and writes to `dist/<m>/`. These checks
//! stay while a harness block can still move its `packs` folder.

use std::path::{Path, PathBuf};

use crate::error::CoreError;
use crate::export::MarketplaceConfig;
use crate::harness::manifest::{check_packs, check_packs_against_sources};
use crate::settings::ROOT_FILE;
use crate::vfs::{Vfs, normalize};

/// `path` with its links resolved, as far as it exists.
fn resolved(vfs: &dyn Vfs, path: &Path) -> PathBuf {
    let path = normalize(path);
    vfs.canonicalize(&path).unwrap_or(path)
}

/// Whether the output and the vault share a folder: one holds the other.
pub(crate) fn output_overlaps_vault(vfs: &dyn Vfs, vault_root: &Path, out_root: &Path) -> bool {
    let (vault, out) = (resolved(vfs, vault_root), resolved(vfs, out_root));
    vault.starts_with(&out) || out.starts_with(&vault)
}

/// Every check on `config`'s `packs` that needs no destination on disk:
/// [`check_packs`], and, when the output overlaps the vault, that a moved
/// folder is clear of what the vault reads: `marketplaces/` and the root file.
///
/// # Errors
///
/// As [`check_packs`] and [`check_packs_against_sources`].
pub(crate) fn check_pack_location(
    vfs: &dyn Vfs,
    vault_root: &Path,
    out_root: &Path,
    config: &MarketplaceConfig,
) -> Result<(), CoreError> {
    check_packs(config)?;
    if !output_overlaps_vault(vfs, vault_root, out_root) {
        return Ok(());
    }
    let sources = [crate::layout::MARKETPLACES_DIR, ROOT_FILE]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (vault, out) = (resolved(vfs, vault_root), resolved(vfs, out_root));
    check_packs_against_sources(config, &sources, |packs| {
        normalize(&out.join(packs))
            .strip_prefix(&vault)
            .ok()
            .map(crate::skill::forward_slashed)
    })
}
