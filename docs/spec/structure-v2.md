# skillwright vault structure, version 2 (draft)

Status: draft for the owner's review. It replaces
[version 1](structure-v1.md) when accepted. Nothing reads both: a vault in
any other layout is refused (section 10).

This spec defines how skillwright reads a vault. A vault has one fixed layout.
It covers the folders, the root file, the notes, links, the frontmatter rules
and the output. The frontmatter keys themselves are in the
[field reference](../reference/fields.md), which is part of this spec.

The decisions this draft leaves to the owner are in section 11. The effect on
the code, the open PRs and issues, and the build order are in
[v2-impact.md](v2-impact.md).

## Stability

- Stable: the layout (sections 1 to 3), the link rules (section 4) and the
  frontmatter fields (section 5). A breaking change to any of them makes a new
  version of this spec.
- There is no experimental layer. Flexibility is added later, as a new
  version.

## 1. The root

1. The vault is the folder that holds both `skillwright.md` and a
   `marketplaces/` folder. That folder is the root.
2. `--vault <path>` names the root. Without it, skillwright walks up from the
   working directory and stops at the first folder that holds both. A
   `skillwright.md` with no `marketplaces/` beside it is passed over.
   This is the simplest rule that needs no reserved names: a component named
   `skillwright` has its note at `.../skillwright/skillwright.md`, and its
   folder never holds `marketplaces/`, because a `marketplaces/` folder below
   the top is refused (section 10).
3. skillwright owns the root. It is not meant to sit inside a larger notes
   vault. Only `skillwright.md` and `marketplaces/` are read. `dist/` is
   written (section 8). Anything else at the root is left alone.

## 2. The root file

1. `skillwright.md` exists only at the root.
2. It holds tool settings under `build:`, and shared defaults: `author`,
   `license`, `repository`, `homepage`, `targets` and the harness blocks
   (`claude:`, `codex:`, `cursor:`, `pi:`). Nothing else.
3. It is never a marketplace. A marketplace field in it, such as `name`,
   `description`, `owner` or `version`, is refused. The error names the
   marketplace note the field belongs in.
4. The body is not read.
5. It is the only configuration file. There is no machine file for the build.
6. The `build:` keys are `harnesses`, `namespace`, `launcher_path`, `bump`,
   `destinations`, `scan_locations` and `exclude` (subject to the decisions in
   section 11). `out`, `patterns`, `assets` and `hooks` are not keys. A key the
   table does not list warns.

## 3. Folders and notes

```text
<root>/
├── skillwright.md                  tool settings (build:) and shared defaults
├── dist/                           output, written by build (section 8)
└── marketplaces/
    └── <m>/
        ├── <m>.md                  the marketplace's note
        └── plugins/
            └── <p>/
                ├── <p>.md          the plugin's note
                ├── CHANGELOG.md    optional, rendered into each pack
                ├── assets/         copied to the plugin's root as is
                ├── skills/<s>/<s>.md
                ├── agents/<a>/<a>.md
                ├── commands/<c>/<c>.md
                └── hooks/<h>/<h>.md
```

1. Every note is `<name>.md` in a folder of the same name. There is no other
   entry form: no flat `<name>.md` in a kind folder, and no `SKILL.md`.
2. Position alone decides what a note is. A folder under `marketplaces/` is a
   marketplace. A folder under `marketplaces/<m>/plugins/` is a plugin. A folder
   under a plugin's `skills/`, `agents/`, `commands/` or `hooks/` is a component
   of that kind. Frontmatter never decides kind, plugin or marketplace.
3. Each marketplace folder must hold `<m>.md`, and each plugin folder `<p>.md`.
   A missing note is an error that names the path it expects.
4. A note's `name`, when set, must equal its folder's name, or it is refused.
   Without it, the name is the folder's name.
5. A name is lowercase words joined by hyphens, at most 64 characters.
6. Names are unique within a plugin, across all kinds. A skill and a command
   with one name in one plugin are refused, naming both. The same name in two
   plugins, or two marketplaces, is allowed.
7. Every file in a component's folder belongs to that component. Inside it,
   `docs/`, `tests/` and `fixtures/` never ship.
8. `assets/bin/tool` ships as `bin/tool`.
9. These never ship anywhere: `docs/`, `tests/`, `fixtures/`, `.git`,
   `.obsidian`, `.trash` and `node_modules`. A plugin or component named `docs`
   or `tests` ships normally.
10. A file in a plugin folder that is not listed above is not read. A note
    there with a `description` warns that it will not ship. The same holds for
    a marketplace folder, beside `<m>.md` and `plugins/`.
11. A component is a note with a `description`, or a `name` alone. A skill,
    command or agent with a `name` and no `description` is an error: `list` and
    `check` report it, and `build` and `publish` refuse it.
12. A component exists only inside a plugin, and a plugin only inside a
    marketplace. A `skills/`, `plugins/` or other kind folder anywhere else is
    an error (section 10).
13. One skill may be copied into two marketplaces. Each copy is its own
    component. There is no shared folder.
14. `check` warns when a plugin's `version` has no entry in its `CHANGELOG.md`.

## 4. Links, embeds and containment

A link is a `[[wikilink]]`, a `![[embed]]` or a relative Markdown link. An
embed is read the same way as a link: what it names must be in scope.

### Containment

1. Each marketplace builds self-contained. A link, an embed or an asset that
   resolves outside `marketplaces/<m>/` is a build error.
2. Every path is checked by its real path, with symlinks resolved. A symlink
   whose target lies outside the scope it is checked against is refused, in
   the same way as a link that spells that target.

### Scope of a link from a component

| Link target | Rule |
|---|---|
| A file in the component's own folder | Ships as written, relative. |
| Another component in the same plugin | Allowed. Rewritten to that component's installed path. |
| Another plugin in the same marketplace | Refused. |
| Anything outside `marketplaces/<m>/` | Refused. |

3. A bare `[[name]]` resolves in the component's own folder first, then among
   the components of its plugin. It never resolves across the plugin or the
   vault. Because names are unique per plugin (section 3), the second step
   finds at most one.
4. A link with a path resolves relative to the linking file. Its result must
   still fall in a row the table allows.
5. A link to another component points at that component's entry note or at a
   file in its folder. It is rewritten to where that file lands in the same
   pack. A component the target harness's pack does not carry, such as an
   agent in any build, has no installed path, and a link to it is refused for
   that harness.
6. A link into a never-ship folder, or to a file `build.exclude` holds, is
   refused with the same error in every link form.
7. A link to a missing file is an error that names the link.
8. Building or publishing one skill alone allows only links inside the skill's
   own folder. Any other link is an error that names it.
9. `/[[name]]` is a call: it renders as the target harness's invocation of the
   named skill and copies nothing. Its scope is decision D3 in section 11.
10. Every error in this section names the linking file, the link as written and
    the rule it broke.

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
6. `build:` is read only in the root file. In any other note it is refused.

## 6. The cascade

A value comes from the first of these that has one: a flag; the note; the
plugin note; the marketplace note; the root file's shared defaults; the folder
position; the environment; a built-in default.

1. Only `author`, `license`, `repository`, `homepage`, `targets` and the harness
   blocks inherit. Harness blocks inherit key by key.
2. The order for an inherited key is component, then plugin, then marketplace,
   then root. Each container is matched by folder, never by `name`.
3. Inheritance applies on every path, including publishing one note or one pack.
4. A block a component inherits cannot set the component's `name` or
   `description`.
5. For one key on a member, the order is: the note's own harness block, the
   note's frontmatter, an inherited block with the nearest container first,
   generated values.
6. A key in an inherited block must be one the member's kind accepts. If it is
   not, it stays with the container.
7. A plugin's `version` is its note's. Without one it is `0.1.0`, with a
   warning. A marketplace's `version` is its note's.

## 7. Harness blocks

1. `claude:`, `codex:`, `cursor:` and `pi:` hold fields only that harness reads.
2. A block is checked against the keys that harness accepts. An unknown key
   warns and is left out of what that harness gets, on a component and on a
   container. A known key with a wrong value is refused.
3. A block is merged over what skillwright generates, and `metadata` merges with
   the note's own.
4. A block is emitted only for its harness.
5. `packs` is not a block key. Each harness's packs folder is fixed (section 8).
   `pi: {naming: bare}` stays.

## 8. Output

1. `build` writes each marketplace to `<root>/dist/<m>/`. There is no setting
   and no flag that moves it.
2. Under `dist/<m>/`, each harness's tree has that harness's default layout,
   for example `dist/<m>/.claude-plugin/marketplace.json` and
   `dist/<m>/.claude-plugin/packs/<p>/`.
3. A build replaces only what skillwright owns in `dist/<m>/`: each harness's
   packs folder and the manifests it writes.
4. Output is never written inside `marketplaces/`, and it is never read as
   source. Since only `marketplaces/` is read, this needs no check.
5. `check` builds in memory and compares with `dist/<m>/`. It writes nothing
   and exits 1 on drift.
6. The build stops at `dist/`. Copying the output to a repository root,
   committing it or publishing a marketplace from it is the consumer's job.
7. `publish` is a separate step. What it keeps is decision D1 in section 11.

## 9. Commands and the layout

1. `build`, `check` and `list` run over every marketplace. `--marketplace <m>`
   picks one.
2. Inside `marketplaces/<m>/`, the working directory picks the marketplace, and
   inside `plugins/<p>/` the plugin. Elsewhere a verb that needs one asks for
   `--marketplace` or `--plugin`.
3. `init` writes `skillwright.md` and `marketplaces/<m>/<m>.md`.
4. `new marketplace` writes `marketplaces/<m>/<m>.md`. `new plugin` writes
   `marketplaces/<m>/plugins/<p>/<p>.md`. `new skill`, `new command`,
   `new agent` and `new hook` write `<kind>/<name>/<name>.md` in a plugin.
5. `rename` and `move` refuse a change that would leave a link out of scope,
   and name each such link.
6. `bump <plugin>` writes the plugin note's `version`. `bump <m>` writes the
   marketplace note's.
7. `adopt` writes notes into a plugin in this layout.
8. `publish <note>` names a note in the vault. A path outside
   `marketplaces/` is refused.

## 10. A vault not in this layout

skillwright reads this layout and no other. A vault that does not match is
refused before anything is read further. The error says what was found, where,
and prints the expected layout:

```text
<root>/
├── skillwright.md                   build settings and shared defaults
└── marketplaces/<m>/
    ├── <m>.md
    └── plugins/<p>/
        ├── <p>.md
        └── skills/<s>/<s>.md   (also agents/, commands/, hooks/)
```

It is refused when:

1. No folder at or above the working directory holds both `skillwright.md` and
   `marketplaces/`, and no `--vault` is given. If a `skillwright.md` was passed
   over on the walk, the error names it.
2. The root file has `workspace: true` or a marketplace field.
3. A marketplace folder has no `<m>.md`. If it holds a `skillwright.md`, the
   error says the marketplace note is now `<m>.md`.
4. A kind folder (`skills/`, `agents/`, `commands/`, `hooks/`) or a `plugins/`
   folder is anywhere other than its place in section 3, such as at the root or
   directly in a marketplace.
5. A `marketplaces/` folder is anywhere below the top.
6. A component's note is not `<name>/<name>.md`, such as `skills/x.md` or
   `skills/x/SKILL.md`.
7. The root file has `build.patterns`, `build.out`, or a `packs` key in a
   harness block.

Each case is reported once, with every path it applies to.

## 11. Decisions for the owner

Each has a recommendation. The rest of this spec is written as if each
recommendation is taken.

### D1. What `publish` becomes

`publish` writes skills straight into a harness's skill folder, such as
`~/.claude/skills`, from `build.destinations`. It renders from the vault, not
from `dist/`. It is the only route for an agent, and for a command on a
harness whose build does not carry it, because a build carries no agents.

- Keep it as a separate step, reduced. It renders a note from the vault with
  the same code as `build`. It drops publishing a note by a path no folder
  places. One skill alone follows section 4 rule 8.
- Reduce it further, to copy from `dist/<m>/`. This gives one render path, but
  it cannot publish agents, which no build carries.
- Remove it, and rely on `harness install` and `harness link`, which read
  `dist/`. This loses agents and one-skill publishing.

Recommendation: keep it as a separate step, reduced as in the first option.

### D2. `build.out` and `--out`

The output is fixed at `dist/<m>/`. No reason found keeps a setting.
`bump --changed-since` builds its baseline in a temporary folder of its own,
and does not need the flag.

Recommendation: drop both `build.out` and `--out`.

### D3. The scope of a call (`/[[name]]`)

A call renders as an invocation, such as `/sdlc:task-work`, and copies no file.
The harness resolves it when the skill runs. Calls to a skill in another plugin
are common.

- Same scope as links: own plugin only.
- The whole marketplace.

Recommendation: the whole marketplace. A call to a skill outside it is
refused. A bare call name that two plugins share is refused as ambiguous,
naming both.

### D4. A plain link to another skill

Today a plain `[[other-skill]]` in a skill or command body is read as a call.
The link table in section 4 makes it a file link to the other skill's
installed path.

Recommendation: follow the table. A plain link is a file link, and a call is
written `/[[name]]`. The repo's own `plugin/` vault and any vault that relies on
the old reading changes output, and `check` shows it.

### D5. `build.assets` and `build.hooks`

Both name a plugin by its bare name, which is ambiguous with several
marketplaces, and both point at files the containment rule already confines to
one marketplace.

- Drop both. A plugin's `assets/` covers assets. A `hooks.json` directly in a
  plugin's `hooks/` folder is its canonical hooks file.
- Keep both, with `plugin: <m>/<p>` and a `from` checked by section 4.

Recommendation: drop both.

### D6. `build.exclude`

It is a glob list of what never ships. It does not change the layout.

- Keep it, read from the root folder only. The per-member layering of v1 goes.
- Drop it. Authors move drafts out of `marketplaces/`.

Recommendation: keep it, root only. Drafts beside a skill are a common need,
and the cost is one glob list.

### D7. Plugin notes are required

v1 lets a plugin folder stand without a note. v2 requires `<p>.md`, so every
plugin has one place for its `version` and fields.

Recommendation: require it. `new plugin` and `init` write it.
