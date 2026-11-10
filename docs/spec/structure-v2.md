# skillwright vault structure, version 2 (draft)

Status: draft for the owner's review. It replaces
[version 1](structure-v1.md) when accepted. Nothing reads both: a vault in
any other layout is refused (section 11).

v2 has one goal: stable marketplace publishing, with transclusion and good
bundling. Its commands are `init`, `build`, `check` and `bump`.

This spec defines how skillwright reads a vault. A vault has one fixed layout.
It covers the folders, the root file, the notes, links, the frontmatter rules
and the output. The frontmatter keys themselves are in the
[field reference](../reference/fields.md), which is part of this spec.

The decisions this draft leaves to the owner are in section 12. The effect on
the code, the open PRs and issues, and the build order are in
[v2-impact.md](v2-impact.md).

## Stability

- Stable: the layout (sections 1 to 3), the link and embed rules (sections 4
  and 5), the frontmatter fields (section 6) and what a pack holds (section 9).
  A breaking change to any of them makes a new version of this spec.
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
   the top is refused (section 11).
3. skillwright owns the root. It is not meant to sit inside a larger notes
   vault. Only `skillwright.md` and `marketplaces/` are read. `dist/` is
   written (section 9). Anything else at the root is left alone.

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
6. The `build:` keys are `harnesses`, `namespace`, `launcher_path` and `bump`
   (subject to the decisions in section 12). `out`, `patterns`, `exclude`,
   `assets`, `hooks`, `destinations` and `scan_locations` are refused
   (section 11). Any other key the table does not list warns.
7. `launcher_path` names a file in each plugin's `scripts/`, such as
   `scripts/tool`, relative to the pack root.

## 3. Folders and notes

```text
<root>/
├── skillwright.md                  tool settings (build:) and shared defaults
├── dist/                           output, written by build (section 9)
└── marketplaces/
    └── <m>/
        ├── <m>.md                  the marketplace's note
        └── plugins/
            └── <p>/
                ├── <p>.md          the plugin's note
                ├── CHANGELOG.md    optional, rendered into each pack
                ├── references/     shared docs, ship as references/
                ├── scripts/        shared code, ship as scripts/
                ├── assets/         shared output files, ship as assets/
                ├── skills/<s>/
                │   ├── <s>.md
                │   ├── references/
                │   ├── scripts/
                │   └── assets/
                ├── agents/<a>/<a>.md
                ├── commands/<c>/<c>.md
                └── hooks/
                    ├── hooks.json  optional (decision D4)
                    └── <h>/<h>.md
```

The layout follows the recommended practice of the Agent Skills standard: a
skill is a folder with its entry note and optional `references/`, `scripts/`
and `assets/` folders, and relative paths resolve from the skill's folder
(harness-reference: `Agent Skills Agent Skills format`,
`Agent Skills Relative paths in SKILL.md`).

### Containers and components

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
7. A component is a note with a `description`, or a `name` alone. A skill,
   command or agent with a `name` and no `description` is an error that
   `build` and `check` report.
8. A component exists only inside a plugin, and a plugin only inside a
   marketplace. A `skills/`, `plugins/` or other kind folder anywhere else is
   an error (section 11).
9. One skill may be copied into two marketplaces. Each copy is its own
   component. There is no shared folder across plugins.
10. `check` warns when a plugin's `version` has no entry in its `CHANGELOG.md`.

### What a folder may hold

Below the root, each folder holds a fixed set of entries. Anything else is an
error that names the file and lists what the folder may hold. OS files such as `.DS_Store` are
ignored.

| Folder | May hold |
|---|---|
| The root | `skillwright.md`, `marketplaces/`, `dist/`. Anything else is left alone, not read. |
| `marketplaces/<m>/` | `<m>.md`, `plugins/`. |
| `plugins/<p>/` | `<p>.md`, `CHANGELOG.md`, `references/`, `scripts/`, `assets/`, the four kind folders. |
| `skills/<s>/` | `<s>.md`, `references/`, `scripts/`, `assets/`. |
| `agents/<a>/`, `commands/<c>/`, `hooks/<h>/` | Their note only. |
| `hooks/` | `hooks.json` and hook folders. |

11. In a skill, `references/` holds docs the agent reads into context when it
    needs them. `scripts/` holds code the agent runs. `assets/` holds files used
    in output, such as templates, images and fonts. Each ships inside the
    skill's folder under the same name.
12. An agent, command or hook installs as a single file, so it has no folder of
    its own to ship. It uses the plugin's shared folders.
13. A plugin's `references/`, `scripts/` and `assets/` are shared by every
    component in the plugin. Each ships under the same name at the pack root.
    Nothing is flattened: `assets/x` ships as `assets/x`.
14. `docs/`, `tests/` and `fixtures/` may sit in any plugin or component folder.
    They never ship, and nothing may link into them. `.git`, `.obsidian`,
    `.trash` and `node_modules` are skipped wherever they are.
15. Files inside `references/`, `scripts/` and `assets/` may be laid out in any
    folders below them. `scripts/` keeps each file's exec bit.

## 4. Links, embeds and containment

A link is a `[[wikilink]]` or a relative Markdown link. An embed
(`![[name]]`, section 5) resolves by the same rules: what it names must be in
scope. The rules apply to links from a component's note and from any Markdown
file it ships.

### Containment

1. Each marketplace builds self-contained. A link, an embed or a shipped file
   that resolves outside `marketplaces/<m>/` is a build error.
2. Every path is checked by its real path, with symlinks resolved. A symlink
   whose target lies outside the scope it is checked against is refused, in
   the same way as a link that spells that target.

### Scope of a link from a component

| Link target | Rule |
|---|---|
| A file in the component's own folder | Ships as written, relative. |
| A file in the plugin's `references/`, `scripts/` or `assets/` | Allowed. Rewritten to its installed path at the pack root. |
| Another component in the same plugin | Allowed. Rewritten to that component's installed path. |
| Another plugin in the same marketplace | Refused. |
| Anything outside `marketplaces/<m>/` | Refused. |

3. A bare `[[name]]` resolves in the linking file's component folder first.
   With no match there, it resolves among the plugin's shared folders and its
   components together. It never resolves across the plugin or the vault. More
   than one match in a step is an error that names each match.
4. A link with a path resolves relative to the linking file. Its result must
   still fall in a row the table allows.
5. An installed path is written relative to where the linking file lands in the
   pack. From a skill's entry note, the plugin's `references/x.md` becomes
   `../../references/x.md`. Every harness built for keeps a pack whole on
   install, so the path holds after install (section 9).
6. A link to another component points at that component's entry note or at a
   file in its folder. It is rewritten to where that file lands in the same
   pack. A component the target harness's pack does not carry, such as an
   agent in any build, has no installed path, and a link to it is refused for
   that harness.
7. A link into a never-ship folder is refused with the same error in every
   link form.
8. A link to a missing file is an error that names the link.
9. `/[[name]]` is a call: it renders as the target harness's invocation of the
   named skill and copies nothing. Its scope is decision D2 in section 12.
10. Every error in this section names the linking file, the link as written and
    the rule it broke.

## 5. Transclusion

An embed, `![[target]]`, is replaced at build time by the target's content.
Embeds are resolved before links are rewritten, so the output holds no `![[`.

### What may be embedded

1. The target resolves by the scope rules of section 4: the embedding file's
   own component folder, then the plugin's shared folders and components. An
   embed outside that scope is refused with the same error as a link.
2. A note (`.md`) is inlined. An image (`.avif`, `.bmp`, `.gif`, `.jpeg`,
   `.jpg`, `.png`, `.svg`, `.webp`) becomes a Markdown image link to the file's
   installed path. Any other file is decision D8 in section 12.
3. An embed whose target does not exist is an error that names the embed.

### What is inlined

4. `![[note]]` inlines the note's body. Its frontmatter is dropped. A note
   whose frontmatter is malformed still embeds: only the body is read.
5. `![[note#Heading]]` inlines one section: the heading line through the line
   before the next heading of the same or a shallower level, so subsections
   come with it. A heading is an ATX heading (`#` to `######`) outside fenced
   code. `![[note#Parent#Child]]` names the heading by its last segment, with
   the segments before it as the nearest headings above it, in order. A
   selector is first tried as one heading's whole text, since a heading may
   contain `#`. The first heading that matches wins.
6. `![[note#^id]]` inlines one block. A `^id` marker at the end of a line labels
   the paragraph that line ends, or the one list item it ends. A marker alone
   on a line labels the paragraph or list above it. The marker is not part of
   the inlined text.
7. A heading or block that is not found is an error, `fragment-unresolved`,
   that names the note and the fragment.
8. The inlined text is trimmed of leading and trailing whitespace and replaces
   the embed in place. Nothing marks where it came from.

### Nesting

9. An embedded note may embed others. Each level is expanded the same way.
10. A cycle is an error, `cycle`, that prints the loop from its first repeat,
    such as `a.md -> b.md#Setup -> a.md`. A hop is a file and a fragment, so
    two embeds of different sections of one note are not a cycle. A note that
    embeds itself is a cycle.

### Links inside embedded text

11. A link or embed inside embedded text resolves from the embedded file: its
    own folder and its own component's scope, as if it were read in place.
12. It is then written relative to where the text lands, the file that holds
    the outermost embed. The same note embedded into two files links
    differently in each.
13. A call (`/[[name]]`) inside embedded text renders as it would in place.

### Differences from v1

14. v1 resolved an embed vault-wide and copied a linked file from anywhere in
    the vault to `references/vault/` or `assets/vault/`. v2 resolves within the
    plugin, and every file it can reach already ships (section 9), so nothing
    is copied for a link.
15. v1 reported an embed or link to nothing and shipped its words. v2 refuses
    it (section 4 rule 8).

## 6. Frontmatter

1. Keys are bare by default.
2. `build.namespace: <stem>` makes every `skillwright`-owned key take a prefix,
   flat (`<stem>-version`) or nested (`<stem>: {version: …}`). A note may not mix
   the two forms. `spec`-owned keys are never prefixed. Keys in the root file are
   always bare.
3. One closed table lists the keys of each kind. It drives the build's strip,
   validation and the field reference.
4. A key outside the table gets a warning. On a component it is emitted under
   `metadata:` as YAML. On a container or in the root file it is ignored. Under a
   namespace, the warning names the right spelling.
5. `license` on a plugin or marketplace is the pack's licence. On a skill it is
   the spec field.
6. `build:` is read only in the root file. In any other note it is refused.

## 7. The cascade

A value comes from the first of these that has one: a flag; the note; the
plugin note; the marketplace note; the root file's shared defaults; the folder
position; the environment; a built-in default.

1. Only `author`, `license`, `repository`, `homepage`, `targets` and the harness
   blocks inherit. Harness blocks inherit key by key.
2. The order for an inherited key is component, then plugin, then marketplace,
   then root. Each container is matched by folder, never by `name`.
3. Inheritance applies to every component a build reads.
4. A block a component inherits cannot set the component's `name` or
   `description`.
5. For one key on a member, the order is: the note's own harness block, the
   note's frontmatter, an inherited block with the nearest container first,
   generated values.
6. A key in an inherited block must be one the member's kind accepts. If it is
   not, it stays with the container.
7. A plugin's `version` is its note's. Without one it is `0.1.0`, with a
   warning. A marketplace's `version` is its note's.

## 8. Harness blocks

1. `claude:`, `codex:`, `cursor:` and `pi:` hold fields only that harness reads.
2. A block is checked against the keys that harness accepts. An unknown key
   warns and is left out of what that harness gets, on a component and on a
   container. A known key with a wrong value is refused.
3. A block is merged over what skillwright generates, and `metadata` merges with
   the note's own.
4. A block is emitted only for its harness.
5. `packs` is not a block key. Each harness's packs folder is fixed (section 9).
   `pi: {naming: bare}` stays.

## 9. Bundling and output

### Where output goes

1. `build` writes each marketplace to `<root>/dist/<m>/`. There is no setting
   and no flag that moves it.
2. Under `dist/<m>/`, each harness's tree has that harness's default layout:
   its marketplace manifest and its packs folder, one pack per plugin.

   | Harness | Marketplace manifest | Pack |
   |---|---|---|
   | Claude Code | `.claude-plugin/marketplace.json` | `.claude-plugin/packs/<p>/` |
   | Codex | `.agents/plugins/marketplace.json` | `.agents/packs/<p>/` |
   | Cursor | `.cursor-plugin/marketplace.json` | `.cursor-plugin/packs/<p>/` |
   | Pi | none | `.pi/packs/<p>/` |

3. A build replaces only what skillwright owns in `dist/<m>/`: each harness's
   packs folder and the manifests it writes.
4. Output is never written inside `marketplaces/`, and it is never read as
   source. Since only `marketplaces/` is read, this needs no check.
5. The build stops at `dist/`. Copying the output to a repository root,
   committing it or publishing a marketplace from it is the consumer's job.

### What a pack holds

This is the one place that says what ships and where it lands. `<pack>` is the
pack folder from the table above.

| Source | Lands at | Harnesses |
|---|---|---|
| The plugin's manifest, from `<p>.md` | The harness's manifest path in `<pack>` | All |
| `plugins/<p>/CHANGELOG.md` | `<pack>/CHANGELOG.md` | All |
| `plugins/<p>/references/`, `scripts/`, `assets/` | `<pack>/references/`, `<pack>/scripts/`, `<pack>/assets/` | All |
| `skills/<s>/<s>.md` | `<pack>/skills/<s>/SKILL.md`, compiled | All. Pi names the folder `<p>-<s>` unless `pi: {naming: bare}`. |
| `skills/<s>/references/`, `scripts/`, `assets/` | The same names inside the skill's folder | All |
| `commands/<c>/<c>.md` | `<pack>/commands/<c>.md`, compiled | Claude Code only |
| `hooks/hooks.json` and hook notes | The harness's hooks file in `<pack>` | Claude Code, Codex, Cursor |
| `agents/<a>/<a>.md` | Nothing. No harness's build carries an agent. | None |

6. Compiled means: embeds inlined (section 5), links rewritten (section 4),
   calls rendered, and the frontmatter reduced to what that harness reads.
7. Every Markdown file in a shipped `references/` folder is compiled the same
   way. Files in `scripts/` and `assets/`, and every other file, are copied byte
   for byte, so a template keeps any `[[` it holds.
8. `docs/`, `tests/` and `fixtures/` never ship from any folder.
9. A file keeps its exec bit. A file in `scripts/` that is meant to run must be
   executable in the source; the build does not set the bit.
10. A pack holds no symlinks. Every shipped file is a copy.
11. Two sources that would land on one path in a pack are an error that names
    both.

### Install keeps a pack whole

12. Links up to the pack root rely on the harness keeping a pack whole on
    install and resolving a skill's relative paths from the skill's folder.
    harness-reference records this for each harness built for:

    | Harness | Pack kept whole | Relative paths resolve from |
   |---|---|---|
    | Claude Code | Yes: a marketplace plugin is copied whole to `~/.claude/plugins/cache/<m>/<p>/<version>`, and nothing outside the plugin folder is copied (`Claude Code Plugin install copy`, `Claude Code Plugin install location`) | The skill's folder (`Claude Code Relative paths in SKILL.md`) |
    | Codex CLI | Yes: the plugin is copied whole to `~/.codex/plugins/cache/<m>/<p>/<version>` (`Codex CLI Plugin install copy`, `Codex CLI Plugin install location`) | Not recorded (`Codex CLI Relative paths in SKILL.md open`) |
    | Cursor | Yes: plugin files are copied to `~/.cursor/plugins/local`, and a symlink loads only when its target is inside that folder (`Cursor Plugin install copy`, `Cursor Plugin install location`) | The skill's root (`Cursor Relative paths in SKILL.md`) |
    | Pi | Yes: a local package loads in place, a git package is cloned, an npm package is installed whole (`Pi Plugin install copy`, `Pi Plugin install location`, `Pi Plugin install 1.5.0`) | The skill's folder, and the model is told it (`Pi Relative paths in SKILL.md`) |

    Cursor's rule that a symlink loads only when its target is inside its
    plugin folder never applies, because a pack holds none (rule 10).

### check

13. `check` runs every rule `build` runs and writes nothing. It also compares
    the result with `dist/<m>/` and exits 1 on drift.

## 10. Commands

v2 has four commands. Every other command is out of v2 (see "Later").

1. `init` writes the skeleton: `skillwright.md`,
   `marketplaces/<m>/<m>.md` and `marketplaces/<m>/plugins/<p>/<p>.md`. It
   refuses a folder that already holds `skillwright.md`.
2. `build` reads the layout and writes `dist/<m>/` for every marketplace.
   `--marketplace <m>` builds one.
3. `check` runs `build`'s rules and writes nothing (section 9 rule 13).
   `--marketplace <m>` checks one.
4. `bump <name>` raises the `version` of the plugin or marketplace with that
   name, in its note. `bump --changed-since <ref|dir>` bumps each plugin whose
   built output differs from the baseline while its `version` has not moved.
5. Inside `marketplaces/<m>/`, the working directory picks the marketplace.
   Elsewhere, a name two marketplaces share needs `--marketplace`.

## 11. A vault not in this layout

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
7. The root file has `build.out`, `build.patterns`, `build.exclude`,
   `build.assets`, `build.hooks` or `build.scan_locations`, or a `packs` key in
   a harness block.
8. A folder holds an entry its row in "What a folder may hold" (section 3)
   does not list, such as a plugin's `bin/` or a `notes.md` beside a skill's
   entry.

Each case is reported once, with every path it applies to.

## 12. Decisions for the owner

Each decision has a recommendation. The rest of this spec is written as if
each recommendation is taken.

### D1. `build.out` and `--out`

The output is fixed at `dist/<m>/`. No reason found keeps a setting.
`bump --changed-since` builds its baseline in a temporary folder of its own,
and does not need the flag.

Recommendation: drop both `build.out` and `--out`.

### D2. The scope of a call (`/[[name]]`)

A call renders as an invocation, such as `/sdlc:task-work`, and copies no file.
The harness resolves it when the skill runs. Calls to a skill in another plugin
are common.

- Same scope as links: own plugin only.
- The whole marketplace.

Recommendation: the whole marketplace. A call to a skill outside it is
refused. A bare call name that two plugins share is refused as ambiguous,
naming both.

### D3. A plain link to another skill

Today a plain `[[other-skill]]` in a skill or command body is read as a call.
The link table in section 4 makes it a file link to the other skill's
installed path.

Recommendation: follow the table. A plain link is a file link, and a call is
written `/[[name]]`. The repo's own `plugin/` vault and any vault that relies on
the old reading changes output, and `check` shows it.

### D4. `build.assets` and `build.hooks`

Both name a plugin by its bare name, which is ambiguous with several
marketplaces, and both point at files the containment rule already confines to
one marketplace.

- Drop both. A plugin's shared `references/`, `scripts/` and `assets/` cover
  what `build.assets` copied. A `hooks.json` directly in a plugin's `hooks/`
  folder is its canonical hooks file. That is the path Claude Code reads by
  default (harness-reference: `Claude Code Plugin manifest fields`, the
  `hooks` field: "loaded together with hooks/hooks.json").
- Keep both, with `plugin: <m>/<p>` and a `from` checked by section 4.

Recommendation: drop both.

### D5. `build.exclude`

It is a glob list of what never ships. Under the strict folder rules in
section 3, an excluded file would still be a file the folder may not hold.

- Drop it. A draft or a note that must not ship goes in a `docs/` folder,
  which never ships.
- Keep it, read from the root folder only, and let it also exempt a file from
  the folder rules.

Recommendation: drop it. `docs/` already does the job, and one fewer setting
keeps the layout the only rule.

### D6. Plugin notes are required

v1 lets a plugin folder stand without a note. v2 requires `<p>.md`, so every
plugin has one place for its `version` and fields.

Recommendation: require it. `init` writes it.

### D7. Plugin-root files with no folder

v1 copied a plugin's `assets/` flat to the pack root, so `assets/bin/tool`
shipped as `bin/tool`. In the code and the tests that serves two things: the
launcher script (`build.launcher_path`, as in the `plugin-layout` probe) and
any file an author wanted at the pack root.

- `bin/`: harness-reference records no harness that reads a plugin's `bin/`.
  It is skillwright's own convention for the launcher. v2 moves the launcher to
  `scripts/`, which ships at the pack root under its own name.
- `.mcp.json`: Claude Code reads it at the plugin root
  (`Claude Code MCP config file`), and Claude Code, Codex and Cursor read an
  `mcpServers` manifest field (`<harness> Plugin manifest fields`). skillwright
  has no MCP kind; the flattening was the only way to ship one. The file and
  its keys differ per harness.

Options for `.mcp.json`:

- No home in v2. An MCP server kind comes later and renders each harness's own
  file.
- A plugin-root `.mcp.json` copied as is to the harnesses that read that name.

Recommendation: no home in v2. A copied file would be right for one harness
and wrong for the others.

### D8. Embedding a file that is neither a note nor an image

`![[run.sh]]` or `![[data.csv]]` has no inline form in Markdown. v1 turns it
into a plain link and copies the file.

- Refuse it, and say to write a link, `[[run.sh]]`, instead.
- Render it as a plain link, as v1 does.
- Inline a text file as a fenced code block.

Recommendation: refuse it. An embed that does not embed misleads the author,
and a link says what happens.

## Later

The layout is deliberately the recommended practice and nothing more. More
flexible structures may come in a later version of this spec.

These commands are out of v2 and may return in later milestones: `publish` to
a skills folder, `harness install`, `update`, `uninstall`, `link` and
`unlink`, `adopt`, `new`, `rename`, `move`, `build --watch`, and the other
current commands (`list`, `status`, `delete`, `convert`, `author`,
`unpublish`).
