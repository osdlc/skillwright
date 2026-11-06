# `skillwright.md` reference

`skillwright.md` is the only configuration file. It is a Markdown note. The
folder holding it is the vault, and skillwright finds it by walking up from the
working directory, the way git finds a repository. `--config` names another
file, and `--vault` names the folder.

- Top-level keys are the marketplace's fields.
- `build:` holds settings for how skillwright runs.
- The body is ignored, so it can serve as the marketplace's README.
- skillwright never writes this file after `init` creates it. `bump` and the
  authoring verbs edit notes, not settings.
- An unknown key gets a warning and is never refused.

```markdown
---
name: osdlc
description: Skills for running a software project from Markdown.
owner: { name: osdlc, url: https://github.com/osdlc }
license: MIT
repository: https://github.com/osdlc/marketplace
homepage: https://osdlc.dev
version: 1.4.0
codex:
  policy: { installation: AVAILABLE, authentication: ON_INSTALL }
build:
  out: .
  harnesses: [claude, codex, cursor, pi]
  exclude: [docs/**, "**/*.draft.md"]
  patterns:
    plugins: packs/*
---

# osdlc marketplace

Anything here is the marketplace's own README.
```

## Marketplace fields

The top-level keys are listed under
[Marketplace in the field reference](fields.md#marketplace). Top-level keys are
always bare, even when `build.namespace` is set.

`author`, `license`, `repository`, `homepage`, `targets` and the harness blocks
are also defaults for every plugin and component below. See
[where a value comes from](#where-a-value-comes-from).

## `build:`

| Key | Value | Meaning |
|---|---|---|
| `out` | path | Where harness trees are written. Required here or as `--out`. There is no default. `.` builds beside this file. |
| `harnesses` | list | Which harnesses to build. Absent means all of them. |
| `namespace` | stem | Restores prefixed keys: `sw` reads `sw-version` or a `sw:` block. Absent means bare keys. |
| `launcher_path` | path | The plugin's launcher, relative to the plugin's root. |
| `require_descriptions` | boolean | Refuse a note with a `name` and no `description`, instead of publishing a filler. |
| `destinations` | list of `{root, target, scope}` | Where `publish` writes when `--to` is not given. Empty means each harness's user skill folder, such as `~/.claude/skills`. `scope` is `project` or `user`, and is informational. |
| `scan_locations` | list of paths | Folders a bare `adopt` reads. Empty means each harness's user skill folder. |
| `assets` | list of `{from, to, plugin}` | Files copied into a plugin's root, besides each plugin's `assets/`. |
| `hooks` | list of `{from, plugin}` | Canonical hooks files. |
| `exclude` | glob or list | Experimental. What never ships, added to the folders that never ship. |
| `patterns` | mapping | Experimental. Globs that stand in for the conventional folders. |

Relative paths resolve against this file's folder. A flag's path resolves against
the working directory. Flags override these keys one by one.

## Patterns

Experimental: this layer may change after 1.0. The folder convention and the
frontmatter fields are stable.

`build.patterns` names things in a layout that differs from the convention. The
convention keeps working beside it.

```yaml
build:
  patterns:
    plugins: packs/*        # each folder under packs/ is a plugin
    skills: notes/*         # each note or folder under notes/ is a skill
    assets: packs/*/files   # each match is a plugin's assets folder
  exclude:
    - "**/*.draft.md"
```

The keys are `plugins`, `skills`, `agents`, `commands`, `hooks` and `assets`, and
they always sit under `build.patterns`, not straight under `build:`. Each takes
one glob or a list. `*` and `?` match within one path segment, and `**` as a whole
segment matches any number of segments. Globs are relative to this file.

- An invalid glob is refused. A glob that matches nothing gets a warning.
- A folder that never ships stays out whatever a pattern says, with a warning.
- Generated output is never read as source.

`exclude` adds to the folders that never ship. It does not apply to a plugin's
`assets/`. A body link or embed that reaches a never-ship or excluded file is
refused with the same diagnostic in every link form, so a skill cannot point at a
file that will not be there.

Patterns are read per marketplace and are not inherited from a workspace. The
authoring verbs (`new`, `move`, `convert`) always write to the conventional
folders.

## Several marketplaces

Keep one `skillwright.md` per marketplace under `marketplaces/<m>/`. An outer
`skillwright.md` with `workspace: true` holds shared defaults, and `build` and
`check` there run over every marketplace. `--marketplace <name>` picks one.

```text
workspace/
├── skillwright.md            workspace: true
└── marketplaces/
    ├── public/skillwright.md
    └── internal/skillwright.md
```

A marketplace's own value wins, key by key. `build.out` is a path, so each
marketplace sets its own.

## Where a value comes from

The first of these that has a value wins:

1. A command-line flag, for that run only.
2. The note's own frontmatter.
3. The nearest container: the plugin note, then `skillwright.md`, then the
   workspace. This applies only to inherited fields: `author`, `license`,
   `repository`, `homepage`, `targets`, and the harness blocks key by key.
4. Patterns in `skillwright.md`, for structure only.
5. The folder convention: the name from the file or folder, the kind from the
   kind folder, the plugin from `plugins/<p>/`.
6. The environment. The editor comes from `$VISUAL` or `$EDITOR`, and `publish`
   and `adopt` default to each harness's user skill folder. Nothing here changes
   what a build emits.
7. Built-in defaults: `version` 0.1.0 with a warning, `license` UNLICENSED, every
   harness.

Inheritance works the same on every path: `build`, `publish` of one note, and
`publish` of a whole pack.

## Harness blocks

`claude:`, `codex:`, `cursor:` and `pi:` hold fields only that harness reads. On
a marketplace an example is Codex's `policy`, on a plugin Codex's `interface`, on
a skill Claude's `allowed-tools` patterns. Each block is checked against the keys
that harness accepts, merged over what skillwright generates, and emitted only
for that harness. The accepted keys are [in the field reference](fields.md#harness-blocks).

- `metadata` in a block merges with the note's `metadata` rather than replacing
  it.
- A block a component inherits cannot set that component's `name` or
  `description`. A block on the note itself can.
- `<harness>: {packs: <path>}` on the marketplace moves that harness's packs
  folder inside the output. Root catalogs stay where they are. The value that
  takes effect is refused if it overlaps a manifest path, a folder the harness
  owns, or, when the output overlaps the vault, a source folder.
- `pi: {naming: bare}` on the marketplace or a plugin publishes Pi skills under
  their bare names. `prefixed` is the default.
