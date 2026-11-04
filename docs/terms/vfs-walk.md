# vfs-walk

Status: planned.

The osdlc crate that walks a folder tree through the [Vfs](vfs.md) trait: sorted entries, prune and skip lists, an entry filter, a symlink policy, an error policy and modification times. It knows nothing about Markdown or vaults; callers pass their rules in ([D-TZFV](../decisions/D-TZFV-shared-vault-model.md)).

## In code

Not built yet. To be split out of `markdown-vault-fs`'s `walk` and `stat` modules. skillwright's asset and script walks and determined's attachment scan use it directly.

## Not to be confused with

`markdown-vault-fs`, the vault layer on top of it: notes versus attachments, visibility, `_system/` and the vault's excluded files (such as Obsidian's `userIgnoreFilters`).
