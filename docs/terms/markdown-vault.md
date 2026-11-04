# markdown-vault

Status: planned.

The osdlc crate that models a vault: every file, every note, each note's outgoing and incoming links, link resolution, and each note's document tree. Shared by skillwright, `markdown-relink` and other osdlc tools. Reads files only through the shared [Vfs](vfs.md) trait ([D-TZFV](../decisions/D-TZFV-shared-vault-model.md)).

## In code

Not built yet. To be extracted from `LinkGraph` and `NoteFacts` in determined's `determined-notes/src/links.rs`. Other copies today: `VaultFacts` in `markdown-relink`, and core's own walks and resolver.

## Not to be confused with

`markdown-vault-fs`, which only walks files. It becomes one part of this.
