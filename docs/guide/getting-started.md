# Getting started

This guide takes you from an empty folder to a built marketplace. It needs the
`skillwright` command. See the [README](../../README.md#install) to install it.

## 1. Make the vault

Run this in an empty folder:

```sh
skillwright init my-marketplace review
```

It writes a vault with one marketplace, `my-marketplace`, and one plugin,
`review`:

```text
./
├── skillwright.md                        the root file
└── marketplaces/
    └── my-marketplace/
        ├── my-marketplace.md             the marketplace note
        └── plugins/
            └── review/
                └── review.md             the plugin note
```

- Each marketplace, plugin and component is a folder with a note of the same
  name. The folder's name is its name.
- A plugin's skills go in `skills/<s>/<s>.md`. Agents go in
  `agents/<a>/<a>.md`, commands in `commands/<c>/<c>.md` and hooks in
  `hooks/hooks.json`.
- `init` refuses a folder that already holds `skillwright.md`.

See the [structure spec](../spec/structure-v2.md) for every rule.

## 2. Write a skill

Create `marketplaces/my-marketplace/plugins/review/skills/triage/triage.md`:

```markdown
---
description: Sort new bug reports by severity. Use when asked to triage issues.
---

Read each open issue and label it low, medium or high.
```

`description` is required. Every other key is optional; see the
[field reference](../reference/fields.md).

The plugin note, `review.md`, holds the plugin's `version`, which `init` sets
to `0.1.0`. A harness compares versions to decide whether an installed plugin
is out of date.

Fields you want on every plugin can go in `skillwright.md`, and each plugin
and skill inherits them:

```markdown
---
license: MIT
repository: https://github.com/me/my-marketplace
---
```

## 3. Build

```sh
skillwright build --yes    # write the output
skillwright check          # exits 1 if the output differs from the vault
```

`build` writes `dist/my-marketplace/`, with one tree per harness:
`.claude-plugin/` (Claude Code), `.agents/` (Codex), `.cursor-plugin/`
(Cursor) and `.pi/` (Pi). It replaces only each harness's packs folder and
the manifests it writes. Without `--yes`, `build` shows what it will write and
asks first.

Set `build.harnesses` in `skillwright.md` to build for fewer harnesses, and
`targets` on a note to limit that note.

## 4. Release a change

After you change a plugin, raise its version and build again:

```sh
skillwright bump review --yes
skillwright build --yes
```

`bump --minor` and `bump --major` raise the other parts.
`bump --changed-since <ref>` bumps every plugin that changed since a git ref.

## 5. Ship the output

The build stops at `dist/`. Copy `dist/my-marketplace/` to the repository a
harness installs from, or commit it there, and run `skillwright check` in CI
so the committed output never drifts from the notes.

## Next

- [`skillwright.md` reference](../reference/skillwright-md.md)
- [Field reference](../reference/fields.md)
- [Structure spec](../spec/structure-v2.md)
