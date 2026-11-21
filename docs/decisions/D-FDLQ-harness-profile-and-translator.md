# D-FDLQ: Each harness is a profile and a translator

Accepted 2026-11-04. [Review](../parley/discuss-architecture.json). Part of
[D-COMG](D-COMG-marketplace-model-and-translators.md). Amended by
[D-WNTQ](D-WNTQ-harness-profile-values-from-config.md) and
[D-QXRN](D-QXRN-harness-facts-from-knowledge.md).

## Decision

- A **harness profile** is one row of data per harness in `tradecraft-core`:
  paths, manifest locations, naming rule, which kinds a pack carries and
  accepted harness-block keys. Tests check it
  against `harness-reference/`.
- One **harness translator** does the common work from the profile. What
  needs code, mostly manifests, is a `match` on the harness.
- Frontmatter dialects and tool names stay in `agent-adapt`.

## Why

Harness facts are in about eight places today, and they cover different
harnesses. Some differences are data and some are code, so a table alone
is not enough.

## Options rejected

- Generate the profile from `harness-reference/`.
- Move it all into `agent-adapt`.
- A generic translator driven only by data.
- A hand-written translator per harness.

## Reopen if

Most of the translator becomes a `match` on the harness.
