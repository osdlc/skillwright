//! Reading a skill *as the vault stores it* — a note, not a folder.
//!
//! The vault keeps a skill in a note named for it,
//! `skills/code-review/code-review.md`. The name is on the note either way,
//! because Obsidian keys its quick-switcher, graph, and backlinks on note
//! names: a vault where forty notes are all called `SKILL` breaks every one
//! of those, and `[[SKILL]]` cannot address anything. The standard's folder
//! layout (`SKILL.md` inside a directory named for the skill) is what a build
//! writes, never what the vault holds.
//!
//! # What is modeled, and what is merely carried
//!
//! Three classes of key, not two. **Ours** — what [`crate::keys`] lists for the
//! note's kind — is lifted into [`VaultNote::frontmatter`] by [`crate::frontmatter`] and
//! never reaches [`SkillContent`] *through this function* — it is
//! configuration about *this vault* and a harness has no business reading
//! it. `arguments` is the one exception, and it is not this function's:
//! `SkillContent::arguments` stays empty here, and [`mod@crate::model::load`]
//! carries it over, once, after this read — see
//! [`SkillContent::apply_arguments`].
//!
//! Of what is left, [`SkillContent`] has a field for a key only when
//! `agent-adapt` renders that key back out. Everything else lands in
//! `annotations` and is re-emitted verbatim, which is the acceptance criterion
//! the vault spec states and the failure that motivated it: deriving a live
//! plugin tree through a model that could not represent it lost 89 of 148
//! files. An agent or a command, whose model has no annotations, writes them
//! into its own frontmatter instead — see [`crate::projection`].
//!
//! Two consequences of the carry rule look like oversights:
//!
//! * **`tags` is carried, not modeled**, even though [`SkillContent`] has a
//!   `tags` field. `agent-adapt`'s skill renderer emits no `tags:` key, so
//!   lifting it out of the frontmatter would delete it from every built
//!   `SKILL.md`. Left in `annotations` it survives the round trip.
//! * **A spec key holding the wrong kind of value is refused.** `model: [a]` or
//!   `user-invocable: yes-please` stops the read, naming the key, by the same
//!   check a harness block applies to that key. A value of the right kind the
//!   model still cannot type stays put and travels on. `metadata:` holding a
//!   nested map is the live case: the Agent Skills spec types it
//!   `dict[str, str]`, so a nested one is carried whole and the renderer folds
//!   it in with the other keys it cannot type. A spec key set to nothing is
//!   unset, and is dropped rather than carried.
//! * **Claude Code's lenient forms are read.** `allowed-tools: Read, Grep` is
//!   two tools, as Claude Code's skills docs say. A boolean field also reads
//!   `yes`, `no`, `on`, `off`, `1` and `0`, and becomes a real boolean, so a
//!   harness that reads only `true` and `false` gets one.
//!
//! Key spellings are the standard's: `allowed-tools`, `argument-hint`,
//! `arguments`, `disable-model-invocation`, `license`, `compatibility`,
//! `metadata`. A snake_case spelling is simply an unknown key and is
//! carried — normalising it would rewrite bytes the author wrote.
//!
//! # Transclusion and link rewriting live in [`crate::compile`], not here
//!
//! Steps 1–3 of the vault spec's five (`![[embed]]`
//! expansion, `[[link]]` collection, reference-file emission) are
//! [`crate::compile`]'s pass, applied *between* this read and the
//! projection. **The body here passes through verbatim** — the vault's bytes
//! are canonical, and a read must not rewrite them. A caller that wants the
//! flattened form runs [`crate::compile::compile_note`] over the body this
//! read returns.

use std::path::Path;

use serde_json::{Map, Value};

use crate::content::SkillContent;
use crate::error::CoreError;
use crate::frontmatter::{EntityKind, VaultFrontmatter};
use crate::skill::{read_utf8, split_frontmatter};
use crate::vfs::Vfs;

/// One vault note, read: what a build renders, and what our own keys
/// said about it.
///
/// The two halves are separate types rather than one flat struct because the
/// separation is the strip. [`SkillContent`] is everything that may reach a
/// harness, and it has nowhere to put a key of ours. [`VaultNote::frontmatter`]
/// is everything that must not travel.
#[derive(Debug, Clone, PartialEq)]
pub struct VaultNote {
    /// The content, ready to compile and project. Carries no key of ours by
    /// construction.
    pub content: SkillContent,
    /// What skillwright's own keys declared, and the note's unknown keys.
    pub frontmatter: VaultFrontmatter,
}

/// Reads the note at `skill_path`. The files beside it are the compile
/// pass's to read.
///
/// Order matters. Our keys are claimed **first**, so a wrong-shaped key of
/// ours is refused as that rather than misdiagnosed a line later as a note
/// with no `description`.
///
/// A `description` is then required, and a note without one is an error
/// naming the path. A container's note is the exception: it reads with an
/// empty description. The name is the file name slugified, via
/// [`crate::discover::default_name`], the one derivation the scan uses too.
///
/// The kind, resolved by the caller from the note's folder, says which of
/// the note's keys are skillwright's: on a plugin note `license` is the
/// pack's, on a skill it is the spec's.
pub fn read_vault_note(
    vfs: &dyn Vfs,
    skill_path: &Path,
    kind: EntityKind,
) -> Result<VaultNote, CoreError> {
    let source = read_utf8(vfs, skill_path)?;
    let (frontmatter, body) = split_frontmatter(&source);
    let mut fields = frontmatter_map(frontmatter, skill_path)?;

    // Ours, out of the map before anything else looks at it. What survives
    // this line is the note's own frontmatter and nothing of ours.
    let ours = VaultFrontmatter::take_from(&mut fields, skill_path, kind)?;
    check_spec_kinds(&mut fields, skill_path, kind)?;

    // A container's note need not describe itself.
    let description = take_string(&mut fields, "description")
        .filter(|d| !d.trim().is_empty())
        .or_else(|| kind.holds_components().then(String::new))
        .ok_or_else(|| {
            CoreError::malformed(
                skill_path,
                "not a skill: frontmatter carries no `description`",
            )
        })?;

    let content = SkillContent {
        name: crate::discover::default_name(skill_path),
        description,
        argument_hint: take_hint(&mut fields),
        // Claude Code's own key, hand-written. Empty and absent both read as
        // `None` — see `SkillContent::argument_names`'s own doc: an author
        // has no reason to write `arguments: []` on purpose, so there is no
        // second outcome worth keeping distinct from "derive it".
        argument_names: {
            let names = take_argument_names(&mut fields);
            (!names.is_empty()).then_some(names)
        },
        // Never our own `arguments` here — see the module doc's three classes.
        // `model::load` calls `SkillContent::apply_arguments` after
        // this read, with `ours.arguments`, once the kind is resolved.
        arguments: Vec::new(),
        // Nor harness blocks, and for the same reason twice over: the block's
        // target names have not been folded yet and its keys have not been
        // put to the capability matrix. `SkillContent::apply_overrides` is
        // called after both, by the same two callers.
        overrides: Default::default(),
        body: body.to_owned(),
        allowed_tools: take_tools(&mut fields),
        // The Agent Skills spec's three optional keys.
        license: take_string(&mut fields, "license"),
        compatibility: take_string(&mut fields, "compatibility"),
        metadata: take_string_map(&mut fields, "metadata"),
        model_override: take_string(&mut fields, "model"),
        context_mode: take_string(&mut fields, "context"),
        user_invocable: take_bool(&mut fields, "user-invocable").unwrap_or(true),
        disable_model_invocation: take_bool(&mut fields, "disable-model-invocation")
            .unwrap_or(false),
        // See the module doc: modeling this would delete it at build.
        tags: Vec::new(),
        // Genuinely foreign keys only. Not an Agent Skills spec field, not ours.
        annotations: fields,
    };

    Ok(VaultNote {
        content,
        frontmatter: ours,
    })
}

/// The frontmatter as a JSON object — the form `annotations` is already in,
/// so unmodeled keys need no second representation on the way through.
///
/// An absent frontmatter block is an empty map rather than an error: the
/// missing `description` is the diagnosis worth reporting, and it is reported
/// one line later against the same path.
///
/// `pub(crate)` for [`crate::discover`], which reads the same notes to a
/// different question — one reader, so a note cannot parse for the build
/// and fail for the scan.
pub(crate) fn frontmatter_map(
    frontmatter: &str,
    path: &Path,
) -> Result<Map<String, Value>, CoreError> {
    if frontmatter.trim().is_empty() {
        return Ok(Map::new());
    }
    let yaml: serde_yaml::Value = serde_yaml::from_str(frontmatter)
        .map_err(|e| CoreError::malformed(path, format!("invalid frontmatter: {e}")))?;
    // YAML this crate cannot re-emit — non-string mapping keys, tagged values —
    // is refused here rather than quietly flattened, because carrying it
    // verbatim is the promise and a lossy conversion would break it.
    let json = serde_json::to_value(&yaml).map_err(|e| {
        CoreError::malformed(path, format!("frontmatter is not representable: {e}"))
    })?;
    match json {
        Value::Object(map) => Ok(map),
        Value::Null => Ok(Map::new()),
        _ => Err(CoreError::malformed(path, "frontmatter is not a mapping")),
    }
}

/// Refuses a spec key on a skill, agent or command whose value is not the kind
/// [`crate::harness::field_types::component_type`] gives it, the check a
/// harness block applies to the same key. A spec key set to nothing is unset,
/// so it is removed rather than carried.
///
/// # Errors
///
/// `refused: malformed`, naming the key and the kind it must be.
pub(crate) fn check_spec_kinds(
    fields: &mut Map<String, Value>,
    path: &Path,
    kind: EntityKind,
) -> Result<(), CoreError> {
    if !matches!(
        kind,
        EntityKind::Skill | EntityKind::Agent | EntityKind::Command
    ) {
        return Ok(());
    }
    let spec_owned = |key: &str| {
        crate::keys::lookup(kind, key).is_some_and(|spec| spec.owner == crate::keys::Owner::Spec)
    };
    let unset: Vec<String> = fields
        .iter()
        .filter(|(key, value)| value.is_null() && spec_owned(key))
        .map(|(key, _)| key.clone())
        .collect();
    for key in unset {
        fields.remove(&key);
    }
    for (key, value) in fields.iter() {
        if !spec_owned(key) {
            continue;
        }
        if let Some(ty) = crate::harness::field_types::component_type(kind, key)
            && !ty.accepts(value)
        {
            return Err(CoreError::wrong_kind(
                path,
                format!("`{key}` {}", ty.refusal(value)),
            ));
        }
    }
    Ok(())
}

/// `argument-hint`, as the hint text. A list of strings is the unquoted
/// `[issue-number]` form Claude's docs show, and reads back as that text.
fn take_hint(fields: &mut Map<String, Value>) -> Option<String> {
    if let Some(hint) = fields
        .get("argument-hint")
        .and_then(crate::harness::field_types::hint_text)
    {
        fields.remove("argument-hint");
        return Some(hint);
    }
    take_string(fields, "argument-hint")
}

/// Removes `key` and returns it as a string — but only when it *is* a string.
/// A wrong-shaped value is left in the map so it is carried verbatim instead
/// of being consumed and discarded.
fn take_string(fields: &mut Map<String, Value>, key: &str) -> Option<String> {
    let matches = fields.get(key).is_some_and(Value::is_string);
    matches
        .then(|| fields.remove(key))
        .flatten()
        .and_then(|v| v.as_str().map(str::to_owned))
}

/// Removes `key` and returns it as a boolean, in any form
/// [`crate::harness::field_types::flag`] reads.
fn take_bool(fields: &mut Map<String, Value>, key: &str) -> Option<bool> {
    let yes = fields
        .get(key)
        .and_then(crate::harness::field_types::flag)?;
    fields.remove(key);
    Some(yes)
}

/// A string-to-string map, all-or-nothing for the same reason
/// [`take_string_list`] is: a mapping holding a non-string value is left in
/// place whole. The Agent Skills spec types `metadata` `dict[str, str]`, so a nested map
/// under it is not a `metadata` this reader can model — it stays an
/// annotation and the renderer folds it in with everything else it cannot
/// type.
fn take_string_map(
    fields: &mut Map<String, Value>,
    key: &str,
) -> std::collections::BTreeMap<String, String> {
    let matches = fields
        .get(key)
        .and_then(Value::as_object)
        .is_some_and(|map| map.values().all(Value::is_string));
    if !matches {
        return Default::default();
    }
    fields
        .remove(key)
        .and_then(|v| match v {
            Value::Object(map) => Some(
                map.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

/// `allowed-tools`, when [`tool_list`] reads it. Any other value stays in place
/// and warns.
fn take_tools(fields: &mut Map<String, Value>) -> Vec<String> {
    let tools = fields.get("allowed-tools").and_then(tool_list);
    if tools.is_some() {
        fields.remove("allowed-tools");
    }
    tools.unwrap_or_default()
}

/// The tools an `allowed-tools` value names: a list of strings, or one string
/// of tools separated by spaces (the Agent Skills spec's form) or commas (which
/// Claude Code's skills docs also accept). A space or comma inside parentheses
/// belongs to the tool, as in `Bash(git status)`. `None` for anything else.
pub(crate) fn tool_list(value: &Value) -> Option<Vec<String>> {
    let tools = match value {
        Value::String(tools) => tools,
        Value::Array(items) => {
            return items
                .iter()
                .map(|item| item.as_str().map(str::to_owned))
                .collect();
        }
        _ => return None,
    };
    let (mut list, mut tool, mut depth) = (Vec::new(), String::new(), 0usize);
    for c in tools.chars() {
        match c {
            c if (c == ',' || c.is_whitespace()) && depth == 0 => {
                if !tool.is_empty() {
                    list.push(std::mem::take(&mut tool));
                }
                continue;
            }
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        tool.push(c);
    }
    if !tool.is_empty() {
        list.push(tool);
    }
    Some(list)
}

/// Claude's `arguments`: a list of names, or the names in one space-separated
/// string.
fn take_argument_names(fields: &mut Map<String, Value>) -> Vec<String> {
    if let Some(Value::String(names)) = fields.get("arguments") {
        let names = names.split_whitespace().map(str::to_owned).collect();
        fields.remove("arguments");
        return names;
    }
    take_string_list(fields, "arguments")
}

/// A list of strings, all-or-nothing: a sequence with a non-string in it is
/// left in place whole rather than partially lifted.
fn take_string_list(fields: &mut Map<String, Value>, key: &str) -> Vec<String> {
    let matches = fields
        .get(key)
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().all(Value::is_string));
    if !matches {
        return Vec::new();
    }
    fields
        .remove(key)
        .and_then(|v| match v {
            Value::Array(items) => Some(
                items
                    .iter()
                    .filter_map(|i| i.as_str().map(str::to_owned))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::MemVfs;
    use std::path::PathBuf;

    fn read_vault_skill(vfs: &dyn Vfs, skill_path: &Path) -> Result<VaultNote, CoreError> {
        read_vault_note(vfs, skill_path, EntityKind::Skill)
    }

    /// A vault with one note at `skills/<name>.md`.
    fn vault(name: &str, source: &str) -> (MemVfs, PathBuf) {
        let vfs = MemVfs::new();
        let path = PathBuf::from("/vault/skills").join(format!("{name}.md"));
        vfs.create_dir_all(Path::new("/vault/skills")).unwrap();
        vfs.write_file(&path, source.as_bytes()).unwrap();
        (vfs, path)
    }

    #[test]
    fn a_note_is_read_as_a_skill() {
        let (vfs, path) = vault(
            "code-review",
            "---\ndescription: reviews code\nallowed-tools:\n  - Read\n  - Grep\n---\nBody prose.\n",
        );
        let note = read_vault_skill(&vfs, &path).unwrap();
        let content = &note.content;

        // The stem names the skill; the note carries no `name`.
        assert_eq!(content.name, "code-review");
        assert_eq!(content.description, "reviews code");
        assert_eq!(content.allowed_tools, vec!["Read", "Grep"]);
        assert_eq!(content.body, "Body prose.\n");
        assert!(content.user_invocable);

        // A note that declares nothing of ours reads as the ordinary vault.
        assert!(note.frontmatter.arguments.is_empty());
    }

    #[test]
    fn a_name_key_is_refused() {
        let (vfs, path) = vault(
            "code-review",
            "---\nname: code-review\ndescription: d\n---\nb\n",
        );
        let err = read_vault_skill(&vfs, &path).unwrap_err().to_string();
        assert!(err.contains("`name` is refused"), "{err}");
        assert!(err.contains("delete it"), "{err}");
    }

    #[test]
    fn a_missing_description_names_the_path() {
        let (vfs, path) = vault("nameless", "---\ntags:\n  - x\n---\nBody.\n");
        let err = read_vault_skill(&vfs, &path).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("/vault/skills/nameless.md"), "{message}");
        assert!(message.contains("description"), "{message}");

        // A note with no frontmatter at all fails the same way.
        let (vfs, path) = vault("bare", "Just prose.\n");
        assert!(
            read_vault_skill(&vfs, &path)
                .unwrap_err()
                .to_string()
                .contains("/vault/skills/bare.md")
        );
    }

    #[test]
    fn unknown_keys_are_carried_verbatim() {
        let (vfs, path) = vault(
            "carrier",
            "---\ndescription: d\nowner: kelly\ntags:\n  - review\n---\nb\n",
        );
        let annotations = read_vault_skill(&vfs, &path).unwrap().content.annotations;

        assert_eq!(annotations["owner"], serde_json::json!("kelly"));
        // `tags` rides in annotations because agent-adapt renders no `tags:`
        // key — modeling it would delete it at build.
        assert_eq!(annotations["tags"], serde_json::json!(["review"]));
        // Modeled keys are consumed, not doubled.
        assert!(!annotations.contains_key("description"));
    }

    #[test]
    fn the_three_optional_spec_keys_are_modeled_not_carried() {
        let (vfs, path) = vault(
            "spec-fields",
            "---\ndescription: d\nlicense: MIT\ncompatibility: Requires bash\n\
             metadata:\n  author: kelly\n---\nb\n",
        );
        let content = read_vault_skill(&vfs, &path).unwrap().content;

        assert_eq!(content.license.as_deref(), Some("MIT"));
        assert_eq!(content.compatibility.as_deref(), Some("Requires bash"));
        assert_eq!(content.metadata["author"], "kelly");
        // Once each: modeled, not also carried.
        assert!(content.annotations.is_empty(), "{:?}", content.annotations);
    }

    /// The whole round trip for the three: a note carrying all of them
    /// publishes each exactly once, as a modeled field rather than as a
    /// modeled field and an annotation both.
    #[test]
    fn the_three_optional_spec_keys_publish_once_each() {
        let (vfs, path) = vault(
            "code-review",
            "---\ndescription: d\nlicense: MIT\ncompatibility: Requires bash\n\
             metadata:\n  author: kelly\nowner: kelly\n---\nb\n",
        );
        let note = read_vault_skill(&vfs, &path).unwrap();
        // A dialect that omits every Claude Code extension key, so what is
        // left is the Agent Skills spec's six and nothing else.
        let files = crate::projection::project_skill(
            &crate::content::SkillProjection {
                content: note.content,
                resources: Vec::new(),
            },
            &["pi".to_owned()],
            None,
        )
        .unwrap();
        let text = String::from_utf8(files[0].content.clone()).unwrap();

        assert_eq!(text.matches("license:").count(), 1, "{text}");
        assert_eq!(text.matches("compatibility:").count(), 1, "{text}");
        assert_eq!(text.matches("metadata:").count(), 1, "{text}");
        assert!(text.contains("license: MIT"), "{text}");
        assert!(text.contains("compatibility: Requires bash"), "{text}");
        // The author's map and the unmodeled top-level key share the block.
        assert!(text.contains("  author: kelly"), "{text}");
        assert!(text.contains("  owner: kelly"), "{text}");
    }

    /// Guards the fallback [`take_string_map`]'s doc describes: a nested
    /// map stays an annotation instead of being half-lifted.
    #[test]
    fn a_nested_metadata_map_stays_an_annotation() {
        let (vfs, path) = vault(
            "nested",
            "---\ndescription: d\nmetadata:\n  outer:\n    inner: v\n---\nb\n",
        );
        let content = read_vault_skill(&vfs, &path).unwrap().content;

        assert!(content.metadata.is_empty());
        assert_eq!(
            content.annotations["metadata"],
            serde_json::json!({"outer": {"inner": "v"}})
        );
    }

    #[test]
    fn our_keys_are_typed_and_never_land_in_annotations() {
        // `arguments` is ours and typed; annotations holds foreign keys only,
        // and a prefixed key is foreign.
        let (vfs, path) = vault(
            "adopted",
            "---\n\
             description: d\n\
             owner: kelly\n\
             arguments:\n  - name: pr\n\
             sw-targets: keep me\n\
             ---\nb\n",
        );
        let note = read_vault_skill(&vfs, &path).unwrap();
        assert_eq!(note.frontmatter.arguments.len(), 1);
        assert!(note.frontmatter.unknown.contains(&"sw-targets".to_owned()));

        let annotations = note.content.annotations;
        assert_eq!(annotations["owner"], serde_json::json!("kelly"));
        assert_eq!(annotations["sw-targets"], serde_json::json!("keep me"));
        assert_eq!(annotations.len(), 2, "{annotations:?}");
    }

    #[test]
    fn a_value_the_model_cannot_type_is_carried_rather_than_swallowed() {
        let (vfs, path) = vault(
            "odd",
            "---\ndescription: d\nmetadata: {team: {lead: Ada}}\n---\nb\n",
        );
        let content = read_vault_skill(&vfs, &path).unwrap().content;

        assert!(content.metadata.is_empty());
        assert_eq!(
            content.annotations["metadata"],
            serde_json::json!({"team": {"lead": "Ada"}})
        );
    }

    #[test]
    fn claude_code_s_lenient_forms_are_read() {
        let (vfs, path) = vault(
            "odd",
            "---\ndescription: d\nallowed-tools: Read, Grep Bash(git log, x)\n\
             user-invocable: NO\ndisable-model-invocation: 1\n---\nb\n",
        );
        let content = read_vault_skill(&vfs, &path).unwrap().content;
        assert_eq!(content.allowed_tools, ["Read", "Grep", "Bash(git log, x)"]);
        assert!(!content.user_invocable);
        assert!(content.disable_model_invocation);
        assert!(content.annotations.is_empty(), "{:?}", content.annotations);
    }

    #[test]
    fn a_quoted_one_or_zero_reads_as_a_boolean() {
        let (vfs, path) = vault(
            "odd",
            "---
description: d
user-invocable: \"0\"
disable-model-invocation: '1'
---
b
",
        );
        let content = read_vault_skill(&vfs, &path).unwrap().content;
        assert!(!content.user_invocable);
        assert!(content.disable_model_invocation);
    }

    #[test]
    fn a_spec_key_set_to_nothing_is_dropped() {
        let (vfs, path) = vault("odd", "---\ndescription: d\nmodel:\nlicense:\n---\nb\n");
        let content = read_vault_skill(&vfs, &path).unwrap().content;
        assert!(content.model_override.is_none());
        assert!(content.annotations.is_empty(), "{:?}", content.annotations);
    }

    #[test]
    fn a_spec_key_of_the_wrong_kind_is_refused_naming_it() {
        for (line, said) in [
            (
                "user-invocable: yes-please",
                "`user-invocable` must be a boolean, not a string; write `true` or `false`",
            ),
            (
                "compatibility: 2048",
                "`compatibility` must be a string, not a number — quote it",
            ),
            ("model: [1]", "`model` must be a string, not a list"),
            (
                "allowed-tools: 5",
                "`allowed-tools` must be a string or a list of strings",
            ),
        ] {
            let (vfs, path) = vault("odd", &format!("---\ndescription: d\n{line}\n---\nb\n"));
            let err = read_vault_skill(&vfs, &path).unwrap_err().to_string();
            assert!(err.contains(said), "{line}: {err}");
        }
    }

    #[test]
    fn an_unquoted_bracket_hint_reads_as_its_text() {
        let (vfs, path) = vault(
            "odd",
            "---\ndescription: d\nargument-hint: [issue-number]\n---\nb\n",
        );
        let content = read_vault_skill(&vfs, &path).unwrap().content;
        assert_eq!(content.argument_hint.as_deref(), Some("[issue-number]"));
        assert!(!content.annotations.contains_key("argument-hint"));
    }

    #[test]
    fn malformed_frontmatter_names_the_path() {
        let (vfs, path) = vault("broken", "---\ndescription: [unclosed\n---\nb\n");
        let message = read_vault_skill(&vfs, &path).unwrap_err().to_string();
        assert!(message.contains("/vault/skills/broken.md"), "{message}");
        assert!(message.contains("invalid frontmatter"), "{message}");
    }

    #[test]
    fn the_body_passes_through_verbatim_wikilinks_and_all() {
        // Transclusion and link rewriting are `crate::compile`'s pass, run
        // after this read; this pins that the read does not run them.
        let (vfs, path) = vault(
            "linky",
            "---\ndescription: d\n---\nSee ![[house-style]] and [[glossary]].\n",
        );
        assert_eq!(
            read_vault_skill(&vfs, &path).unwrap().content.body,
            "See ![[house-style]] and [[glossary]].\n"
        );
    }
}
