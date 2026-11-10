# Getting started

This guide takes you from an empty folder to a built marketplace. It needs the
`skillwright` command. See the [README](../../README.md#install) to install it.

## 1. Lay out the folder

The folders decide what each note is. A small marketplace looks like this:

```text
my-marketplace/
├── skillwright.md            the root file
└── plugins/
    └── review/
        ├── review.md         optional plugin note
        └── skills/
            ├── lint/SKILL.md
            └── triage.md
```

- A folder under `plugins/` is a plugin, named after the folder.
- A note under `skills/` is a skill. Agents go in `agents/`, commands in
  `commands/` and hooks in `hooks/`.
- A `skills/` folder at the top is a plugin named after the marketplace. It
  takes its `version` from `skillwright.md`.

See the [structure spec](../spec/structure-v1.md) for every rule.

## 2. Write the root file

Run this in the folder:

```sh
skillwright init --out dist
```

It writes a `skillwright.md` with the marketplace's name and its output folder:

```markdown
---
name: my-marketplace
build:
  out: dist
---
```

The name defaults to the folder's name; `--name` sets another. On a terminal,
`init` asks for the output folder when `--out` is missing. It refuses to replace
an existing `skillwright.md`.

The output folder is required, here or as `--out` on each run. There is no
default. `out: .` builds beside `skillwright.md`, which makes the repository
installable as a marketplace.

## 3. Write a skill

Create `plugins/review/skills/triage.md`:

```markdown
---
description: Sort new bug reports by severity. Use when asked to triage issues.
---

Read each open issue and label it low, medium or high.
```

`description` is what opts a note in. `name` defaults to the file's name. Every
other key is optional; see the [field reference](../reference/fields.md).

Give the plugin a version in `plugins/review/review.md`. A harness compares
versions to decide whether an installed plugin is out of date, so a plugin with
none is published as `0.1.0` with a `missing-version` warning:

```markdown
---
description: Review helpers.
version: 0.1.0
---
```

Fields you want on every plugin can go in `skillwright.md`, and each plugin and
skill inherits them:

```markdown
---
name: my-marketplace
license: MIT
repository: https://github.com/me/my-marketplace
build:
  out: dist
---
```

## 4. Build

```sh
skillwright list           # what the vault holds
skillwright build --yes    # write the output
skillwright check          # exits 1 if the output differs from the vault
```

`build` writes one tree per harness under the output folder: `.claude-plugin/`
(Claude Code), `.agents/` (Codex), `.cursor-plugin/` (Cursor) and `.pi/` (Pi).
It replaces only each harness's packs folder and the manifests it writes.

Use `build.harnesses` to build for fewer harnesses, and `targets` on a note to
limit that note.

## 5. Install

```sh
skillwright harness install claude --yes
```

`codex` works the same way. Cursor also needs `--git-url <url>`, because it only
registers a marketplace from git. `harness install pi` runs `pi install` for each
built pack under `.pi/packs/`.

To write skills straight into a harness's user folder instead of building a
marketplace, run `skillwright publish --yes`. It writes each skill on its own and
no plugin manifests. `publish --to <root> --target <harness>` writes to one
folder instead, which must sit outside the vault.

`publish <path>` also publishes a note that sits outside every kind folder, as a skill. Only that
command does: `list`, `check`, `build` and `publish --all` leave it out. To bring a folder in for
every command, add a `build.patterns` glob. See
[Publishing one note by path](../reference/skillwright-md.md#publishing-one-note-by-path).

## Next

- [`skillwright.md` reference](../reference/skillwright-md.md)
- [Field reference](../reference/fields.md)
- [Structure spec](../spec/structure-v1.md)
