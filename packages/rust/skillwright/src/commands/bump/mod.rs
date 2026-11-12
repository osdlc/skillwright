//! `skillwright bump` — raise a `version` by a semver level, rewriting only
//! that frontmatter key. Contract: spec section 10 rule 4.
//!
//! Three forms. `bump <p>` raises the plugin `<p>` in `<p>.md`; when plugins
//! in two marketplaces share the name, `--marketplace` picks one. `bump
//! --marketplace <m>` with no plugin raises the marketplace in `<m>.md`.
//! `bump --changed-since <ref|dir>` raises every plugin whose rendered
//! members differ from a baseline and whose version has not moved since: a
//! directory is one built marketplace as it stands, and anything else is a
//! git ref, built in a temporary folder from the ref's own `skillwright.md`
//! and `marketplaces/`.
//!
//! A version that does not parse as plain `MAJOR.MINOR.PATCH` is refused
//! rather than guessed at (`semver::next_version`); an absent
//! `version` starts from the core's own default, `0.1.0` — so a note that
//! has never carried the key is bumped, not refused. The root file's
//! `build.bump.patch` picks how the patch number is written ([`PatchScheme`]).

mod baseline;
mod changed;
mod semver;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tradecraft_core::discover::discover_in;
use tradecraft_core::{
    CoreError, DiscoveredComponent, EntityKind, NativeVfs, PatchScheme, Vfs, read_version, refusal,
    set_version,
};

use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_REFUSED, failed, refused, usage_error};
use crate::prompt::confirm_write;
use crate::report::{dry_run_notice, print_json};
use crate::resolve::{RunFlags, resolve_run};
use changed::run_changed;
pub use semver::BumpLevel;
use semver::{next_version, parse_semver};

/// The resolved flags of `skillwright bump`: its own, and the run's.
pub struct BumpFlags {
    /// The plugin to bump, by its folder name. `None` with `changed_since`,
    /// or to bump the marketplace `--marketplace` names.
    pub plugin: Option<String>,
    /// `--changed-since`: bump every plugin that changed since this git ref
    /// or built directory.
    pub changed_since: Option<String>,
    /// How much of the version to bump.
    pub level: BumpLevel,
    /// The flags every verb shares. `--harness` names the harnesses to
    /// build and compare.
    pub run: RunFlags,
}

/// One row's `--json` object. `status` is `written`, `dry-run`, `unchanged`,
/// `skipped` or `refused`. Not one of the CLI spec's worked transcripts —
/// those are human-only — so the shape here is this file's own judgement
/// call, kept minimal on purpose. [`JsonShape`] says which fields a row
/// sets.
#[derive(Serialize)]
struct JsonRow<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    plugin: Option<&'a str>,
    path: &'a str,
    status: &'a str,
    current: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    new: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

/// One report row, printed as the text line in human mode or as one JSON
/// object in `--json` mode — [`crate::commands::marketplace::run_export`]'s
/// own `--json` branch, applied to a single-unit verb. `Err` is the exit code
/// a report that would not serialize earns; every other outcome is `Ok(())`.
fn write_row(ctx: &mut CommandContext<'_>, json: bool, row: &Row) -> Result<(), i32> {
    if json {
        print_json(ctx, &row.json())
    } else {
        let _ = writeln!(ctx.out, "{}", row.line());
        Ok(())
    }
}

/// The version a bump sets, or `None` for a current version that is not
/// plain semver or has no room at `level`. `now` is the UTC time as
/// `YYYYMMDDHHMMSS`, read only by [`PatchScheme::Timestamp`].
pub(crate) fn bumped_version(
    current: &str,
    level: BumpLevel,
    scheme: PatchScheme,
    now: u64,
) -> Option<String> {
    match scheme {
        PatchScheme::Increment => next_version(current, level),
        PatchScheme::Timestamp => {
            let (major, minor, patch) = parse_semver(current)?;
            Some(match level {
                BumpLevel::Patch => format!("{major}.{minor}.{}", now.max(patch.checked_add(1)?)),
                BumpLevel::Minor => format!("{major}.{}.{now}", minor.checked_add(1)?),
                BumpLevel::Major => format!("{}.0.{now}", major.checked_add(1)?),
            })
        }
    }
}

fn utc_now_stamp() -> u64 {
    chrono::Utc::now()
        .format("%Y%m%d%H%M%S")
        .to_string()
        .parse()
        .unwrap_or_default()
}

/// One plugin note read and its bump worked out; nothing written yet.
struct Planned {
    rel: String,
    entry_path: PathBuf,
    source: String,
    current: String,
    /// Whether the note carries a version key at all.
    versioned: bool,
    /// `None`: the current version is not plain semver.
    new: Option<String>,
}

/// A note that holds a plugin's version.
struct VersionNote {
    path: PathBuf,
    rel: String,
}

/// The reason on a row whose `version` is not semver.
fn non_semver_reason(current: &str) -> String {
    format!(
        "{}: `version` reads `{current}`",
        refusal::NON_SEMVER.as_str()
    )
}

fn plan_bump(
    note: VersionNote,
    level: BumpLevel,
    scheme: PatchScheme,
    now: u64,
) -> Result<Planned, String> {
    let VersionNote { path, rel } = note;
    let bytes = NativeVfs.read_file(&path).map_err(|e| e.to_string())?;
    let source = String::from_utf8(bytes)
        .map_err(|e| CoreError::unreadable(&path, format!("not UTF-8: {e}")).reason())?;
    let found = read_version(&source).map_err(|e| e.reason())?;
    let versioned = found.is_some();
    // No version key in either shape is not a refusal — the core's stated
    // default is what a bump starts from.
    let current = found.unwrap_or_else(|| "0.1.0".to_string());
    let new = bumped_version(&current, level, scheme, now);
    Ok(Planned {
        rel,
        entry_path: path,
        source,
        current,
        versioned,
        new,
    })
}

fn write_bump(planned: &Planned, new: &str) -> Result<(), String> {
    let edited = set_version(&planned.source, new).map_err(|e| e.reason())?;
    NativeVfs
        .write_file(&planned.entry_path, edited.as_bytes())
        .map_err(|e| e.to_string())
}

/// Run `skillwright bump` and return the exit code.
pub fn run_bump(flags: &BumpFlags, ctx: &mut CommandContext<'_>) -> i32 {
    let loaded = match resolve_run(&flags.run, &ctx.cwd, ctx.err) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let scan = match discover_in(&NativeVfs, &loaded.layout) {
        Ok(scan) => scan,
        Err(err) => {
            return refused(
                ctx.err,
                &format!("the vault scan could not complete: {}", err.reason()),
            );
        }
    };
    // The plugin notes of the marketplaces this run covers.
    let plugins: Vec<&DiscoveredComponent> = scan
        .components
        .iter()
        .filter(|c| {
            c.position.kind() == EntityKind::Plugin
                && loaded.marketplace(c.position.marketplace()).is_some()
        })
        .collect();
    let scheme = loaded
        .settings
        .build
        .bump
        .as_ref()
        .map(|bump| bump.patch)
        .unwrap_or_default();

    match (&flags.plugin, &flags.changed_since) {
        (Some(plugin), None) => match plugin_note(&plugins, plugin) {
            Ok(note) => run_one(flags, ctx, plugin, note, scheme),
            Err(message) => usage_error(ctx.err, &message),
        },
        (None, None) => match (
            flags.run.marketplace.as_deref(),
            loaded.marketplaces.as_slice(),
        ) {
            (Some(_), [market]) => {
                let note = VersionNote {
                    path: market.note.clone(),
                    rel: vault_relative(&loaded.root, &market.note),
                };
                run_one(flags, ctx, &market.name, note, scheme)
            }
            _ => usage_error(
                ctx.err,
                "bump takes a plugin name, --marketplace or --changed-since",
            ),
        },
        (None, Some(since)) => run_changed(flags, ctx, &loaded, &plugins, since, scheme),
        _ => usage_error(
            ctx.err,
            "bump takes a plugin name or --changed-since, and not both",
        ),
    }
}

/// `path` relative to `root`, forward-slashed.
fn vault_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The note of the plugin `name`, or why there is none to bump.
fn plugin_note(plugins: &[&DiscoveredComponent], name: &str) -> Result<VersionNote, String> {
    let matches: Vec<&&DiscoveredComponent> = plugins
        .iter()
        .filter(|c| c.position.name() == name)
        .collect();
    match matches.as_slice() {
        [] => {
            let names: BTreeSet<&str> = plugins.iter().map(|c| c.position.name()).collect();
            let held = if names.is_empty() {
                "none".to_owned()
            } else {
                names
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            Err(format!("no plugin `{name}` in this vault: it holds {held}"))
        }
        [one] => Ok(plugin_version_note(one)),
        many => {
            let paths: Vec<&str> = many
                .iter()
                .map(|c| c.vault_relative_path.as_str())
                .collect();
            Err(format!(
                "`{name}` names a plugin in more than one marketplace: {} — name one with --marketplace",
                paths.join(", ")
            ))
        }
    }
}

/// The version note of the plugin note `component`.
fn plugin_version_note(component: &DiscoveredComponent) -> VersionNote {
    VersionNote {
        path: component.entry_path.clone(),
        rel: component.vault_relative_path.clone(),
    }
}

/// Bump the one note `note`, a plugin's or a marketplace's, named `label` in
/// the report.
fn run_one(
    flags: &BumpFlags,
    ctx: &mut CommandContext<'_>,
    label: &str,
    note: VersionNote,
    scheme: PatchScheme,
) -> i32 {
    let plugin = label;
    let planned = match plan_bump(note, flags.level, scheme, utc_now_stamp()) {
        Ok(planned) => planned,
        Err(err) => return failed(ctx.err, "bump", &err),
    };
    let Some(new) = planned.new.clone() else {
        let reason = non_semver_reason(&planned.current);
        let row = Row::refused(plugin, &planned.rel, &planned.current, reason).single();
        if let Err(code) = write_row(ctx, flags.run.json, &row) {
            return code;
        }
        if !flags.run.quiet {
            let _ = writeln!(ctx.err, "1 refused");
        }
        return EXIT_REFUSED;
    };
    if flags.run.dry_run {
        let row = Row::bumped(plugin, planned, "dry-run", new).single();
        if let Err(code) = write_row(ctx, flags.run.json, &row) {
            return code;
        }
        if !flags.run.quiet {
            dry_run_notice(ctx, "the bump this run would make");
        }
        return EXIT_OK;
    }

    if !flags.run.yes {
        let plan = vec![format!(
            "{}  version {} -> {new}",
            planned.rel, planned.current
        )];
        let gate = confirm_write("bump", &plan, ctx.err);
        if gate != EXIT_OK {
            return gate;
        }
    }

    if let Err(err) = write_bump(&planned, &new) {
        return failed(ctx.err, "bump", &err);
    }

    let row = Row::bumped(plugin, planned, "written", new).single();
    if let Err(code) = write_row(ctx, flags.run.json, &row) {
        return code;
    }
    if !flags.run.quiet {
        let _ = writeln!(ctx.err, "1 written");
    }
    EXIT_OK
}

/// One plugin's row in a `bump` report: one for a single plugin, one per
/// plugin for `--changed-since`.
struct Row {
    plugin: String,
    /// Which `--json` object the row prints as.
    shape: JsonShape,
    rel: String,
    status: &'static str,
    current: String,
    new: Option<String>,
    reason: Option<String>,
    /// What `reason` means, for the text line; `None` when `reason` says it.
    explain: Option<&'static str>,
    /// The bump to make; `None` for a row that bumps nothing.
    planned: Option<Planned>,
}

/// The two `--json` row objects `bump` prints.
#[derive(Clone, Copy, PartialEq, Eq)]
enum JsonShape {
    /// A single-plugin run's object: the note, its status and versions, and
    /// why when there is a reason. It names no plugin.
    Single,
    /// A `--changed-since` row: the plugin, and why when there is a reason.
    Changed,
}

impl Row {
    fn plain(plugin: &str, planned: &Planned, status: &'static str, reason: Option<String>) -> Row {
        Row {
            plugin: plugin.to_owned(),
            shape: JsonShape::Changed,
            rel: planned.rel.clone(),
            status,
            current: planned.current.clone(),
            new: None,
            reason,
            explain: None,
            planned: None,
        }
    }

    fn refused(plugin: &str, rel: &str, current: &str, reason: String) -> Row {
        Row {
            plugin: plugin.to_owned(),
            shape: JsonShape::Changed,
            rel: rel.to_owned(),
            status: "refused",
            current: current.to_owned(),
            new: None,
            reason: Some(reason),
            explain: None,
            planned: None,
        }
    }

    fn bump(plugin: &str, planned: Planned) -> Row {
        Row {
            plugin: plugin.to_owned(),
            shape: JsonShape::Changed,
            rel: planned.rel.clone(),
            status: "pending",
            current: planned.current.clone(),
            new: None,
            reason: None,
            explain: None,
            planned: Some(planned),
        }
    }

    /// A bump settled as `status`, `dry-run` or `written`, to `new`.
    fn bumped(plugin: &str, planned: Planned, status: &'static str, new: String) -> Row {
        let mut row = Row::bump(plugin, planned);
        row.status = status;
        row.new = Some(new);
        row
    }

    fn line(&self) -> String {
        let label = if self.rel.is_empty() {
            &self.plugin
        } else {
            &self.rel
        };
        match (self.status, &self.new, &self.reason) {
            ("written", Some(new), _) => {
                format!("{label}  written · version {} -> {new}", self.current)
            }
            ("dry-run", Some(new), _) => format!("{label}  {} \u{2192} {new}", self.current),
            (status, _, Some(reason)) => match self.explain {
                Some(explain) => format!("{label}  {status} · {reason}: {explain}"),
                None => format!("{label}  {status} · {reason}"),
            },
            (status, _, None) => format!("{label}  {status}"),
        }
    }

    /// The row as a single-plugin run prints it.
    fn single(mut self) -> Row {
        self.shape = JsonShape::Single;
        self
    }

    /// The row's `--json` object, in its [`JsonShape`].
    fn json(&self) -> JsonRow<'_> {
        let changed = self.shape == JsonShape::Changed;
        JsonRow {
            plugin: changed.then_some(self.plugin.as_str()),
            path: &self.rel,
            status: self.status,
            current: &self.current,
            new: self.new.as_deref(),
            reason: self.reason.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 20_261_105_143_000;

    #[test]
    fn the_increment_scheme_is_plain_semver() {
        for (level, want) in [
            (BumpLevel::Patch, "1.2.4"),
            (BumpLevel::Minor, "1.3.0"),
            (BumpLevel::Major, "2.0.0"),
        ] {
            assert_eq!(
                bumped_version("1.2.3", level, PatchScheme::Increment, NOW).as_deref(),
                Some(want)
            );
        }
    }

    #[test]
    fn the_timestamp_scheme_writes_the_time_as_the_patch() {
        for (level, want) in [
            (BumpLevel::Patch, "1.2.20261105143000"),
            (BumpLevel::Minor, "1.3.20261105143000"),
            (BumpLevel::Major, "2.0.20261105143000"),
        ] {
            assert_eq!(
                bumped_version("1.2.3", level, PatchScheme::Timestamp, NOW).as_deref(),
                Some(want)
            );
        }
        // Twice in one second still moves the version.
        assert_eq!(
            bumped_version(
                "1.2.20261105143000",
                BumpLevel::Patch,
                PatchScheme::Timestamp,
                NOW
            )
            .as_deref(),
            Some("1.2.20261105143001")
        );
        assert_eq!(
            bumped_version("v1", BumpLevel::Patch, PatchScheme::Timestamp, NOW),
            None
        );
    }
}
