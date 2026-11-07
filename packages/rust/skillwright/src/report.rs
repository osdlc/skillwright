//! Rendering a run's report.
//!
//! The line shape here is `path  status[ · outcome][ · reason]`, where a
//! reason is the core's closed-set token followed by its prose
//! ([`tradecraft_core::PublishReason`]). The CLI spec's worked transcripts
//! use a fuller three-line shape — `outcome    path`, with the recovery
//! prose indented beneath — which a later change to the CLI docs is to
//! reconcile, not this file.
//!
//! These functions bind directly to the core's native structs — no `--json`
//! round trip in between. [`tradecraft_core::PublishReason::from_refusal`]
//! turns a real [`tradecraft_core::CoreError`] into a reason token.

use std::collections::BTreeMap;

use serde::{Serialize, Serializer};
use tradecraft_core::status::PublishState;
use tradecraft_core::{
    AdoptStatus, AdoptedSkill, DiscoveredComponent, DiscoveryWarning, EntityKind, ExportMode,
    ExportUnit, PublishOutcome, PublishReason, PublishStatus, PublishedFile, StaleKind, UnitStatus,
};

/// What a (skill, destination, target) pair was, on any line this file
/// renders.
#[derive(Debug, Clone, Serialize)]
pub struct Attribution {
    /// The skill name.
    pub skill: String,
    /// The destination root.
    pub destination: String,
    /// The target rendered there.
    pub target: String,
}

/// A destination the note's targets key excluded — no files, and no
/// outcome.
///
/// Attributed like a file line because the whole point is naming *which*
/// destination got nothing: a run over three destinations where one was
/// excluded has to say which one, and a run over one destination has to say
/// something at all rather than print an empty report.
#[derive(Debug, Clone)]
pub struct AttributedExclusion {
    /// The skill, destination and target the row is about.
    pub attribution: Attribution,
    /// `PublishReport::not_published` verbatim — the core's prose, not ours.
    pub reason: String,
}

/// `NOT_APPLICABLE` cell, or one of the core's five — see [`AttributedState`].
///
/// Serializes as the token it prints, so `--json` carries the same five
/// words the table does and nothing a reader has to translate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// One of the core's five publish states.
    State(PublishState),
    /// The `n/a` cell: no destination installs this kind of unit.
    NotApplicable,
}

impl RowState {
    /// The token this value prints.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::State(state) => state.as_str(),
            Self::NotApplicable => NOT_APPLICABLE,
        }
    }
}

impl Serialize for RowState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The cell a `status` row carries when the question does not apply to the
/// unit at all: its kind is one no destination installs, so no publish will
/// ever move it off `unknown` and no `--check` should read it as drift.
pub const NOT_APPLICABLE: &str = "n/a";

/// One `status` row: a unit, a destination, and the state the core decided.
///
/// `state` is `None` exactly when the note's targets key excluded this
/// destination — the `n/a` cell the status spec refuses to make a sixth state —
/// and `reason` then carries the core's own prose for why.
#[derive(Debug, Clone, Serialize)]
pub struct AttributedState {
    /// The skill, destination and target the row is about.
    #[serde(flatten)]
    pub attribution: Attribution,
    /// The state, or `None` when the destination was excluded.
    pub state: Option<RowState>,
    /// The core's prose for why there is no state.
    pub reason: Option<String>,
    /// From the destination's manifest; `None` where it records nothing.
    pub published_at: Option<String>,
    /// Destination-relative paths this state is about: the conflicted files
    /// for `conflict`, empty otherwise.
    pub files: Vec<String>,
}

/// `<token>: <prose>`, or the token alone — the core's `PublishReason`,
/// printed the way its own `Display` already prints it.
pub fn render_reason(reason: &PublishReason) -> String {
    reason.to_string()
}

/// The outcomes a line spells out beside its status. `written` is both the
/// first write of a file into an empty destination and a rewrite of our own
/// output the source note has moved past, and those are the difference
/// between installing a unit and repairing a stale one. Every other outcome
/// is already said by the status or by the reason it carries.
fn spelled_outcome(outcome: PublishOutcome) -> Option<&'static str> {
    match outcome {
        PublishOutcome::FirstWrite => Some("first-write"),
        PublishOutcome::Rewrite => Some("rewrite"),
        PublishOutcome::Claimed
        | PublishOutcome::Overwritten
        | PublishOutcome::Unchanged
        | PublishOutcome::Conflict
        | PublishOutcome::Foreign
        | PublishOutcome::Blocked
        | PublishOutcome::Manifest => None,
        _ => None,
    }
}

fn publish_status_str(status: PublishStatus) -> &'static str {
    status.as_str()
}

/// `path  status[ · outcome][ · reason]` — one line, no truncation.
pub fn render_file_line(file: &PublishedFile) -> String {
    let mut detail = Vec::new();
    if let Some(outcome) = spelled_outcome(file.outcome) {
        detail.push(outcome.to_string());
    }
    if let Some(reason) = &file.reason {
        detail.push(render_reason(reason));
    }
    let head = format!("{}  {}", file.path, publish_status_str(file.status));
    if detail.is_empty() {
        head
    } else {
        format!("{head} · {}", detail.join(" · "))
    }
}

/// `skill -> destination (target)  not published · reason` — one stderr
/// line.
///
/// Not [`render_file_line`]'s shape, and deliberately: that shape leads with
/// a destination-relative *file* path, and an excluded destination has no
/// file to name. `not published` is also not one of the three status
/// tokens — it is the absence of one.
pub fn render_exclusion(exclusion: &AttributedExclusion) -> String {
    format!(
        "{} -> {} ({})  not published · {}",
        exclusion.attribution.skill,
        exclusion.attribution.destination,
        exclusion.attribution.target,
        exclusion.reason
    )
}

/// `3 written, 2 unchanged, 1 refused` — the stderr tally line.
///
/// `excluded` counts the destinations the note's targets key left out. It is
/// **not** a tally entry: listed beside the three statuses it would read as
/// a fourth outcome. It only changes the empty case, where "nothing to
/// report" would otherwise be the sole thing a user saw for a run that had a
/// specific, nameable reason for reporting nothing.
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

pub fn render_tally(files: &[PublishedFile], excluded: usize) -> String {
    let parts = tally_words(
        &["written", "unchanged", "refused"],
        files.iter().map(|f| f.status.as_str()),
    );
    if !parts.is_empty() {
        return parts.join(", ");
    }
    if excluded > 0 {
        let destinations = format!(
            "{excluded} destination{}",
            if excluded == 1 { "" } else { "s" }
        );
        return format!("nothing published — {destinations} excluded by the note's own targets");
    }
    "nothing to report".to_string()
}

/// One stderr line for an `--overwrite` path that matched no projected file
/// anywhere in the run.
///
/// `--overwrite` is the one flag that discards somebody's bytes on purpose,
/// and the core consults it by exact string equality against each projected
/// file's destination-relative path. A typo, a stale path, or a
/// destination-absolute spelling all behave identically to omitting the
/// flag, so the caller believes a conflict is resolved while it still
/// stands.
pub fn render_unmatched_overwrite(path: &str) -> String {
    let shape = if path.starts_with('/') {
        " That path is absolute; the core matches destination-relative paths."
    } else {
        " The core matches destination-relative paths — the spelling the refusal printed."
    };
    format!("--overwrite matched no projected file: {path}.{shape}")
}

/// `true` when any file in the run was refused — the 0-vs-1 exit split.
pub fn has_refusal(files: &[PublishedFile]) -> bool {
    files.iter().any(|f| f.status == PublishStatus::Refused)
}

/// A thrown message as the prose an adoption's or an export's reason column
/// carries.
///
/// [`tradecraft_core::CoreError::Refused`]'s `Display` is
/// `refused: <token>: <prose>`, and the `refused:` is the status, carried by
/// the line's own status column — left whole the line reads
/// `… refused · refused: no-renderer: …`. The token stays at the front, which
/// is what the CLI spec wants leading a reason.
pub fn normalize_thrown_reason(message: &str) -> &str {
    message.strip_prefix("refused: ").unwrap_or(message)
}

// --- the marketplace ---------------------------------------------------------

fn unit_status_str(status: UnitStatus) -> &'static str {
    status.as_str()
}

/// One export unit as `path  status` — the same `path  outcome` column order
/// every other line in this file uses, so one `awk '{print $2}'` reads a
/// publish, a build, and a check alike.
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
            unit_status_str(*status),
            if *skills == 1 { "" } else { "s" }
        ),
        ExportUnit::Manifest { root, status } => {
            format!(
                "{root}  {} · marketplace manifest",
                unit_status_str(*status)
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
/// Stale and not-carried sit outside the status counts for the same reason
/// an excluded destination sits outside a publish's: neither is one of the
/// report's status tokens, and listing either beside them would read as a
/// further outcome.
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

// --- the roster --------------------------------------------------------------

/// How a note's targets key reads, with its three states kept apart.
///
/// Absent, empty and populated are three different instructions — an empty
/// list is not "no answer", it is the answer "nowhere".
pub fn render_targets(targets: Option<&[String]>) -> String {
    match targets {
        None => "every destination".to_string(),
        Some([]) => "nowhere".to_string(),
        Some(list) => list.join(", "),
    }
}

/// One roster row, in the fields the human table and `--json` both carry.
#[derive(Debug, Clone, Serialize)]
pub struct RosterRow<'a> {
    /// The vault-relative path of the note.
    pub path: &'a str,
    /// The note kind.
    pub kind: EntityKind,
    /// The kind of the note's folder, when it differs.
    pub folder_kind: Option<EntityKind>,
    /// The note name.
    pub name: &'a str,
    /// The note description.
    pub description: &'a str,
    /// The plugin the note belongs to.
    pub plugin: Option<&'a str>,
    /// The targets the note restricts itself to.
    pub targets: Option<&'a [String]>,
}

/// One discovered component as the row `list --json` prints.
pub fn roster_row(component: &DiscoveredComponent) -> RosterRow<'_> {
    RosterRow {
        path: &component.vault_relative_path,
        kind: component.kind,
        folder_kind: component.folder_kind,
        name: &component.name,
        description: &component.description,
        plugin: component.plugin.as_deref(),
        targets: component.targets.as_deref(),
    }
}

/// The roster as aligned lines: `path  kind  name  plugin  targets ·
/// description`.
///
/// The path leads, in the spelling `vault_relative_path` carries. The
/// description trails after a separator rather than in a column, because it
/// is the one field with no useful width.
pub fn render_roster(components: &[DiscoveredComponent]) -> Vec<String> {
    let cells: Vec<[String; 5]> = components
        .iter()
        .map(|c| {
            [
                c.vault_relative_path.clone(),
                c.kind.as_str().to_string(),
                c.name.clone(),
                c.plugin.as_deref().unwrap_or("-").to_string(),
                render_targets(c.targets.as_deref()),
            ]
        })
        .collect();
    let mut widths = [0usize; 5];
    for row in &cells {
        for (col, cell) in row.iter().enumerate() {
            widths[col] = widths[col].max(cell.chars().count());
        }
    }
    cells
        .iter()
        .zip(components)
        .map(|(row, component)| {
            let padded: String = row
                .iter()
                .enumerate()
                .map(|(col, cell)| format!("{:<1$}", cell, widths[col]))
                .collect::<Vec<_>>()
                .join("  ");
            let padded = padded.trim_end();
            if component.description.is_empty() {
                padded.to_string()
            } else {
                format!("{padded} · {}", component.description)
            }
        })
        .collect()
}

/// `<path>  warning · <reason>: <detail>` — one note admitted with something
/// filled in. Stderr, and printed under `--quiet`.
pub fn render_warning_line(warning: &DiscoveryWarning) -> String {
    format!(
        "{}  warning · {}: {}",
        warning.path, warning.reason, warning.detail
    )
}

/// `3 skills, 1 plugin` — the stderr tally for a roster.
pub fn render_roster_tally(components: &[DiscoveredComponent]) -> String {
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for component in components {
        *counts.entry(component.kind.as_str()).or_insert(0) += 1;
    }
    let parts: Vec<String> = counts
        .into_iter()
        .map(|(kind, n)| format!("{n} {kind}{}", if n == 1 { "" } else { "s" }))
        .collect();
    if parts.is_empty() {
        "nothing to report".to_string()
    } else {
        parts.join(", ")
    }
}

// --- adoption ----------------------------------------------------------------

fn adopt_status_str(status: AdoptStatus) -> &'static str {
    status.as_str()
}

/// `path  status[ · reason]` for one adopted skill, where the path is where
/// the note landed — the file the user will open next.
pub fn render_adopted_line(skill: &AdoptedSkill) -> String {
    let line = format!(
        "{}  {}",
        skill.entry_path.display(),
        adopt_status_str(skill.status)
    );
    match &skill.reason {
        Some(reason) => format!("{line} · {reason}"),
        None => line,
    }
}

/// `2 adopted, 1 unchanged, 1 refused` — the stderr tally for an adoption.
///
/// Counted by status rather than by subtracting from the total. A status the
/// core adds next is counted under its own word, after these.
pub fn render_adopt_tally(skills: &[AdoptedSkill]) -> String {
    let parts = tally_words(
        &["adopted", "updated", "unchanged", "refused"],
        skills.iter().map(|skill| skill.status.as_str()),
    );
    if parts.is_empty() {
        "nothing to report".to_string()
    } else {
        parts.join(", ")
    }
}

// --- tracking ----------------------------------------------------------------

/// `note -> destination (target)  state[ · reason][ · files][ · last
/// published …]` — one stdout line per row.
///
/// The same `subject  outcome · why` order every other line in this file
/// uses. An excluded destination prints `not published`, which is
/// deliberately not one of the five state tokens: grepping for a state must
/// not find it.
pub fn render_state_line(row: &AttributedState) -> String {
    let state_label = match row.state {
        Some(state) => state.as_str(),
        None => "not published",
    };
    let mut parts = vec![format!(
        "{} -> {} ({})  {state_label}",
        row.attribution.skill, row.attribution.destination, row.attribution.target
    )];
    if let Some(reason) = &row.reason {
        parts.push(reason.clone());
    }
    if !row.files.is_empty() {
        parts.push(row.files.join(", "));
    }
    if let Some(published_at) = &row.published_at {
        parts.push(format!("last published {published_at}"));
    }
    parts.join(" · ")
}

/// `1 conflict, 2 stale, 4 current` — the stderr tally for a status run.
///
/// Worst first, so the line leads with what a reader has to do something
/// about.
pub fn render_state_tally(rows: &[AttributedState]) -> String {
    let mut states = Vec::new();
    let mut not_applicable = 0usize;
    let mut excluded = 0usize;
    for row in rows {
        match &row.state {
            None => excluded += 1,
            Some(RowState::NotApplicable) => not_applicable += 1,
            Some(RowState::State(state)) => states.push(state.as_str()),
        }
    }
    let mut parts = tally_words(
        &["conflict", "stale", "unknown", "never-published", "current"],
        states,
    );
    if not_applicable > 0 {
        parts.push(format!("{not_applicable} {NOT_APPLICABLE}"));
    }
    if excluded > 0 {
        parts.push(format!("{excluded} not published"));
    }
    if parts.is_empty() {
        "nothing to report".to_string()
    } else {
        parts.join(", ")
    }
}

/// The three states `--check` fails on.
///
/// `never-published` is not one of them, and neither is an excluded
/// destination: a unit that was never sent somewhere is not drift, and a CI
/// check that failed on it would fail every vault holding a note not yet
/// published everywhere. [`NOT_APPLICABLE`] is not one either, for a stronger
/// reason — no publish can change it, so failing on it would make `--check`
/// unpassable rather than actionable.
fn check_fails_on(state: PublishState) -> bool {
    matches!(
        state,
        PublishState::Stale | PublishState::Conflict | PublishState::Unknown
    )
}

/// `true` when any row is stale, conflicted or unknown — the `--check` split.
pub fn has_drift(rows: &[AttributedState]) -> bool {
    rows.iter().any(|row| match row.state {
        Some(RowState::State(state)) => check_fails_on(state),
        _ => false,
    })
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
