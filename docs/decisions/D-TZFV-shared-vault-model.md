# D-TZFV: Read the vault through shared osdlc crates

Accepted 2026-11-04. Part of [D-COMG](D-COMG-marketplace-model-and-translators.md)
and [D-EPSA](D-EPSA-resolve-at-load.md).

## Decision

- **One file-access trait for everything.** Core's `Vfs` moves out into its
  own osdlc crate, `vfs-seam`: the trait and its backends (native, memory,
  preview), nothing else. `tradecraft-core`, `vfs-walk`, `markdown-vault-fs`,
  `markdown-relink` and `markdown-vault` read and write only through it.
- **One general walk: new crate `vfs-walk`.** A sorted walk over the trait,
  with prune and skip lists, an entry filter, a symlink policy, an error
  policy and modification times. It knows nothing about Markdown or
  Obsidian; the caller passes its rules in. It is separate from `vfs-seam`
  so walk changes never force a breaking release of the trait every crate
  depends on.
- **`markdown-vault-fs` is the vault layer on that walk:** notes versus
  attachments, the visibility presets, the `_system/` rules, and Obsidian's
  excluded files. It keeps its walk functions as thin wrappers.
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
- **Excluded files are honoured by every tool.** Reading the list from
  `.obsidian/app.json` (`userIgnoreFilters`) moves from `determined-vault`
  into `markdown-vault-fs`, which passes it to `vfs-walk` as a filter. Every
  vault walk applies it. skillwright does not read
  it today, so a note excluded in Obsidian is still published.
- **One bad note never stops a walk.** `determined-vault`'s `read_note`,
  which reads a malformed frontmatter block as body-only, moves into
  `markdown-vault`.
- **`determined-vault` stays as determined's own wrapper:** vault location
  from `vault.config.yaml`, canonical YAML output for generated documents,
  and sidecar files.
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
- The `vfs` crate instead of `vfs-seam`: no symlinks, no execute bit, and it
  requires `Send + Sync`, which the JS host cannot meet.
- The general walk inside `vfs-seam` or inside `markdown-vault-fs`: the
  first makes the trait crate change with the walk, the second makes a
  plain file walk depend on vault rules.

## Consequences

- osdlc work lands first: `vfs-seam`, `vfs-walk`, `markdown-vault-fs`,
  `markdown-link` options, `markdown-vault`, then `markdown-relink`.
  determined and skillwright then move onto them.
- Every tool resolves a vault's links the same way.
- skillwright's `build --watch` can use incremental refresh instead of a
  full rebuild.

## Reopen if

A host cannot implement the trait, for example a fully async file API.
