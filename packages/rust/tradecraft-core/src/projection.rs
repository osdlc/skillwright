//! Projection — one note rendered into per-runtime files.
//!
//! Rendering is delegated entirely to `agent-adapt` (the crate used for
//! projection): per-runtime `skills_dir`, frontmatter dialect,
//! tool-name translation, annotations passthrough, and peer-resource
//! emission are its contract. This module maps a
//! [`SkillProjection`] into `agent_adapt::Skill` and hashes what comes back.
//!
//! # The strip, again
//!
//! [`crate::vault::read_vault_note`] lifts skillwright's keys out of a
//! note's frontmatter before a [`crate::SkillContent`] exists. But
//! `SkillContent` is a public type with a public `annotations` map, and
//! `agent-adapt` emits `annotations` verbatim, so `skill_metadata` drops
//! skillwright's keys again at the emit seam.

use agent_adapt::RuntimeId;

use crate::content::SkillProjection;
use crate::error::CoreError;
use crate::refusal;
use crate::skill::forward_slashed;

/// One file projection wants on disk, relative to a pack root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectedFile {
    /// Forward-slashed, relative to the pack root.
    pub relative_path: String,
    /// The file's bytes.
    pub content: Vec<u8>,
    /// Whether the file is executable.
    pub executable: bool,
}

/// Renders one snapshot as a skill through the runtime `id`, with the
/// override block named `block` applied. Output is sorted by path.
pub(crate) fn project_skill(
    snapshot: &SkillProjection,
    id: RuntimeId,
    block: &str,
) -> Result<Vec<ProjectedFile>, CoreError> {
    let skill = to_adapt_skill(snapshot, block);
    let runtime = agent_adapt::runtimes::skill_renderer_for(id).ok_or_else(|| {
        CoreError::Projection(format!("{}: runtime renders no skills", id.as_str()))
    })?;
    let files = runtime
        .render_skill(&skill)
        .map_err(|e| refusal(id.as_str(), e))?;
    Ok(projected(files))
}

/// Rendered files as projected ones, sorted by path.
fn projected(files: Vec<agent_adapt::ExportedFile>) -> Vec<ProjectedFile> {
    let mut out: Vec<ProjectedFile> = files
        .into_iter()
        .map(|file| ProjectedFile {
            relative_path: forward_slashed(&file.path),
            content: file.content,
            executable: file.executable,
        })
        .collect();
    out.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    out
}

/// A render failure, as the variant that says what actually happened.
///
/// `agent-adapt` refuses a resource path that would not land inside the skill
/// directory with `Error::UnsafePath`, and that is a **refusal**, not a
/// rendering fault. The distinction is not cosmetic: a host classifies
/// [`CoreError::Refused`] as denied and [`CoreError::Projection`] as an error,
/// so an attempted path escape logged as a generic render failure is an
/// attempted path escape nobody will ever notice.
///
/// The token is `unsafe-path`, the same one [`crate::guard::within`]
/// refuses under: two layers apply one
/// rule, so they answer in one word. Everything else `agent-adapt` can fail
/// with is a projection error, unchanged.
fn refusal(target: &str, error: agent_adapt::Error) -> CoreError {
    match error {
        agent_adapt::Error::UnsafePath { path, reason } => CoreError::refused(
            refusal::UNSAFE_PATH,
            format!(
                "{target}: `{path}` would not stay inside the skill directory — {reason}. \
                 Nothing was rendered, and nothing was written"
            ),
        ),
        other => CoreError::Projection(other.to_string()),
    }
}

/// Renders one note as a **slash command** for `harness`.
///
/// The second entry point beside [`project_skill`], and separate for a
/// reason: a different file at a different path, from a different reading of
/// the same note.
///
/// A runtime with no command layout is refused by name.
/// [`agent_adapt::runtimes::command_renderer_for`] is `Some` for Claude Code
/// and Codex CLI, and `None` for Cursor and Pi, so a command published to
/// either of those refuses `no-renderer` instead of landing somewhere the
/// harness never looks.
///
/// A key the command has no field for (`disable-model-invocation`,
/// `user-invocable`, `context`, a foreign key) is written into its
/// frontmatter as the author spelled it, unless the target's
/// [`agent_adapt::FrontmatterDialect::omit_fields`] leaves it out. The
/// harness block merges over those.
pub(crate) fn project_command(
    snapshot: &SkillProjection,
    id: RuntimeId,
    block: &str,
) -> Result<Vec<ProjectedFile>, CoreError> {
    let command = to_adapt_command(snapshot, id, block)?;
    let runtime = agent_adapt::runtimes::command_renderer_for(id).ok_or_else(|| {
        CoreError::refused(
            refusal::NO_RENDERER,
            format!(
                "{} has no command layout, so a command cannot be published there",
                id.as_str()
            ),
        )
    })?;
    let files = runtime
        .render_command(&command)
        .map_err(|e| refusal(id.as_str(), e))?;
    Ok(projected(files))
}

/// Renders a plugin's hooks as the bytes of one `hooks/hooks.json`, in the
/// shape the runtime's own `HookCapability` writes.
///
/// The third entry point, and the one that is not per note: a harness reads
/// one hooks file per plugin, so every hook of a pack renders together or the
/// second one overwrites the first. [`crate::export`] collects the members
/// and calls this once per plugin per root.
///
/// Nothing is written for an empty list — a pack with no hooks has no hooks
/// file, rather than one saying so.
///
/// # Errors
///
/// * `refused: no-renderer` — the runtime has no hooks layout at all.
/// * [`CoreError::Projection`] -- the runtime does not carry one of the events; see the docs of `agent_adapt::render::render_hooks_json` (<https://docs.rs/agent-adapt>) for the refusal contract.
pub(crate) fn project_plugin_hooks(
    hooks: &[agent_adapt::Hook],
    id: RuntimeId,
) -> Result<Option<Vec<u8>>, CoreError> {
    if hooks.is_empty() {
        return Ok(None);
    }
    let target = id.as_str();
    let Some(renderer) = agent_adapt::runtimes::hook_renderer_for(id) else {
        return Err(CoreError::refused(
            refusal::NO_RENDERER,
            format!("{target} has no hooks layout, so a hook cannot be published there"),
        ));
    };
    let files = renderer
        .render_hooks(hooks)
        .map_err(|e| refusal(target, e))?;
    let Some(file) = files.into_iter().next() else {
        return Ok(None);
    };
    // One trailing newline, as every other file a build writes ends.
    let mut bytes = file.content;
    bytes.push(b'\n');
    Ok(Some(bytes))
}

/// The note's projectable half as a command.
///
/// The body becomes the prompt, and `argument-hint` finally means something:
/// a skill carries the key and no renderer of a skill emits it, because a
/// skill is chosen by the model rather than typed by a person. A command is
/// typed, so the hint is what the harness shows while it is being typed.
///
/// `agent_adapt::Command` has no field for the keys [`unplaced_keys`]
/// collects, so they ride verbatim in the command's override map. That map
/// skips the renderer's omit list, so a key the runtime's dialect omits is dropped
/// here, the same answer the override roster gives for it. A foreign key has
/// no roster entry and passes, as on the skill path. The author's own
/// harness block, already checked by
/// [`crate::model::cascade::fold_harness_blocks`], merges over them.
///
/// # Errors
///
/// [`CoreError::Refused`] · `unsafe-path` when the name cannot become a safe
/// file stem: the renderer joins a slug into a filename without slugifying
/// it.
fn to_adapt_command(
    snapshot: &SkillProjection,
    id: RuntimeId,
    block: &str,
) -> Result<agent_adapt::Command, CoreError> {
    let content = &snapshot.content;
    let slug = agent_adapt::naming::slugify_skill_name(&content.name).map_err(|e| {
        CoreError::refused(
            refusal::UNSAFE_PATH,
            format!("`{}` cannot become a command file name: {e}", content.name),
        )
    })?;
    let mut command = agent_adapt::Command::new(content.name.clone(), slug);
    command.description = Some(content.description.clone());
    command.argument_hint = content.argument_hint.clone();
    command.argument_names = content.argument_names.clone();
    command.arguments = content.arguments.clone();
    command.tools = content.allowed_tools.clone();
    command.model = content.model_override.clone();
    command.body = content.body.clone();
    command.overrides = unplaced_keys(content)
        .into_iter()
        .filter(|(key, _)| !omitted_by(id, key))
        .collect();
    if let Some(own) = content.overrides.get(block) {
        command
            .overrides
            .extend(own.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    Ok(command)
}

/// What a note sets that a command has no field for:
/// the spec's optional keys, `context`, `user-invocable: false`,
/// `disable-model-invocation: true`, and every foreign key. Carried
/// verbatim, as the skill path carries its annotations, because a key the
/// author wrote for publication must not vanish at it.
fn unplaced_keys(
    content: &crate::content::SkillContent,
) -> serde_json::Map<String, serde_json::Value> {
    let mut keys = serde_json::Map::new();
    if let Some(license) = &content.license {
        keys.insert("license".into(), license.clone().into());
    }
    if let Some(compatibility) = &content.compatibility {
        keys.insert("compatibility".into(), compatibility.clone().into());
    }
    if !content.metadata.is_empty() {
        let metadata = content
            .metadata
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().into()))
            .collect();
        keys.insert("metadata".into(), serde_json::Value::Object(metadata));
    }
    if let Some(mode) = &content.context_mode {
        keys.insert("context".into(), mode.clone().into());
    }
    if !content.user_invocable {
        keys.insert("user-invocable".into(), false.into());
    }
    if content.disable_model_invocation {
        keys.insert("disable-model-invocation".into(), true.into());
    }
    keys.extend(
        content
            .annotations
            .iter()
            .map(|(k, v)| (k.clone(), v.clone())),
    );
    keys
}

/// Whether `id`'s dialect leaves `key` out of every frontmatter it renders.
/// The omit list holds canonical snake_case names, so `user-invocable` is
/// looked up as `user_invocable`. A foreign key is in no omit list.
fn omitted_by(id: RuntimeId, key: &str) -> bool {
    let canonical = key.replace('-', "_");
    agent_adapt::runtimes::for_id(id).is_some_and(|runtime| {
        runtime
            .frontmatter_dialect()
            .omit_fields
            .contains(&canonical.as_str())
    })
}

fn to_adapt_skill(snapshot: &SkillProjection, block: &str) -> agent_adapt::Skill {
    let content = &snapshot.content;
    let mut skill = agent_adapt::Skill::new(
        content.name.clone(),
        content.description.clone(),
        content.body.clone(),
    );
    skill.argument_hint = content.argument_hint.clone();
    skill.argument_names = content.argument_names.clone();
    skill.arguments = content.arguments.clone();
    skill.allowed_tools = content.allowed_tools.clone();
    skill.license = content.license.clone();
    skill.compatibility = content.compatibility.clone();
    skill.model_override = content.model_override.clone();
    skill.context_mode = content.context_mode.clone();
    skill.user_invocable = content.user_invocable;
    skill.disable_model_invocation = content.disable_model_invocation;
    skill.tags = content.tags.clone();
    skill.resources = snapshot
        .resources
        .iter()
        .map(
            |(relative_path, content, executable)| agent_adapt::SkillResource {
                relative_path: relative_path.clone(),
                content: content.clone(),
                executable: *executable,
            },
        )
        .collect();
    // An invariant, not an oversight: nothing an override says for one
    // harness is visible to the render of another.
    skill.overrides = content.overrides.get(block).cloned().unwrap_or_default();
    // Through the override, not `Skill::annotations`: the renderer writes
    // annotations as JSON strings, and a nested value must stay YAML. A
    // harness block's `metadata` merges over the note's own, key by key.
    let mut metadata = skill_metadata(content);
    if let Some(serde_json::Value::Object(block)) = skill.overrides.get("metadata") {
        metadata.extend(block.clone());
    }
    if !metadata.is_empty() {
        skill
            .overrides
            .insert("metadata".to_owned(), serde_json::Value::Object(metadata));
    }
    skill
}

/// A skill's `metadata:` block: its unknown keys, as YAML, with the note's
/// own `metadata` over them. skillwright's own keys never travel, whichever
/// route they arrived by.
pub(crate) fn skill_metadata(
    content: &crate::content::SkillContent,
) -> serde_json::Map<String, serde_json::Value> {
    let ours = |key: &str| crate::keys::is_owned(crate::frontmatter::EntityKind::Skill, key);
    let mut metadata = serde_json::Map::new();
    for (key, value) in content.annotations.iter().filter(|(key, _)| !ours(key)) {
        // A nested `metadata:` map is carried as an annotation of that name.
        match (key.as_str(), value) {
            ("metadata", serde_json::Value::Object(nested)) => {
                metadata.extend(nested.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
            _ => {
                metadata.insert(key.clone(), value.clone());
            }
        }
    }
    for (key, value) in &content.metadata {
        metadata.insert(key.clone(), value.clone().into());
    }
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::SkillContent;
    use crate::harness::Harness;

    fn project_skill(
        snapshot: &SkillProjection,
        harness: Harness,
    ) -> Result<Vec<ProjectedFile>, CoreError> {
        super::project_skill(snapshot, harness.profile().runtime, harness.as_str())
    }

    fn project_command(
        snapshot: &SkillProjection,
        harness: Harness,
    ) -> Result<Vec<ProjectedFile>, CoreError> {
        super::project_command(snapshot, harness.profile().runtime, harness.as_str())
    }

    fn content(name: &str) -> SkillContent {
        SkillContent {
            name: name.to_owned(),
            description: "does a thing".into(),
            argument_hint: None,
            argument_names: None,
            arguments: Vec::new(),
            body: format!("# {name}\n\nInstructions.\n"),
            allowed_tools: vec!["Read".into()],
            license: None,
            compatibility: None,
            metadata: Default::default(),
            model_override: None,
            context_mode: None,
            user_invocable: true,
            disable_model_invocation: false,
            tags: vec![],
            annotations: serde_json::Map::new(),
            overrides: Default::default(),
        }
    }

    fn snapshot(name: &str, resources: Vec<(String, Vec<u8>, bool)>) -> SkillProjection {
        SkillProjection {
            content: content(name),
            resources,
        }
    }

    #[test]
    fn a_target_with_no_hooks_layout_refuses_by_name() {
        let hook = agent_adapt::Hook::command("h", "h", "PreToolUse", "echo");
        let err = project_plugin_hooks(&[hook], RuntimeId::Pi)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no-renderer"), "{err}");
        assert!(err.contains("pi"), "{err}");
    }

    #[test]
    fn no_hooks_means_no_document() {
        assert!(
            project_plugin_hooks(&[], RuntimeId::ClaudeCode)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn per_target_layouts_match_the_live_corpus() {
        let snap = snapshot("alpha", vec![]);
        let cases = [
            (Harness::Claude, ".claude/skills/alpha/SKILL.md"),
            (Harness::Codex, ".agents/skills/alpha/SKILL.md"),
        ];
        for (target, expected) in cases {
            let files = project_skill(&snap, target).unwrap();
            assert_eq!(files.len(), 1, "{target:?}");
            assert_eq!(files[0].relative_path, expected, "{target:?}");
        }
    }

    #[test]
    fn resources_land_beside_the_skill_md_with_exec_bits() {
        let snap = snapshot(
            "alpha",
            vec![
                ("run.sh".to_owned(), b"#!/bin/sh\n".to_vec(), true),
                ("helper.ts".to_owned(), b"export {}\n".to_vec(), false),
            ],
        );
        let files = project_skill(&snap, Harness::Claude).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                ".claude/skills/alpha/SKILL.md",
                ".claude/skills/alpha/helper.ts",
                ".claude/skills/alpha/run.sh",
            ]
        );
        let run = files
            .iter()
            .find(|f| f.relative_path.ends_with("run.sh"))
            .unwrap();
        assert!(run.executable);
    }

    #[test]
    fn annotations_are_emitted_before_the_closing_fence_sorted() {
        let mut c = content("alpha");
        c.annotations
            .insert("zeta".into(), serde_json::Value::from(3));
        c.annotations
            .insert("alpha_key".into(), serde_json::Value::from("v"));
        let snap = SkillProjection {
            content: c,
            resources: vec![],
        };
        let files = project_skill(&snap, Harness::Claude).unwrap();
        let text = String::from_utf8(files[0].content.clone()).unwrap();

        let close = text.find("\n---\n").expect("closing fence");
        let fm = &text[..close];

        // What is pinned is that an unmodelled key survives into the
        // frontmatter block, with its value, in sorted order. The *spelling*
        // is `agent-adapt`'s to decide and is changing: the standard defines a
        // closed six-key field set and types `metadata` as `dict[str, str]`,
        // so a conformant renderer nests these under `metadata:` and encodes
        // the value as a string. Both forms satisfy the property this test is
        // about, and asserting the dialect instead would make a deliberate
        // conformance fix look like a regression.
        for (key, value) in [("alpha_key", "v"), ("zeta", "3")] {
            assert!(
                fm.contains(&format!("{key}: {value}"))
                    || fm.contains(&format!("{key}: \"{value}\"")),
                "`{key}` did not survive with its value:\n{fm}"
            );
        }
        assert!(
            fm.find("alpha_key").unwrap() < fm.find("zeta").unwrap(),
            "sorted key order"
        );
        // Rendering twice with the same input is byte-stable.
        let again = project_skill(&snap, Harness::Claude).unwrap();
        assert_eq!(files[0].content, again[0].content);
    }

    #[test]
    fn our_keys_are_stripped_at_emit_whatever_route_they_arrived_by() {
        // `read_vault_note` never puts one of our keys in `annotations`, but
        // `SkillContent` is public and a caller can build one from anywhere,
        // so the emit seam checks too. A prefixed key is not ours.
        let mut c = content("alpha");
        for key in ["arguments", "claude"] {
            c.annotations
                .insert(key.into(), serde_json::Value::from("ours"));
        }
        c.annotations
            .insert("sw-kind".into(), serde_json::Value::from("theirs"));
        let snap = SkillProjection {
            content: c,
            resources: vec![],
        };

        let files = project_skill(&snap, Harness::Claude).unwrap();
        let text = String::from_utf8(files[0].content.clone()).unwrap();

        assert!(!text.contains("ours"), "{text}");
        assert!(text.contains("sw-kind: theirs"), "{text}");
    }

    #[test]
    fn a_resource_path_that_escapes_is_refused_under_the_unsafe_path_token() {
        // The renderer catches these, and what matters here is the variant they
        // arrive as: `Refused`, which a host logs as denied, rather than
        // `Projection`, which it logs as a generic error.
        for escape in [
            "../../../../etc/evil.sh",
            "/absolute/evil.md",
            "a/../b.md",
            "./evil.md",
            "a//b.md",
            "C:\\Windows\\evil.md",
        ] {
            let snap = snapshot("alpha", vec![(escape.to_owned(), b"x".to_vec(), true)]);
            let err = project_skill(&snap, Harness::Claude).unwrap_err();
            let message = err.to_string();

            assert!(
                matches!(err, CoreError::Refused { .. }),
                "{escape}: {message}"
            );
            assert!(
                message.starts_with("refused: unsafe-path: "),
                "{escape}: {message}"
            );
            // The path, verbatim, so the note that carries it can be found.
            assert!(message.contains(escape), "{escape}: {message}");
            assert!(message.contains("claude-code"), "{escape}: {message}");
        }
    }

    #[test]
    fn every_other_render_failure_is_still_a_projection_error() {
        // A name with no legal slug is refused by the renderer too, and it is
        // not a path escape — so the `unsafe-path` mapping must not swallow it.
        let snap = snapshot("!!!", vec![]);
        let err = project_skill(&snap, Harness::Claude).unwrap_err();
        assert!(matches!(err, CoreError::Projection(_)), "{err}");
    }

    // --- Commands, across every runtime with a command renderer ------------

    #[test]
    fn a_command_projects_into_every_verified_runtimes_layout() {
        let snap = snapshot("ship", vec![]);
        let cases = [
            (Harness::Claude, ".claude/commands/ship.md"),
            (Harness::Codex, ".codex/prompts/ship.md"),
        ];
        for (target, expected) in cases {
            let files = project_command(&snap, target).unwrap();
            assert_eq!(files.len(), 1, "{target:?}");
            assert_eq!(files[0].relative_path, expected, "{target:?}");
        }
    }

    #[test]
    fn a_command_has_no_renderer_for_cursor_or_pi() {
        let snap = snapshot("ship", vec![]);
        for target in [Harness::Cursor, Harness::Pi] {
            let err = project_command(&snap, target).unwrap_err();
            assert!(
                matches!(err, CoreError::Refused { .. }),
                "{target:?}: {err}"
            );
            assert!(err.to_string().contains("no-renderer"), "{target:?}: {err}");
        }
    }

    /// A JSON object literal as a frontmatter key map.
    fn keys(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        value.as_object().unwrap().clone()
    }

    /// The text of the one file a render produced.
    fn only_file(files: &[ProjectedFile]) -> String {
        assert_eq!(files.len(), 1);
        String::from_utf8(files[0].content.clone()).unwrap()
    }

    /// The frontmatter of a rendered file, parsed.
    fn frontmatter(text: &str) -> serde_json::Map<String, serde_json::Value> {
        let yaml = frontmatter_lossless::split(text).yaml.unwrap();
        serde_yaml::from_str(yaml).unwrap()
    }

    /// A command's carried keys follow the target's omit list too: Codex's
    /// command file gets the foreign key but not the Claude Code extension
    /// keys.
    #[test]
    fn a_command_leaves_out_the_keys_its_target_omits() {
        let mut snap = snapshot("ship", vec![]);
        snap.content.disable_model_invocation = true;
        snap.content.user_invocable = false;
        snap.content.context_mode = Some("fork".into());
        snap.content.annotations = keys(serde_json::json!({"foo": "bar"}));
        let text = only_file(&project_command(&snap, Harness::Codex).unwrap());
        let fm = frontmatter(&text);
        assert_eq!(fm.get("foo"), Some(&serde_json::json!("bar")), "{text}");
        for key in ["disable-model-invocation", "user-invocable", "context"] {
            assert!(!fm.contains_key(key), "{key}: {text}");
        }
    }

    /// A command has no field for these either. Dropping them would publish
    /// a command the author barred from the model as one the model can call.
    #[test]
    fn a_command_carries_every_key_it_has_no_field_for() {
        let mut snap = snapshot("ship", vec![]);
        snap.content.disable_model_invocation = true;
        snap.content.user_invocable = false;
        snap.content.context_mode = Some("fork".into());
        snap.content.annotations = keys(serde_json::json!({"foo": "bar"}));

        let text = only_file(&project_command(&snap, Harness::Claude).unwrap());
        let fm = frontmatter(&text);
        for (key, value) in [
            ("disable-model-invocation", serde_json::json!(true)),
            ("user-invocable", serde_json::json!(false)),
            ("context", serde_json::json!("fork")),
            ("foo", serde_json::json!("bar")),
        ] {
            assert_eq!(fm.get(key), Some(&value), "{key}: {text}");
        }

        // The author's own override for this harness still has the last
        // word over a carried key.
        snap.content
            .overrides
            .insert("claude".into(), keys(serde_json::json!({"foo": "baz"})));
        let text = only_file(&project_command(&snap, Harness::Claude).unwrap());
        assert_eq!(
            frontmatter(&text).get("foo"),
            Some(&serde_json::json!("baz")),
            "{text}"
        );
    }
}
