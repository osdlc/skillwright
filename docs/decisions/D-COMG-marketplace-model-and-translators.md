# D-COMG: One marketplace model, translated per harness

Accepted 2026-11-04. [Review](../parley/discuss-architecture.json).

## Decision

`build` and `check` run one pipeline: **load** the vault into a marketplace
model, **validate** it, **translate** it once per harness into a file tree,
and **emit** the files, or diff them for `check`.

- The model is an in-memory object graph.
- It is a public API: the wasm export `loadMarketplace` returns a narrow view
  of it.

## Why

Two pipelines that each read, compile and write drift apart: one ignores a
key the other reads, or writes packs for fewer harnesses. One model and one
translator per harness keep every output in step.

## Options rejected

- Keep a pipeline per verb and merge their harness tables.

## Consequences

- `build` and `check` produce the same output for the same note.
- A new harness is added in one place.
- Changing the model changes the TypeScript types. The Obsidian plugin and
  agent-pants must follow.
- Unchanged: the `Vfs` seam and refusal tokens.

## Reopen if

A harness's output cannot be produced from a harness-neutral model.
