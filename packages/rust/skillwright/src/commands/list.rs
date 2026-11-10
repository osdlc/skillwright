//! `skillwright list` — the roster, and nothing else.
//!
//! A read-only verb, like `status` and `check`. It walks the vault's content
//! root through the core's [`tradecraft_core::discover_roster`] and prints what it found:
//! one row per publishable note, with the note's own answer to "where does
//! this publish". It writes nothing, prompts about nothing, and never touches
//! a destination.
//!
//! What it deliberately does not report is each row's state at each
//! destination — never published, current, stale, conflicted. That needs the
//! manifest, and it is `status`'s job (the status spec).
//!
//! A note the scan could not read does not hide the rest of the roster: it
//! is named on the error sink, one line per note, and the exit is 2 when a
//! build would refuse it.
//!
//! `--model` prints something else: the marketplace model `build` renders
//! from, loaded by the core's [`load`] with the root file's marketplace
//! settings, as one JSON document. It is a separate flag rather than what
//! `--json` prints because `--json`'s rows are an existing contract.

use tradecraft_core::model::load::{LoadRequest, load};
use tradecraft_core::settings::ROOT_FILE;
use tradecraft_core::{
    DiscoveryReport, EntityKind, MISSING_DESCRIPTION_REASON, discover_roster, inherit_targets,
};

use crate::commands::marketplace::marketplace_inputs;
use crate::config::relative_path;
use crate::ctx::CommandContext;
use crate::exit::{EXIT_BLOCKED, EXIT_OK, blocked};
use crate::report::{render_roster, render_roster_tally, render_warning_line, roster_row};
use crate::resolve::{
    CommonFlags, ResolvedRun, member_workspace_warnings, refuse_workspace, resolve_run,
    resolve_skills_root, workspace_runs, workspace_warnings,
};
use tradecraft_core::MarketplaceInputs;

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

    let mut scan = match discover_roster(
        &run.source_vfs(skills_root.as_deref()),
        &run.vault,
        skills_root.as_deref(),
        run.core_namespace(),
    ) {
        Ok(scan) => scan,
        // What still fails is run-level by construction — a symlink, a
        // missing root, a root file whose exclusions cannot be read (which
        // `resolve_run` has already refused) — so this stays a blocker rather
        // than a refused row. A
        // note the scan could not read is not one of these; it comes back in
        // `skipped` instead. A duplicate name or bundle overlap does not
        // abort: it flags the rows involved and the exit code.
        Err(err) => {
            return blocked(ctx.err, &format!("the vault scan could not run: {err}"));
        }
    };

    // A roster says where a note goes, so it shows what the note inherits.
    inherit_targets(&mut scan, &run.root_config());
    // A workspace builds only its members; `None` is the whole vault.
    let members = if run.is_workspace() {
        match read_members(&mut scan, &run, &flags.common, ctx) {
            Ok(members) => Some(members),
            Err(code) => return code,
        }
    } else {
        None
    };
    scan.warnings.extend(member_workspace_warnings(&run));

    // What a build refuses, by note: each row it names is flagged, and the
    // exit says so.
    let mut errors: std::collections::BTreeMap<&str, Vec<(&str, &str)>> =
        std::collections::BTreeMap::new();
    for w in scan.warnings.iter().filter(|w| w.is_error()) {
        errors
            .entry(w.path.as_str())
            .or_default()
            .push((w.reason.as_str(), w.detail.as_str()));
    }
    if flags.json {
        let rows: Vec<_> = scan
            .components
            .iter()
            .map(|c| {
                let mut row = serde_json::to_value(roster_row(c)).unwrap_or_default();
                if let (Some(found), Some(map)) = (
                    errors.get(c.vault_relative_path.as_str()),
                    row.as_object_mut(),
                ) {
                    for (reason, detail) in found {
                        let key = if *reason == MISSING_DESCRIPTION_REASON {
                            "missing_description"
                        } else {
                            "collision"
                        };
                        map.insert(key.to_owned(), (*detail).into());
                    }
                }
                row
            })
            .collect();
        match serde_json::to_string_pretty(&rows) {
            Ok(document) => {
                let _ = writeln!(ctx.out, "{document}");
            }
            Err(err) => return blocked(ctx.err, &format!("the report would not serialize: {err}")),
        }
    } else {
        for (line, c) in render_roster(&scan.components)
            .into_iter()
            .zip(&scan.components)
        {
            let flag: String = errors
                .get(c.vault_relative_path.as_str())
                .into_iter()
                .flatten()
                .map(|(reason, _)| format!("  [{reason}]"))
                .collect();
            let _ = writeln!(ctx.out, "{line}{flag}");
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

    // A build refuses a note it could not read, so the exit says so too. A
    // member's build reads the workspace's root file as well as its own folder.
    let refused_skip = scan.skipped.iter().any(|note| match &members {
        None => true,
        Some(members) => {
            note.path == ROOT_FILE || members.iter().any(|m| note.path.starts_with(m.as_str()))
        }
    });
    if errors.is_empty() && !refused_skip {
        EXIT_OK
    } else {
        EXIT_BLOCKED
    }
}

/// Replaces what a workspace scan says about each marketplace with that
/// marketplace's own scan: a build reads each one as a vault of its own, with
/// its own `exclude`, `patterns` and namespace. What sits outside them stays.
/// Returns each member's folder, `/`-terminated, as the rows spell it.
fn read_members(
    scan: &mut DiscoveryReport,
    run: &ResolvedRun,
    common: &CommonFlags,
    ctx: &mut CommandContext<'_>,
) -> Result<Vec<String>, i32> {
    let mut members = Vec::new();
    for member in workspace_runs(run, common, None, ctx.err)? {
        let namespace = member.core_namespace();
        let mut inner =
            discover_roster(&member.source_vfs(None), &member.vault, None, namespace)
                .map_err(|err| blocked(ctx.err, &format!("the vault scan could not run: {err}")))?;
        inherit_targets(&mut inner, &member.root_config());

        let folder = member.folder_name();
        let prefix =
            format!("{}/", relative_path(&run.vault, &member.vault).display()).replace('\\', "/");
        let note = format!("{prefix}{ROOT_FILE}");
        scan.components.retain(|c| {
            c.kind == EntityKind::Marketplace || !c.vault_relative_path.starts_with(&prefix)
        });
        scan.skipped.retain(|s| !s.path.starts_with(&prefix));
        scan.warnings
            .retain(|w| w.path == note || !w.path.starts_with(&prefix));
        for mut component in inner.components {
            component.vault_relative_path = format!("{prefix}{}", component.vault_relative_path);
            component.marketplace = Some(folder.clone());
            scan.components.push(component);
        }
        for mut skipped in inner.skipped {
            skipped.path = format!("{prefix}{}", skipped.path);
            scan.skipped.push(skipped);
        }
        // A collision names its notes in its text too; spell them from the
        // workspace root, as the rows are.
        let collided: Vec<String> = inner
            .warnings
            .iter()
            .filter(|w| w.reason == "path-collision")
            .map(|w| w.path.clone())
            .collect();
        for mut warning in inner.warnings {
            if warning.reason == "path-collision" {
                for path in &collided {
                    warning.detail = warning
                        .detail
                        .replace(&format!(" {path}"), &format!(" {prefix}{path}"));
                }
            }
            warning.path = format!("{prefix}{}", warning.path);
            scan.warnings.push(warning);
        }
        members.push(prefix);
    }
    scan.components
        .sort_by(|a, b| a.vault_relative_path.cmp(&b.vault_relative_path));
    scan.warnings.extend(workspace_warnings(run));
    Ok(members)
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
        ..
    } = marketplace_inputs(run);
    let mut request = LoadRequest::new(run.vault.clone(), config);
    request.skills_root = skills_root;
    request.harnesses = harnesses;
    request.assets = assets;
    request.hooks = hooks;
    let vfs = run.source_vfs(request.skills_root.as_deref());
    let market = match load(&vfs, &request, run.core_namespace()) {
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
