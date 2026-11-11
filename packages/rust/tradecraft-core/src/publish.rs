//! The target names, the harness-block fold and the per-harness plugin facts.

use std::collections::BTreeMap;
use std::path::Path;

use crate::content::SkillProjection;
use crate::error::CoreError;
use crate::frontmatter::{EntityKind, VaultFrontmatter};
use crate::harness::{profile_for, profile_named};
use crate::namespace::Namespace;

/// Whether a note of this kind renders on its own into a target's layout.
///
/// Skill, command and agent each render into the target's layout for that
/// kind; a plugin takes the pack projection. A marketplace and a hook do
/// not.
pub fn kind_has_renderer(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Skill | EntityKind::Agent | EntityKind::Command | EntityKind::Plugin
    )
}

/// Whether a canonical target's plugin format has a place for a command.
/// Read from the harness profile, so the per-target fact exists once.
pub fn plugin_format_carries_commands(canonical: &str) -> bool {
    profile_named(canonical)
        .and_then(|p| p.plugin.as_ref())
        .is_some_and(|f| f.carries_commands)
}

/// Whether a canonical target's plugin format has a place for a hook.
///
/// Claude Code, Codex CLI and Cursor plugins all carry a hooks file at
/// [`PLUGIN_HOOKS_PATH`]. Claude and Codex load it from the plugin root by
/// default (<https://code.claude.com/docs/en/plugins-reference>,
/// <https://developers.openai.com/codex/hooks>,
/// <https://developers.openai.com/codex/plugins/build>); Cursor reads the file
/// its manifest's `hooks` entry names
/// (<https://cursor.com/docs/reference/plugins>), all read 2026-10-05. Pi takes
/// lifecycle events as TypeScript extensions and has no hooks file. The
/// answer is the harness profile's.
pub fn plugin_format_carries_hooks(canonical: &str) -> bool {
    profile_named(canonical)
        .and_then(|p| p.plugin.as_ref())
        .is_some_and(|f| f.carries_hooks)
}

/// Where a plugin's hooks land inside its own root, for the formats that
/// carry them.
pub const PLUGIN_HOOKS_PATH: &str = "hooks/hooks.json";

/// Refuses a single-file kind that carries peer resources.
///
/// A skill's projection has a directory to put them in; an agent's and a
/// command's are one file each, so peers would be dropped. Naming them is what
/// makes the refusal actionable — the author moves them, or publishes the note
/// as a skill.
///
pub(crate) fn refuse_carried_peers(
    projection: &SkillProjection,
    kind: &str,
) -> Result<(), CoreError> {
    if projection.resources.is_empty() {
        return Ok(());
    }
    let carried: Vec<&str> = projection
        .resources
        .iter()
        .map(|(path, _, _)| path.as_str())
        .collect();
    Err(CoreError::refused(
        "no-renderer",
        format!(
            "a {kind} publishes as a single file and carries no peers, but {} travels with \
             this note: {}. Move them, or publish this as a skill",
            carried.len(),
            carried.join(", ")
        ),
    ))
}

/// A note's harness blocks, folded onto canonical destination names and
/// checked against the harness that will receive each block. Every block is
/// folded and checked, whichever harnesses are being built. A container's
/// blocks follow the same rule.
///
/// **Each block keeps only the keys its harness reads.** The rest are
/// dropped; discovery warns about them, and they are never refused. A target
/// the matrix has no declaration for keeps no keys, so an unmapped harness
/// never receives unchecked ones. A kept key with the wrong kind of value is
/// refused.
///
/// # Errors
///
/// * `refused: unknown-target` — a block is keyed by a name outside
///   [`canonical_target`].
/// * `refused: malformed` — a key a block's harness reads holds the wrong
///   kind of value, such as `metadata` that is not a mapping. Not when
///   `kind` is an agent: an agent's blocks never ship.
pub fn overrides_for(
    note: &VaultFrontmatter,
    note_path: &Path,
    kind: EntityKind,
    ns: &Namespace,
) -> Result<BTreeMap<String, serde_json::Map<String, serde_json::Value>>, CoreError> {
    let mut folded: BTreeMap<String, serde_json::Map<String, serde_json::Value>> = BTreeMap::new();
    for (declared, block) in &note.blocks {
        let Some(canonical) = canonical_target(declared) else {
            return Err(CoreError::refused(
                "unknown-target",
                format!(
                    "{}: a harness block names `{declared}`, which this build does not know \
                     (known: {}). An unrecognised name is never read as \"every destination\"",
                    note_path.display(),
                    KNOWN_TARGETS.join(", ")
                ),
            ));
        };
        // `VaultFrontmatter::take_from` refused anything else, so this cannot
        // lose a block the author wrote.
        let block = block.as_object().cloned().unwrap_or_default();
        // Two spellings of one destination merge rather than one winning
        // silently.
        folded
            .entry(canonical.to_owned())
            .or_default()
            .extend(block);
    }

    // A key a harness does not read warns at discovery and is dropped here,
    // never refused. A key it reads takes the form every harness reads: a
    // flag such as `yes` becomes a real boolean, a list hint its text.
    for (canonical, block) in &mut folded {
        let accepted = accepted_override_keys(canonical);
        // Not `Map::retain`: the serde_json floor predates it.
        *block = std::mem::take(block)
            .into_iter()
            .filter(|(key, _)| accepted.contains(key))
            .map(|(key, value)| {
                let value = crate::harness::field_types::normalized(EntityKind::Skill, &key, value);
                (key, value)
            })
            .collect();
    }
    // A key a harness reads with the wrong kind of value is refused, in
    // every block that can ship.
    if kind == EntityKind::Agent {
        return Ok(folded);
    }
    for (canonical, block) in &folded {
        for (key, value) in block {
            if let Some(ty) = crate::harness::field_types::component_type(EntityKind::Skill, key)
                && !ty.accepts(value)
            {
                return Err(CoreError::wrong_kind(
                    note_path,
                    format!("`{}.{key}` {}", ns.key(canonical), ty.refusal(value)),
                ));
            }
        }
    }
    Ok(folded)
}

/// Every key one destination reads in an override block, from that runtime's
/// own capability declaration.
///
/// Empty for a name this build maps to no runtime, which is what makes every
/// key under such a target a refusal rather than an unchecked write.
fn accepted_override_keys(canonical: &str) -> std::collections::BTreeSet<String> {
    profile_named(canonical)
        .map(|p| p.accepted_override_keys())
        .unwrap_or_default()
}

/// Every name `targets` accepts, sorted: the list a refusal prints so the
/// fix is on screen. The harness names plus their runtime spellings.
pub const KNOWN_TARGETS: &[&str] = &[
    "amp",
    "antigravity",
    "claude",
    "claude-code",
    "codex",
    "codex-cli",
    "copilot",
    "cursor",
    "github-copilot",
    "opencode",
    "pi",
    "portable",
];

/// One name per harness, folding its spellings together. `None` is
/// `unknown-target`.
pub fn canonical_target(name: &str) -> Option<&'static str> {
    profile_for(name).map(|p| p.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_target_folds_onto_a_harness() {
        for name in KNOWN_TARGETS {
            assert!(canonical_target(name).is_some(), "{name}");
        }
        assert_eq!(canonical_target("claude-code"), Some("claude"));
        assert_eq!(canonical_target("nope"), None);
    }
}
