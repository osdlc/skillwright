//! Rendering a run's report: the export's unit lines and tally, the
//! discovery warnings, and the `--json` document.
//!
//! These functions bind directly to the core's native structs, with no
//! `--json` round trip in between.

use tradecraft_core::{DiscoveryWarning, ExportMode, ExportUnit, StaleKind, UnitStatus};

use crate::ctx::CommandContext;
use crate::exit::refused;

/// `dry run — reporting <what>; nothing will be written`, on stderr.
pub fn dry_run_notice(ctx: &mut CommandContext<'_>, what: &str) {
    let _ = writeln!(
        ctx.err,
        "dry run — reporting {what}; nothing will be written"
    );
}

/// Writes `value` to stdout as one pretty JSON document. `Err` is the exit
/// code of a report that would not serialize.
pub fn print_json(ctx: &mut CommandContext<'_>, value: &impl serde::Serialize) -> Result<(), i32> {
    match serde_json::to_string_pretty(value) {
        Ok(document) => {
            let _ = writeln!(ctx.out, "{document}");
            Ok(())
        }
        Err(err) => Err(refused(
            ctx.err,
            &format!("the report would not serialize: {err}"),
        )),
    }
}

/// What a unit's status reads as under `mode`: a check says `ok` or `drift`,
/// a build `unchanged` or `written`.
fn status_word(status: UnitStatus, mode: ExportMode) -> &'static str {
    match (status, mode == ExportMode::Check) {
        (UnitStatus::Same, true) => "ok",
        (UnitStatus::Same, false) => "unchanged",
        (UnitStatus::Differs, true) => "drift",
        (UnitStatus::Differs, false) => "written",
    }
}

/// One export unit as `path  status` — the same `path  outcome` column order
/// every other line in this file uses, so one `awk '{print $2}'` reads a
/// build and a check alike.
pub fn render_unit_line(unit: &ExportUnit, mode: ExportMode) -> String {
    match unit {
        ExportUnit::Plugin {
            root,
            name,
            skills,
            status,
            ..
        } => format!(
            "{root}/{name}  {} · {skills} skill{}",
            status_word(*status, mode),
            if *skills == 1 { "" } else { "s" }
        ),
        ExportUnit::Manifest { root, status } => {
            format!(
                "{root}  {} · marketplace manifest",
                status_word(*status, mode)
            )
        }
        // What is going is worth naming: a whole pack leaving the
        // marketplace and a leftover file both read as `stale` otherwise,
        // and only one of them is news.
        //
        // `root` is not printed. A stale path is relative to the marketplace
        // directory, so it already opens with the tree its root owns
        // (`.claude-plugin/`, `.agents/`, `.cursor-plugin/`, `.pi/`) and prefixing
        // the root name spells the harness twice. The field stays on the
        // unit for the JSON consumers, which group by it.
        ExportUnit::Stale { path, kind, .. } => {
            let word = if mode == ExportMode::Check {
                "drift"
            } else {
                "removed"
            };
            let what = match kind {
                StaleKind::Plugin => "stale plugin",
                StaleKind::File => "stale file",
                StaleKind::EmptyDir => "stale empty directory",
                _ => "stale",
            };
            format!("{path}  {word} · {what}")
        }
        // A component the marketplace layout has no place for. The build
        // neither wrote nor removed it, so it carries no status word of its
        // own — the line says what it is, what kind, and which root when
        // only one root lacks the place.
        ExportUnit::NotCarried { root, path, kind } => match root {
            Some(root) => format!("{path}  not carried · {} on {root}", kind.as_str()),
            None => format!("{path}  not carried · {}", kind.as_str()),
        },
        // A unit this build does not know yet: its JSON is the honest line.
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// How many units in the report carry a status — one per plugin per root,
/// plus one per root manifest. This is what the leading count of the
/// `check`/`build` summary line (`N checked — <tally>`) means: the rows
/// [`render_export_tally`] counts into `written`/`unchanged`/`drift`/`ok`.
/// Stale and not-carried units are excluded because neither carries a
/// status of its own — the same reason [`render_export_tally`] keeps their
/// counts outside those four.
pub fn count_checked_units(units: &[ExportUnit]) -> usize {
    units
        .iter()
        .filter(|unit| {
            matches!(
                unit,
                ExportUnit::Plugin { .. } | ExportUnit::Manifest { .. }
            )
        })
        .count()
}

/// `2 written, 1 unchanged` / `1 drift, 3 ok`, plus the stale and
/// not-carried counts — the stderr tally for an export in `mode`.
///
/// Stale and not-carried sit outside the status counts: neither is one of
/// the report's status tokens, and listing either beside them would read as
/// a further outcome.
pub fn render_export_tally(units: &[ExportUnit], mode: ExportMode) -> String {
    let mut counts = [
        ("written", 0usize),
        ("unchanged", 0),
        ("drift", 0),
        ("ok", 0),
        ("stale", 0),
        ("not carried", 0),
        ("other", 0),
    ];
    for unit in units {
        let word = match unit {
            ExportUnit::Plugin { status, .. } | ExportUnit::Manifest { status, .. } => {
                status_word(*status, mode)
            }
            ExportUnit::Stale { .. } => "stale",
            ExportUnit::NotCarried { .. } => "not carried",
            // A kind of unit this build does not know yet.
            _ => "other",
        };
        if let Some((_, count)) = counts.iter_mut().find(|(known, _)| *known == word) {
            *count += 1;
        }
    }
    let parts: Vec<String> = counts
        .iter()
        .filter(|(_, count)| *count > 0)
        .map(|(word, count)| format!("{count} {word}"))
        .collect();
    if parts.is_empty() {
        "nothing to report".to_string()
    } else {
        parts.join(", ")
    }
}

/// `<path>  warning · <reason>: <detail>`, or `error ·` for a note a build
/// refuses. Stderr, and printed under `--quiet`.
pub fn render_warning_line(warning: &DiscoveryWarning) -> String {
    let level = if warning.is_error() {
        "error"
    } else {
        "warning"
    };
    format!(
        "{}  {level} · {}: {}",
        warning.path, warning.reason, warning.detail
    )
}
