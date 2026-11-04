# Kind

Status: current.

What a component is: marketplace, plugin, skill, command, hook or agent. Decided by the note's `sw-kind`, else by the kind folder it sits in (`skills/`, `plugins/`, ...).

## In code

`EntityKind`, `resolve_kind` in `frontmatter.rs`; `KIND_DIRS` in `discover.rs`.
