# Hook

Status: current.

A command a harness runs on an event, from a plugin's `hooks/hooks.json`.
Claude Code and Codex carry hooks; Cursor and Pi do not.

## In code

`hooks_file.rs`; `PluginFormat::carries_hooks` in `harness/profile.rs`.
