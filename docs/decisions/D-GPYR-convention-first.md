# D-GPYR: Structure comes from convention

Accepted 2026-11-05. [Review](https://github.com/osdlc/skillwright/issues/4).

## Decision

A vault's API has two layers:

1. **Convention.** One fixed folder layout builds with no settings. Position
   alone decides kind, plugin and marketplace. The `kind` and `plugin` keys
   are removed. There are no globs, patterns or exclusions.
2. **Frontmatter.** Notes carry the fields harness manifests need.

`skillwright.md` is the only configuration file. It holds the shared defaults
and `build:`, the tool settings. A marketplace's fields go in its own note,
`marketplaces/<m>/<m>.md`. There is no machine file. Only `author`, `license`,
`repository`, `homepage`, `targets` and the harness blocks inherit. Output is
fixed at `dist/<m>/` beside `skillwright.md`, and never inside
`marketplaces/`, which is the only folder read as source.

## Why

One setting had several homes, and only the code said which one won. A build
needed settings even when every note already sat in a plugin folder. With one
file, one cascade and folders that decide structure, a user can predict a build
from the tree.

## Options rejected

- Keep `kind` and `plugin` as overrides of the folder. Two sources for one fact is
  the problem being fixed.
- A separate YAML config beside a marketplace note.
- A patterns layer of globs and exclusions for layouts that differ. It made
  the layout hard to predict, and no vault needed it.
- A settable output folder. A fixed `dist/<m>/` cannot land among the sources.

## Reopen if

Many vaults cannot fit the fixed layout.
