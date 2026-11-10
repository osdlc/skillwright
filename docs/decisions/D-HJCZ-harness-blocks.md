# D-HJCZ: Harness-only fields live in a top-level block per harness

Accepted 2026-11-05. [Review](https://github.com/osdlc/skillwright/issues/4).
Supersedes [D-ISOU](D-ISOU-harness-extensions.md).

## Decision

A note holds fields only one harness supports in a top-level block named for that
harness: `claude:`, `codex:`, `cursor:` or `pi:`. The blocks replace
`sw-overrides`. The model still holds them as a validated key/value map per
harness, and only that harness's translator reads the map.

The blocks work on marketplaces, plugins and components. They inherit down the
cascade key by key. An inherited block cannot set a component's `name` or
`description`. `metadata` in a block merges with the note's own.

## Why

Containers need harness fields too, such as Codex's `interface` and `policy`, and
a block reads the same at every level. A block named for the harness also takes
the place of one nested key in every note. The map stays because a new field is
still a change to a key list and not to the model's types.

## Options rejected

- Keep `sw-overrides`.
- A typed struct per harness.

## Reopen if

Translators start making decisions from these fields. Then type those fields.
