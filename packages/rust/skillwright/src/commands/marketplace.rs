//! `skillwright build` and `skillwright check` — the vault rendered into its
//! standalone marketplace roots, and the drift check over them.
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
//! file resolves flags against the root file, calls once per marketplace,
//! and renders.
//!
//! # `--dry-run` on `build` is not `check`
//!
//! Both write nothing, and they answer different questions. The CLI spec
//! fixes `--dry-run` as "classify and report; write nothing — the exit code
//! is what the real run would have returned", so `build --dry-run` reports
//! the drift and exits 0, exactly as the build it previews would. `check` is
//! the CI verb: the same render, the same report, and exit 1 on any drift.

use std::collections::BTreeSet;
use std::path::PathBuf;

use tradecraft_core::{
    AdoptHashFix, AdoptHashStatus, AdoptMode, AssetSpec, ExportMode, ExportRequest, ExportUnit,
    HookFileSpec, MarketplaceConfig, NativeVfs, effective_config, export_skills, fix_adopt_hashes,
    owned_paths,
};

use crate::config::{ROOT_FILE, resolve_against_cwd, resolve_configured_path};
use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_REFUSED, blocked, no_config, usage_error};
use crate::prompt::{confirm_write, plan_flow};
use crate::report::{
    count_checked_units, normalize_thrown_reason, render_export_tally, render_unit_line,
    render_warning_line,
};
use crate::resolve::{CommonFlags, ResolvedRun, resolve_run, resolve_skills_root, workspace_runs};

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
    /// Where the roots land. Overrides `build.out`.
    pub out: Option<String>,
    /// The skills root to scan. Absent means the vault root.
    pub skills_root: Option<String>,
    /// Which marketplace to build. In a workspace, the one under
    /// `marketplaces/<name>/`; refused when there is none.
    pub marketplace: Option<String>,
    /// The global `--vault`, `--config` and `--namespace` flags.
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

/// What a marketplace load reads from the root file besides the vault: the
/// identity, the harnesses, the assets and the hooks files.
pub(crate) struct MarketplaceInputs {
    pub config: MarketplaceConfig,
    pub harnesses: BTreeSet<String>,
    pub assets: Vec<AssetSpec>,
    pub hooks: Vec<HookFileSpec>,
}

/// The root file as the core's inputs, resolved the one way `build`,
/// `check` and `list --model` share.
pub(crate) fn marketplace_inputs(run: &ResolvedRun) -> MarketplaceInputs {
    let root = &run.config;
    // Which harnesses this marketplace is built for. Absent is every one the
    // core supports; the core refuses a name no harness answers to, so a typo
    // is named rather than quietly publishing less than was asked for.
    let harnesses = root.harness_set();
    let config = root.to_config(&run.folder_name());

    // Each `from` resolves against the root file's own folder — the same
    // rule `build.out` follows — because the core never reads an ambient
    // config file and has nothing to resolve a relative path against (see
    // [`tradecraft_core::export::AssetSpec`]).
    let assets: Vec<AssetSpec> = root
        .build
        .assets
        .iter()
        .map(|entry| AssetSpec {
            source: resolve_configured_path(&entry.from, &run.config_dir),
            dest: entry.to.clone(),
            plugin: entry.plugin.clone(),
        })
        .collect();
    let hooks: Vec<HookFileSpec> = root
        .build
        .hooks
        .iter()
        .map(|entry| {
            let source = resolve_configured_path(&entry.from, &run.config_dir);
            // Root-relative, so the gap report names no host path.
            let from = crate::config::relative_path(&run.config_dir, &source)
                .to_string_lossy()
                .replace('\\', "/");
            HookFileSpec {
                source,
                from,
                plugin: entry.plugin.clone(),
            }
        })
        .collect();

    MarketplaceInputs {
        config,
        harnesses,
        assets,
        hooks,
    }
}

/// Run `skillwright check --fix-adopt-hashes` and return the exit code.
///
/// Like every verb that writes, it lists the plan and asks; `--yes` accepts,
/// and `--dry-run` stops at the list. It reads no output folder. Exits 1 when
/// a note was really edited, because that hash is left alone.
pub fn run_fix_adopt_hashes(flags: &ExportFlags, ctx: &mut CommandContext<'_>) -> i32 {
    let run = match resolve_run(&flags.common, &ctx.cwd, ctx.err) {
        Ok(run) => run,
        Err(code) => return code,
    };
    let runs = if run.is_workspace() {
        match workspace_runs(&run, &flags.common, flags.marketplace.as_deref(), ctx.err) {
            Ok(runs) => runs,
            Err(code) => return code,
        }
    } else {
        vec![run]
    };

    let mut exit = EXIT_OK;
    for member in &runs {
        let code = fix_hashes_of(flags, member, ctx);
        if exit == EXIT_OK {
            exit = code;
        }
    }
    exit
}

/// [`run_fix_adopt_hashes`] for one marketplace.
fn fix_hashes_of(flags: &ExportFlags, run: &ResolvedRun, ctx: &mut CommandContext<'_>) -> i32 {
    let skills_root = resolve_skills_root(flags.skills_root.as_deref(), &ctx.cwd);
    let fix = |mode| {
        fix_adopt_hashes(
            &NativeVfs,
            &run.vault,
            skills_root.as_deref(),
            &run.core_namespace(),
            mode,
        )
    };
    let plan = match fix(AdoptMode::DryRun) {
        Ok(plan) => plan,
        Err(err) => return blocked(ctx.err, &format!("the vault scan could not run: {err}")),
    };
    let refreshed = |fix: &&AdoptHashFix| fix.status == AdoptHashStatus::Refreshed;

    if !flags.dry_run && !flags.yes && plan.iter().any(|fix| refreshed(&fix)) {
        let lines: Vec<String> = plan
            .iter()
            .filter(refreshed)
            .map(|fix| format!("refresh the adopt hash of {}", fix.note))
            .collect();
        let gate = confirm_write("check --fix-adopt-hashes", &lines, ctx.err);
        if gate != EXIT_OK {
            return gate;
        }
    }
    if !flags.dry_run
        && let Err(err) = fix(AdoptMode::Write)
    {
        return blocked(ctx.err, &format!("the hashes could not be written: {err}"));
    }

    if flags.json {
        match serde_json::to_string_pretty(&plan) {
            Ok(document) => {
                let _ = writeln!(ctx.out, "{document}");
            }
            Err(err) => return blocked(ctx.err, &format!("the report would not serialize: {err}")),
        }
    } else {
        for fix in &plan {
            let word = match fix.status {
                AdoptHashStatus::Edited => "edited, left alone",
                _ if flags.dry_run => "would refresh",
                _ => "refreshed",
            };
            let _ = writeln!(ctx.out, "{}  {word}", fix.note);
        }
    }
    if plan
        .iter()
        .all(|fix| fix.status == AdoptHashStatus::Refreshed)
    {
        EXIT_OK
    } else {
        EXIT_REFUSED
    }
}

/// Run one export. `verb` decides the mode a plain run uses and what a
/// drifted report exits with; everything else about the two verbs is
/// identical.
///
/// In a workspace, each marketplace under `marketplaces/` is built in turn,
/// or only the one `--marketplace` names. The run exits with the first
/// failure's code, after trying every marketplace.
pub fn run_export(verb: ExportVerb, flags: &ExportFlags, ctx: &mut CommandContext<'_>) -> i32 {
    let run = match resolve_run(&flags.common, &ctx.cwd, ctx.err) {
        Ok(run) => run,
        Err(code) => return code,
    };
    if !run.is_workspace() {
        return export_one(verb, flags, &run, flags.marketplace.clone(), ctx);
    }
    if flags.out.is_some() {
        return usage_error(
            ctx.err,
            "--out names one output folder, and a workspace builds several; pass --marketplace to build one",
        );
    }
    let runs = match workspace_runs(&run, &flags.common, flags.marketplace.as_deref(), ctx.err) {
        Ok(runs) => runs,
        Err(code) => return code,
    };
    let mut exit = EXIT_OK;
    for member in &runs {
        if !flags.quiet {
            let _ = writeln!(ctx.err, "marketplace {}:", member.vault.display());
        }
        let code = export_one(verb, flags, member, None, ctx);
        if exit == EXIT_OK {
            exit = code;
        }
    }
    exit
}

/// Where one marketplace's harness trees land: `--out`, else `build.out`.
/// There is no default, so nothing builds into a vault unless asked.
///
/// # Errors
///
/// The exit code, having said so, when neither names a folder.
pub(crate) fn out_root(
    flags: &ExportFlags,
    run: &ResolvedRun,
    cwd: &std::path::Path,
    err: &mut dyn std::io::Write,
) -> Result<PathBuf, i32> {
    match &flags.out {
        Some(out) => Ok(resolve_against_cwd(out, cwd)),
        None => configured_out(run, err),
    }
}

/// `build.out`, resolved against the root file's folder.
///
/// # Errors
///
/// The exit code, having said so, when the file sets none.
pub(crate) fn configured_out(
    run: &ResolvedRun,
    err: &mut dyn std::io::Write,
) -> Result<PathBuf, i32> {
    match &run.config.build.out {
        Some(out) => Ok(resolve_configured_path(out, &run.config_dir)),
        None => Err(no_config(
            err,
            &format!(
                "No output folder for {}: pass --out, or set `build.out` in its {ROOT_FILE}. `out: .` builds beside {ROOT_FILE}.",
                run.vault.display()
            ),
        )),
    }
}

/// [`run_export`] over one marketplace's run.
fn export_one(
    verb: ExportVerb,
    flags: &ExportFlags,
    run: &ResolvedRun,
    marketplace: Option<String>,
    ctx: &mut CommandContext<'_>,
) -> i32 {
    // `--marketplace` outside a workspace is the core's selector: which of
    // the vault's `marketplaces/` to build. It reaches
    // `ExportRequest::marketplace`, which is validated against what the vault
    // actually holds, so a typo is refused by name.
    let out_root = match out_root(flags, run, &ctx.cwd, ctx.err) {
        Ok(out_root) => out_root,
        Err(code) => return code,
    };

    let skills_root = resolve_skills_root(flags.skills_root.as_deref(), &ctx.cwd);

    let MarketplaceInputs {
        config: marketplace_config,
        harnesses,
        assets,
        hooks,
    } = marketplace_inputs(run);
    let vfs = run.config_vfs();
    let marketplace_config = effective_config(
        &vfs,
        run.scope(skills_root.as_deref(), marketplace.as_deref()),
        &marketplace_config,
        &run.core_namespace(),
    );

    // --- the mode -----------------------------------------------------------
    // `check` never writes; `build --dry-run` renders through the same
    // write-nothing mode and differs only in what it exits with (see the
    // module doc). So the only run that reaches `ExportMode::Build` is a
    // plain `build`.
    let writes = verb == ExportVerb::Build && !flags.dry_run;
    let request = ExportRequest {
        vault_root: run.vault.clone(),
        skills_root,
        marketplace,
        out_root: out_root.clone(),
        mode: if writes {
            ExportMode::Build
        } else {
            ExportMode::Check
        },
        config: marketplace_config,
        harnesses,
        assets,
        hooks,
    };

    // --- confirmation -------------------------------------------------------
    // A build replaces each harness's owned paths at the output path, so it
    // confirms like a publish. A run that writes nothing has nothing to
    // confirm and is not asked, rather than auto-answered.
    if writes && !flags.yes {
        // The harnesses are named, and the paths each one replaces with them.
        // The marketplace directory can be a repository root, so "everything
        // under <out>" would be alarming and also untrue.
        // The list comes from `owned_paths`, not from a copy of the table here.
        let owned = match owned_paths(&request) {
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
        let mut plan = vec!["build the marketplace".to_string(), String::new()];
        plan.extend(plan_flow(&[
            ("from vault", vec![run.vault.display().to_string()]),
            ("into", vec![out_root.display().to_string()]),
            (
                "replacing",
                owned
                    .iter()
                    .map(|(name, path)| format!("{path:width$}  {name}"))
                    .collect(),
            ),
        ]));
        plan.push(String::new());
        plan.push("Each of those is replaced whole, and what the vault no longer".to_string());
        plan.push("produces is removed. Nothing else in that directory is touched.".to_string());
        let gate = confirm_write("build", &plan, ctx.err);
        if gate != EXIT_OK {
            return gate;
        }
    }

    // The flag is answered on both verbs, and answered differently. On
    // `build` it marks the run as a plan; on `check` it is redundant, and
    // saying so is the difference between a flag that does nothing and a flag
    // that was never read — the second is indistinguishable from a bug.
    if flags.dry_run {
        let _ = writeln!(
            ctx.err,
            "{}",
            match verb {
                ExportVerb::Check =>
                    "check writes nothing under any outcome — --dry-run changes nothing about this run",
                ExportVerb::Build =>
                    "dry run — reporting the drift a build would act on; nothing will be written",
            }
        );
    }

    // --- run ----------------------------------------------------------------
    let report = match export_skills(&vfs, &request, &run.core_namespace()) {
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
                    "advisory: {root}/{name} version {} unmoved since members changed — `skillwright bump {name}` is available",
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
