# D-EPSA: Resolve links and calls when loading, not when translating

Accepted 2026-11-04. [Review](../parley/discuss-architecture.json). Part of
[D-COMG](D-COMG-marketplace-model-and-translators.md).

## Decision

Loading has two passes:

1. **Read** every note into a model node.
2. **Resolve** every `[[link]]` and `/[[call]]` to the node it names. A broken
   one is a validation error.

Translators only decide how to write a resolved reference for their harness,
for example `/skillwright:setup` on Claude and `skillwright-setup` on Pi.

## Why

Today `compile.rs` writes calls in one harness's syntax, so `build` reads
and compiles every note once per harness.

## Options rejected

- Placeholder tokens in the body, filled in by translators.
- One compiled body per harness in the model.

## Consequences

- Each note is read and compiled once.
- Every verb reports broken links and calls the same way.

## Reopen if

Whether a link is valid depends on the harness.
