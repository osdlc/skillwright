//! A plugin's hooks from its `hooks/hooks.json` (spec section 4 rule 23).
//!
//! The file has Claude Code's shape —
//! `{"hooks": {"<Event>": [{"matcher": "…", "hooks": [{"type": "command",
//! "command": "…", "timeout": 5}]}]}}` — and parses into
//! [`agent_adapt::Hook`]s.
//!
//! A hook a harness cannot carry does not stop the build: it is left out of
//! that harness's file.
//!
//! A command names the plugin's own files as `${PLUGIN_ROOT}/…`, and each
//! such path must name a file the pack ships (`check_hook_paths`). Each
//! pack's copy names its harness's own variable instead ([`localize`]),
//! because a hook runs in the session's working directory, not the
//! plugin's. The same scanner, `plugin_root_refs`, reads plugin-root paths
//! in Markdown. Two placeholders are filled per pack: `{{harness}}`, the
//! harness's name, and `{{event}}`, the event the hook is registered under,
//! spelled as in the file. Any other `{{…}}`, or an unclosed `{{`, is refused when the file is
//! parsed. Only `command` is rewritten; other fields are written as they are.

use agent_adapt::capabilities;
use agent_adapt::{Hook, HookType, RuntimeId};
use serde_json::{Map, Value};

use crate::error::CoreError;
use crate::refusal;

/// The variable a hook command names a plugin's installed root by on one
/// harness, and the page it was read from.
#[derive(Debug, Clone, Copy)]
pub struct PluginRootVar {
    /// The variable's name, without `$` or braces.
    pub name: &'static str,
    /// What else the source says, including what it leaves open.
    pub note: Option<&'static str>,
    /// ISO date the source was read.
    pub observed_on: &'static str,
    /// The page the name was read from.
    pub url: &'static str,
}

/// The token source text names the plugin root by (spec section 4 rule 15).
pub(crate) const CANONICAL_ROOT: &str = "${PLUGIN_ROOT}";

/// Claude Code's own variable, which source must not write (rule 22).
pub(crate) const CLAUDE_ROOT: &str = "${CLAUDE_PLUGIN_ROOT}";

/// One plugin-root variable in a text, `start..end` its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RootRef<'a> {
    pub start: usize,
    pub end: usize,
    pub kind: RootKind<'a>,
}

/// What a [`RootRef`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootKind<'a> {
    /// `${PLUGIN_ROOT}/<path>`, the path without its trailing `.`s.
    Path(&'a str),
    /// `${PLUGIN_ROOT}` with no path after it.
    Lone,
    /// `${CLAUDE_PLUGIN_ROOT}`.
    Claude,
}

/// Every `${PLUGIN_ROOT}` and `${CLAUDE_PLUGIN_ROOT}` in `text`, in order. A
/// path runs to the first character that is not a letter, digit, `.`, `_`,
/// `-` or `/`, and a `.` at its end is not part of it (spec section 4 rule
/// 16).
pub(crate) fn plugin_root_refs(text: &str) -> Vec<RootRef<'_>> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(found) = text[at..].find("${") {
        let start = at + found;
        let rest = &text[start..];
        if rest.starts_with(CLAUDE_ROOT) {
            let end = start + CLAUDE_ROOT.len();
            out.push(RootRef {
                start,
                end,
                kind: RootKind::Claude,
            });
            at = end;
        } else if rest.starts_with(CANONICAL_ROOT) {
            let after = start + CANONICAL_ROOT.len();
            let tail = text[after..].strip_prefix('/').unwrap_or("");
            let len = tail
                .find(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/')))
                .unwrap_or(tail.len());
            let path = tail[..len].trim_end_matches('.');
            let (end, kind) = if path.is_empty() {
                (after, RootKind::Lone)
            } else {
                (after + 1 + path.len(), RootKind::Path(path))
            };
            out.push(RootRef { start, end, kind });
            at = end;
        } else {
            at = start + 2;
        }
    }
    out
}

/// Refuses a hook command's `${PLUGIN_ROOT}/<path>` that names a file the
/// `label` pack does not ship (spec section 4 rules 17 and 23). A lone
/// `${PLUGIN_ROOT}` is allowed: the variable is kept.
///
/// # Errors
///
/// `refused: plugin-root-not-shipped`, naming `from` and the path as
/// written.
pub(crate) fn check_hook_paths(
    hooks: &[Hook],
    from: &str,
    label: &str,
    ships: impl Fn(&str) -> bool,
) -> Result<(), CoreError> {
    for command in hooks.iter().filter_map(|hook| hook.command.as_deref()) {
        for found in plugin_root_refs(command) {
            if let RootKind::Path(path) = found.kind
                && !ships(path)
            {
                return Err(not_shipped(
                    from,
                    &command[found.start..found.end],
                    path,
                    label,
                ));
            }
        }
    }
    Ok(())
}

/// The refusal for a plugin-root path naming nothing the `label` pack
/// ships.
pub(crate) fn not_shipped(from: &str, written: &str, path: &str, label: &str) -> CoreError {
    CoreError::refused(
        refusal::PLUGIN_ROOT_NOT_SHIPPED,
        format!(
            "{from}: `{written}` names `{path}`, which the {label} pack does not contain \
             (section 4 rules 17 and 20)"
        ),
    )
}

/// The placeholders a hook command may carry; [`localize`] pairs them with
/// their values by position.
const PLACEHOLDERS: [&str; 2] = ["harness", "event"];

/// Parse a canonical hooks file. `from` names the file in every refusal.
///
/// # Errors
///
/// * `refused: malformed-hooks-file` — not JSON, or an entry of the wrong
///   shape; the message names the entry.
/// * `refused: unknown-hook-event` — an event no harness documents.
/// * `refused: unknown-hook-placeholder` — a command carries a `{{…}}` other
///   than `{{harness}}` and `{{event}}`.
/// * `refused: claude-plugin-root` — the file writes `${CLAUDE_PLUGIN_ROOT}`
///   (spec section 4 rule 22).
pub fn parse_hooks_file(text: &str, from: &str) -> Result<Vec<Hook>, CoreError> {
    if text.contains(CLAUDE_ROOT) {
        return Err(CoreError::refused(
            refusal::CLAUDE_PLUGIN_ROOT,
            format!("{from}: `{CLAUDE_ROOT}` is Claude Code's own; write `{CANONICAL_ROOT}`"),
        ));
    }
    let malformed =
        |what: String| CoreError::refused(refusal::MALFORMED_HOOKS_FILE, format!("{from}: {what}"));
    let root: Value = serde_json::from_str(text).map_err(|e| malformed(e.to_string()))?;
    let Some(root) = root.as_object() else {
        return Err(malformed("the file must be a JSON object".into()));
    };
    if let Some(key) = root.keys().find(|key| *key != "hooks") {
        return Err(malformed(format!("unknown top-level key `{key}`")));
    }
    let Some(events) = root.get("hooks").and_then(Value::as_object) else {
        return Err(malformed("`hooks` must be an object keyed by event".into()));
    };

    let mut out = Vec::new();
    for (event, groups) in events {
        if !capabilities::hook_events().any(|(known, _)| known == event) {
            return Err(CoreError::refused(
                refusal::UNKNOWN_HOOK_EVENT,
                format!("{from}: `hooks.{event}` is not a hook event any harness documents"),
            ));
        }
        let Some(groups) = groups.as_array() else {
            return Err(malformed(format!("`hooks.{event}` must be an array")));
        };
        for (g, group) in groups.iter().enumerate() {
            let at = format!("hooks.{event}[{g}]");
            let Some(group) = group.as_object() else {
                return Err(malformed(format!("`{at}` must be an object")));
            };
            if let Some(key) = group.keys().find(|k| *k != "matcher" && *k != "hooks") {
                return Err(malformed(format!("`{at}` has unknown key `{key}`")));
            }
            let matcher = optional_string(group, "matcher", &at).map_err(malformed)?;
            let Some(entries) = group.get("hooks").and_then(Value::as_array) else {
                return Err(malformed(format!("`{at}.hooks` must be an array")));
            };
            for (h, entry) in entries.iter().enumerate() {
                let at = format!("{at}.hooks[{h}]");
                let hook = parse_entry(entry, event, matcher.clone(), &at).map_err(malformed)?;
                if let Some(bad) = hook.command.as_deref().and_then(unknown_placeholder) {
                    let known = PLACEHOLDERS.map(|p| format!("`{{{{{p}}}}}`")).join(", ");
                    return Err(CoreError::refused(
                        refusal::UNKNOWN_HOOK_PLACEHOLDER,
                        format!(
                            "{from}: `{at}.command` has `{bad}`; a hook command may use \
                             {known}"
                        ),
                    ));
                }
                out.push(hook);
            }
        }
    }
    Ok(out)
}

/// One hook object. A key the model does not carry travels as an override,
/// merged into the rendered entry unchanged.
fn parse_entry(
    entry: &Value,
    event: &str,
    matcher: Option<String>,
    at: &str,
) -> Result<Hook, String> {
    let Some(entry) = entry.as_object() else {
        return Err(format!("`{at}` must be an object"));
    };
    let r#type = match optional_string(entry, "type", at)? {
        None => HookType::Command,
        Some(spelled) => spelled
            .parse::<HookType>()
            .map_err(|e| format!("`{at}`: {e}"))?,
    };
    let timeout_ms = match entry.get("timeout") {
        None => 0,
        Some(value) => {
            let seconds = value
                .as_i64()
                .filter(|seconds| *seconds >= 0)
                .ok_or_else(|| format!("`{at}.timeout` must be a whole number of seconds"))?;
            seconds.checked_mul(1000).ok_or_else(|| {
                format!(
                    "`{at}.timeout` is too large: {seconds} seconds does not fit in milliseconds"
                )
            })?
        }
    };
    let mut hook = Hook::command(at, at, event, "");
    hook.matcher = matcher;
    hook.r#type = Some(r#type);
    hook.command = optional_string(entry, "command", at)?;
    hook.url = optional_string(entry, "url", at)?;
    hook.prompt = optional_string(entry, "prompt", at)?;
    hook.r#if = optional_string(entry, "if", at)?;
    hook.r#async = optional_bool(entry, "async", at)?;
    hook.once = optional_bool(entry, "once", at)?;
    hook.timeout_ms = timeout_ms;
    const MODELLED: &[&str] = &[
        "type", "timeout", "command", "url", "prompt", "if", "async", "once",
    ];
    hook.overrides = entry
        .iter()
        .filter(|(key, _)| !MODELLED.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();

    let missing = match r#type {
        HookType::Command => hook.command.is_none().then_some("command"),
        HookType::Http => hook.url.is_none().then_some("url"),
        HookType::Prompt => hook.prompt.is_none().then_some("prompt"),
        HookType::McpTool | HookType::Agent => None,
    };
    match missing {
        Some(key) => Err(format!("`{at}` is a `{type}` hook with no `{key}`")),
        None => Ok(hook),
    }
}

fn optional_string(
    map: &Map<String, Value>,
    key: &str,
    at: &str,
) -> Result<Option<String>, String> {
    match map.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone()).filter(|v| !v.trim().is_empty())),
        Some(_) => Err(format!("`{at}.{key}` must be a string")),
    }
}

fn optional_bool(map: &Map<String, Value>, key: &str, at: &str) -> Result<Option<bool>, String> {
    match map.get(key) {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("`{at}.{key}` must be `true` or `false`")),
    }
}

/// The first `{{name}}` in `command` that is not a known placeholder, or an
/// unclosed `{{` and the rest of the command, as written.
fn unknown_placeholder(command: &str) -> Option<&str> {
    let mut rest = command;
    while let Some(open) = rest.find("{{") {
        let Some(close) = rest[open + 2..].find("}}") else {
            return Some(&rest[open..]);
        };
        let end = open + 2 + close + 2;
        if !PLACEHOLDERS.contains(&&rest[open + 2..end - 2]) {
            return Some(&rest[open..end]);
        }
        rest = &rest[end..];
    }
    None
}

/// `hook` as the pack for root `harness` carries it: `${PLUGIN_ROOT}` in its
/// command becomes `${<plugin_root_var>}` (left as is when `None`), and the
/// placeholders are filled.
pub fn localize(mut hook: Hook, harness: &str, plugin_root_var: Option<&str>) -> Hook {
    let Some(mut command) = hook.command.take() else {
        return hook;
    };
    if let Some(var) = plugin_root_var {
        let with = format!("${{{var}}}");
        command = replace_bounded(&command, CANONICAL_ROOT, &with, |c| {
            c.is_alphanumeric() || c == '_'
        });
    }
    for (name, value) in PLACEHOLDERS.iter().zip([harness, hook.event.as_str()]) {
        command = command.replace(&format!("{{{{{name}}}}}"), value);
    }
    hook.command = Some(command);
    hook
}

/// The hooks `runtime` carries, in file order. The others are left out; the
/// load warns for each (`hook-left-out`).
pub fn carried_hooks(hooks: &[Hook], runtime: RuntimeId) -> Vec<Hook> {
    hooks
        .iter()
        .filter(|hook| agent_adapt::render::hook_refusal(hook, runtime).is_none())
        .cloned()
        .collect()
}

/// Replaces each `needle` in `text` with `with`, except where the next
/// character `continues` it into a longer token.
fn replace_bounded(
    text: &str,
    needle: &str,
    with: &str,
    continues: impl Fn(char) -> bool,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(offset) = rest.find(needle) {
        let (head, tail) = rest.split_at(offset);
        let after = &tail[needle.len()..];
        out.push_str(head);
        if after.chars().next().is_some_and(&continues) {
            out.push_str(needle);
        } else {
            out.push_str(with);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"{"hooks": {
        "SessionStart": [{"hooks": [{"type": "command", "command": "start", "timeout": 5}]}],
        "Stop": [{"matcher": "", "hooks": [{"command": "stop", "statusMessage": "bye"}]}]
    }}"#;

    #[test]
    fn a_canonical_file_parses_into_hooks() {
        let hooks = parse_hooks_file(FILE, "hooks.json").unwrap();
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].event, "SessionStart");
        assert_eq!(hooks[0].timeout_ms, 5000);
        assert_eq!(hooks[1].command.as_deref(), Some("stop"));
        assert_eq!(hooks[1].overrides["statusMessage"], "bye");
    }

    #[test]
    fn an_unknown_event_names_the_file_and_the_event() {
        let err = parse_hooks_file(r#"{"hooks": {"OnBoot": []}}"#, "plugin/hooks.json")
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown-hook-event"), "{err}");
        assert!(err.contains("plugin/hooks.json"), "{err}");
        assert!(err.contains("OnBoot"), "{err}");
    }

    #[test]
    fn a_malformed_entry_names_the_entry() {
        let err = parse_hooks_file(
            r#"{"hooks": {"Stop": [{"hooks": [{"command": 3}]}]}}"#,
            "hooks.json",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("malformed-hooks-file"), "{err}");
        assert!(err.contains("hooks.Stop[0].hooks[0].command"), "{err}");
    }

    #[test]
    fn a_timeout_too_large_for_milliseconds_is_refused() {
        let err = parse_hooks_file(
            r#"{"hooks": {"Stop": [{"hooks": [{"command": "x", "timeout": 9223372036854776}]}]}}"#,
            "hooks.json",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("malformed-hooks-file"), "{err}");
        assert!(err.contains("timeout` is too large"), "{err}");
        assert!(!err.contains("whole number"), "{err}");
    }

    #[test]
    fn a_command_hook_without_a_command_is_refused() {
        let err = parse_hooks_file(r#"{"hooks": {"Stop": [{"hooks": [{}]}]}}"#, "h.json")
            .unwrap_err()
            .to_string();
        assert!(err.contains("no `command`"), "{err}");
    }

    #[test]
    fn carried_hooks_leaves_out_what_the_runtime_does_not_carry() {
        let hooks = parse_hooks_file(
            r#"{"hooks": {"Stop": [{"hooks": [{"command": "a"}]}], "Notification": [{"hooks": [{"command": "b"}]}]}}"#,
            "h.json",
        )
        .unwrap();
        let cursor = carried_hooks(&hooks, RuntimeId::Cursor);
        assert_eq!(cursor.len(), 1);
        assert_eq!(cursor[0].event, "Stop");
        assert_eq!(carried_hooks(&hooks, RuntimeId::ClaudeCode).len(), 2);
    }

    fn command(text: &str) -> Hook {
        Hook::command("h", "h", "Stop", text)
    }

    #[test]
    fn localize_names_the_harness_variable() {
        let hook = localize(
            command("${PLUGIN_ROOT}/scripts/tool x ${PLUGIN_ROOT}"),
            "claude",
            Some("CLAUDE_PLUGIN_ROOT"),
        );
        assert_eq!(
            hook.command.as_deref(),
            Some("${CLAUDE_PLUGIN_ROOT}/scripts/tool x ${CLAUDE_PLUGIN_ROOT}")
        );
        let hook = localize(command("${PLUGIN_ROOT}/x"), "codex", Some("PLUGIN_ROOT"));
        assert_eq!(hook.command.as_deref(), Some("${PLUGIN_ROOT}/x"));
    }

    #[test]
    fn claude_plugin_root_is_refused_naming_the_source_form() {
        let err = parse_hooks_file(
            r#"{"hooks": {"Stop": [{"hooks": [{"command": "${CLAUDE_PLUGIN_ROOT}/x"}]}]}}"#,
            "hooks/hooks.json",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("claude-plugin-root"), "{err}");
        assert!(err.contains("hooks/hooks.json"), "{err}");
        assert!(err.contains("write `${PLUGIN_ROOT}`"), "{err}");
    }

    #[test]
    fn localize_fills_the_placeholders() {
        let hook = localize(
            command("run --harness {{harness}} --event {{event}}"),
            "cursor",
            None,
        );
        assert_eq!(
            hook.command.as_deref(),
            Some("run --harness cursor --event Stop")
        );
    }

    #[test]
    fn an_unclosed_placeholder_is_refused() {
        let err = parse_hooks_file(
            r#"{"hooks": {"Stop": [{"hooks": [{"command": "run {{harness"}]}]}}"#,
            "h.json",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("unknown-hook-placeholder"), "{err}");
        assert!(err.contains("`{{harness`"), "{err}");
    }

    #[test]
    fn an_unknown_placeholder_names_the_hook() {
        let err = parse_hooks_file(
            r#"{"hooks": {"Stop": [{"hooks": [{"command": "run {{harness}} {{foo}}"}]}]}}"#,
            "h.json",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("unknown-hook-placeholder"), "{err}");
        assert!(err.contains("hooks.Stop[0].hooks[0]"), "{err}");
        assert!(err.contains("{{foo}}"), "{err}");
    }
}
