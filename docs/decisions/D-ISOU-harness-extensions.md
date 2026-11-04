# D-ISOU: Harness-only fields are a validated map per harness

Accepted 2026-11-04. [Review](../parley/discuss-architecture.json). Part of
[D-COMG](D-COMG-marketplace-model-and-translators.md).

## Decision

A model node holds fields that only one harness supports (such as Claude's
`disable-model-invocation`) in a key/value map for that harness. The map
comes from `sw-overrides` and is checked on load against the keys that
harness accepts. Only that harness's translator reads it.

## Why

It matches how `sw-overrides` works today. A new field is a change to a key
list, not to the model's types.

## Options rejected

- A typed struct per harness.
- Typed fields for some keys, a map for the rest.

## Reopen if

Translators start making decisions from these fields. Then type those
fields.
