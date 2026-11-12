//! The commit: stage every changed harness, then swap each owned path into
//! place with a backup, and roll back on failure.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::CoreError;
use crate::vfs::{PreviewWrite, Vfs};

/// Where a build stages and backs up, under the root's own base — created
/// only when a swap is actually needed, removed when the build ends.
///
/// Under the base rather than beside it, because a swap is a rename and a
/// rename is only atomic within one filesystem: the one directory guaranteed
/// to share a filesystem with the thing being replaced is the thing's own
/// parent. It stays invisible to the
/// diff because `scan_destination` reads only the entries a root owns, and
/// this is never one of them.
const STAGE_DIR: &str = ".skillwright-stage";

/// What a build replaces in one output folder: each of `owned`, a path
/// relative to `base`, gives way to what `files` holds under it.
pub(super) struct Swap {
    pub(super) base: PathBuf,
    pub(super) owned: Vec<String>,
    pub(super) files: BTreeMap<String, PreviewWrite>,
}

/// Stages every swap, then swaps them all into place. A swap that owns
/// nothing is skipped. A failed swap rolls back every path already swapped,
/// in every swap's folder.
pub(super) fn commit(vfs: &dyn Vfs, swaps: &[Swap]) -> Result<(), CoreError> {
    let swaps: Vec<&Swap> = swaps.iter().filter(|swap| !swap.owned.is_empty()).collect();
    let stages: Vec<PathBuf> = swaps.iter().map(|swap| swap.base.join(STAGE_DIR)).collect();
    for (swap, stage) in swaps.iter().zip(&stages) {
        if let Err(error) = stage_swap(vfs, swap, stage) {
            for stage in &stages {
                let _ = vfs.remove(stage);
            }
            return Err(error);
        }
    }

    // Every completed swap, by index into `swaps`: its destination and its
    // backup.
    let mut swapped: Vec<(usize, PathBuf, PathBuf)> = Vec::new();
    for (index, (swap, stage)) in swaps.iter().zip(&stages).enumerate() {
        for entry in &swap.owned {
            let dest = swap.base.join(entry);
            let staged = stage.join("new").join(entry);
            let saved = stage.join("backup").join(entry);
            let staged = vfs.exists(&staged).then_some(staged);
            // Nothing here and nothing to put here: a root that renders nothing.
            if staged.is_none() && !vfs.exists(&dest) {
                continue;
            }
            for path in [&saved, &dest] {
                if let Some(parent) = path.parent() {
                    vfs.create_dir_all(parent)?;
                }
            }
            if let Err(error) = swap_one(vfs, &dest, &saved, staged.as_deref()) {
                // Everything the rollback could not put back, the failing path
                // included: `swap_one` restores its own backup best-effort, and
                // this is the check on whether it managed.
                let mut unrestored: Vec<PathBuf> = Vec::new();
                if !vfs.exists(&dest) && vfs.exists(&saved) {
                    unrestored.push(dest.clone());
                }
                if let Err(failed) = roll_back(vfs, &swapped) {
                    unrestored.push(failed);
                }
                let message = format!(
                    "swapping `{entry}` into {}: {error}; everything already swapped was rolled \
                     back and the destination is as it was",
                    swap.base.display()
                );
                return Err(swap_failed(vfs, message, &unrestored, &stages));
            }
            swapped.push((index, dest, saved));
        }
    }
    for (index, dest, _) in &swapped {
        remove_emptied_folders(vfs, &swaps[*index].base, dest)?;
    }
    for stage in &stages {
        vfs.remove(stage)?;
    }
    Ok(())
}

/// The error for a failed swap, after the rollback. With every path restored
/// the stages go. Otherwise `message` also names what was not restored, and
/// its backup is kept.
fn swap_failed(
    vfs: &dyn Vfs,
    mut message: String,
    unrestored: &[PathBuf],
    stages: &[PathBuf],
) -> CoreError {
    if unrestored.is_empty() {
        for stage in stages {
            let _ = vfs.remove(stage);
        }
        return CoreError::SwapFailed(message);
    }
    let names: Vec<String> = unrestored.iter().map(|p| p.display().to_string()).collect();
    let backups: Vec<String> = stages
        .iter()
        .map(|stage| stage.join("backup"))
        .filter(|backup| {
            unrestored.iter().any(|p| {
                backup
                    .parent()
                    .and_then(Path::parent)
                    .is_some_and(|out| p.starts_with(out))
            })
        })
        .map(|backup| backup.display().to_string())
        .collect();
    message.push_str(&format!(
        " — except {}, whose rollback failed; the backup survives under {}",
        names.join(", "),
        backups.join(", ")
    ));
    CoreError::SwapFailed(message)
}

/// Writes `swap`'s files under `<stage>/new/`.
fn stage_swap(vfs: &dyn Vfs, swap: &Swap, stage: &Path) -> Result<(), CoreError> {
    // A stage left by a crashed run is dead weight; a fresh build owns the
    // path.
    if vfs.exists(stage) {
        vfs.remove(stage)?;
    }
    vfs.create_dir_all(&stage.join("backup"))?;
    vfs.create_dir_all(&swap.base)?;
    for (relative, write) in &swap.files {
        let path = stage.join("new").join(Path::new(relative));
        if let Some(parent) = path.parent() {
            vfs.create_dir_all(parent)?;
        }
        vfs.write_file(&path, &write.bytes)?;
        if write.executable {
            vfs.set_executable(&path, true)?;
        }
    }
    Ok(())
}

/// Removes the folders above a swapped-out `path` that it left empty, up to
/// `out`. A swap that kept the path leaves them all in place.
fn remove_emptied_folders(vfs: &dyn Vfs, out: &Path, path: &Path) -> Result<(), CoreError> {
    if vfs.exists(path) {
        return Ok(());
    }
    for folder in path.ancestors().skip(1) {
        if folder == out || !vfs.read_dir(folder)?.is_empty() {
            break;
        }
        vfs.remove(folder)?;
    }
    Ok(())
}

/// One path's swap: the live entry aside, the staged entry in. On failure its
/// own backup is restored before the error surfaces, so the caller only ever
/// rolls back *completed* swaps.
fn swap_one(
    vfs: &dyn Vfs,
    dest: &Path,
    saved: &Path,
    staged: Option<&Path>,
) -> Result<(), CoreError> {
    if vfs.exists(dest) {
        vfs.rename(dest, saved)?;
    }
    // Nothing staged is not nothing happening: the entry was owned, it is
    // backed up, and leaving it unrestored is how what the vault no longer
    // produces goes away.
    let Some(staged) = staged else {
        return Ok(());
    };
    if let Err(error) = vfs.rename(staged, dest) {
        if vfs.exists(saved) && !vfs.exists(dest) {
            let _ = vfs.rename(saved, dest);
        }
        return Err(error.into());
    }
    Ok(())
}

/// Unwinds completed swaps, newest first. `Err` names the first destination
/// whose restore failed; its backup is deliberately left in place.
fn roll_back(vfs: &dyn Vfs, swapped: &[(usize, PathBuf, PathBuf)]) -> Result<(), PathBuf> {
    for (_, dest, saved) in swapped.iter().rev() {
        if vfs.exists(dest) && vfs.remove(dest).is_err() {
            return Err(dest.clone());
        }
        if vfs.exists(saved) && vfs.rename(saved, dest).is_err() {
            return Err(dest.clone());
        }
    }
    Ok(())
}
