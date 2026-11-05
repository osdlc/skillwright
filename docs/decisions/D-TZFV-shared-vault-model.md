# D-TZFV: Read the vault through osdlc's shared crates

Accepted 2026-11-04. Part of [D-COMG](D-COMG-marketplace-model-and-translators.md)
and [D-EPSA](D-EPSA-resolve-at-load.md). Follows osdlc's
[D-YRKH](https://github.com/osdlc/osdlc/blob/main/docs/decisions/D-YRKH-one-implementation-per-vault-concept.md):
one implementation per vault concept.

## Decision

- **skillwright keeps no vault code of its own.** Load reads the vault
  through osdlc's crates: `vfs-seam` for file access, `vfs-walk` and
  `markdown-vault-fs` for the walk, `markdown-link` for parsing, the link
  index and resolution, `markdown-contract` for heading and block selection.
  Renames use `markdown-corpus`, because the planner needs every note's
  links. Load does not: it needs only the walk's paths, and a corpus would
  read and scan every note to give them.
- **What stays in skillwright is its own:** skill calls (`/[[call]]`) as a
  layer over `markdown-link`'s tokens, component kinds, `sw-` frontmatter,
  and the refusals that guard publishing.
- **`tradecraft-core`'s `Vfs` is `vfs-seam`'s,** re-exported.
  `tradecraft-wasm`'s `JsVfs` implements it for JS consumers.
- **Asset and script walks use `vfs-walk` directly,** without vault rules.
- **Excluded files are honoured.** A note the vault excludes is not
  published. skillwright did not read the list before.
- **Gaps are fixed upstream, as options,** not worked around here.

## Why

Core kept its own walks, wikilink scanner, resolver and heading/block
extraction, because the shared crates called `std::fs` and core must run as
wasm. osdlc's crates now read through a trait, so that reason is gone.

## Options rejected

- Keep core's own code.
- Use the shared crates in the CLI only, and keep core's copies for wasm.

## Consequences

- osdlc releases first; skillwright then bumps to it.
- skillwright resolves a vault's links the same way determined does.
- `build --watch` can use `markdown-corpus`'s incremental refresh instead of
  a full rebuild.

## Reopen if

skillwright needs vault behaviour that can't be added upstream as an option.
