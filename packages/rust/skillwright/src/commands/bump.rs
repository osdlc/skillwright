//! `skillwright bump` — raise a `version` by a semver level, rewriting only
//! that frontmatter key (or, for a note that uses the nested `sw:` block,
//! only that block). Contract: spec section 10 rule 4.
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
//! rather than guessed at ([`tradecraft_core::next_version`]); an absent
//! `version` starts from the core's own default, `0.1.0` — so a note that
//! has never carried the key is bumped, not refused. The root file's
//! `build.bump.patch` picks how the patch number is written ([`PatchScheme`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use tradecraft_core::harness::with_plugin_format;
use tradecraft_core::layout::MARKETPLACES_DIR;
use tradecraft_core::{
    BumpLevel, DiscoveredComponent, EntityKind, ExportMode, ExportRequest, ExportUnit,
    MarketplaceConfig, NativeVfs, PatchScheme, ROOT_FILE, UnitStatus, Vfs, check_packs,
    discover_components, export_skills, is_vault_root, next_version, owned_paths, packs_path,
    parse_semver, plugin_manifest_path, read_version, set_version,
};

use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_REFUSED, blocked, usage_error};
use crate::prompt::confirm_write;
use crate::report::normalize_thrown_reason;
use crate::resolve::{CommonFlags, ResolvedRun, resolve_at, resolve_run};

/// The resolved flags of `skillwright bump`.
pub struct BumpFlags {
    /// The plugin to bump, by its folder name. `None` with `changed_since`,
    /// or to bump the marketplace `--marketplace` names.
    pub plugin: Option<String>,
    /// `--changed-since`: bump every plugin that changed since this git ref
    /// or built directory.
    pub changed_since: Option<String>,
    /// `--marketplace`: the marketplace to work in, or to bump.
    pub marketplace: Option<String>,
    /// How much of the version to bump.
    pub level: BumpLevel,
    /// The global `--vault` flag.
    pub common: CommonFlags,
    /// `--json`: print the report as one JSON document.
    pub json: bool,
    /// `--dry-run`: classify and report; write nothing.
    pub dry_run: bool,
    /// `--yes`: accept the plan without prompting.
    pub yes: bool,
    /// `--quiet`: drop progress and the tally from stderr.
    pub quiet: bool,
}

/// `--changed-since`'s `--json` document: the baseline, and one row per
/// plugin the render holds.
#[derive(Serialize)]
struct ChangedReport<'a> {
    baseline: &'a str,
    plugins: Vec<JsonRow<'a>>,
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

fn print_json(ctx: &mut CommandContext<'_>, value: &impl Serialize) -> Result<(), i32> {
    match serde_json::to_string_pretty(value) {
        Ok(document) => {
            let _ = writeln!(ctx.out, "{document}");
            Ok(())
        }
        Err(err) => Err(blocked(
            ctx.err,
            &format!("the report would not serialize: {err}"),
        )),
    }
}

fn bump_failed(err: &mut dyn std::io::Write, e: impl std::fmt::Display) -> i32 {
    blocked(
        err,
        &format!("bump failed: {}", normalize_thrown_reason(&e.to_string())),
    )
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

fn plan_bump(
    note: VersionNote,
    level: BumpLevel,
    scheme: PatchScheme,
    now: u64,
) -> Result<Planned, String> {
    let VersionNote { path, rel } = note;
    let bytes = NativeVfs.read_file(&path).map_err(|e| e.to_string())?;
    let source = String::from_utf8(bytes).map_err(|e| format!("{rel} is not valid UTF-8: {e}"))?;
    let found = read_version(&source).map_err(|e| e.to_string())?;
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
    let edited = set_version(&planned.source, new).map_err(|e| e.to_string())?;
    NativeVfs
        .write_file(&planned.entry_path, edited.as_bytes())
        .map_err(|e| e.to_string())
}

/// Run `skillwright bump` and return the exit code.
pub fn run_bump(flags: &BumpFlags, ctx: &mut CommandContext<'_>) -> i32 {
    let run = match resolve_run(
        &flags.common,
        flags.marketplace.as_deref(),
        &ctx.cwd,
        ctx.err,
    ) {
        Ok(run) => run,
        Err(code) => return code,
    };
    let scan = match discover_components(&NativeVfs, &run.vault) {
        Ok(scan) => scan,
        Err(err) => {
            return blocked(
                ctx.err,
                &format!(
                    "the vault scan could not complete: {}",
                    normalize_thrown_reason(&err.to_string())
                ),
            );
        }
    };
    // The plugin notes of the marketplaces this run covers.
    let plugins: Vec<&DiscoveredComponent> = scan
        .components
        .iter()
        .filter(|c| {
            c.kind == EntityKind::Plugin
                && c.marketplace
                    .as_deref()
                    .is_some_and(|m| run.loaded.marketplace(m).is_some())
        })
        .collect();
    let scheme = run
        .loaded
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
        (None, None) => match (flags.marketplace.as_deref(), run.marketplaces()) {
            (Some(_), [market]) => {
                let note = VersionNote {
                    path: market.note.clone(),
                    rel: vault_relative(&run.vault, &market.note),
                };
                run_one(flags, ctx, &market.name, note, scheme)
            }
            _ => usage_error(
                ctx.err,
                "bump takes a plugin name, --marketplace or --changed-since",
            ),
        },
        (None, Some(since)) => run_changed(flags, ctx, &run, &plugins, since, scheme),
        _ => usage_error(
            ctx.err,
            "bump takes a plugin name or --changed-since, and not both",
        ),
    }
}

/// `path` relative to `root`, forward-slashed.
fn vault_relative(root: &Path, path: &Path) -> String {
    crate::config::relative_path(root, path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The note of the plugin `name`, or why there is none to bump.
fn plugin_note(plugins: &[&DiscoveredComponent], name: &str) -> Result<VersionNote, String> {
    let matches: Vec<&&DiscoveredComponent> = plugins.iter().filter(|c| c.name == name).collect();
    match matches.as_slice() {
        [] => {
            let names: BTreeSet<&str> = plugins.iter().map(|c| c.name.as_str()).collect();
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
        Err(err) => return bump_failed(ctx.err, err),
    };
    let Some(new) = planned.new.clone() else {
        let reason = format!("non-semver: `version` reads `{}`", planned.current);
        let row = Row::refused(plugin, &planned.rel, &planned.current, reason).single();
        if let Err(code) = write_row(ctx, flags.json, &row) {
            return code;
        }
        if !flags.quiet {
            let _ = writeln!(ctx.err, "1 refused");
        }
        return EXIT_REFUSED;
    };
    if flags.dry_run {
        let row = Row::bumped(plugin, planned, "dry-run", new).single();
        if let Err(code) = write_row(ctx, flags.json, &row) {
            return code;
        }
        let _ = writeln!(
            ctx.err,
            "dry run — reporting the bump this run would make; nothing will be written"
        );
        return EXIT_OK;
    }

    if !flags.yes {
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
        return bump_failed(ctx.err, err);
    }

    let row = Row::bumped(plugin, planned, "written", new).single();
    if let Err(code) = write_row(ctx, flags.json, &row) {
        return code;
    }
    if !flags.quiet {
        let _ = writeln!(ctx.err, "1 written");
    }
    EXIT_OK
}

/// What the comparison against the baseline says about one plugin.
#[derive(Debug, Default)]
struct Verdict<'a> {
    /// Its members differ and its version has not moved, in some root.
    bump: bool,
    /// Anything of it differs, in some root.
    drift: bool,
    /// The harness roots the render holds it in.
    roots: Vec<&'a str>,
}

/// Why a drifted, versioned plugin is not bumped: the `reason` token of a
/// `skipped` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Skip {
    /// Its version already differs from the baseline's.
    VersionMoved,
    /// No configured harness root of the baseline holds its manifest.
    NotInBaseline,
    /// Its members match; only its manifest differs.
    ManifestOnly,
}

impl Skip {
    fn token(self) -> &'static str {
        match self {
            Skip::VersionMoved => "version-moved",
            Skip::NotInBaseline => "not-in-baseline",
            Skip::ManifestOnly => "manifest-only",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Skip::VersionMoved => "its version already moved since the baseline",
            Skip::NotInBaseline => "the baseline does not hold this plugin",
            Skip::ManifestOnly => "only its manifest changed",
        }
    }

    /// Read the baseline's own manifests of `plugin` in `roots` to tell the
    /// three causes apart.
    fn of(
        baseline: &Path,
        config: &MarketplaceConfig,
        roots: &[&str],
        plugin: &str,
        current: &str,
    ) -> Skip {
        let mut held = false;
        for root in roots {
            let (Some(packs), Some(manifest)) =
                (packs_path(config, root), plugin_manifest_path(root))
            else {
                continue;
            };
            let Ok(bytes) = std::fs::read(baseline.join(packs).join(plugin).join(manifest)) else {
                continue;
            };
            held = true;
            let version = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|doc| doc.get("version")?.as_str().map(str::to_owned));
            if version.as_deref() != Some(current) {
                return Skip::VersionMoved;
            }
        }
        if held {
            Skip::ManifestOnly
        } else {
            Skip::NotInBaseline
        }
    }
}

/// Whether `dir` holds a plugin manifest for any of `roots`.
fn holds_a_plugin_manifest(dir: &Path, config: &MarketplaceConfig, roots: &[&str]) -> bool {
    roots.iter().any(|root| {
        let (Some(packs), Some(manifest)) = (packs_path(config, root), plugin_manifest_path(root))
        else {
            return false;
        };
        std::fs::read_dir(dir.join(packs)).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().join(manifest).is_file())
        })
    })
}

fn run_changed(
    flags: &BumpFlags,
    ctx: &mut CommandContext<'_>,
    run: &ResolvedRun,
    plugins: &[&DiscoveredComponent],
    since: &str,
    scheme: PatchScheme,
) -> i32 {
    // Each request's `out_root` becomes its baseline once that resolves.
    let requests: Vec<(&str, ExportRequest)> = run
        .marketplaces()
        .iter()
        .map(|m| {
            (
                m.name.as_str(),
                run.loaded.export_request(m, ExportMode::Check),
            )
        })
        .collect();
    // Every request reads the root file's config.
    let current = run.loaded.inputs.config.clone();
    // The build's own `packs` check, before a ref is built and its packs are
    // moved to where this config keeps them.
    if let Err(err) = check_packs(&current) {
        return blocked(
            ctx.err,
            &format!(
                "--changed-since `{since}`: {}",
                normalize_thrown_reason(&err.to_string())
            ),
        );
    }
    let baseline = match Baseline::resolve(since, run, &current, ctx) {
        Ok(baseline) => baseline,
        Err(code) => return code,
    };

    let now = utc_now_stamp();
    let mut rows: Vec<Row> = Vec::new();
    for (market, mut request) in requests {
        request.out_root = baseline.root(market);
        let mut roots: Vec<&str> = match owned_paths(&request) {
            Ok(owned) => owned.into_iter().map(|(root, _)| root).collect(),
            Err(err) => return blocked(ctx.err, normalize_thrown_reason(&err.to_string())),
        };
        roots.dedup();
        if let Baseline::Dir(dir) = &baseline
            && !holds_a_plugin_manifest(dir, &request.config, &roots)
        {
            let expected: Vec<String> = roots
                .iter()
                .filter_map(|root| {
                    Some(format!(
                        "{}/<plugin>/{}",
                        packs_path(&request.config, root)?,
                        plugin_manifest_path(root)?
                    ))
                })
                .collect();
            return blocked(
                ctx.err,
                &format!(
                    "--changed-since `{since}`: {} holds no plugin manifest for any configured harness; a built marketplace holds one at {}",
                    dir.display(),
                    expected.join(" or ")
                ),
            );
        }
        let report = match export_skills(&NativeVfs, &request) {
            Ok(report) => report,
            Err(err) => {
                return blocked(
                    ctx.err,
                    &format!(
                        "comparing against `{since}` failed: {}",
                        normalize_thrown_reason(&err.to_string())
                    ),
                );
            }
        };

        let mut verdicts: BTreeMap<&str, Verdict> = BTreeMap::new();
        for unit in &report.units {
            if let ExportUnit::Plugin {
                root,
                name,
                status,
                version_bump_suggestion,
                ..
            } = unit
            {
                let verdict = verdicts.entry(name.as_str()).or_default();
                verdict.bump |= version_bump_suggestion.is_some();
                verdict.drift |= *status == UnitStatus::Drift;
                verdict.roots.push(root.as_str());
            }
        }

        for (name, verdict) in verdicts {
            let note = plugins
                .iter()
                .find(|c| c.name == name && c.marketplace.as_deref() == Some(market));
            let Some(note) = note else {
                let reason = format!("`{name}` has no note to bump; add one with a `version`");
                let mut row = Row::refused(name, "", "", reason);
                if !verdict.drift {
                    row.status = "unchanged";
                    row.reason = None;
                }
                rows.push(row);
                continue;
            };
            let planned = match plan_bump(plugin_version_note(note), flags.level, scheme, now) {
                Ok(planned) => planned,
                Err(err) => return bump_failed(ctx.err, err),
            };
            // A note with no version key has no version to have moved, so any
            // drift is a change.
            let changed = verdict.bump || (verdict.drift && !planned.versioned);
            let row = if !verdict.drift {
                Row::plain(name, &planned, "unchanged", None)
            } else if !changed {
                let skip = Skip::of(
                    &request.out_root,
                    &request.config,
                    &verdict.roots,
                    name,
                    &planned.current,
                );
                let mut row = Row::plain(name, &planned, "skipped", Some(skip.token().to_owned()));
                row.explain = Some(skip.text());
                row
            } else if planned.new.is_none() {
                let reason = format!("non-semver: `version` reads `{}`", planned.current);
                Row::refused(name, &planned.rel, &planned.current, reason)
            } else {
                Row::bump(name, planned)
            };
            rows.push(row);
        }
    }
    drop(baseline);

    let pending: Vec<&Row> = rows.iter().filter(|r| r.planned.is_some()).collect();
    if !flags.dry_run && !flags.yes && !pending.is_empty() {
        let plan: Vec<String> = pending
            .iter()
            .filter_map(|r| {
                let p = r.planned.as_ref()?;
                Some(format!(
                    "{}  version {} -> {}",
                    p.rel,
                    p.current,
                    p.new.as_deref()?
                ))
            })
            .collect();
        let gate = confirm_write("bump", &plan, ctx.err);
        if gate != EXIT_OK {
            return gate;
        }
    }

    for row in &mut rows {
        let Some(planned) = &row.planned else {
            continue;
        };
        let Some(new) = planned.new.clone() else {
            continue;
        };
        if flags.dry_run {
            row.status = "dry-run";
        } else {
            // A failed write is reported on its row; the bumps already
            // written still reach the report.
            if let Err(err) = write_bump(planned, &new) {
                row.status = "refused";
                row.reason = Some(format!(
                    "the write failed: {}",
                    normalize_thrown_reason(&err)
                ));
                continue;
            }
            row.status = "written";
        }
        row.new = Some(new);
    }

    if flags.json {
        let document = ChangedReport {
            baseline: since,
            plugins: rows.iter().map(Row::json).collect(),
        };
        if let Err(code) = print_json(ctx, &document) {
            return code;
        }
    } else {
        for row in &rows {
            let _ = writeln!(ctx.out, "{}", row.line());
        }
    }
    if flags.dry_run {
        let _ = writeln!(
            ctx.err,
            "dry run — reporting the bumps this run would make; nothing will be written"
        );
    }
    if !flags.quiet {
        let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
        for row in &rows {
            *tally.entry(row.status).or_default() += 1;
        }
        let parts: Vec<String> = tally.iter().map(|(s, n)| format!("{n} {s}")).collect();
        let _ = writeln!(
            ctx.err,
            "{} checked against `{since}` — {}",
            rows.len(),
            if parts.is_empty() {
                "no plugins".to_owned()
            } else {
                parts.join(", ")
            }
        );
    }
    if rows.iter().any(|r| r.status == "refused") {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
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
    /// A single-plugin run's object: the note, its status and versions. It
    /// names no plugin and carries no reason.
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
            reason: self.reason.as_deref().filter(|_| changed),
        }
    }
}

/// What `--changed-since` compares against: one built marketplace in a
/// directory as it is, or a git ref checked out and built in a temporary
/// folder that is removed on drop.
enum Baseline {
    Dir(PathBuf),
    Built(BuiltRef),
}

impl Baseline {
    /// The built tree of the marketplace `market`.
    fn root(&self, market: &str) -> PathBuf {
        match self {
            Baseline::Dir(dir) => dir.clone(),
            Baseline::Built(built) => built.out.join(market),
        }
    }

    /// A directory that exists is a built baseline; anything else is a git
    /// ref, built from the ref's own root file and `marketplaces/`. `current`
    /// is this run's config, where a built ref's packs are moved to.
    ///
    /// A directory holds one marketplace, so a run over several is refused.
    fn resolve(
        since: &str,
        run: &ResolvedRun,
        current: &MarketplaceConfig,
        ctx: &mut CommandContext<'_>,
    ) -> Result<Baseline, i32> {
        let as_dir = crate::config::resolve_against_cwd(since, &ctx.cwd);
        if as_dir.is_dir() {
            if let [_, _, ..] = run.marketplaces() {
                return Err(usage_error(
                    ctx.err,
                    &format!(
                        "--changed-since `{since}` is one built marketplace, and this vault holds several; name one with --marketplace"
                    ),
                ));
            }
            return Ok(Baseline::Dir(as_dir));
        }
        BuiltRef::build(since, run, current, ctx).map(Baseline::Built)
    }
}

/// The folder under the system temp dir that holds every baseline build:
/// `<id>.lock` beside `<id>/`, the lock held for the run's lifetime.
const BASELINES_DIR: &str = "skillwright-bump-baselines";

/// The file in [`BASELINES_DIR`] locked while a run prunes and claims.
const GUARD_FILE: &str = "guard";

/// Keeps two claims in one process and one clock tick apart.
static NEXT_CLAIM: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A git ref's build inputs checked out into a temporary folder, and built
/// there: each marketplace into `out/<m>/`. No worktree is registered and
/// the repository's own index is not touched.
struct BuiltRef {
    temp: PathBuf,
    checkout: PathBuf,
    out: PathBuf,
    lock_path: PathBuf,
    /// Held until drop, so a concurrent run's prune leaves this one alone.
    _lock: std::fs::File,
}

impl BuiltRef {
    fn build(
        since: &str,
        run: &ResolvedRun,
        current: &MarketplaceConfig,
        ctx: &mut CommandContext<'_>,
    ) -> Result<BuiltRef, i32> {
        let fail = |ctx: &mut CommandContext<'_>, why: String| {
            Err(blocked(
                ctx.err,
                &format!("--changed-since `{since}`: {why}"),
            ))
        };
        let root = run.vault.canonicalize().unwrap_or(run.vault.clone());
        let repo = match typed_git::git()
            .dir(&root)
            .args(&["rev-parse", "--show-toplevel"])
            .run()
        {
            Ok(top) => PathBuf::from(top),
            Err(_) => {
                return fail(
                    ctx,
                    format!(
                        "is not a directory, and {} is not in a git repository",
                        root.display()
                    ),
                );
            }
        };
        let repo = repo.canonicalize().unwrap_or(repo);
        let Ok(sha) = typed_git::query::resolve_commit(&repo, since) else {
            return fail(
                ctx,
                format!("is neither a directory nor a commit in {}", repo.display()),
            );
        };
        let Ok(vault_rel) = root.strip_prefix(&repo).map(Path::to_path_buf) else {
            return fail(
                ctx,
                format!("{} is outside {}", root.display(), repo.display()),
            );
        };

        let baselines = std::env::temp_dir().join(BASELINES_DIR);
        let (id, lock_path, lock) = match claim_baseline(&baselines, &repo) {
            Ok(claimed) => claimed,
            Err(why) => return fail(ctx, why),
        };
        let temp = baselines.join(&id);
        if let Err(err) = std::fs::create_dir_all(&temp) {
            let _ = std::fs::remove_file(&lock_path);
            return fail(ctx, format!("cannot create {}: {err}", temp.display()));
        }
        // Canonical: on macOS the temp dir sits behind the `/var` link.
        let temp = temp.canonicalize().unwrap_or(temp);
        let built = BuiltRef {
            checkout: temp.join("checkout"),
            out: temp.join("out"),
            temp,
            lock_path,
            _lock: lock,
        };
        // Git refuses a work tree that is not there.
        if let Err(err) = std::fs::create_dir(&built.checkout) {
            return fail(
                ctx,
                format!("cannot create {}: {err}", built.checkout.display()),
            );
        }
        let index = built.temp.join("index");
        let check_out = |candidates| {
            let set = path_set(&repo, &sha, candidates)?;
            check_out_paths(&repo, &sha, &index, &built.checkout, &set).map(|()| set)
        };
        // The vault first: the other files the build reads are known only
        // once the ref's root file is read.
        let vault = match check_out(vault_candidates(&repo, &root)) {
            Ok(set) => set,
            Err(err) => return fail(ctx, format!("checking it out failed: {err}")),
        };

        let base_root = built.checkout.join(&vault_rel);
        if !is_vault_root(&NativeVfs, &base_root) {
            return fail(
                ctx,
                format!(
                    "{since} holds no `{ROOT_FILE}` and `{MARKETPLACES_DIR}/` at {}, so it cannot be built. \
                     Compare against a built folder instead",
                    if vault_rel.as_os_str().is_empty() {
                        "the repository top".to_owned()
                    } else {
                        vault_rel.display().to_string()
                    }
                ),
            );
        }
        // The ref's refusal names the temporary checkout, so it is said as
        // this flag's failure.
        let mut said: Vec<u8> = Vec::new();
        let base_run = match resolve_at(&base_root, None, &mut said) {
            Ok(base_run) => base_run,
            Err(_) => {
                let said = String::from_utf8_lossy(&said);
                return fail(ctx, format!("building it failed: {}", said.trim_end()));
            }
        };
        if vault != [WHOLE_TREE]
            && let Err(err) = check_out(build_input_candidates(&built.checkout, &base_run))
        {
            return fail(ctx, format!("checking it out failed: {err}"));
        }
        for market in run.marketplaces() {
            let out = built.out.join(&market.name);
            // A marketplace the ref does not hold has an empty baseline.
            let Some(base_market) = base_run.loaded.marketplace(&market.name) else {
                if let Err(err) = std::fs::create_dir_all(&out) {
                    return fail(ctx, format!("cannot create {}: {err}", out.display()));
                }
                continue;
            };
            let mut request = base_run
                .loaded
                .export_request(base_market, ExportMode::Build);
            request.out_root = out.clone();
            if let Err(err) = export_skills(&NativeVfs, &request) {
                return fail(
                    ctx,
                    format!(
                        "building it failed: {}",
                        normalize_thrown_reason(&err.to_string())
                    ),
                );
            }
            if let Err(err) = align_packs(&out, &request.config, current) {
                return fail(ctx, format!("moving its packs failed: {err}"));
            }
        }
        Ok(built)
    }
}

/// The path set that stands for the whole tree.
const WHOLE_TREE: &str = ".";

/// `path` relative to `top`, forward-slashed, [`WHOLE_TREE`] for `top`
/// itself. `None` for a path outside `top`, which is not a git path.
fn relative_to(top: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(top).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|part| match part {
            std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Option<_>>()?;
    Some(if parts.is_empty() {
        WHOLE_TREE.to_owned()
    } else {
        parts.join("/")
    })
}

/// What the build reads of the vault at `root`, relative to `repo`: the root
/// file and `marketplaces/`.
fn vault_candidates(repo: &Path, root: &Path) -> Vec<String> {
    [root.join(ROOT_FILE), root.join(MARKETPLACES_DIR)]
        .iter()
        .filter_map(|path| relative_to(repo, path))
        .collect()
}

/// What the ref's build reads outside its vault, relative to `checkout`: each
/// `build.assets` and `build.hooks` source. The sources come from the loaded
/// root file, as the build's own request does. A source outside the checkout
/// is read from disk and is left out.
fn build_input_candidates(checkout: &Path, run: &ResolvedRun) -> Vec<String> {
    let inputs = &run.loaded.inputs;
    inputs
        .assets
        .iter()
        .map(|asset| &asset.source)
        .chain(inputs.hooks.iter().map(|hook| &hook.source))
        .filter_map(|path| relative_to(checkout, path))
        .collect()
}

/// What to check out for `candidates`, repo-relative paths at `sha`: the
/// candidates the ref holds, with every `.gitattributes` above them, since
/// a path checkout applies only the attributes it checks out. `[WHOLE_TREE]`
/// when a candidate is the repository top or a symlink sits in, above or
/// under one: a link can point anywhere, and the whole tree reads it as a
/// full checkout does.
///
/// # Errors
///
/// The git failure, as the line to print.
fn path_set(repo: &Path, sha: &str, candidates: Vec<String>) -> Result<Vec<String>, String> {
    let mut set: BTreeSet<String> = candidates.into_iter().collect();
    if set.contains(WHOLE_TREE) {
        return Ok(vec![WHOLE_TREE.to_owned()]);
    }
    let attributes: Vec<String> = set.iter().flat_map(|path| attribute_files(path)).collect();
    set.extend(attributes);
    // A path inside another in the set comes with it.
    let set: Vec<String> = set
        .iter()
        .filter(|path| !set.iter().any(|outer| is_under(path, outer)))
        .cloned()
        .collect();
    if has_symlink(repo, sha, &set)? {
        return Ok(vec![WHOLE_TREE.to_owned()]);
    }
    paths_at_ref(repo, sha, &set)
}

/// Whether repo-relative `path` lies strictly inside the folder `outer`.
fn is_under(path: &str, outer: &str) -> bool {
    path.strip_prefix(outer)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// The `.gitattributes` in each folder above `path`, the top included.
fn attribute_files(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').collect();
    (0..parts.len())
        .map(|depth| match parts[..depth].join("/") {
            folder if folder.is_empty() => ".gitattributes".to_owned(),
            folder => format!("{folder}/.gitattributes"),
        })
        .collect()
}

/// `git` in `repo`, reading every pathspec as a literal path: a `*` or `[`
/// in a folder name is part of the name. `checkout` globs without it;
/// `ls-tree` reads paths literally anyway.
fn literal_git(repo: &Path) -> typed_git::GitCommand {
    let mut cmd = typed_git::git();
    cmd.dir(repo).env("GIT_LITERAL_PATHSPECS", "1");
    // Unit tests read no machine-wide git config.
    #[cfg(test)]
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    cmd
}

/// Whether `sha` holds a symlink at, above or under any of `paths`.
/// Non-recursively, a link above a path shows as itself, since `ls-tree`
/// cannot descend through it.
fn has_symlink(repo: &Path, sha: &str, paths: &[String]) -> Result<bool, String> {
    if paths.is_empty() {
        return Ok(false);
    }
    let chain: BTreeSet<String> = paths
        .iter()
        .flat_map(|path| {
            let parts: Vec<&str> = path.split('/').collect();
            (1..=parts.len()).map(move |depth| parts[..depth].join("/"))
        })
        .collect();
    let links = |recursive: bool, paths: &mut dyn Iterator<Item = &String>| {
        let mut cmd = literal_git(repo);
        cmd.args(&["ls-tree", "-z", "--full-tree"]);
        if recursive {
            cmd.arg("-r");
        }
        cmd.arg(sha).arg("--");
        for path in paths {
            cmd.arg(path);
        }
        cmd.run()
            .map(|listing| {
                listing
                    .split('\0')
                    .any(|entry| entry.starts_with("120000 "))
            })
            .map_err(|err| err.to_string())
    };
    Ok(links(false, &mut chain.iter())? || links(true, &mut paths.iter())?)
}

/// The `candidates` that `sha` holds, as a file or a folder, in order.
fn paths_at_ref(repo: &Path, sha: &str, candidates: &[String]) -> Result<Vec<String>, String> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let mut cmd = literal_git(repo);
    cmd.args(&["ls-tree", "-z", "--name-only", "--full-tree"])
        .arg(sha)
        .arg("--");
    for candidate in candidates {
        cmd.arg(candidate);
    }
    let listing = cmd.run().map_err(|err| err.to_string())?;
    let held: Vec<&str> = listing.split('\0').collect();
    // `ls-tree` lists the deepest match, so a folder may show only as the
    // entries under it.
    Ok(candidates
        .iter()
        .filter(|candidate| {
            held.iter()
                .any(|name| name == candidate || is_under(name, candidate))
        })
        .cloned()
        .collect())
}

/// Writes `pathspecs` as they are at `sha` into `dest`, which must exist.
/// Moves no `HEAD` and registers no worktree. An empty set runs nothing,
/// since a checkout with no pathspec switches `HEAD`.
fn check_out_paths(
    repo: &Path,
    sha: &str,
    index: &Path,
    dest: &Path,
    pathspecs: &[String],
) -> Result<(), String> {
    if pathspecs.is_empty() {
        return Ok(());
    }
    let mut cmd = literal_git(repo);
    // A private index, so the repository's own is never read or written.
    cmd.env("GIT_INDEX_FILE", index.to_string_lossy())
        // LFS files stay pointers.
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .args(&[
            "-c",
            // Runs none of the repository's hooks.
            "core.hooksPath=/dev/null",
            "-c",
            // `core.hooksPath` does not cover an fsmonitor command.
            "core.fsmonitor=false",
            "-c",
            // A split index writes its shared part into the repository's `.git`.
            "core.splitIndex=false",
            "-c",
            // A sparse repository does not filter the baseline.
            "core.sparseCheckout=false",
        ])
        .arg(format!("--work-tree={}", dest.to_string_lossy()))
        .args(&["checkout", "--quiet"])
        .arg(sha)
        .arg("--");
    for path in pathspecs {
        cmd.arg(path);
    }
    cmd.run().map(drop).map_err(|err| err.to_string())
}

/// Moves each harness's packs in a baseline built under `base` to where
/// `current` keeps them, so a `packs` setting that changed since the ref
/// still compares each plugin with itself.
fn align_packs(
    out: &Path,
    base: &MarketplaceConfig,
    current: &MarketplaceConfig,
) -> std::io::Result<()> {
    let side = out.join(".moving");
    let moved: Vec<(&str, &str, PathBuf)> = with_plugin_format()
        .filter_map(|spec| {
            let from = packs_path(base, spec.name)?;
            let to = packs_path(current, spec.name)?;
            (from != to && out.join(from).is_dir()).then(|| (from, to, side.join(spec.name)))
        })
        .collect();
    // Through a side folder: one harness's new folder can be another's old one.
    for (from, _, aside) in &moved {
        std::fs::create_dir_all(&side)?;
        std::fs::rename(out.join(from), aside)?;
    }
    for (_, to, aside) in &moved {
        let target = out.join(to);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(aside, target)?;
    }
    if !moved.is_empty() {
        std::fs::remove_dir(&side)?;
    }
    Ok(())
}

/// Claims a baseline under `baselines`: an `<id>.lock`, created and locked.
/// [`GUARD_FILE`] is held across the prune and the claim, so one run's prune
/// never takes a lock another run has created and not yet locked.
///
/// # Errors
///
/// What failed, as the line to print.
fn claim_baseline(
    baselines: &Path,
    repo: &Path,
) -> Result<(String, PathBuf, std::fs::File), String> {
    std::fs::create_dir_all(baselines)
        .map_err(|err| format!("cannot create {}: {err}", baselines.display()))?;
    let guard_path = baselines.join(GUARD_FILE);
    let guard = std::fs::File::create(&guard_path)
        .and_then(|file| file.lock().map(|()| file))
        .map_err(|err| format!("cannot lock {}: {err}", guard_path.display()))?;
    prune_stale_baselines(baselines, repo);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let claim = NEXT_CLAIM.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let id = format!("{}-{stamp}-{claim}", std::process::id());
    let lock_path = baselines.join(format!("{id}.lock"));
    let lock = std::fs::File::create(&lock_path)
        .and_then(|file| {
            file.try_lock().map_err(std::io::Error::from)?;
            Ok(file)
        })
        .map_err(|err| format!("cannot lock {}: {err}", lock_path.display()))?;
    drop(guard);
    Ok((id, lock_path, lock))
}

/// Remove the baselines a killed run left behind: each `<id>.lock` no live
/// run holds, its `<id>/` folder and, for one an older version built in a
/// worktree, that worktree's registration.
fn prune_stale_baselines(baselines: &Path, repo: &Path) {
    let Ok(entries) = std::fs::read_dir(baselines) else {
        return;
    };
    let mut pruned = false;
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.extension().is_none_or(|ext| ext != "lock") {
            continue;
        }
        let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path) else {
            continue;
        };
        if file.try_lock().is_err() {
            continue;
        }
        let dir = path.with_extension("");
        // Fails on a checkout that is not a worktree, as every new one is;
        // `remove_dir_all` does the work then.
        let _ = typed_git::worktree::remove(repo, &dir.join("checkout"));
        let _ = std::fs::remove_dir_all(&dir);
        drop(file);
        let _ = std::fs::remove_file(&path);
        pruned = true;
    }
    if pruned {
        prune_worktrees(repo);
    }
}

/// `git worktree prune`: drop the registration of every worktree whose
/// folder is gone. Best-effort.
fn prune_worktrees(repo: &Path) {
    let _ = typed_git::git()
        .dir(repo)
        .args(&["worktree", "prune"])
        .run();
}

impl Drop for BuiltRef {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.temp);
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One run's prune never takes the lock of a claim in progress.
    #[test]
    fn concurrent_claims_never_fail_on_each_others_lock() {
        let dir = tempfile::tempdir().unwrap();
        let baselines = dir.path().join("baselines");
        let failures: usize = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        (0..200)
                            .filter(|_| match claim_baseline(&baselines, dir.path()) {
                                Ok((_, path, lock)) => {
                                    let _ = std::fs::remove_file(path);
                                    drop(lock);
                                    false
                                }
                                Err(_) => true,
                            })
                            .count()
                    })
                })
                .collect();
            runs.into_iter().map(|run| run.join().unwrap()).sum()
        });
        assert_eq!(failures, 0);
    }

    /// Runs `git` in `repo` with no system or global config and a fixed
    /// identity. Its stdout.
    fn git(repo: &Path, args: &[&str]) -> String {
        let mut cmd = typed_git::git();
        cmd.dir(repo)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "T")
            .env("GIT_AUTHOR_EMAIL", "t@t.invalid")
            .env("GIT_COMMITTER_NAME", "T")
            .env("GIT_COMMITTER_EMAIL", "t@t.invalid");
        cmd.run().unwrap()
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A canonical temp folder holding a repository of `files`, committed
    /// once, and that commit's sha.
    fn repo_of(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf, String) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().canonicalize().unwrap();
        for (path, text) in files {
            write(&repo.join(path), text);
        }
        git(&repo, &["init", "--quiet", "--initial-branch", "main"]);
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "--quiet", "-m", "one"]);
        let sha = git(&repo, &["rev-parse", "HEAD"]);
        (dir, repo, sha)
    }

    /// Every file under `dir`, relative and forward-slashed, sorted.
    fn files_under(dir: &Path) -> Vec<String> {
        fn walk(dir: &Path, base: &Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, base, found);
                } else {
                    let rel = path.strip_prefix(base).unwrap();
                    found.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let mut found = Vec::new();
        walk(dir, dir, &mut found);
        found.sort();
        found
    }

    fn strings(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| (*p).to_owned()).collect()
    }

    /// The run `cwd` resolves to, as `bump` resolves it.
    fn run_at(cwd: &Path) -> ResolvedRun {
        let mut err = Vec::new();
        resolve_run(&CommonFlags::default(), None, cwd, &mut err)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&err)))
    }

    /// A fresh `checkout` folder under a new temp folder, and the private
    /// index path beside it.
    fn dest() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("checkout");
        std::fs::create_dir(&dest).unwrap();
        let index = temp.path().join("index");
        (temp, dest, index)
    }

    #[test]
    fn check_out_paths_writes_only_the_named_paths() {
        let (_dir, repo, old) = repo_of(&[("a/x", "x\n"), ("b/y", "y\n"), ("c", "c\n")]);
        // The checkout is of an older commit than the index holds, so
        // writing it into the repository's own index would change it.
        std::fs::write(repo.join("a/x"), "x2\n").unwrap();
        git(&repo, &["commit", "--quiet", "-am", "two"]);
        // A modified tracked file: the repository's own index must not move.
        std::fs::write(repo.join("c"), "changed\n").unwrap();
        // The index bytes are read before any `git status` can refresh them.
        let index_before = std::fs::read(repo.join(".git/index")).unwrap();
        let config_before = std::fs::read(repo.join(".git/config")).unwrap();
        let worktrees_before = git(&repo, &["worktree", "list", "--porcelain"]);
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &old, &index, &dest, &strings(&["a"])).unwrap();

        assert_eq!(
            std::fs::read(repo.join(".git/index")).unwrap(),
            index_before
        );
        assert_eq!(files_under(&dest), ["a/x"]);
        assert_eq!(std::fs::read_to_string(dest.join("a/x")).unwrap(), "x\n");
        assert_eq!(
            std::fs::read(repo.join(".git/config")).unwrap(),
            config_before
        );
        assert_eq!(
            git(&repo, &["worktree", "list", "--porcelain"]),
            worktrees_before
        );
        assert_eq!(git(&repo, &["status", "--porcelain"]), "M c");
    }

    #[cfg(unix)]
    #[test]
    fn check_out_paths_runs_no_hook() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, repo, sha) = repo_of(&[("a/x", "x\n")]);
        let marker = repo.join("hook-ran");
        let hook = repo.join(".git/hooks/post-checkout");
        write(&hook, &format!("#!/bin/sh\ntouch '{}'\n", marker.display()));
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &sha, &index, &dest, &strings(&["a"])).unwrap();

        assert_eq!(files_under(&dest), ["a/x"]);
        assert!(!marker.exists(), "the post-checkout hook ran");
    }

    #[cfg(unix)]
    #[test]
    fn check_out_paths_runs_no_fsmonitor() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, repo, sha) = repo_of(&[("a/x", "x\n")]);
        let marker = repo.join("fsmonitor-ran");
        let script = repo.join("fsmonitor.sh");
        write(
            &script,
            &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        );
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        git(
            &repo,
            &["config", "core.fsmonitor", &script.to_string_lossy()],
        );
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &sha, &index, &dest, &strings(&["a"])).unwrap();

        assert_eq!(files_under(&dest), ["a/x"]);
        assert!(!marker.exists(), "the fsmonitor hook ran");
    }

    #[test]
    fn check_out_paths_writes_nothing_into_a_split_index_repository() {
        let (_dir, repo, sha) = repo_of(&[("a/x", "x\n")]);
        git(&repo, &["config", "core.splitIndex", "true"]);
        let listing = || {
            let mut names: Vec<String> = std::fs::read_dir(repo.join(".git"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        let before = listing();
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &sha, &index, &dest, &strings(&["a"])).unwrap();

        assert_eq!(listing(), before);
    }

    #[test]
    fn paths_at_ref_keeps_only_what_the_ref_holds() {
        // `v[1]` as a glob would match the file `v1`; `ls-tree` reads it as a
        // name, and so does the filter over its output.
        let (_dir, repo, sha) = repo_of(&[("a/x", "x\n"), ("c", "c\n"), ("v1", "1\n")]);
        let held = paths_at_ref(&repo, &sha, &strings(&["c", "a", "missing", "v[1]"])).unwrap();
        assert_eq!(held, ["c", "a"]);
        let (_dir, repo, sha) = repo_of(&[("v[1]/f", "f\n"), ("v1", "1\n")]);
        let held = paths_at_ref(&repo, &sha, &strings(&["v[1]"])).unwrap();
        assert_eq!(held, ["v[1]"]);
    }

    #[test]
    fn an_empty_set_runs_no_checkout() {
        let (_dir, repo, sha) = repo_of(&[("c", "c\n")]);
        let head = || {
            (
                git(&repo, &["rev-parse", "HEAD"]),
                git(&repo, &["symbolic-ref", "HEAD"]),
            )
        };
        let before = head();
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &sha, &index, &dest, &[]).unwrap();

        assert_eq!(head(), before);
        assert!(files_under(&dest).is_empty());
    }

    /// A v2 vault: the root file, one marketplace and one plugin, under
    /// `prefix`, with `root` as the root file.
    fn vault_files(prefix: &str, root: &str) -> Vec<(String, String)> {
        vec![
            (format!("{prefix}skillwright.md"), root.to_owned()),
            (
                format!("{prefix}marketplaces/m1/m1.md"),
                "---\n---\n".to_owned(),
            ),
            (
                format!("{prefix}marketplaces/m1/plugins/p/p.md"),
                "---\nversion: 1.0.0\n---\n".to_owned(),
            ),
            (
                format!("{prefix}marketplaces/m1/plugins/p/skills/s/s.md"),
                "---\ndescription: S.\n---\n".to_owned(),
            ),
        ]
    }

    fn borrowed(files: &[(String, String)]) -> Vec<(&str, &str)> {
        files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect()
    }

    #[test]
    fn a_vault_at_the_repo_top_checks_out_only_what_the_build_reads() {
        let mut files = vault_files("", "---\n---\n");
        files.push(("unread/file".to_owned(), "u\n".to_owned()));
        let (_dir, repo, sha) = repo_of(&borrowed(&files));
        let set = path_set(&repo, &sha, vault_candidates(&repo, &repo)).unwrap();
        assert_eq!(set, ["marketplaces", "skillwright.md"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_folder_above_the_vault_gives_the_whole_tree() {
        let files = vault_files("real/docs/vault/", "---\n---\n");
        let (_dir, repo, _) = repo_of(&borrowed(&files));
        std::os::unix::fs::symlink("real/docs", repo.join("docs")).unwrap();
        git(&repo, &["add", "docs"]);
        git(&repo, &["commit", "--quiet", "-m", "link"]);
        let sha = git(&repo, &["rev-parse", "HEAD"]);

        let root = repo.join("docs/vault");
        let set = path_set(&repo, &sha, vault_candidates(&repo, &root)).unwrap();
        assert_eq!(set, ["."]);
    }

    #[test]
    fn the_path_set_holds_ancestor_gitattributes() {
        let mut files = vault_files("sub/vault/", "---\n---\n");
        for path in [
            ".gitattributes",
            "sub/.gitattributes",
            "other/.gitattributes",
        ] {
            files.push((path.to_owned(), "*.md text\n".to_owned()));
        }
        let (_dir, repo, sha) = repo_of(&borrowed(&files));
        let root = repo.join("sub/vault");
        let set = path_set(&repo, &sha, vault_candidates(&repo, &root)).unwrap();
        assert_eq!(
            set,
            [
                ".gitattributes",
                "sub/.gitattributes",
                "sub/vault/marketplaces",
                "sub/vault/skillwright.md"
            ]
        );
    }

    #[test]
    fn the_baseline_path_set_names_the_vault_assets_and_hooks() {
        let outside = tempfile::tempdir().unwrap();
        let elsewhere = outside.path().canonicalize().unwrap().join("ELSEWHERE");
        write(&elsewhere, "x\n");
        let root_file = format!(
            "---\nbuild:\n  assets:\n    \
             - {{from: NOTICE, to: NOTICE, plugin: p}}\n    \
             - {{from: ../tools/run.sh, to: run.sh, plugin: p}}\n    \
             - {{from: {}, to: ELSEWHERE, plugin: p}}\n  hooks:\n    \
             - {{from: ../hooks/hooks.json, plugin: p}}\n---\n",
            elsewhere.display()
        );
        let mut files = vault_files("vault/", &root_file);
        for (path, text) in [
            ("vault/NOTICE", "n\n"),
            ("tools/run.sh", "echo\n"),
            ("hooks/hooks.json", "{\"hooks\": {}}\n"),
            ("unread/file", "u\n"),
        ] {
            files.push((path.to_owned(), text.to_owned()));
        }
        let (_dir, repo, sha) = repo_of(&borrowed(&files));
        let root = repo.join("vault");
        let run = run_at(&root);

        let mut set = path_set(&repo, &sha, vault_candidates(&repo, &root)).unwrap();
        set.extend(path_set(&repo, &sha, build_input_candidates(&repo, &run)).unwrap());
        set.sort();
        assert_eq!(
            set,
            [
                "hooks/hooks.json",
                "tools/run.sh",
                "vault/NOTICE",
                "vault/marketplaces",
                "vault/skillwright.md",
            ]
        );
    }

    #[test]
    fn a_path_with_glob_characters_is_checked_out_literally() {
        // As a glob, `v[1]` also matches the file `v1`.
        let (_dir, repo, sha) = repo_of(&[("v[1]/f", "f\n"), ("v1", "1\n")]);
        let (_temp, dest, index) = dest();

        check_out_paths(&repo, &sha, &index, &dest, &strings(&["v[1]"])).unwrap();

        assert_eq!(files_under(&dest), ["v[1]/f"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_inside_an_asset_folder_gives_the_whole_tree() {
        let mut files = vault_files(
            "vault/",
            "---\nbuild:\n  assets:\n    - {from: ../tools, to: tools, plugin: p}\n---\n",
        );
        files.push(("tools/run.sh".to_owned(), "echo\n".to_owned()));
        files.push(("elsewhere/f".to_owned(), "f\n".to_owned()));
        let (_dir, repo, _) = repo_of(&borrowed(&files));
        std::os::unix::fs::symlink("../elsewhere/f", repo.join("tools/link")).unwrap();
        git(&repo, &["add", "tools/link"]);
        git(&repo, &["commit", "--quiet", "-m", "link"]);
        let sha = git(&repo, &["rev-parse", "HEAD"]);
        let run = run_at(&repo.join("vault"));

        let set = path_set(&repo, &sha, build_input_candidates(&repo, &run)).unwrap();
        assert_eq!(set, ["."]);
    }

    const NOW: u64 = 20_261_105_143_000;

    #[test]
    fn a_prune_removes_only_the_baselines_no_run_holds() {
        let tmp = tempfile::tempdir().unwrap();
        let baselines = tmp.path().join("baselines");
        for id in ["stale", "live"] {
            std::fs::create_dir_all(baselines.join(id).join("out")).unwrap();
        }
        std::fs::write(baselines.join("stale.lock"), "").unwrap();
        let live = std::fs::File::create(baselines.join("live.lock")).unwrap();
        live.try_lock().unwrap();

        prune_stale_baselines(&baselines, tmp.path());

        assert!(!baselines.join("stale").exists());
        assert!(!baselines.join("stale.lock").exists());
        assert!(baselines.join("live").exists());
        assert!(baselines.join("live.lock").exists());
    }

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
