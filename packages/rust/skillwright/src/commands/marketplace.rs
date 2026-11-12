//! `skillwright build` and `skillwright check` — each marketplace rendered
//! into `<root>/dist/<m>/`, and the drift check over it.
//!
//! One function for both verbs, because in the core they are one path: a
//! single render, compared once against `dist/<m>/`. `build` acts on that
//! comparison and `check` returns it. This file's whole contribution is which
//! mode goes on the request, and what a drifted report means for the exit
//! code.
//!
//! Nothing about the marketplace is decided here. Which skills ship, how they
//! group into plugins, what each manifest says, which harnesses are built,
//! what counts as stale, and whether the swap is safe
//! all live in [`tradecraft_core::export`]. This file has the core plan every
//! marketplace once, asks the user, has the core commit the plan, and renders
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
    BuildOptions, ExportMode, ExportReport, ExportUnit, LoadedVault, NativeVfs, owned_paths, plan,
};

use crate::ctx::CommandContext;
use crate::exit::{EXIT_DRIFT, EXIT_OK, EXIT_REFUSED, failed};
use crate::prompt::{confirm_write, plan_flow};
use crate::report::{
    count_checked_units, dry_run_notice, print_json, render_export_tally, render_unit_line,
    render_warning_line,
};
use crate::resolve::{CommonFlags, resolve_run};

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
/// then asks once, and writes every one or none. The run exits 2 when any
/// marketplace was refused, else 1 when a check found drift.
pub fn run_export(verb: ExportVerb, flags: &ExportFlags, ctx: &mut CommandContext<'_>) -> i32 {
    let loaded = match resolve_run(
        &flags.common,
        flags.marketplace.as_deref(),
        &ctx.cwd,
        ctx.err,
    ) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };

    // `check` never writes; `build --dry-run` renders through the same
    // write-nothing mode and differs only in what it exits with (see the
    // module doc). So the only run that reaches `ExportMode::Build` is a
    // plain `build`.
    let writes = verb == ExportVerb::Build && !flags.dry_run;
    let planned = match plan(&NativeVfs, &loaded, &build_options(writes, flags)) {
        Ok(planned) => planned,
        Err(err) => return failed(ctx.err, verb.as_str(), &err.reason()),
    };
    let outcomes = planned.outcomes();
    let several = loaded.marketplaces.len() > 1;
    // `bump` needs `--marketplace` only when the vault holds several, but
    // naming it always runs.
    let name_market = flags.marketplace.is_some() || several;
    let header = |name: &str, ctx: &mut CommandContext<'_>| {
        if several && !flags.quiet {
            let _ = writeln!(ctx.err, "marketplace {name}:");
        }
    };

    // A build writes nothing when any marketplace failed.
    if writes && outcomes.iter().any(Result::is_err) {
        for refusal in outcomes.iter().filter_map(|outcome| outcome.as_ref().err()) {
            header(&refusal.marketplace, ctx);
            failed(ctx.err, verb.as_str(), &refusal.reason());
        }
        let _ = writeln!(ctx.err, "Nothing was written.");
        return EXIT_REFUSED;
    }
    if writes && !flags.yes {
        let gate = confirm_build(&loaded, flags, ctx);
        if gate != EXIT_OK {
            return gate;
        }
    }
    if flags.dry_run && !flags.quiet {
        dry_run_notice(ctx, "the drift a build would act on");
    }

    // With several marketplaces, `--json` prints one document, the core's
    // `VaultReport`.
    let document = (flags.json && several).then(|| planned.report());
    if let Err(err) = planned.commit(&NativeVfs) {
        return failed(ctx.err, verb.as_str(), &err.reason());
    }
    let mut exit = EXIT_OK;
    for outcome in outcomes {
        let code = match outcome {
            Err(refusal) => {
                header(&refusal.marketplace, ctx);
                failed(ctx.err, verb.as_str(), &refusal.reason())
            }
            Ok(market) => {
                header(&market.marketplace, ctx);
                let advise = name_market.then(|| market.marketplace.clone());
                export_one(verb, flags, &market.report, advise, document.is_none(), ctx)
            }
        };
        // A refusal (2) outranks drift (1): exit 1 means drift and nothing else.
        exit = exit.max(code);
    }
    if let Some(document) = document
        && let Err(code) = print_json(ctx, &document)
    {
        exit = exit.max(code);
    }
    exit
}

/// The core's options for this run: a build only when it `writes`.
fn build_options(writes: bool, flags: &ExportFlags) -> BuildOptions {
    let mut options = BuildOptions::default();
    options.harnesses.clone_from(&flags.harnesses);
    options.mode = if writes {
        ExportMode::Build
    } else {
        ExportMode::Check
    };
    options
}

/// Ask before a build replaces what it owns in each `dist/<m>/`.
fn confirm_build(loaded: &LoadedVault, flags: &ExportFlags, ctx: &mut CommandContext<'_>) -> i32 {
    // The harnesses are named, and the paths each one replaces with them.
    // The list comes from the core, not from a copy of the table here. The
    // plan before this one has already refused an unknown harness.
    let owned = owned_paths(&loaded.settings, &flags.harnesses).unwrap_or_default();
    // The paths are padded to each other so the harness names line up after
    // them, the way `plan_flow` lines the labels up against each other.
    let width = owned.iter().map(|(_, path)| path.len()).max().unwrap_or(0);
    let mut lines = vec!["build the marketplace".to_string(), String::new()];
    for market in &loaded.marketplaces {
        lines.extend(plan_flow(&[
            ("from vault", vec![loaded.root.display().to_string()]),
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
            render_export_tally(&report.units, report.mode)
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
        EXIT_DRIFT
    } else {
        EXIT_OK
    }
}
