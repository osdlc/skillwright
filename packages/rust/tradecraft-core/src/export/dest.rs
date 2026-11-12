//! The output folder read back and compared with the render: one scan of
//! what each harness owns there, and both the swap's and the report's
//! answer from it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::report::{ExportUnit, StaleKind, UnitStatus};
use crate::error::CoreError;
use crate::harness::manifest::{is_os_file, overrides_for};
use crate::harness::translator::GENERATED_MARKER;
use crate::harness::{Harness, Translator};
use crate::model::Marketplace;
use crate::model::config::MarketplaceConfig;
use crate::skill::forward_slashed;
use crate::status::{VersionBumpSuggestion, version_bump_if_unmoved};
use crate::vfs::{PreviewWrite, Vfs};

/// One root's comparison: equality across what it owns (the swap's grain) and
/// the per-unit stories a report tells about it.
pub(super) struct RootDiff {
    pub(super) equal: bool,
    pub(super) units: Vec<ExportUnit>,
    /// What this root owns is already there, and none of it is this build's
    /// — so it was put there by someone else. See [`DestTree::built_here`].
    pub(super) foreign: bool,
}

/// What the destination root actually holds.
#[derive(Default)]
struct DestState {
    /// Root-relative file paths → is-symlink. A symlink never matches: the
    /// render emits bytes, and a link that reads the same resolves elsewhere.
    files: BTreeMap<String, bool>,
    /// Directories holding nothing — invisible to a file diff, removed by the
    /// swap, so the report has to carry them.
    empty_dirs: Vec<String>,
}

/// Reads `base`, but only the `owned` paths, never the whole directory: what
/// the swap would replace is exactly what the report may call stale, and
/// `dist/<m>/` may hold files of its own beside the packs.
fn scan_destination(vfs: &dyn Vfs, base: &Path, owned: &[&str]) -> Result<DestState, CoreError> {
    let mut state = DestState::default();
    for &owned in owned {
        // A link or a file above an owned path is recorded and goes no
        // further: a swap through it would land somewhere else, or fail.
        if let Some((blocker, is_link)) = blocked_above(vfs, base, owned) {
            state.files.insert(blocker, is_link);
            continue;
        }
        let path = base.join(owned);
        // A symlink AT an owned path is one unmatched path, never descended
        // into — the same rule `walk_destination` applies further down.
        if vfs.is_symlink(&path) {
            state.files.insert(owned.to_owned(), true);
        } else if !vfs.exists(&path) {
            continue;
        } else if vfs.read_dir(&path).is_err() {
            // A file: a manifest, or something where the render wants a folder.
            state.files.insert(owned.to_owned(), false);
        } else {
            walk_destination(vfs, base, &path, &mut state)?;
        }
    }
    Ok(state)
}

/// The folders directly in `dir`, by name, links left out. None when `dir`
/// is missing.
fn folders_in(vfs: &dyn Vfs, dir: &Path) -> Result<Vec<String>, CoreError> {
    if !vfs.exists(dir) {
        return Ok(Vec::new());
    }
    let mut entries = vfs.read_dir(dir)?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries
        .into_iter()
        .filter(|entry| entry.is_dir && !vfs.is_symlink(&dir.join(&entry.name)))
        .map(|entry| entry.name)
        .collect())
}

/// The first folder on the way to `owned` that is a link or not a folder, with
/// whether it is a link.
fn blocked_above(vfs: &dyn Vfs, base: &Path, owned: &str) -> Option<(String, bool)> {
    let parent = Path::new(owned).parent()?;
    let mut ancestors: Vec<&Path> = parent
        .ancestors()
        .filter(|path| !path.as_os_str().is_empty())
        .collect();
    ancestors.reverse();
    ancestors.into_iter().find_map(|relative| {
        let path = base.join(relative);
        let is_link = vfs.is_symlink(&path);
        (is_link || (vfs.exists(&path) && vfs.read_dir(&path).is_err()))
            .then(|| (forward_slashed(relative), is_link))
    })
}

fn walk_destination(
    vfs: &dyn Vfs,
    base: &Path,
    dir: &Path,
    state: &mut DestState,
) -> Result<(), CoreError> {
    let mut entries = vfs.read_dir(dir)?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    if entries.is_empty() && dir != base {
        let relative = dir.strip_prefix(base).unwrap_or(dir);
        state.empty_dirs.push(forward_slashed(relative));
        return Ok(());
    }
    for entry in entries {
        let path = dir.join(&entry.name);
        let relative = path.strip_prefix(base).unwrap_or(&path);
        if vfs.is_symlink(&path) {
            // The link itself, wherever it points — a symlinked directory is
            // never descended into, so it records as one unmatched entry.
            state.files.insert(forward_slashed(relative), true);
        } else if entry.is_dir {
            walk_destination(vfs, base, &path, state)?;
        } else {
            state.files.insert(forward_slashed(relative), false);
        }
    }
    Ok(())
}

/// One root's destination, scanned once, with the byte-and-bit comparison
/// every caller shares.
///
/// The comparison lives here rather than in a closure per caller because two
/// callers ask it: the swap, which wants one boolean about all a root owns,
/// and the report, which wants a story per plugin. Read off one scan and one
/// `matches`, they cannot disagree.
struct DestTree<'v> {
    vfs: &'v dyn Vfs,
    base: PathBuf,
    state: DestState,
}

impl<'v> DestTree<'v> {
    fn scan(vfs: &'v dyn Vfs, base: PathBuf, owned: &[&str]) -> Result<Self, CoreError> {
        let state = scan_destination(vfs, &base, owned)?;
        Ok(Self { vfs, base, state })
    }

    /// Does the destination hold exactly this rendered file — a real file
    /// rather than a symlink that happens to read the same, the same bytes,
    /// and the same execute bit?
    fn matches(&self, relative: &str, write: &PreviewWrite) -> bool {
        if self.state.files.get(relative) != Some(&false) {
            return false;
        }
        let path = self.base.join(Path::new(relative));
        self.vfs
            .read_file(&path)
            .is_ok_and(|bytes| bytes == write.bytes)
            && self.vfs.is_executable(&path) == write.executable
    }

    /// The render and the destination bucketed against each other, once.
    fn compare(&self, rendered: &BTreeMap<String, PreviewWrite>) -> FileSets {
        let mut sets = FileSets::default();
        for (relative, write) in rendered {
            let bucket = if self.matches(relative, write) {
                &mut sets.same
            } else {
                &mut sets.differs
            };
            bucket.insert(relative.clone());
        }
        sets.extra = self
            .state
            .files
            .keys()
            .filter(|relative| !rendered.contains_key(*relative) && !is_os_file(relative))
            .cloned()
            .collect();
        sets
    }

    /// Did a previous build of this vault write what this root owns here?
    ///
    /// [`GENERATED_MARKER`] is the one thing here that only this crate
    /// writes. Every emitted skill body opens with it, so packs this vault
    /// built carry it and packs someone wrote by hand do not.
    ///
    /// A commands-only plugin leaves no marker in the harnesses that carry no
    /// commands, so the second clause also accepts a destination holding
    /// nothing but paths this render produces. It also answers for nothing
    /// being here at all.
    fn built_here(&self, rendered: &BTreeMap<String, PreviewWrite>) -> bool {
        let mut files = self.state.files.keys().filter(|file| !is_os_file(file));
        files.clone().any(|relative| self.carries_marker(relative))
            || files.all(|relative| rendered.contains_key(relative))
    }

    /// Does this installed file carry the provenance marker? Read lazily and short-circuited by
    /// the `any` above, so an ordinary rebuild stops at the first skill it
    /// finds.
    fn carries_marker(&self, relative: &str) -> bool {
        self.read(relative)
            .is_some_and(|bytes| contains(&bytes, GENERATED_MARKER))
    }

    /// One installed file's bytes, for the report alone. The comparison
    /// never needs them — `matches` has already answered.
    fn read(&self, relative: &str) -> Option<Vec<u8>> {
        self.vfs
            .read_file(&self.base.join(Path::new(relative)))
            .ok()
    }
}

/// Every path either tree holds, bucketed once: `same` and `differs`
/// partition the render, and `extra` is what the destination holds beyond it.
#[derive(Default)]
struct FileSets {
    same: BTreeSet<String>,
    differs: BTreeSet<String>,
    extra: BTreeSet<String>,
}

/// The swap's own question: is what this root owns exactly the render?
///
/// Every rendered file matched, nothing else present, no empty directory.
/// Given every rendered path matched, the destination holds all of them, so
/// "the destination holds no more files than the render" is exactly `extra`
/// being empty.
fn root_is_equal(sets: &FileSets, dest: &DestTree<'_>) -> bool {
    sets.differs.is_empty() && sets.extra.is_empty() && dest.state.empty_dirs.is_empty()
}

/// One carried plugin's share of the comparison.
#[derive(Default)]
struct PluginSlice<'a> {
    differs: Vec<&'a str>,
    extra: Vec<&'a str>,
    /// An empty directory inside this plugin's tree — invisible to a file
    /// comparison, removed by the swap, so it drifts the plugin holding it.
    dirty: bool,
}

/// Which plugins this root carries, and how many skills each contributes.
///
/// Admission folds all three kinds through the translator's own `admits` —
/// the same call, so the diff cannot admit a plugin the render left out. The
/// count stays skills-only because that is what [`ExportUnit::Plugin`]'s
/// `skills` reports.
pub(super) fn admitted_plugins(
    translator: Translator,
    market: &Marketplace,
) -> BTreeMap<&str, usize> {
    market
        .plugins
        .iter()
        .filter_map(|(name, plugin)| {
            let carried = plugin.carried_by(|c| translator.admits(c));
            let skills = plugin
                .skills
                .iter()
                .filter(|c| translator.admits(c))
                .count();
            carried.then_some((name.as_str(), skills))
        })
        .collect()
}

/// The report's side of the comparison: one unit per carried plugin, then
/// this root's manifest, then whatever the destination holds that no unit
/// already speaks for.
///
/// `rendered` is still needed beside `sets`, for one rule: an empty
/// directory is suppressed when the render owns files beneath it, and that
/// is a prefix query rather than a set membership.
fn describe_root(
    dest: &DestTree<'_>,
    spec: Harness,
    plugin_dirs: &[String],
    admitted: &BTreeMap<&str, usize>,
    rendered: &BTreeMap<String, PreviewWrite>,
    sets: &FileSets,
    config: &MarketplaceConfig,
) -> Vec<ExportUnit> {
    let packs = spec.format().policy.packs;
    // A destination plugin directory this root no longer carries. It is one
    // unit, and its files are never reported on their own: the swap removes
    // the directory whole, and a line per file inside it would say the same
    // thing many times.
    let stale_plugins: BTreeSet<&str> = plugin_dirs
        .iter()
        .map(String::as_str)
        .filter(|name| !admitted.contains_key(name))
        .collect();

    let (slices, mut stale_paths) =
        slices_of(dest, sets, packs, admitted, &stale_plugins, rendered);

    let mut units = Vec::new();
    for (plugin, skills) in admitted {
        let slice = slices.get(plugin);
        let ok = slice.is_none_or(|s| s.differs.is_empty() && s.extra.is_empty() && !s.dirty);
        units.push(ExportUnit::Plugin {
            root: spec,
            name: (*plugin).to_owned(),
            skills: *skills,
            status: if ok {
                UnitStatus::Same
            } else {
                UnitStatus::Differs
            },
            version_bump_suggestion: plugin_version_bump(
                dest, spec, plugin, rendered, slice, config,
            ),
        });
    }

    if let Some(manifest) = spec.format().marketplace_manifest {
        units.push(ExportUnit::Manifest {
            root: spec,
            status: if sets.same.contains(manifest) {
                UnitStatus::Same
            } else {
                UnitStatus::Differs
            },
        });
    }

    for name in &stale_plugins {
        units.push(ExportUnit::Stale {
            root: spec,
            path: format!("{packs}/{name}"),
            kind: StaleKind::Plugin,
        });
    }
    stale_paths.sort_by(|a, b| a.0.cmp(&b.0));
    stale_paths.dedup_by(|a, b| a.0 == b.0);
    for (path, kind) in stale_paths {
        units.push(ExportUnit::Stale {
            root: spec,
            path,
            kind,
        });
    }
    units
}

/// What `sets` and the empty directories in `dest` say about each carried
/// plugin, and every stale path that is not a whole plugin.
fn slices_of<'a>(
    dest: &'a DestTree<'_>,
    sets: &'a FileSets,
    packs: &str,
    admitted: &BTreeMap<&str, usize>,
    stale_plugins: &BTreeSet<&str>,
    rendered: &BTreeMap<String, PreviewWrite>,
) -> (BTreeMap<&'a str, PluginSlice<'a>>, Vec<(String, StaleKind)>) {
    // Bucket what carries a story by the plugin that owns it. Membership is
    // decided against the two NAME sets, never by path shape alone: a
    // symlinked plugin directory is absent from the plugin folders and present
    // among the files, so it is neither carried nor stale, and falls to the
    // loose pile like any other stray.
    let mut slices: BTreeMap<&str, PluginSlice> = BTreeMap::new();
    let mut loose: Vec<&str> = Vec::new();
    for relative in &sets.extra {
        match split_pack(packs, relative) {
            Some((name, tail)) if admitted.contains_key(name) && !tail.is_empty() => {
                slices.entry(name).or_default().extra.push(relative);
            }
            Some((name, _)) if stale_plugins.contains(name) => {}
            _ => loose.push(relative),
        }
    }
    for relative in &sets.differs {
        if let Some((name, tail)) = split_pack(packs, relative)
            && !tail.is_empty()
            && admitted.contains_key(name)
        {
            slices.entry(name).or_default().differs.push(relative);
        }
    }

    // Empty directories, in the one order that keeps each to a single story:
    // inside a carried plugin it drifts that plugin; inside a stale one that
    // plugin's unit already speaks for it; where the render owns files
    // beneath it the owning unit reports the gap; otherwise it is stale on
    // its own.
    let mut stale_paths: Vec<(String, StaleKind)> = Vec::new();
    for dir in &dest.state.empty_dirs {
        match split_pack(packs, dir) {
            Some((name, tail)) if admitted.contains_key(name) && !tail.is_empty() => {
                slices.entry(name).or_default().dirty = true;
            }
            Some((name, _)) if stale_plugins.contains(name) => {}
            _ if rendered.keys().any(|k| k.starts_with(&format!("{dir}/"))) => {}
            _ => stale_paths.push((dir.clone(), StaleKind::EmptyDir)),
        }
    }
    stale_paths.extend(
        loose
            .into_iter()
            .map(|path| (path.to_owned(), StaleKind::File)),
    );
    (slices, stale_paths)
}

/// The version-bump signal for one plugin at one root: its MEMBER files
/// drifted, and the installed pack manifest still carries the version the
/// vault currently declares.
///
/// `None` when the plugin's note carries no `version`, or when nothing
/// among its members actually changed — the pack manifest can drift on its
/// own (a description, an author) without a member having moved, and that is
/// not this rule. The tail of the rule, the one that reads the installed
/// manifest back, is [`crate::status::version_bump_if_unmoved`].
fn plugin_version_bump(
    dest: &DestTree<'_>,
    spec: Harness,
    plugin: &str,
    rendered: &BTreeMap<String, PreviewWrite>,
    slice: Option<&PluginSlice<'_>>,
    config: &MarketplaceConfig,
) -> Option<VersionBumpSuggestion> {
    let current_version = overrides_for(config, plugin).and_then(|o| o.identity.version.clone())?;
    let manifest_key = format!(
        "{}/{plugin}/{}",
        spec.format().policy.packs,
        spec.format().plugin_manifest
    );
    if !rendered.contains_key(&manifest_key) {
        return None;
    }
    // Members changed when a rendered member did not match, or when the
    // destination holds a member the render does not.
    let slice = slice?;
    let members_changed =
        slice.differs.iter().any(|p| *p != manifest_key) || !slice.extra.is_empty();
    if !members_changed {
        return None;
    }
    version_bump_if_unmoved(dest.read(&manifest_key).as_deref(), current_version)
}

/// One root's comparison, both answers from one reading of the destination.
///
/// The two halves are computed side by side here, and nowhere else, because
/// `check` promises its verdict is `build`'s: a report saying `drift` while
/// the swap sees an equal tree, or the reverse, is the one way this file
/// could lie.
pub(super) fn diff_root(
    vfs: &dyn Vfs,
    out: &Path,
    spec: Harness,
    admitted: &BTreeMap<&str, usize>,
    rendered: &BTreeMap<String, PreviewWrite>,
    config: &MarketplaceConfig,
) -> Result<RootDiff, CoreError> {
    let dest = DestTree::scan(vfs, out.to_path_buf(), &spec.owned_paths())?;
    let plugin_dirs = folders_in(vfs, &out.join(spec.format().policy.packs))?;
    let sets = dest.compare(rendered);
    Ok(RootDiff {
        equal: root_is_equal(&sets, &dest),
        units: describe_root(&dest, spec, &plugin_dirs, admitted, rendered, &sets, config),
        foreign: !dest.built_here(rendered),
    })
}

/// Does `bytes` hold `marker` anywhere?
fn contains(bytes: &[u8], marker: &str) -> bool {
    bytes.windows(marker.len()).any(|w| w == marker.as_bytes())
}

/// `<packs>/<name>/<tail>` for a harness's pack directory: the plugin and
/// the rest, `tail` empty for the pack's own directory, `None` for a path
/// naming no pack of the harness's.
fn split_pack<'p>(packs: &str, relative: &'p str) -> Option<(&'p str, &'p str)> {
    let rest = relative
        .strip_prefix(packs)?
        .strip_prefix('/')
        .unwrap_or("");
    if rest.is_empty() {
        return None;
    }
    Some(match rest.split_once('/') {
        Some((name, tail)) => (name, tail),
        None => (rest, ""),
    })
}
