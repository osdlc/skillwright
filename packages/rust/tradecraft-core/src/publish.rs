//! The target names, the reason tokens, the harness-block fold and the
//! per-harness plugin facts.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::content::SkillProjection;
use crate::error::CoreError;
use crate::frontmatter::{EntityKind, VaultFrontmatter};
use crate::harness::{profile_for, profile_named};
use crate::namespace::Namespace;

/// The closed set of reason tokens a refusal leads with
/// ([`CoreError::refused`]), plus three that name an outcome which wrote.
///
/// Closed, so a report, a `--json` document and a host all read the same
/// word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ReasonToken {
    /// The destination's copy moved since we published it.
    Conflict,
    /// It exists at one of our paths, and no manifest entry says it is ours.
    Foreign,
    /// A path leaves the tree it must stay inside.
    UnsafePath,
    /// We cannot read it, so we must not act on it.
    Io,
    /// No renderer for this target, or none for this note's kind at it.
    NoRenderer,
    /// A plugin manager owns the tree and rotates it.
    PluginManaged,
    /// The run stopped before reaching this file.
    Stopped,
    /// The note's embeds form a cycle, so its projection cannot be computed.
    Cycle,
    /// The destination's resolved write paths overlap the vault.
    UnsafeRoot,
    /// The destination's manifest cannot be used: corrupt, unreadable, newer,
    /// or shared.
    BadManifest,
    /// Two notes, targets, or destinations resolve onto one path.
    PathCollision,
    /// A targets key, or a request, names a target this build does not know.
    UnknownTarget,
    /// A harness block names a key the receiving harness does not
    /// read. Its own token rather than `unknown-target`: the destination is
    /// fine and the key is not, and a reader has to be told which.
    UnknownOverrideKey,
    /// A kind key names a value this build does not know.
    UnknownKind,
    /// A plugin's body names a member that resolves to no note.
    MemberUnresolved,
    /// A plugin note with no resolvable members.
    EmptyPlugin,
    /// The note carries both our nested block and our flat keys.
    MixedFrontmatterShape,
    /// `build.exclude` or a folder that never ships holds the note.
    ExcludedNote,
    /// A member note has a `name` and no `description`.
    MissingDescription,
    /// A member note states no `name`, and the one its file name gives is
    /// not a name the Agent Skills spec admits.
    InvalidName,
    /// A note or root file that parsed holds a value of the wrong kind or
    /// shape for a key it sets, bare or in a harness block.
    Malformed,
    /// A root file, or the workspace file above it, cannot be read: its
    /// frontmatter is not YAML, or a key it sets has the wrong shape. When
    /// the keys that say what is excluded are the ones that cannot be read,
    /// a scan and a publish refuse too, rather than ship what may be excluded.
    MalformedRootFile,
    /// Not a refusal, and the one token here that names a success: the file
    /// was already byte-identical to our own output, so this run claimed it
    /// into the destination's manifest. The table's refusal set is
    /// [`ReasonToken::REFUSALS`]; a `written` or `unchanged` line needs a way
    /// to say this, and it is a reason like any other.
    Claimed,
    /// Not a refusal either: a harness block key said what the projection
    /// already writes, so the file published unchanged and the author has a
    /// key they can delete.
    RedundantOverride,
    /// Not a refusal either: a file this destination's manifest did not
    /// record held different bytes, and the request named it for overwrite,
    /// so its content was replaced rather than adopted.
    Overwritten,
}

impl ReasonToken {
    /// Tokens that name an outcome which wrote, so they are absent from
    /// [`Self::REFUSALS`].
    pub const WROTE: &'static [ReasonToken] =
        &[Self::Claimed, Self::RedundantOverride, Self::Overwritten];

    /// Every refusal token, in the order the publish spec's table
    /// lists them. The tokens in [`Self::WROTE`] are deliberately absent.
    pub const REFUSALS: &'static [ReasonToken] = &[
        Self::Conflict,
        Self::Foreign,
        Self::UnsafePath,
        Self::Io,
        Self::NoRenderer,
        Self::PluginManaged,
        Self::Stopped,
        Self::Cycle,
        Self::UnsafeRoot,
        Self::BadManifest,
        Self::PathCollision,
        Self::UnknownTarget,
        Self::UnknownOverrideKey,
        Self::UnknownKind,
        Self::MemberUnresolved,
        Self::EmptyPlugin,
        Self::MixedFrontmatterShape,
        Self::ExcludedNote,
        Self::Malformed,
        Self::MalformedRootFile,
        Self::MissingDescription,
        Self::InvalidName,
    ];

    /// The wire spelling, which prose never varies from.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conflict => "conflict",
            Self::Foreign => "foreign",
            Self::UnsafePath => "unsafe-path",
            Self::Io => "io",
            Self::NoRenderer => "no-renderer",
            Self::PluginManaged => "plugin-managed",
            Self::Stopped => "stopped",
            Self::Cycle => "cycle",
            Self::UnsafeRoot => "unsafe-root",
            Self::BadManifest => "bad-manifest",
            Self::PathCollision => "path-collision",
            Self::UnknownTarget => "unknown-target",
            Self::UnknownOverrideKey => "unknown-override-key",
            Self::UnknownKind => "unknown-kind",
            Self::MemberUnresolved => "member-unresolved",
            Self::EmptyPlugin => "empty-plugin",
            Self::MixedFrontmatterShape => "mixed-frontmatter-shape",
            Self::ExcludedNote => "excluded-note",
            Self::Malformed => "malformed",
            Self::MalformedRootFile => "malformed-root-file",
            Self::MissingDescription => crate::discover::MISSING_DESCRIPTION_REASON,
            Self::InvalidName => crate::discover::INVALID_NAME_REASON,
            Self::Claimed => "claimed",
            Self::RedundantOverride => "redundant-override",
            Self::Overwritten => "overwritten",
        }
    }

    /// The token this spelling names, or `None` for a word outside the set.
    pub fn parse(token: &str) -> Option<Self> {
        Self::REFUSALS
            .iter()
            .chain(Self::WROTE)
            .copied()
            .find(|candidate| candidate.as_str() == token)
    }

    /// Whether this token names a refusal: whether it is in [`Self::REFUSALS`].
    pub fn is_refusal(self) -> bool {
        Self::REFUSALS.contains(&self)
    }
}

impl std::fmt::Display for ReasonToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

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

    #[test]
    fn the_refusal_tokens_are_the_tables_outbound_set() {
        let spelled: Vec<&str> = ReasonToken::REFUSALS.iter().map(|t| t.as_str()).collect();
        assert_eq!(
            spelled,
            vec![
                "conflict",
                "foreign",
                "unsafe-path",
                "io",
                "no-renderer",
                "plugin-managed",
                "stopped",
                "cycle",
                "unsafe-root",
                "bad-manifest",
                "path-collision",
                "unknown-target",
                "unknown-override-key",
                "unknown-kind",
                "member-unresolved",
                "empty-plugin",
                "mixed-frontmatter-shape",
                "excluded-note",
                "malformed",
                "malformed-root-file",
                "missing-description",
                "invalid-name",
            ]
        );
        assert!(ReasonToken::REFUSALS.iter().all(|t| t.is_refusal()));
        assert!(ReasonToken::WROTE.iter().all(|t| !t.is_refusal()));
        assert_eq!(
            ReasonToken::WROTE,
            &[
                ReasonToken::Claimed,
                ReasonToken::RedundantOverride,
                ReasonToken::Overwritten,
            ]
        );
    }

    #[test]
    fn a_token_round_trips_through_its_spelling_and_the_wire() {
        for token in ReasonToken::REFUSALS
            .iter()
            .chain(ReasonToken::WROTE)
            .copied()
        {
            assert_eq!(ReasonToken::parse(token.as_str()), Some(token));
            let json = serde_json::to_string(&token).unwrap();
            assert_eq!(json, format!("\"{token}\""));
            let back: ReasonToken = serde_json::from_str(&json).unwrap();
            assert_eq!(back, token);
        }
        assert_eq!(ReasonToken::parse("not-a-reason"), None);
    }
}
