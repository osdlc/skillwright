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
| A default output folder | `build.out` or `--out`, required |

## Example

A vault before:

```yaml
# skillwright.yaml
vault: notes
marketplace:
  name: house
  owner: acme
  default_plugin: tools
  out: .
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
  out: .
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

A vault that cannot rename its keys can keep them: set `build.namespace: sw`.
Everything else on this page still applies.
