//! `skillwright build` and `skillwright check` — each marketplace rendered
//! into `<root>/dist/<m>/`, and the drift check over it.
//!
//! One function for both verbs, because in the core they are one path: a
//! single render into a capturing overlay, compared once against the
//! destination. `build` acts on that comparison and `check` returns it. This
//! file's whole contribution is which mode goes on the request, and what a
//! drifted report means for the exit code.
//!
//! Nothing about the marketplace is decided here. Which skills are
//! publishable, how they group into plugins, what each manifest says, which
//! root a skill's `targets` admits it to, what counts as stale, and
//! whether the swap is safe all live in [`tradecraft_core::export`]. This
//! file calls it once per marketplace and renders.
//!
//! # `--dry-run` on `build` is not `check`
//!
//! Both write nothing, and they answer different questions. The CLI spec
//! fixes `--dry-run` as "classify and report; write nothing — the exit code
//! is what the real run would have returned", so `build --dry-run` reports
//! the drift and exits 0, exactly as the build it previews would. `check` is
//! the CI verb: the same render, the same report, and exit 1 on any drift.

use tradecraft_core::{
    ExportMode, ExportRequest, ExportUnit, VaultMarketplace, export_skills, owned_paths,
};

use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_REFUSED, blocked};
use crate::prompt::{confirm_write, plan_flow};
use crate::report::{
    count_checked_units, normalize_thrown_reason, render_export_tally, render_unit_line,
    render_warning_line,
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
    /// The global `--vault` and `--namespace` flags.
    pub common: CommonFlags,
    /// `--json`: print the report as one JSON document.
    pub json: bool,
    /// `--dry-run`: classify and report; write nothing. `build` only.
    pub dry_run: bool,
    /// `--yes`: accept the plan without prompting. `build` only.
    pub yes: bool,
    /// `--quiet`: drop progress and the tally from stderr.
    pub quiet: bool,
}

/// Run one export. `verb` decides the mode a plain run uses and what a
/// drifted report exits with; everything else about the two verbs is
/// identical.
///
/// Each marketplace is built in turn into its own `dist/<m>/`, or only the
/// one `--marketplace` names. A build asks once, for every marketplace,
/// before it writes any. The run exits with the first failure's code, after
/// trying every marketplace.
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
    let requests: Vec<(&VaultMarketplace, ExportRequest)> = run
        .marketplaces()
        .iter()
        .map(|market| (market, run.loaded.export_request(market, mode)))
        .collect();

    if writes && !flags.yes {
        let gate = confirm_build(&run, &requests, ctx);
        if gate != EXIT_OK {
            return gate;
        }
    }
    if flags.dry_run {
        let _ = writeln!(
            ctx.err,
            "dry run — reporting the drift a build would act on; nothing will be written"
        );
    }

    // `bump` needs `--marketplace` only when the vault holds several, but
    // naming it always runs.
    let name_market = flags.marketplace.is_some() || requests.len() > 1;
    let mut exit = EXIT_OK;
    for (market, request) in &requests {
        if requests.len() > 1 && !flags.quiet {
            let _ = writeln!(ctx.err, "marketplace {}:", market.name);
        }
        let advise = name_market.then(|| market.name.clone());
        let code = export_one(verb, flags, &run, request, advise, ctx);
        if exit == EXIT_OK {
            exit = code;
        }
    }
    exit
}

/// Ask before a build replaces what it owns in each `dist/<m>/`.
fn confirm_build(
    run: &ResolvedRun,
    requests: &[(&VaultMarketplace, ExportRequest)],
    ctx: &mut CommandContext<'_>,
) -> i32 {
    let mut plan = vec!["build the marketplace".to_string(), String::new()];
    for (_, request) in requests {
        // The harnesses are named, and the paths each one replaces with them.
        // The list comes from `owned_paths`, not from a copy of the table here.
        let owned = match owned_paths(request) {
            Ok(owned) => owned,
            Err(err) => {
                return blocked(
                    ctx.err,
                    &format!(
                        "build failed: {}",
                        normalize_thrown_reason(&err.to_string())
                    ),
                );
            }
        };
        // The paths are padded to each other so the harness names line up
        // after them, the way `plan_flow` lines the labels up against each
        // other.
        let width = owned.iter().map(|(_, path)| path.len()).max().unwrap_or(0);
        plan.extend(plan_flow(&[
            ("from vault", vec![run.vault.display().to_string()]),
            ("into", vec![request.out_root.display().to_string()]),
            (
                "replacing",
                owned
                    .iter()
                    .map(|(name, path)| format!("{path:width$}  {name}"))
                    .collect(),
            ),
        ]));
        plan.push(String::new());
    }
    plan.push("Each of those is replaced whole, and what the vault no longer".to_string());
    plan.push("produces is removed. Nothing else in that directory is touched.".to_string());
    confirm_write("build", &plan, ctx.err)
}

/// [`run_export`] over one marketplace. `advise` is the `--marketplace` a
/// `bump` advised by `check` names.
fn export_one(
    verb: ExportVerb,
    flags: &ExportFlags,
    run: &ResolvedRun,
    request: &ExportRequest,
    advise: Option<String>,
    ctx: &mut CommandContext<'_>,
) -> i32 {
    // --- run ----------------------------------------------------------------
    let report = match export_skills(&tradecraft_core::NativeVfs, request, run.core_namespace()) {
        Ok(report) => report,
        // An export has no partial report: its rails fire before anything is
        // staged, and a failed swap rolls back. So a failure means nothing
        // was written and there is no report to print — a run-level blocker.
        Err(err) => {
            let message = err.to_string();
            return blocked(
                ctx.err,
                &format!(
                    "{} failed: {}",
                    verb.as_str(),
                    normalize_thrown_reason(&message)
                ),
            );
        }
    };

    if flags.json {
        match serde_json::to_string_pretty(&report) {
            Ok(document) => {
                let _ = writeln!(ctx.out, "{document}");
            }
            Err(err) => return blocked(ctx.err, &format!("the report would not serialize: {err}")),
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
