//! `skillwright build` and `skillwright check` — each marketplace rendered
//! into `<root>/dist/<m>/`, and the drift check over it.
//!
//! One function for both verbs, because in the core they are one path: a
//! single render into a capturing overlay, compared once against
//! `dist/<m>/`. `build` acts on that comparison and `check` returns it. This
//! file's whole contribution is which mode goes on the request, and what a
//! drifted report means for the exit code.
//!
//! Nothing about the marketplace is decided here. Which skills ship, how they
//! group into plugins, what each manifest says, which harness a skill's
//! `targets` admits it to, what counts as stale, and whether the swap is safe
//! all live in [`tradecraft_core::export`]. This file has the core plan every
//! marketplace once, asks the user, has the core write the plan, and renders
//! the report.
//!
//! # `--dry-run` on `build` is not `check`
//!
//! Both write nothing, and they answer different questions. The CLI spec
//! fixes `--dry-run` as "classify and report; write nothing — the exit code
//! is what the real run would have returned", so `build --dry-run` reports
//! the drift and exits 0, exactly as the build it previews would. `check` is
//! the CI verb: the same render, the same report, and exit 1 on any drift.

use std::collections::BTreeSet;

use tradecraft_core::{
    CoreError, ExportMode, ExportReport, ExportUnit, NativeVfs, VaultPlan, plan_loaded,
};

use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_REFUSED, blocked};
use crate::prompt::{confirm_write, plan_flow};
use crate::report::{
    count_checked_units, normalize_thrown_reason, print_json, render_export_tally,
    render_unit_line, render_warning_line,
};
use crate::resolve::{CommonFlags, ResolvedRun, resolve_run};

/// Which of the two verbs this run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportVerb {
    /// Write the roots.
    Build,
    /// Report drift against the roots; write nothing.
    Check,
}

impl ExportVerb {
    fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Check => "check",
        }
    }
}

/// The resolved flags of `skillwright build and check`.
pub struct ExportFlags {
    /// Which marketplace to build. Absent means every one.
    pub marketplace: Option<String>,
    /// The global `--vault` flag.
    pub common: CommonFlags,
    /// `--json`: print the report as one JSON document.
    pub json: bool,
    /// `--dry-run`: classify and report; write nothing. `build` only.
    pub dry_run: bool,
    /// `--yes`: accept the plan without prompting. `build` only.
    pub yes: bool,
    /// `--quiet`: drop progress and the tally from stderr.
    pub quiet: bool,
    /// `--harness`: the harnesses to build. Empty means the root file's
    /// `build.harnesses`.
    pub harnesses: BTreeSet<String>,
}

/// Run one export. `verb` decides the mode a plain run uses and what a
/// drifted report exits with; everything else about the two verbs is
/// identical.
///
/// Each marketplace is built into its own `dist/<m>/`, or only the one
/// `--marketplace` names. The core plans every marketplace first; a build
/// then asks once, and writes every one or none. The run exits with the
/// first failure's code.
pub fn run_export(verb: ExportVerb, flags: &ExportFlags, ctx: &mut CommandContext<'_>) -> i32 {
    let run = match resolve_run(
        &flags.common,
        flags.marketplace.as_deref(),
        &ctx.cwd,
        ctx.err,
    ) {
        Ok(run) => run,
        Err(code) => return code,
    };

    // `check` never writes; `build --dry-run` renders through the same
    // write-nothing mode and differs only in what it exits with (see the
    // module doc). So the only run that reaches `ExportMode::Build` is a
    // plain `build`.
    let writes = verb == ExportVerb::Build && !flags.dry_run;
    let mode = if writes {
        ExportMode::Build
    } else {
        ExportMode::Check
    };
    let plan = plan_loaded(&NativeVfs, &run.loaded, mode, &flags.harnesses);
    let several = plan.marketplaces.len() > 1;
    // `bump` needs `--marketplace` only when the vault holds several, but
    // naming it always runs.
    let name_market = flags.marketplace.is_some() || several;
    let header = |name: &str, ctx: &mut CommandContext<'_>| {
        if several && !flags.quiet {
            let _ = writeln!(ctx.err, "marketplace {name}:");
        }
    };

    // A build writes nothing when any marketplace failed.
    if writes && plan.failed() {
        let mut exit = EXIT_OK;
        for (market, planned) in &plan.marketplaces {
            if let Err(err) = planned {
                header(&market.name, ctx);
                let code = failed_run(verb, err, ctx);
                if exit == EXIT_OK {
                    exit = code;
                }
            }
        }
        let _ = writeln!(ctx.err, "Nothing was written.");
        return exit;
    }

    if writes && !flags.yes {
        let gate = confirm_build(&run, &plan, ctx);
        if gate != EXIT_OK {
            return gate;
        }
    }
    if flags.dry_run && !flags.quiet {
        let _ = writeln!(
            ctx.err,
            "dry run — reporting the drift a build would act on; nothing will be written"
        );
    }

    let reports: Vec<(String, Result<ExportReport, CoreError>)> = if writes {
        match plan.write(&NativeVfs) {
            Ok(report) => report
                .marketplaces
                .into_iter()
                .map(|m| (m.marketplace, Ok(m.report)))
                .collect(),
            Err(err) => return failed_run(verb, &err, ctx),
        }
    } else {
        plan.marketplaces
            .into_iter()
            .map(|(market, planned)| {
                (
                    market.name,
                    planned.and_then(|planned| planned.write(&NativeVfs)),
                )
            })
            .collect()
    };

    // With several marketplaces, `--json` gathers every report into one
    // document, the shape of the core's `VaultReport`.
    let mut document = (flags.json && several).then(VaultDocument::default);
    let mut exit = EXIT_OK;
    for (name, report) in reports {
        header(&name, ctx);
        let code = match report {
            Err(err) => {
                if let Some(document) = &mut document {
                    document.refused.push(Refusal {
                        marketplace: name.clone(),
                        error: err.to_string(),
                    });
                }
                failed_run(verb, &err, ctx)
            }
            Ok(report) => {
                let advise = name_market.then(|| name.clone());
                let code = export_one(verb, flags, &report, advise, document.is_none(), ctx);
                if let Some(document) = &mut document {
                    document.marketplaces.push(Market {
                        marketplace: name,
                        report,
                    });
                }
                code
            }
        };
        if exit == EXIT_OK {
            exit = code;
        }
    }
    if let Some(document) = document
        && let Err(code) = print_json(ctx, &document)
        && exit == EXIT_OK
    {
        exit = code;
    }
    exit
}

/// `--json`'s one document over several marketplaces.
#[derive(Default, serde::Serialize)]
struct VaultDocument {
    marketplaces: Vec<Market>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    refused: Vec<Refusal>,
}

#[derive(serde::Serialize)]
struct Market {
    marketplace: String,
    report: ExportReport,
}

#[derive(serde::Serialize)]
struct Refusal {
    marketplace: String,
    error: String,
}

/// Report a run-level failure: an export has no partial report, since its
/// checks run before anything is staged and a failed swap rolls back.
fn failed_run(verb: ExportVerb, err: &CoreError, ctx: &mut CommandContext<'_>) -> i32 {
    blocked(
        ctx.err,
        &format!(
            "{} failed: {}",
            verb.as_str(),
            normalize_thrown_reason(&err.to_string())
        ),
    )
}

/// Ask before a build replaces what it owns in each `dist/<m>/`.
fn confirm_build(run: &ResolvedRun, plan: &VaultPlan, ctx: &mut CommandContext<'_>) -> i32 {
    let mut lines = vec!["build the marketplace".to_string(), String::new()];
    for (market, planned) in &plan.marketplaces {
        // The harnesses are named, and the paths each one replaces with them.
        // The list comes from the plan, not from a copy of the table here.
        let owned = planned
            .as_ref()
            .map(|planned| planned.owned_paths())
            .unwrap_or_default();
        // The paths are padded to each other so the harness names line up
        // after them, the way `plan_flow` lines the labels up against each
        // other.
        let width = owned.iter().map(|(_, path)| path.len()).max().unwrap_or(0);
        lines.extend(plan_flow(&[
            ("from vault", vec![run.vault.display().to_string()]),
            ("into", vec![market.dist.display().to_string()]),
            (
                "replacing",
                owned
                    .iter()
                    .map(|(name, path)| format!("{path:width$}  {name}"))
                    .collect(),
            ),
        ]));
        lines.push(String::new());
    }
    lines.push("Each of those is replaced whole, and what the vault no longer".to_string());
    lines.push("produces is removed. Nothing else in that directory is touched.".to_string());
    confirm_write("build", &lines, ctx.err)
}

/// Print one marketplace's report. `advise` is the `--marketplace` a `bump`
/// advised by `check` names. `own_json` is false when the report goes into
/// one document over several marketplaces instead.
fn export_one(
    verb: ExportVerb,
    flags: &ExportFlags,
    report: &ExportReport,
    advise: Option<String>,
    own_json: bool,
    ctx: &mut CommandContext<'_>,
) -> i32 {
    if flags.json {
        if own_json && let Err(code) = print_json(ctx, report) {
            return code;
        }
    } else {
        for unit in &report.units {
            let _ = writeln!(ctx.out, "{}", render_unit_line(unit, report.mode));
        }
    }

    // Stderr, and not dropped by `--quiet`: each names something published
    // that the author did not write.
    for warning in &report.warnings {
        let _ = writeln!(ctx.err, "{}", render_warning_line(warning));
    }

    if !flags.quiet {
        let suffix = if verb == ExportVerb::Build && flags.dry_run {
            " (dry run)"
        } else {
            ""
        };
        // The leading count and the tally both count units — a plugin row
        // per root plus a manifest row per root — so the two numbers always
        // agree. `report.checked` is a different count (skills discovered)
        // and belongs on the wire, not this line; see
        // [`crate::report::count_checked_units`].
        let _ = writeln!(
            ctx.err,
            "{} checked — {}{suffix}",
            count_checked_units(&report.units),
            render_export_tally(&report.units)
        );
    }

    // A plugin whose members drifted with its own `version` unmoved since
    // is a `check`-only callout — `build` just swapped the drift away, so
    // there is nothing left to suggest bumping. Stderr, never stdout: the
    // human table and `--json` already carry this fact (the report's own
    // `version_bump_suggestion` field), and stdout has to read identically
    // whether or not this advisory exists.
    if verb == ExportVerb::Check && !flags.json {
        let scope = advise
            .map(|name| format!(" --marketplace {name}"))
            .unwrap_or_default();
        for unit in &report.units {
            if let ExportUnit::Plugin {
                root,
                name,
                version_bump_suggestion: Some(vbs),
                ..
            } = unit
            {
                let _ = writeln!(
                    ctx.err,
                    "advisory: {root}/{name} version {} unmoved since members changed — `skillwright bump {name}{scope}` is available",
                    vbs.current_version
                );
            }
        }
    }

    // `check` is the CI verb and drift is what it exists to catch, so a
    // drifted marketplace exits 1 (the CLI spec). A build exits 0 whether it
    // swapped or found everything already in place: both are the run
    // completing and every outcome being written or unchanged.
    if verb == ExportVerb::Check && report.changed {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
}
