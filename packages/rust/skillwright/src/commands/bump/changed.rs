//! `bump --changed-since`: every plugin whose rendered members differ from a
//! baseline, with its own version unmoved, bumped together.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use tradecraft_core::{
    CoreError, DiscoveredComponent, ExportMode, ExportReport, ExportUnit, Harness, LoadedVault,
    NativeVfs, PatchScheme, UnitStatus, VaultReport, VaultRequest, build_vault, owned_paths,
};

use super::baseline::Baseline;
use super::{
    BumpFlags, JsonRow, Row, non_semver_reason, plan_bump, plugin_version_note, utc_now_stamp,
    write_bump,
};
use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_REFUSED, failed, refused};
use crate::prompt::confirm_write;
use crate::report::{dry_run_notice, print_json};

/// `--changed-since`'s `--json` document: the baseline, and one row per
/// plugin the render holds.
#[derive(Serialize)]
struct ChangedReport<'a> {
    baseline: &'a str,
    plugins: Vec<JsonRow<'a>>,
}

/// What the comparison against the baseline says about one plugin.
#[derive(Debug, Default)]
struct Verdict {
    /// Its members differ and its version has not moved, in some root.
    bump: bool,
    /// Anything of it differs, in some root.
    drift: bool,
    /// The harness roots the render holds it in.
    roots: Vec<Harness>,
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
    fn of(baseline: &Path, roots: &[Harness], plugin: &str, current: &str) -> Skip {
        let mut held = false;
        for root in roots {
            let format = root.format();
            let (packs, manifest) = (format.policy.packs, format.plugin_manifest);
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
fn holds_a_plugin_manifest(dir: &Path, roots: &[Harness]) -> bool {
    roots.iter().any(|root| {
        let format = root.format();
        let (packs, manifest) = (format.policy.packs, format.plugin_manifest);
        std::fs::read_dir(dir.join(packs)).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().join(manifest).is_file())
        })
    })
}

/// The one marketplace's report from a request for one, or what refused it.
fn one_report(report: Result<VaultReport, CoreError>) -> Result<ExportReport, String> {
    let mut report = report.map_err(|err| err.reason())?;
    if let Some(refusal) = report.refused.pop() {
        return Err(refusal.reason());
    }
    report
        .marketplaces
        .pop()
        .map(|m| m.report)
        .ok_or_else(|| "the build reported no marketplace".to_owned())
}

/// Run `bump --changed-since <since>` and return the exit code.
pub(super) fn run_changed(
    flags: &BumpFlags,
    ctx: &mut CommandContext<'_>,
    loaded: &LoadedVault,
    plugins: &[&DiscoveredComponent],
    since: &str,
    scheme: PatchScheme,
) -> i32 {
    let baseline = match Baseline::resolve(since, loaded, &flags.run.harnesses, ctx) {
        Ok(baseline) => baseline,
        Err(code) => return code,
    };
    let run = Changed {
        flags,
        loaded,
        plugins,
        since,
        scheme,
        now: utc_now_stamp(),
    };
    let mut rows: Vec<Row> = Vec::new();
    for market in &loaded.marketplaces {
        let found = run
            .compare(ctx, &baseline, &market.name)
            .and_then(|report| run.classify(ctx, &baseline, &market.name, &report));
        match found {
            Ok(found) => rows.extend(found),
            Err(code) => return code,
        }
    }
    drop(baseline);

    if let Err(code) = apply(flags, &mut rows, ctx) {
        return code;
    }
    if let Err(code) = print(flags, since, &rows, ctx) {
        return code;
    }
    if rows.iter().any(|r| r.status == "refused") {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
}

/// One `--changed-since` run: what it compares, and against what.
struct Changed<'a> {
    flags: &'a BumpFlags,
    loaded: &'a LoadedVault,
    plugins: &'a [&'a DiscoveredComponent],
    since: &'a str,
    scheme: PatchScheme,
    /// The UTC time a timestamp patch is written as.
    now: u64,
}

impl Changed<'_> {
    /// The check of `market` against its built tree in `baseline`.
    fn compare(
        &self,
        ctx: &mut CommandContext<'_>,
        baseline: &Baseline,
        market: &str,
    ) -> Result<ExportReport, i32> {
        let (flags, since) = (self.flags, self.since);
        let mut request = VaultRequest::for_marketplace(&self.loaded.root, market);
        request.options.mode = ExportMode::Check;
        request.options.harnesses.clone_from(&flags.run.harnesses);
        request.options.compare_with = Some(baseline.root(market));
        let mut roots: Vec<Harness> = match owned_paths(&self.loaded.settings, &flags.run.harnesses)
        {
            Ok(owned) => owned.into_iter().map(|(root, _)| root).collect(),
            Err(err) => return Err(refused(ctx.err, &err.reason())),
        };
        roots.dedup();
        if let Baseline::Dir(dir) = baseline
            && !holds_a_plugin_manifest(dir, &roots)
        {
            let expected: Vec<String> = roots
                .iter()
                .map(|root| {
                    let format = root.format();
                    format!(
                        "{}/<plugin>/{}",
                        format.policy.packs, format.plugin_manifest
                    )
                })
                .collect();
            return Err(refused(
                ctx.err,
                &format!(
                    "--changed-since `{since}`: {} holds no plugin manifest for any configured harness; a built marketplace holds one at {}",
                    dir.display(),
                    expected.join(" or ")
                ),
            ));
        }
        one_report(build_vault(&NativeVfs, &request))
            .map_err(|err| failed(ctx.err, &format!("comparing against `{since}`"), &err))
    }

    /// One row per plugin `report` holds for `market`, with the bump each
    /// would get.
    fn classify(
        &self,
        ctx: &mut CommandContext<'_>,
        baseline: &Baseline,
        market: &str,
        report: &ExportReport,
    ) -> Result<Vec<Row>, i32> {
        let mut rows = Vec::new();
        for (name, verdict) in verdicts(report) {
            let note = self
                .plugins
                .iter()
                .find(|c| c.position.name() == name && c.position.marketplace() == market);
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
            let note = plugin_version_note(note);
            let planned = match plan_bump(note, self.flags.level, self.scheme, self.now) {
                Ok(planned) => planned,
                Err(err) => return Err(failed(ctx.err, "bump", &err)),
            };
            // A note with no version key has no version to have moved, so any
            // drift is a change.
            let changed = verdict.bump || (verdict.drift && !planned.versioned);
            let row = if !verdict.drift {
                Row::plain(name, &planned, "unchanged", None)
            } else if !changed {
                let base = baseline.root(market);
                let skip = Skip::of(&base, &verdict.roots, name, &planned.current);
                let mut row = Row::plain(name, &planned, "skipped", Some(skip.token().to_owned()));
                row.explain = Some(skip.text());
                row
            } else if planned.new.is_none() {
                let reason = non_semver_reason(&planned.current);
                Row::refused(name, &planned.rel, &planned.current, reason)
            } else {
                Row::bump(name, planned)
            };
            rows.push(row);
        }
        Ok(rows)
    }
}

/// Each plugin of `report`, by name, with what its units say.
fn verdicts(report: &ExportReport) -> BTreeMap<&str, Verdict> {
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
            verdict.drift |= *status == UnitStatus::Differs;
            verdict.roots.push(*root);
        }
    }
    verdicts
}

/// Confirms the pending bumps unless `--yes` or `--dry-run`, then writes
/// each, or marks it `dry-run`. A failed write is reported on its row.
fn apply(flags: &BumpFlags, rows: &mut [Row], ctx: &mut CommandContext<'_>) -> Result<(), i32> {
    let pending: Vec<&Row> = rows.iter().filter(|r| r.planned.is_some()).collect();
    if !flags.run.dry_run && !flags.run.yes && !pending.is_empty() {
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
            return Err(gate);
        }
    }

    for row in rows {
        let Some(planned) = &row.planned else {
            continue;
        };
        let Some(new) = planned.new.clone() else {
            continue;
        };
        if flags.run.dry_run {
            row.status = "dry-run";
        } else {
            // A failed write is reported on its row; the bumps already
            // written still reach the report.
            if let Err(err) = write_bump(planned, &new) {
                row.status = "refused";
                row.reason = Some(format!("the write failed: {err}"));
                continue;
            }
            row.status = "written";
        }
        row.new = Some(new);
    }
    Ok(())
}

/// The report: the rows on stdout, and the dry-run notice and the tally on
/// stderr.
fn print(
    flags: &BumpFlags,
    since: &str,
    rows: &[Row],
    ctx: &mut CommandContext<'_>,
) -> Result<(), i32> {
    if flags.run.json {
        let document = ChangedReport {
            baseline: since,
            plugins: rows.iter().map(Row::json).collect(),
        };
        print_json(ctx, &document)?;
    } else {
        for row in rows {
            let _ = writeln!(ctx.out, "{}", row.line());
        }
    }
    if flags.run.dry_run && !flags.run.quiet {
        dry_run_notice(ctx, "the bumps this run would make");
    }
    if !flags.run.quiet {
        let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
        for row in rows {
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
    Ok(())
}
