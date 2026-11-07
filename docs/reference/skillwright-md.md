# `skillwright.md` reference

`skillwright.md` is the only configuration file. It is a Markdown note. The
folder holding it is the vault, and skillwright finds it by walking up from the
working directory, the way git finds a repository. `--config` names another
file, and `--vault` names the folder.

- Top-level keys are the marketplace's fields.
- `build:` holds settings for how skillwright runs.
- The body is ignored, so it can serve as the marketplace's README.
- After `init` creates this file, skillwright writes only its `version`, with
  `bump <marketplace>`. Its keys are never prefixed, so that key is `version`
  under any `build.namespace`. The authoring verbs edit notes, not settings.
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
| `harnesses` | list | Which harnesses to build: `claude`, `codex`, `cursor`, `pi`. Absent means all of them. The long spellings `targets` takes, `claude-code` and `codex-cli`, are refused here. |
| `namespace` | stem | Restores prefixed keys: `sw` reads `sw-version` or a `sw:` block. Absent means bare keys. |
| `launcher_path` | path | The plugin's launcher, relative to the plugin's root. |
| `bump` | `{patch}` | How `bump` writes a version. `patch` is `increment`, the default, or `timestamp`, which sets the patch to the UTC time as `YYYYMMDDHHMMSS`, or to one more than the current patch when that is larger. An unknown key under it gets a warning; a wrong `patch` value and a top-level `bump:` are refused. |
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

- An invalid glob is refused. A glob that matches nothing gets a warning from
  `build` and `check`.
- A folder that never ships stays out whatever a pattern says, with a warning.
- A note with a `description` in a never-ship folder under a kind folder, such as
  `plugins/p/docs/x.md`, warns that it will not ship. So does
  `plugins/p/skills/docs/z.md`, where no skill claims `docs`. A `docs/` page at the
  vault root stays the author's own and gets no warning.
- A plugin named `docs` or `tests` ships normally, and so does a skill with that
  name.
- Inside a skill's folder, `docs/`, `tests/` and `fixtures/` never ship, and any other `.md`
  ships as a file of that skill, with or without a `description`.
- A plugin's `assets/` ships whatever its folders are called, and a link into it
  is allowed.
- Output under the current `out` and `packs` is never read as source, also when
  `--out` or `bump --changed-since` builds somewhere else. A folder an earlier
  build left elsewhere can be: see `packs` below.

`exclude` adds to the folders that never ship, and `publish <note>` refuses a note
it matches. It does not apply to a plugin's `assets/`. A body link or embed that reaches a never-ship or excluded file is
refused with the same diagnostic in every link form, so a skill cannot point at a
file that will not be there.

`build.exclude` and each key of `build.patterns` inherit from a workspace, key by
key, and their globs read from each member's folder. A member's empty
`exclude: []` cannot clear an inherited exclusion. The
authoring verbs (`new`, `move`, `convert`) always write to the conventional
folders: `new skill` to `skills/`, and `new plugin` to `plugins/`.

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

A marketplace's own value wins, key by key. Every `build:` key inherits from the
workspace except `out`: each marketplace sets its own, and it resolves against
that marketplace's file. A relative path or glob in an inherited key, such as
`assets[].from`, reads from the member's folder, not the workspace's. A list a
member leaves empty inherits, so `[]` cannot clear an inherited list.

`skillwright new marketplace <name> --description <text>` at the workspace root
writes `marketplaces/<name>/skillwright.md`. Add its `build.out` before you build.
It is refused inside a marketplace. Other authoring verbs run inside a marketplace
folder.

A workspace builds only its marketplaces. `build`, `check` and `list` warn about
components at its root, and about a `build.out` on the workspace file, which is
not read. `list` at the root reads each marketplace with its own `exclude`,
`patterns` and namespace. A workspace file that cannot be read stops every verb
in its marketplaces.

An inherited glob that matches nothing in one marketplace warns only if it matches
nothing in any of them.

`rename` of a marketplace works in a vault that holds `marketplaces/`, not in a
workspace: at the workspace root it is refused, and inside a member it finds no
component of that name. In a workspace, rename the folder and change `name` in its
`skillwright.md` by hand. In a vault, the verb moves the folder and updates the note's `name`.
Links inside the folder move with it. It rewrites nothing else:

- A wikilink or embed from outside that spells the old path still resolves by file
  name. If another file has that name, the build stops with `link-ambiguous`.
- A plain markdown link from outside goes dead, and nothing reports it.
- A path in `skillwright.md`, such as a `build.exclude` entry, keeps the old folder.
  The file it excluded then ships, with only an `unmatched-glob` warning. Update
  these paths by hand.

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
6. The environment. `publish` and `adopt` default to each harness's user skill
   folder. Nothing here changes what a build emits.
7. Built-in defaults: `version` 0.1.0 with a warning, `license` UNLICENSED,
   every harness. Only the plugin of a root `skills/` folder takes `version`
   from this file, so its warning asks for it here. A `plugins/<marketplace>/`
   plugin takes its own note's `version`.

Inheritance works the same on every path: `build`, `publish` of one note, and
`publish` of a whole pack. Containers are matched by folder, never by `name`.

For one key on a member, the first of these wins:

1. The note's own harness block.
2. The note's frontmatter.
3. An inherited harness block, nearest container first.
4. What skillwright generates.

A key in an inherited block must be one the member's kind accepts. If it is not,
it stays with the container.

A note under no marketplace folder takes the vault's one marketplace. With
several, it stays on the root file. A folder under `marketplaces/` with no
component in it does not count.

A plugin note's `name` must match its folder, or it is refused.

A note under a kind folder with frontmatter but no `name` or `description` warns
`no-component-keys`, so a `desciption:` typo does not make it vanish.

## What reaches each harness

| Kind | Claude | Codex | Cursor | Pi |
|---|---|---|---|---|
| Skill | build, publish | build, publish | build, publish | build, publish |
| Command | build, `publish <note>` | `publish <note>` | no | no |
| Hook | build | build | build | no |
| Agent | `publish <note>` | `publish <note>` | no | no |

Agents reach no build on any harness, by design: a pack has no place for them, so
each one is listed `not carried`. `publish <note>` writes one to a harness with an
agent layout. A kind a harness cannot carry shows as `n/a` in `status`, and
`status --check` passes it.

`build` refuses a hand-written call to a known skill with `literal-skill-call`.
`publish` refuses `/plugin:name` too, but publishing one skill does not check a bare
`/name`, because that needs a vault scan. `build` is the check for those.

Two skills that would land on one folder, such as `review` in two plugins, stop
the run with `path-collision`, naming both notes, and nothing is written. Name one
note to publish it, or rename one. A bare `publish` writes skills only. A plugin's pack, an agent and a command
publish when their note is named.

## What a manifest holds

Each manifest keeps to what that harness documents:

- Claude's marketplace entry has no `version`. `plugin.json` holds it: with both,
  that one wins and `claude plugin validate` warns.
- Cursor gets no `author.url` or `owner.url`, because its docs and schema list only
  `name` and `email`.
- Cursor's marketplace entry keeps `version`, `category` and `tags`. Cursor's docs
  list them, and the entry is merged with `plugin.json`. The schema of the
  `cursor/plugins` repository rejects them, so a listing there needs them removed.

## Harness blocks

`claude:`, `codex:`, `cursor:` and `pi:` hold fields only that harness reads. On
a marketplace an example is Codex's `policy`, on a plugin Codex's `interface`, on
a skill Claude's `allowed-tools` patterns. Each block is checked against the keys
that harness accepts, merged over what skillwright generates, and emitted only
for that harness. The accepted keys are [in the field reference](fields.md#harness-blocks).

- A key the block accepts must hold the kind of value that harness documents. A
  component's keys take the type they take written bare, as the
  [field reference](fields.md) says. A container's keys take the type in
  harness-reference's manifest and marketplace facts. A wrong kind is refused,
  naming the block, the key and the kind, such as
  `` `codex.policy` of plugin `p` must be a mapping, not a number ``. Every block
  is checked, whichever harnesses `build.harnesses` selects. A key
  the block does not accept only warns, whatever it holds. A key with no
  documented type is not checked.
- A key in a block set to nothing is unset and is dropped, as it is written bare.
- `metadata` in a block merges with the note's `metadata` rather than replacing
  it.
- A block a component inherits cannot set that component's `name` or
  `description`. A block on the note itself can.
- `<harness>: {packs: <path>}` on the marketplace moves that harness's packs
  folder inside the output. `.DS_Store` and similar OS files in a moved folder are
  ignored. Root catalogs stay where they are. The value that
  takes effect is refused if it overlaps a manifest path, a folder the harness
  owns or, when the output overlaps the vault, a source folder. It is also refused
  when it equals the root file.
  Nothing records an old `out` or `packs`, so a move leaves the old folder in
  place. What happens next depends on the folder, and the fix is always to delete
  it:
  - `out: dist` to `out: dist2` reports nothing when the old folder holds only
    default packs folders. One that holds a moved `packs` folder inside the vault
    reads as source, as below.
  - A harness's default packs folder under `out`, after `packs` moved away from it,
    gets a `stale-output` warning.
  - A moved `packs` folder inside the vault reads as source, dot-named or not. After
    `packs: alt` moves, `build` and `check` stop on `dist/alt`, and the message says
    an earlier build wrote the file.
  - A folder outside the vault gets no warning.
- `pi: {naming: bare}` on the marketplace or a plugin publishes Pi skills under
  their bare names. `prefixed` is the default.
