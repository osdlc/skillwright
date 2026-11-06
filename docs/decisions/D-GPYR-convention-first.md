# D-GPYR: Structure comes from convention first

Accepted 2026-11-05. [Review](https://github.com/osdlc/skillwright/issues/4).

## Decision

A vault's API has three layers:

1. **Convention.** A known folder shape builds with no settings. Folders alone
   decide kind and plugin. The `kind` and `plugin` keys are removed.
2. **Patterns.** `skillwright.md` adds globs and exclusions for layouts that
   differ. This layer is experimental.
3. **Frontmatter.** Notes carry the fields harness manifests need.

`skillwright.md` is the only configuration file. Its top-level keys are the
marketplace's fields and `build:` holds tool settings. There is no machine file.
Only `author`, `license`, `repository`, `homepage`, `targets` and the harness
blocks inherit. The output folder is required, so nothing builds into a vault
unless asked.

## Why

One setting had several homes, and only the code said which one won. A build
needed settings even when every note already sat in a plugin folder. With one
file, one cascade and folders that decide structure, a user can predict a build
from the tree.

## Options rejected

- Keep `kind` and `plugin` as overrides of the folder. Two sources for one fact is
  the problem being fixed.
- A separate YAML config beside a marketplace note.
- A default output folder. A build could then land in the vault by accident.

## Reopen if

Most vaults need patterns. Then the convention is wrong, or patterns should
become stable.
