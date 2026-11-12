# D-EPSA: Resolve links and calls when loading, not when translating

Accepted 2026-11-04. [Review](../parley/discuss-architecture.json). Part of
[D-COMG](D-COMG-marketplace-model-and-translators.md).

## Decision

Loading reads the vault once, through the shared vault reader
([D-TZFV](D-TZFV-shared-vault-model.md)), and resolves every `[[link]]` and
`/[[call]]` to the node it names before any translator runs. A broken one is
a validation error.

How many passes that takes is a detail. Today it is two: read every note,
then resolve the bodies from the notes already in memory. A call needs the
target's kind and name, and an embed needs the target's body.

Translators only decide how to write a resolved reference for their harness,
for example `/skillwright:setup` on Claude and `/skill:skillwright-setup` on Pi.

## Why

Before this, `compile.rs` wrote calls in one harness's syntax, so `build` read
and compiled every note once per harness.

## Options rejected

- Placeholder tokens in the body, filled in by translators.
- One compiled body per harness in the model.

## Consequences

- Each note is read and compiled once.
- Every verb reports broken links and calls the same way.

## Reopen if

Whether a link is valid depends on the harness.
