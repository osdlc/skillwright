# D-TZFV: Read the vault through shared osdlc crates

Accepted 2026-11-04. Part of [D-COMG](D-COMG-marketplace-model-and-translators.md)
and [D-EPSA](D-EPSA-two-phase-load.md).

## Decision

- **One file-access trait for everything.** Core's `Vfs` moves out into its
  own osdlc crate (name open). `tradecraft-core`, `markdown-vault-fs`,
  `markdown-relink` and `markdown-vault` read and write only through it.
  Each keeps a native implementation for existing callers.
- **`markdown-vault-fs` is the one vault walk.** It walks through the trait,
  not `walkdir` and `std::fs`. Its "`fs`, not `vfs`" rule is reversed.
- **New crate `markdown-vault`, extracted from determined.** It is
  `determined-notes`' `LinkGraph` and `NoteFacts`, moved to osdlc and put on
  the trait: every file, every note, outgoing links, backlinks, inline tags,
  link resolution through `markdown_link::LinkIndex`, and incremental
  refresh. It adds each note's document tree from `markdown-contract`
  (`comrak`, `native` off) for heading and block selection.
- **One implementation per concept.** A vault concept (walk, excluded
  files, link parsing, link resolution, the link graph, frontmatter split)
  lives in exactly one osdlc crate. determined, `markdown-relink` and
  skillwright depend on that crate and keep no copy of their own. A tool
  adds only what is its own, such as skillwright's skill calls.
- **Every vault model becomes a user of it:**
  - determined's `LinkGraph` and the walk behind `VaultGraph`.
  - `markdown-relink`: `VaultFacts::scan` and rename planning.
  - skillwright's load stage. Core drops its own walks, link scanner,
    resolver and heading/block extraction. Skill calls (`/[[call]]`) stay in
    skillwright as a layer over `markdown-link`'s tokens.
- **Sync now, async later.** The API is sync. An async layer, when needed,
  wraps it at the edges, as determined's port traits do today.
- **Gaps are fixed upstream, as options.** Known ones for `markdown-link`:
  skip links in HTML comments, refuse an ambiguous note name instead of
  taking the shallowest, and read `[[#Heading]]` as a link to the same note.
  Defaults stay as they are.
- **The APIs of `markdown-vault-fs` and `markdown-relink` may change** to
  fit this.

## Why

There are four vault link models: determined's `LinkGraph`,
`markdown-relink`'s `VaultFacts`, skillwright core's own scanner and
resolver, and the walk behind determined's `VaultGraph`. Core keeps its own
because the shared crates call `std::fs` and core must run as wasm.
`LinkGraph` is the most complete and already uses the shared crates.

## Options rejected

- Write `markdown-vault` new.
- Keep core's own code.
- Use the shared crates in the CLI only, and keep core's copies for wasm.

## Consequences

- osdlc work lands first: the trait crate, `markdown-vault-fs`,
  `markdown-link` options, `markdown-vault`, then `markdown-relink`.
  determined and skillwright then move onto them.
- Every tool resolves a vault's links the same way.
- skillwright's `build --watch` can use incremental refresh instead of a
  full rebuild.

## Reopen if

A host cannot implement the trait, for example a fully async file API.
