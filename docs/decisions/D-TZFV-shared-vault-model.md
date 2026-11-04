# D-TZFV: Read the vault through shared osdlc crates

Accepted 2026-11-04. Part of [D-COMG](D-COMG-marketplace-model-and-translators.md)
and [D-EPSA](D-EPSA-two-phase-load.md).

## Decision

- **One file-access trait for everything.** Core's `Vfs` moves out into its
  own osdlc crate (name open). `tradecraft-core`, `markdown-vault-fs`,
  `markdown-relink` and `markdown-vault` read and write only through it.
  Each keeps a native implementation for existing callers.
- **`markdown-vault-fs` walks through the trait.** Its walk stops calling
  `walkdir` and `std::fs` directly. Its "`fs`, not `vfs`" rule is reversed.
- **New crate `markdown-vault`:** the vault model shared by every osdlc
  tool. It holds every file, every note, each note's outgoing and incoming
  links, link resolution, and each note's document tree.
  - Walk and excluded files from `markdown-vault-fs`.
  - Parsing and resolution from `markdown-link`.
  - Document tree from `markdown-contract` (`comrak`), with `native` off.
- **`markdown-relink` builds on it.** `VaultFacts::scan` and rename
  planning read through `markdown-vault`, not `std::fs`.
- **skillwright's load stage builds on it.** Core drops its own walks, link
  scanner, resolver and heading/block extraction. Skill calls (`/[[call]]`)
  stay in skillwright as a layer over `markdown-link`'s tokens.
- **Gaps are fixed upstream, as options.** Known ones for `markdown-link`:
  skip links in HTML comments, refuse an ambiguous note name instead of
  taking the shallowest, and read `[[#Heading]]` as a link to the same note.
  Defaults stay as they are.

## Why

Core has about five vault walks, its own wikilink scanner and its own
resolver, because the shared crates call `std::fs` and core must run as
wasm. Two copies of link rules give two answers to the same vault.

## Options rejected

- Keep core's own code.
- Use the shared crates in the CLI only, and keep core's copies for wasm.

## Consequences

- Work in osdlc lands and is released first: the trait crate,
  `markdown-vault-fs`, `markdown-link` options, `markdown-vault`, then
  `markdown-relink`.
- Every tool that reads a vault resolves its links the same way.

## Reopen if

A host cannot implement the trait, for example a fully async file API.
