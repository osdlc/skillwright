# skillwright vault structure, version 1

This spec defines how skillwright reads a vault. It covers the folders, the root
file, the frontmatter rules and the output. The frontmatter keys themselves are
in the [field reference](../reference/fields.md), which is generated from the
code and is part of this spec.

## Stability

- Stable: the folder convention (sections 2 and 3) and the frontmatter fields
  (section 5). A breaking change to either makes a new version of this spec.
- Experimental: patterns (section 4). They may change without a new version.

## 1. The root file

1. The vault is the folder holding `skillwright.md`.
2. skillwright finds the file by walking up from the working directory. A
   `skillwright.md` at `<kind>/skillwright/skillwright.md` is the note of a
   component named `skillwright`, and the walk goes past it. A `skillwright.md`
   directly in a kind folder, such as `skills/skillwright.md`, is a reserved name:
   it is refused `reserved-name`, and the walk goes past it to the vault's root
   file.
3. The frontmatter's top-level keys are the marketplace's fields. `build:` holds
   tool settings. The body is not read.
4. It is the only configuration file. There is no machine file.
5. With no root file, a folder holding `plugins/` or `skills/` is read with every
   default, and a note suggests adding one.
6. An outer root file with `workspace: true` holds shared defaults. Each
   `marketplaces/<m>/skillwright.md` below it is a marketplace. That file is the
   only form a marketplace's note has. Every `build:` key inherits from the
   workspace except `out`, and a relative path in an inherited key reads from the
   member's folder. It builds only its marketplaces: components at its root and a
   `build.out` on its own file warn.

## 2. Folders

```text
<vault>/
├── skillwright.md
├── plugins/
│   └── <p>/
│       ├── <p>.md          optional plugin note
│       ├── CHANGELOG.md    optional, rendered into each pack
│       ├── assets/         copied to the plugin's root as is
│       ├── agents/
│       ├── commands/
│       ├── hooks/
│       └── skills/
└── skills/                 a plugin named after the marketplace
```

1. A folder under `plugins/` is a plugin, named after the folder. The folder is
   enough: the plugin note is optional, and a plugin note with no `name` or
   `description` is kept and takes both defaults. A plugin note's `name` must
   match its folder, or it is refused.
2. `skills/`, `agents/`, `commands/` and `hooks/` decide the kind of what they
   hold. Frontmatter never decides kind or plugin.
3. A root `skills/`, `agents/`, `commands/` or `hooks/` folder belongs to a plugin
   named after the marketplace. That plugin has no note of its own: its note is
   `skillwright.md`, so it takes `version` from there. `bump <marketplace>`
   edits that key. A `plugins/<marketplace>/` folder is an ordinary plugin: its
   version is its own note's, else `0.1.0`, never the root file's.
4. `assets/bin/tool` ships as `bin/tool`.
5. `check` warns when a plugin's `version` has no entry in its `CHANGELOG.md`.
6. These never ship: `docs/`, `tests/`, `fixtures/`, `.git`, `.obsidian`, `.trash`
   and `node_modules`. A plugin named `docs` or `tests` ships normally.
7. A note with a `description` in a never-ship folder under a kind folder warns
   that it will not ship.

## 3. Components

1. A skill is `<s>.md`, `<s>/<s>.md` or `<s>/SKILL.md`. Anything else in a skill's
   folder ships with the skill, including a `.md` with a `description`, except
   `docs/`, `tests/` and `fixtures/`, which never ship. A note in `skills/docs/` with no
   `docs` skill warns that it will not ship.
2. Agents and commands are `<name>.md` or `<name>/<name>.md`.
3. A note is a component only if it has a `description`, or a `name` alone.
4. A name is lowercase words joined by hyphens, at most 64 characters. It defaults
   to the file or folder name.

## 4. Patterns (experimental)

`build.patterns` adds globs beside the conventional folders, and `build.exclude`
adds to what never ships. See the
[reference](../reference/skillwright-md.md#patterns).

1. An invalid glob is refused. A glob that matches nothing gets a warning from
   `build` and `check`.
2. A folder from section 2 that never ships stays out whatever a pattern says,
   with a warning.
3. `exclude` does not apply to a plugin's `assets/`. `publish <note>` refuses a
   note it matches.
4. A link to a never-ship or excluded file is refused with the same diagnostic in
   every link form.
5. Each key of `build.patterns` inherits from a workspace, key by key, and its
   globs read from each member's folder.
6. A `build.exclude` glob reads from the folder of the `skillwright.md` it is
   written in, like a nested `.gitignore`. A workspace's globs apply to every
   member; a member's apply to its own folder only. A member's exclusions add to
   the workspace's, and cannot drop them. A workspace glob that matches nothing
   in any member warns once per run, at the workspace root or in a member.

## 5. Frontmatter

1. Keys are bare by default.
2. `build.namespace: <stem>` makes every `skillwright`-owned key take a prefix,
   flat (`<stem>-version`) or nested (`<stem>: {version: …}`). A note may not mix
   the two forms. `spec`-owned keys are never prefixed. Keys in the root file are
   always bare.
3. One closed table lists the keys of each kind. It drives the publish strip,
   validation and the field reference.
4. A key outside the table gets a warning. On a component it is published under
   `metadata:` as YAML. On a container or in the root file it is ignored. Under a
   namespace, the warning names the right spelling.
5. `license` on a plugin or marketplace is the pack's licence. On a skill it is
   the spec field.

## 6. The cascade

A value comes from the first of these that has one: a flag; the note; the nearest
container; patterns; the folder convention; the environment; a built-in default.
The [reference](../reference/skillwright-md.md#where-a-value-comes-from) defines
each one.

1. Only `author`, `license`, `repository`, `homepage`, `targets` and the harness
   blocks inherit. Harness blocks inherit key by key.
2. Inheritance applies on every path, including publishing one note or one pack.
3. Containers are matched by folder, never by `name`.
4. A block a component inherits cannot set the component's `name` or
   `description`.
5. For one key on a member, the order is: the note's own harness block, the note's
   frontmatter, an inherited block with the nearest container first, generated
   values.
6. A key in an inherited block must be one the member's kind accepts. If it is
   not, it stays with the container.
7. A note under no marketplace folder takes the vault's one marketplace. With
   several, it stays on the root file. A folder under `marketplaces/` with no
   component in it does not count.

## 7. Harness blocks

1. `claude:`, `codex:`, `cursor:` and `pi:` hold fields only that harness reads.
2. A block is checked against the keys that harness accepts. An unknown key
   warns and is left out of what that harness gets, on a component and on a
   container. A known key with a wrong value is refused.
3. A block is merged over what skillwright generates, and `metadata` merges with
   the note's own.
4. A block is emitted only for its harness.

## 8. Output

1. `build` and `check` need an output folder, from `--out` or `build.out`. There
   is no default.
2. A build replaces only what skillwright owns: each harness's packs folder and
   the manifests it writes. Everything else in those folders stays.
3. Generated output is never read as source.
4. A marketplace may move a harness's packs folder with `<harness>: {packs: …}`.
   The value that takes effect is refused if it overlaps a manifest path, a folder
   the harness owns or, when the output overlaps the vault, a source folder, and
   if it equals the root file.
5. `.DS_Store` and similar OS files in a moved packs folder are ignored.
6. Moving the output from one folder outside the vault to another leaves the old
   folder in place with no warning, because nothing records it.
