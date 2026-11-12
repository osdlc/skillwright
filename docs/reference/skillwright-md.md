# `skillwright.md` reference

`skillwright.md` is the root file and the only configuration file. It is a
Markdown note at the vault root, beside `marketplaces/`:

```text
<root>/
├── skillwright.md            the root file
├── dist/                     build output
└── marketplaces/<m>/
    ├── <m>.md                the marketplace
    └── plugins/<p>/<p>.md    a plugin
```

- skillwright finds the root by walking up from the working directory to the
  first folder that holds both `skillwright.md` and `marketplaces/`.
  `--vault <path>` names the root instead.
- Its frontmatter holds two kinds of key: the shared defaults, and `build:`,
  the tool settings.
- Its body is not read.
- `init` writes it with no keys, so every default holds. No command writes it
  after that. `bump` edits the `version` in `<m>.md` and `<p>.md`.

A complete root file, with every key it may hold:

```markdown
---
author: Ada Lovelace # or {name, email, url}
license: MIT
repository: https://github.com/acme/skills
homepage: https://acme.dev/skills
claude: {}
codex: {}
cursor: { naming: bare } # keep plain skill names on Cursor
pi: {}
build:
  harnesses: [claude, codex, cursor, pi] # the default; absent means all four
  bump:
    patch: increment # or timestamp
---

# Anything here is ignored.
```

## Shared defaults

The shared defaults are the keys the [field reference](fields.md) marks as
inherited. Every marketplace, plugin and component takes each one it does not
set itself. See [where a value comes from](#where-a-value-comes-from).

| Key | Value |
|---|---|
| `author` | a name, or `{name, email, url}` |
| `license` | an SPDX expression |
| `repository` | the source repository URL |
| `homepage` | the documentation URL |
| `claude`, `codex`, `cursor`, `pi` | harness blocks: fields only that harness reads. See [Harness blocks](#harness-blocks). |

The marketplace and plugin keys that are not inherited, such as `owner`,
`version` and `description`, go in `<m>.md` and `<p>.md`. The
[field reference](fields.md#marketplace) lists them.

## `build:`

`build:` holds settings for how skillwright runs. It is read only here; in
any other note it is refused.

| Key | Value | Meaning |
|---|---|---|
| `harnesses` | list | Which harnesses to build by default: `claude`, `codex`, `cursor`, `pi`. Absent means all four. `--harness`, or a core request's `harnesses`, replaces it for that run. Any other name is refused. |
| `bump` | `{patch}` | How `bump` writes the patch number. `increment`, the default, adds one: `1.2.3` becomes `1.2.4`. `timestamp` sets it to the UTC time as `YYYYMMDDHHMMSS`, or to one more than the current patch when that is larger: `1.2.3` becomes `1.2.20261110153000`. Any other value is refused. |

Output is fixed at `<root>/dist/<m>/`. No key moves it.

## Warnings

An unknown key warns and is ignored. The build goes on.

- A top-level key the field reference does not list, such as `bump:` written
  outside `build:`.
- A `build:` key other than `harnesses` and `bump`, unless it is one of the
  removed keys below.
- A key under `build.bump` other than `patch`.
- A harness-block key that harness does not read on a marketplace.

## What is refused

Each of these stops `build`, `check` and `bump` with an error that names the
key. They exit 2 and write nothing.

- `name`, also in a harness block. The folder's name is the name.
- `workspace`. There is one root file, and every marketplace sits under it.
- `targets`, here and on every note. Pick the harnesses with `build.harnesses`
  or `--harness`.
- A key the field reference lists but does not mark as inherited, such as
  `version`, `owner` or `description`. The message names the notes that take
  it.
- The removed `build:` keys: `out`, `patterns`, `exclude`, `assets`, `hooks`,
  `destinations`, `scan_locations`, `launcher_path` and `namespace`.
- `packs` in a harness block. Each harness's packs folder is fixed.
- A name in `build.harnesses` that is not `claude`, `codex`, `cursor` or
  `pi`.
- A known key with the wrong kind of value, such as `license: [MIT]`.
- Frontmatter that is not valid YAML, or that opens with `---` and never
  closes.

## Reading it from a program

`parse_root_file` in `tradecraft-core`, and `parseRootFile` in
`@osdlc/tradecraft-wasm`, read the file's text into three fields:

| Field | Holds |
|---|---|
| `defaults` | the shared defaults, with the harness blocks under `blocks` |
| `build` | `harnesses` and `bump` |
| `warnings` | one line per key that warned |

A refused key is an error, not a warning.

## Where a value comes from

The first of these that has a value wins:

1. The note itself.
2. The plugin note, `<p>.md`.
3. The marketplace note, `<m>.md`.
4. The root file's shared defaults.
5. The folder position: the name from the folder, the kind from the kind
   folder, the plugin from `plugins/<p>/`.
6. A built-in default: a plugin's `version` is `0.1.0` with a warning, and
   `license` is `UNLICENSED`. A marketplace with no `version` writes none,
   with no warning.

Only the inherited keys pass down: `author`, `license`, `repository`,
`homepage` and the harness blocks, key by key. Every other key
comes only from the note itself. Containers are matched by folder.

For an inherited key that a harness reads, the harness block wins over the
plain key at each level, nearest first:

1. the note's own harness block;
2. the note's frontmatter;
3. for each container, nearest first: its harness block, then its
   frontmatter.

So a plugin's `claude: {author: Bob}` beats its plain `author: Ada` for Claude
Code only, and its plain `author` beats a marketplace's
`claude: {author: …}`.

## Harness blocks

`claude:`, `codex:`, `cursor:` and `pi:` hold fields only that harness reads.
On a marketplace an example is Codex's `policy`, on a plugin Codex's
`interface`, on a skill Claude's `allowed-tools`. Each block is checked
against the keys that harness accepts, merged over what skillwright
generates, and emitted only for that harness. The accepted keys are
[in the field reference](fields.md#harness-blocks).

- A key the block does not accept warns and is left out.
- A key the block accepts must hold the kind of value that harness documents.
  A wrong kind is refused, naming the block, the key and the kind. Every block
  is checked, whichever harnesses `build.harnesses` selects.
- A key set to nothing is dropped. It does not clear a value the note
  inherits.
- `metadata` in a block merges with the note's `metadata`.
- Some keys of a container's block stay with the container: the keys of that
  harness's plugin manifest and marketplace entry, and on a marketplace the
  keys of its marketplace manifest. So a block a component inherits cannot
  set its `description` or `version`.
- `name` and `packs` are refused in every block.

### Skill names on Cursor and Pi

Cursor and Pi have no plugin namespace, so skillwright names a plugin's skill
`<p>-<s>` there: its folder, its `name` and every call to it. Claude Code and
Codex keep `<s>`, because they put the plugin name in front themselves.

- `cursor: {naming: bare}` or `pi: {naming: bare}` keeps the plain `<s>` on
  that harness. The values are `bare` and `prefixed`, the default.
- `naming` may be set in the root file, a marketplace or a plugin, and
  inherits like any other block key. On a component it is refused: set it on
  the plugin.
- Two skills that end up with one name on a harness stop the build with
  `path-collision`, naming both plugins. A prefixed name over 64 characters is
  refused.

## What reaches each harness

| Kind | Claude Code | Codex | Cursor | Pi |
|---|---|---|---|---|
| Skill | yes | yes | yes | yes |
| Command | yes | no | no | no |
| Hooks, from `hooks/hooks.json` | yes | yes | no, for now, with a warning | no |
| Agent | no | no | no | no |

No pack holds an agent. An agent note is still read and checked by every
build.

## What a manifest holds

Each manifest keeps to what that harness documents:

- Claude's marketplace entry has no `version`. `plugin.json` holds it: with
  both, that one wins and `claude plugin validate` warns.
- Cursor gets no `author.url` or `owner.url`, because its docs and schema list
  only `name` and `email`.
- Cursor's marketplace entry keeps `version`, `category` and `tags`. Cursor's
  docs list them, and the entry is merged with `plugin.json`. The schema of
  the `cursor/plugins` repository rejects them, so a listing there needs them
  removed.
