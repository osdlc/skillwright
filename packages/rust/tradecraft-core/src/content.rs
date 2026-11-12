//! The shape of a skill, and what projection needs to render one.
//!
//! The markdown tree is canonical and git is the history, so a skill carries
//! no id or version. What projection renders is a skill's content plus its
//! peer resources, nothing more.

use agent_adapt::Argument;

/// A skill's content: its modeled fields plus `annotations` (frontmatter keys
/// outside the modeled set, preserved verbatim).
#[derive(Debug, Clone, PartialEq)]
pub struct SkillContent {
    /// The skill's name.
    pub name: String,
    /// What the skill does and when to use it.
    pub description: String,
    /// Hint shown for the arguments the skill takes.
    pub argument_hint: Option<String>,
    /// Claude Code's own `arguments:` frontmatter key — see
    /// [`agent_adapt::Skill::argument_names`]. Hand-written, or derived by
    /// [`SkillContent::apply_arguments`] the same way [`Self::argument_hint`]
    /// is.
    pub argument_names: Option<Vec<String>>,
    /// What this skill or command's invocation takes.
    ///
    /// The one field here sourced from skillwright's own keys rather than from
    /// the standard's own frontmatter — see
    /// [`crate::frontmatter::VaultFrontmatter::arguments`] for why that is
    /// not the contradiction it looks like. `Vec::new()` by construction
    /// whenever this `SkillContent` comes from
    /// [`crate::vault::read_vault_note`]; only
    /// [`SkillContent::apply_arguments`] fills it in, called by
    /// [`mod@crate::model::load`] after the read, before the projection.
    pub arguments: Vec<Argument>,
    /// The markdown body, without frontmatter.
    pub body: String,
    /// Tools the skill may use without asking.
    pub allowed_tools: Vec<String>,
    /// The Agent Skills spec's `license:` — a licence name, or a reference to a bundled
    /// licence file.
    pub license: Option<String>,
    /// The Agent Skills spec's `compatibility:` — what environment this needs, capped at
    /// 500 characters by [`crate::discover`].
    pub compatibility: Option<String>,
    /// The Agent Skills spec's `metadata:`. Only the entries the note wrote under that key;
    /// the unmodeled top-level keys in [`Self::annotations`] join them in one block at render
    /// time, as YAML, so an unquoted `true` publishes as a boolean.
    pub metadata: std::collections::BTreeMap<String, String>,
    /// Model the skill asks to run on, when it names one.
    pub model_override: Option<String>,
    /// Context mode the skill asks for, when it names one.
    pub context_mode: Option<String>,
    /// Whether a user can invoke the skill directly.
    pub user_invocable: bool,
    /// Whether the model is barred from invoking the skill on its own.
    pub disable_model_invocation: bool,
    /// Free-form tags.
    pub tags: Vec<String>,
    /// Unmodeled frontmatter, preserved as-is. A JSON object.
    pub annotations: serde_json::Map<String, serde_json::Value>,
    /// Harness blocks, keyed by **canonical** harness name — what one
    /// harness receives that the shared model cannot say.
    ///
    /// The second field here sourced from skillwright's own keys, and it is
    /// here for the same reason [`Self::arguments`] is: a renderer needs it.
    /// `BTreeMap::new()` by construction from
    /// [`crate::vault::read_vault_note`]; only
    /// [`SkillContent::apply_overrides`] fills it in, and only after
    /// [`crate::model::cascade::fold_harness_blocks`] has folded the target names and
    /// checked every key against the harness that will receive it.
    ///
    /// The raw harness block key still reaches no harness — the strip
    /// is unaffected, because this is not `annotations`.
    pub overrides: std::collections::BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
}

impl SkillContent {
    /// Carries `arguments` onto this content, and derives
    /// [`SkillContent::argument_hint`] and
    /// [`SkillContent::argument_names`] from it when the note wrote neither
    /// of its own.
    ///
    /// A hand-written hint or name list always wins — this never overwrites
    /// a `Some`. Never called by [`crate::vault::read_vault_note`];
    /// [`mod@crate::model::load`] calls it once, between reading a note and
    /// projecting it, so every harness sees the same derived hint, name list,
    /// and argument list.
    pub fn apply_arguments(&mut self, arguments: &[Argument]) {
        if self.argument_hint.is_none() {
            self.argument_hint = agent_adapt::render::derive_argument_hint(arguments);
        }
        if self.argument_names.is_none() {
            self.argument_names = agent_adapt::render::derive_argument_names(arguments);
        }
        self.arguments = arguments.to_vec();
    }

    /// Carries the checked per-harness override blocks onto this content.
    ///
    /// Keyed by the harness name, which is what the projection looks one up
    /// by. Called by the translator between the model and the projection —
    /// never by [`crate::vault::read_vault_note`], which has no harness
    /// roster.
    pub fn apply_overrides(
        &mut self,
        overrides: std::collections::BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
    ) {
        self.overrides = overrides;
    }
}

/// A skill and its peer files, ready to project onto a target.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillProjection {
    /// The skill's content.
    pub content: SkillContent,
    /// `(relative_path, bytes, executable)`, in path order.
    pub resources: Vec<(String, Vec<u8>, bool)>,
}
