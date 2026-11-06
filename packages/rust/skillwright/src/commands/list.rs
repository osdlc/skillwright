//! `skillwright list` — the roster, and nothing else.
//!
//! A read-only verb, like `status` and `check`. It walks the vault's content
//! root through the core's [`discover_components`] and prints what it found:
//! one row per publishable note, with the note's own answer to "where does
//! this publish". It writes nothing, prompts about nothing, and never touches
//! a destination.
//!
//! What it deliberately does not report is each row's state at each
//! destination — never published, current, stale, conflicted. That needs the
//! manifest, and it is `status`'s job (the status spec).
//!
//! A note the scan could not read does not hide the rest of the roster: it
//! is named on the error sink, one line per note.
//!
//! `--model` prints something else: the marketplace model `build` renders
//! from, loaded by the core's [`load`] with the root file's marketplace
//! settings, as one JSON document. It is a separate flag rather than what
//! `--json` prints because `--json`'s rows are an existing contract.

use tradecraft_core::model::load::{LoadRequest, load};
use tradecraft_core::{DiscoveryReport, discover_components, inherit_targets};

use crate::commands::marketplace::{MarketplaceInputs, marketplace_inputs};
use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, blocked};
use crate::report::{render_roster, render_roster_tally, render_warning_line, roster_row};
use crate::resolve::{
    CommonFlags, ResolvedRun, refuse_workspace, resolve_run, resolve_skills_root, workspace_runs,
};

/// The resolved flags of `skillwright list`.
pub struct ListFlags {
    /// The content root to scan. Overrides the configured one.
    pub skills_root: Option<String>,
    /// The global `--vault`, `--config` and `--namespace` flags.
    pub common: CommonFlags,
    /// `--json`: print the report as one JSON document.
    pub json: bool,
    /// `--model`: print the loaded marketplace model instead of the roster.
    pub model: bool,
    /// `--quiet`: drop progress and the tally from stderr.
    pub quiet: bool,
}

/// Run `skillwright list` and return the exit code.
pub fn run_list(flags: &ListFlags, ctx: &mut CommandContext<'_>) -> i32 {
    // The same three questions, answered the same way, as every other verb —
    // so `--vault` and `--namespace` behave here exactly as they do on
    // `build`, including their exit codes: 4 when no vault is found and no
    // `--vault`, 2 for a vault that is not there.
    let run = match resolve_run(&flags.common, &ctx.cwd, ctx.err) {
        Ok(run) => run,
        Err(code) => return code,
    };
    let skills_root = resolve_skills_root(flags.skills_root.as_deref(), &ctx.cwd);

    if flags.model {
        return print_model(&run, skills_root, ctx);
    }

    let mut scan = match discover_components(
        &run.source_vfs(skills_root.as_deref()),
        &run.vault,
        skills_root.as_deref(),
        &run.core_namespace(),
    ) {
        Ok(scan) => scan,
        // What still fails is run-level by construction — a duplicate name, a
        // bundle overlap, a symlink, a missing root — so this stays a blocker
        // rather than a refused row. A note the scan could not read is not
        // one of these; it comes back in `skipped` instead.
        Err(err) => {
            return blocked(ctx.err, &format!("the vault scan could not run: {err}"));
        }
    };

    // A roster says where a note goes, so it shows what the note inherits.
    inherit_targets(&mut scan, &run.root_config());
    if run.is_workspace()
        && let Err(code) = inherit_from_members(&mut scan, &run, &flags.common, ctx)
    {
        return code;
    }

    if flags.json {
        let rows: Vec<_> = scan.components.iter().map(roster_row).collect();
        match serde_json::to_string_pretty(&rows) {
            Ok(document) => {
                let _ = writeln!(ctx.out, "{document}");
            }
            Err(err) => return blocked(ctx.err, &format!("the report would not serialize: {err}")),
        }
    } else {
        for line in render_roster(&scan.components) {
            let _ = writeln!(ctx.out, "{line}");
        }
    }
    for note in &scan.skipped {
        let _ = writeln!(
            ctx.err,
            "{}  skipped · {}: {}",
            note.path, note.reason, note.detail
        );
    }
    for warning in &scan.warnings {
        let _ = writeln!(ctx.err, "{}", render_warning_line(warning));
    }
    if !flags.quiet {
        let skipped = if scan.skipped.is_empty() {
            String::new()
        } else {
            format!(", {} skipped", scan.skipped.len())
        };
        let _ = writeln!(
            ctx.err,
            "{}{skipped}",
            render_roster_tally(&scan.components)
        );
    }

    EXIT_OK
}

/// Gives each row of a workspace roster the `targets` its marketplace's own
/// run gives it: a build reads each marketplace as a vault of its own.
fn inherit_from_members(
    scan: &mut DiscoveryReport,
    run: &ResolvedRun,
    common: &CommonFlags,
    ctx: &mut CommandContext<'_>,
) -> Result<(), i32> {
    for member in workspace_runs(run, common, None, ctx.err)? {
        let namespace = member.core_namespace();
        let mut inner =
            discover_components(&member.source_vfs(None), &member.vault, None, &namespace)
                .map_err(|err| blocked(ctx.err, &format!("the vault scan could not run: {err}")))?;
        inherit_targets(&mut inner, &member.root_config());
        for component in inner.components {
            if let Some(row) = scan
                .components
                .iter_mut()
                .find(|row| row.entry_path == component.entry_path)
            {
                row.targets = component.targets;
            }
        }
    }
    Ok(())
}

/// `list --model`: the marketplace model as one JSON document on stdout.
///
/// Loaded with the same inputs `build` resolves, through the same function,
/// so what this prints is what a build would translate.
fn print_model(
    run: &ResolvedRun,
    skills_root: Option<std::path::PathBuf>,
    ctx: &mut CommandContext<'_>,
) -> i32 {
    if let Err(code) = refuse_workspace("list --model", run, ctx.err) {
        return code;
    }
    let MarketplaceInputs {
        config,
        harnesses,
        assets,
        hooks,
    } = marketplace_inputs(run);
    let request = LoadRequest {
        vault_root: run.vault.clone(),
        skills_root,
        marketplace: None,
        harnesses,
        config,
        assets,
        hooks,
    };
    let vfs = run.source_vfs(request.skills_root.as_deref());
    let market = match load(&vfs, &request, &run.core_namespace()) {
        Ok(market) => market,
        Err(err) => return blocked(ctx.err, &format!("the marketplace would not load: {err}")),
    };
    match serde_json::to_string_pretty(&market) {
        Ok(document) => {
            let _ = writeln!(ctx.out, "{document}");
            EXIT_OK
        }
        Err(err) => blocked(ctx.err, &format!("the model would not serialize: {err}")),
    }
}
