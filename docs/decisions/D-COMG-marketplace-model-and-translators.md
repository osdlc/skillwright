# D-COMG: One marketplace model, translated per harness

Accepted 2026-11-04. [Review](../parley/discuss-architecture.json).

## Decision

Every verb runs one pipeline: **load** the vault into a marketplace model,
**validate** it, **translate** it once per harness into a file tree, and
**emit** the files.

- The model is an in-memory object graph.
- It is a public API: exported through wasm and printed by `--json`.
- `adopt` runs the pipeline in reverse: harness files → model → notes.

## Why

`build` (`export.rs`) and `publish` (`publish.rs`) are two pipelines that
each read, compile and write. They have drifted: `build` ignores
`sw-overrides`, and `publish` writes plugin packs for only two harnesses.

## Options rejected

- Keep both pipelines and merge their harness tables.
- Use the model for `build` and `check` only.

## Consequences

- All verbs produce the same output for the same note.
- A new harness is added in one place.
- Changing the model changes the TypeScript types. The Obsidian plugin and
  agent-pants must follow.
- Unchanged: the `Vfs` seam, the write guards, the destination manifest,
  refusal tokens.

## Reopen if

A harness's output cannot be produced from a harness-neutral model.
