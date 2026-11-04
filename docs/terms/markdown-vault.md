# markdown-vault

Status: planned.

The osdlc crate that models a vault: every file, every note, each note's outgoing and incoming links, link resolution, and each note's document tree. Shared by skillwright, `markdown-relink` and other osdlc tools. Reads files only through the shared [Vfs](vfs.md) trait ([D-TZFV](../decisions/D-TZFV-shared-vault-model.md)).

## In code

Not built yet. Today the pieces are in `markdown-vault-fs` (walk), `markdown-link` (links), `markdown-contract` (document tree) and `markdown-relink` (`VaultFacts`), and core keeps its own copies.

## Not to be confused with

`markdown-vault-fs`, which only walks files. It becomes one part of this.
