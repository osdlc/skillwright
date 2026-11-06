# Migrating a vault

This page is for an agent, or a person with one, moving a vault to the
`skillwright.md` structure. skillwright has no `migrate` verb and reads none of
the old shape, so migrate in one pass.

## How to run it

With the skillwright plugin installed, run its `migrate` skill.

Without the plugin, follow the same steps in
[`migrate.md`](../../plugin/plugins/skillwright/skills/migrate/migrate.md). That
file is the only copy of the procedure and its key tables.

## What changes

| Before | After |
|---|---|
| `skillwright.yaml` and a machine file | One `skillwright.md`, in the vault folder |
| `sw-version`, `sw-targets`, … | `version`, `targets`, … |
| `sw-kind`, `sw-plugin` | The note's folder |
| `sw-overrides: {claude: …}` | A `claude:` block |
| Six `sw-` provenance keys | One `adopted:` block |
| A default output folder | `build.out` or `--out`, required to build |
| `marketplace.shim_target` | `build.launcher_path` |
| A `scripts/<name>` shim in each skill | No shim: calls to `${CLAUDE_PLUGIN_ROOT}/<launcher>` become paths relative to the skill on Codex, Cursor and Pi |

The last row changes many built files in a vault with a launcher. The `migrate`
skill's verify step lists every difference to expect.

## Example

A vault before:

```yaml
# skillwright.yaml
vault: notes
marketplace:
  name: house
  owner: acme
  default_plugin: tools
  out: .                # relative to skillwright.yaml: the repository root
```

```markdown
<!-- notes/skills/lint.md -->
---
description: Lint the code.
sw-targets: [claude-code]
sw-overrides:
  claude:
    allowed-tools: Bash(cargo:*)
---
```

The same vault after:

```markdown
<!-- notes/skillwright.md -->
---
name: house
owner: acme
build:
  out: ..               # relative to this file: still the repository root
---
```

```markdown
<!-- notes/plugins/tools/skills/lint.md -->
---
description: Lint the code.
targets: [claude-code]
claude:
  allowed-tools: Bash(cargo:*)
---
```

The file moved down into `notes/`, which changes two things:

- `out` is relative to `skillwright.md`, so `.` became `..`. With `.`, the build
  would succeed and land in `notes/`.
- skillwright finds the file by walking up, so a command run from the repository
  root no longer finds it. Run from `notes/`, or pass `--config notes/skillwright.md`.

## Keeping the prefix

A vault that cannot rename its keys can keep them by setting
`build.namespace: sw`. Under a namespace every key skillwright owns takes the
prefix, so the blocks do too:

| Bare vault | Namespaced vault |
|---|---|
| `version`, `targets` | `sw-version`, `sw-targets` |
| `claude:` | `sw-claude:` |
| `adopted:` | `sw-adopted:` |

With no namespace set, a leftover `sw-version` or any other `sw-<key>` spelling of
a key skillwright owns is refused `removed-key`, so nothing ships wrong by accident.

A bare `claude:` in a namespaced vault does not reach Claude, and a bare `adopted:`
would ship its source path under `metadata:`. Keys in `skillwright.md` itself are
always bare.
