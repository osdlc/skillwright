# Kind

Status: current.

What a note is: marketplace, plugin, skill, command or agent. Decided by its position in the layout (`marketplaces/<m>/`, `plugins/<p>/`, `skills/<s>/`, ...). Hooks are a plugin's `hooks/hooks.json`, not a kind of note.

## In code

`EntityKind` in `frontmatter.rs`; `KIND_DIRS` and `Position::kind` in `layout.rs`.
