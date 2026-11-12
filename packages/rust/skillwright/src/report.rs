//! Rendering a run's report: the export's unit lines and tally, the
//! discovery warnings, and the `--json` document.
//!
//! These functions bind directly to the core's native structs, with no
//! `--json` round trip in between.

use tradecraft_core::{DiscoveryWarning, ExportMode, ExportUnit, StaleKind};

use crate::ctx::CommandContext;
use crate::exit::refused;

/// `N word` for each word in `words`, counted: the `order` words first, as
/// listed, then any other word in the order it first appears. A status the
/// core adds later is counted under its own word rather than dropped.
fn tally_words<'a>(order: &[&str], words: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut counts: Vec<(&str, usize)> = order.iter().map(|word| (*word, 0)).collect();
    for word in words {
        match counts.iter_mut().find(|(known, _)| *known == word) {
            Some((_, count)) => *count += 1,
            None => counts.push((word, 1)),
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(word, count)| format!("{count} {word}"))
        .collect()
}

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
            status.as_str(),
            if *skills == 1 { "" } else { "s" }
        ),
        ExportUnit::Manifest { root, status } => {
            format!("{root}  {} · marketplace manifest", status.as_str())
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
            Some(root) => format!("{path}  not carried · {kind} on {root}"),
            None => format!("{path}  not carried · {kind}"),
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
/// not-carried counts — the stderr tally for an export.
///
/// Stale and not-carried sit outside the status counts: neither is one of
/// the report's status tokens, and listing either beside them would read as
/// a further outcome.
pub fn render_export_tally(units: &[ExportUnit]) -> String {
    let mut statuses = Vec::new();
    let mut stale = 0usize;
    let mut not_carried = 0usize;
    let mut other = 0usize;
    for unit in units {
        match unit {
            ExportUnit::Stale { .. } => stale += 1,
            ExportUnit::NotCarried { .. } => not_carried += 1,
            ExportUnit::Plugin { status, .. } | ExportUnit::Manifest { status, .. } => {
                statuses.push(status.as_str());
            }
            // A kind of unit this build does not know yet.
            _ => other += 1,
        }
    }
    let mut parts = tally_words(&["written", "unchanged", "drift", "ok"], statuses);
    if stale > 0 {
        parts.push(format!("{stale} stale"));
    }
    if not_carried > 0 {
        parts.push(format!("{not_carried} not carried"));
    }
    if other > 0 {
        parts.push(format!("{other} other"));
    }
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

#[cfg(test)]
mod tests {
    use super::tally_words;

    #[test]
    fn a_word_outside_the_order_is_counted_after_it() {
        let parts = tally_words(
            &["written", "unchanged", "refused"],
            ["refused", "held", "written", "held", "written"],
        );
        assert_eq!(parts, ["2 written", "1 refused", "2 held"]);
    }

    #[test]
    fn a_word_with_no_count_is_left_out() {
        assert!(tally_words(&["written"], []).is_empty());
    }
}
