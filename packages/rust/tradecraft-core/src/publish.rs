//! The target names and the per-harness plugin facts.

use crate::content::SkillProjection;
use crate::error::CoreError;
use crate::frontmatter::EntityKind;
use crate::harness::profile_named;

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

/// Every name `targets` and `build.harnesses` accept, sorted: the list a
/// refusal prints so the fix is on screen. There are no aliases.
pub const KNOWN_TARGETS: &[&str] = &["claude", "codex", "cursor", "pi"];

/// `name` when it is one of [`KNOWN_TARGETS`]. `None` is `unknown-target`.
pub fn canonical_target(name: &str) -> Option<&'static str> {
    KNOWN_TARGETS.iter().copied().find(|known| *known == name)
}

/// What a refusal of the names in `unknown` says after naming the key:
/// which names are not harnesses, and the four that are.
pub(crate) fn unknown_targets_detail(unknown: &[&str]) -> String {
    let names: Vec<String> = unknown.iter().map(|name| format!("`{name}`")).collect();
    let known: Vec<String> = KNOWN_TARGETS
        .iter()
        .map(|name| format!("`{name}`"))
        .collect();
    let (last, rest) = known.split_last().expect("four names");
    format!(
        "names {}, which {} not a harness. The harnesses are {} and {last}",
        names.join(", "),
        if unknown.len() == 1 { "is" } else { "are" },
        rest.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_known_targets_are_the_four_harnesses_with_no_aliases() {
        for name in KNOWN_TARGETS {
            assert_eq!(canonical_target(name), Some(*name));
        }
        let built: Vec<&str> = crate::harness::with_plugin_format()
            .map(|profile| profile.name)
            .collect();
        assert_eq!(KNOWN_TARGETS, built);
        assert_eq!(canonical_target("claude-code"), None);
        assert_eq!(canonical_target("nope"), None);
    }
}
