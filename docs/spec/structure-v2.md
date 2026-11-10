# skillwright vault structure, version 2 (draft)

Status: draft, ready for final review. It replaces
[version 1](structure-v1.md) when accepted. Nothing reads both: a vault in
any other layout is refused (section 11).

v2 has one goal: stable marketplace publishing, with transclusion and good
bundling. Its commands are `init`, `build`, `check` and `bump`.

This spec defines how skillwright reads a vault. A vault has one fixed layout.
It covers the folders, the root file, the notes, links, the frontmatter rules
and the output. The frontmatter keys themselves are in the
[field reference](../reference/fields.md), which is part of this spec.

The decisions behind this spec are recorded in section 12. The effect on
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

The syntax decides what a token is, not its target. `/[[x]]` is a call (rule
8). `![[x]]` is an embed (section 5). Any other `[[x]]`, and a relative
Markdown link, is an ordinary link. An embed resolves by the same rules as a
link: what it names must be in scope. The rules apply to links from a
component's note and from any Markdown file it ships. Code is fully literal:
nothing inside a code span or fenced block is read, so a call, link or embed
written there ships exactly as written (rule 12).

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
| Another component's note, or a file in its folder, same plugin | Allowed. Rewritten to its installed path. A link to a skill's or command's note also warns (rule 7). |
| A component the harness's build does not carry, such as any agent | Refused for that harness. |
| A file in another plugin | Refused. |
| Anything outside `marketplaces/<m>/` | Refused. |

A call, `/[[name]]`, is not a link and copies nothing. Its scope is the whole
marketplace (rule 8, decision D2).

3. A bare `[[name]]` resolves in the linking file's component folder first.
   With no match there, it resolves among the plugin's shared folders and its
   components together. It never resolves outside the plugin. More than one
   match in a step is an error that names each match.
4. A link with a path resolves relative to the linking file. Its result must
   still fall in a row the table allows.
5. An installed path is written relative to where the linking file lands in the
   pack. From a skill's entry note, the plugin's `references/x.md` becomes
   `../../references/x.md`. Every harness built for keeps a pack whole on
   install, so the path holds after install (section 9).
6. A link to another component's note, or to a file in its folder, is rewritten
   to where that file lands in the same pack. From `skills/author/author.md`,
   `[[setup]]` becomes `[setup](../setup/SKILL.md)` and `[[setup#Usage]]`
   becomes `[setup](../setup/SKILL.md#Usage)`: a fragment is kept. A skill's
   link to its own note is a file link to its own file, `[author](SKILL.md)`.
   A component the target harness's build does not carry, such as an agent in
   any build or a command outside Claude Code, has no installed path, and a
   link to it is refused for that harness.
7. A plain link to another skill's or command's note warns, naming the file
   and the link: `Did you mean to call /[[setup]]?`. The link still ships as a
   file link. A skill's link to its own note does not warn.
8. `/[[name]]` is a call (decision D3). The `/` counts where it starts
   a line or follows whitespace or one of `` ( " ' ` * > _ | [ = ``, as in v1.
   Anywhere else, such as `dir/[[x]]` or `[[A]]/[[B]]`, the `/` is text and
   `[[x]]` is an ordinary link.
   - A call to a skill renders as the string that runs it on that harness, in
     backticks: `` `/skillwright:setup` `` on Claude Code,
     `` `$skillwright:setup` `` on Codex. Nothing is copied.
   - A call with words keeps them: `/[[setup|the setup skill]]` renders as
     ``the setup skill (`/skillwright:setup`)``.
   - A fragment on a call, `/[[setup#Usage]]`, is an error, because a call
     cannot target a section. Link or embed the section instead.
   - The name is looked up among the marketplace's skills (decision D2). A
     name two plugins share is refused, naming both; write the path of the one
     meant. A call to a skill the harness's build does not carry is refused.
   - A call to a command renders the same way on Claude Code, the only harness
     whose build carries commands, and is refused for any other harness. A
     call to an agent is refused: no build carries one.
9. `\/[[x]]` ships as the text `/[[x]]`, as in v1.
10. A link into a never-ship folder is refused with the same error in every
    link form.
11. A link to a missing file is an error that names the link.
12. Code is fully literal. Nothing inside a code span or fenced block is read:
    no call, link or embed. A call written in code ships as written, so write
    calls in prose, where they render in backticks.
13. After the build, no `[[` may remain in prose. A leftover is an error that
    names the file and the text. The check skips code spans, fenced blocks and
    `\[[`-escaped text. In prose, `\[[x]]` ships as `[[x]]`.
14. Every error in this section names the linking file, the link as written and
    the rule it broke.

## 5. Transclusion

An embed, `![[target]]`, is replaced at build time by the target's content.
Embeds are resolved before links are rewritten, so the output holds no `![[`.

### What may be embedded

1. The target resolves by the scope rules of section 4: the embedding file's
   own component folder, then the plugin's shared folders and components. An
   embed outside that scope is refused with the same error as a link.
2. A note (`.md`) is inlined (rules 4 to 8). An image (`.avif`, `.bmp`,
   `.gif`, `.jpeg`, `.jpg`, `.png`, `.svg`, `.webp`) becomes a Markdown image
   link to the file's installed path. Any other text file is inlined as a
   fenced code block (rules 9 to 14, decision D8).
3. An embed whose target does not exist is an error that names the embed. An
   embed inside a code span or fenced block is not read (section 4 rule 12).

### Inlining a note

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

### Inlining another text file

9. `![[run.sh]]` inlines the file as a fenced code block. The content is never
   escaped or changed, and nothing in it is read as a link, call or embed.
10. The fence's info string comes from the file's extension through a fixed
    table, such as `.sh` to `bash` and `.py` to `python`. An extension the
    table does not list gives no info string.
11. The fence is backticks, one more than the longest run of backticks that
    starts any line of the file, and never fewer than three. A file holding a
    ```` ``` ```` block is fenced with four.
12. Inside a list item, every line of the block is indented to the item's
    content, so the block stays in the item.
13. A file that is not valid UTF-8 is binary, and embedding it is refused. A
    fragment on a non-note file, such as `![[run.sh#main]]`, is refused.
14. A plain `[[run.sh]]` is not an embed. It stays a file link to the file's
    installed path.

### Nesting

15. An embedded note may embed others. Each level is expanded the same way.
16. A cycle is an error, `cycle`, that prints the loop from its first repeat,
    such as `a.md -> b.md#Setup -> a.md`. A hop is a file and a fragment, so
    two embeds of different sections of one note are not a cycle. A note that
    embeds itself is a cycle.

### Links inside embedded text

17. A link or embed inside embedded text resolves from the embedded file: its
    own folder and its own component's scope, as if it were read in place.
18. It is then written relative to where the text lands, the file that holds
    the outermost embed. The same note embedded into two files links
    differently in each.
19. A call (`/[[name]]`) inside embedded text renders as it would in place.

### Differences from v1

20. v1 resolved an embed vault-wide and copied a linked file from anywhere in
    the vault to `references/vault/` or `assets/vault/`. v2 resolves within the
    plugin, and every file it can reach already ships (section 9), so nothing
    is copied for a link.
21. v1 reported an embed or link to nothing and shipped its words. v2 refuses
    it (section 4 rule 11).
22. v1 turned an embed of a file that is neither a note nor an image into a
    plain link. v2 inlines a text file as a fenced code block (rules 9 to 14).
23. v1 read calls inside code spans and fenced blocks. v2 reads nothing there
    (section 4 rule 12).

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

## 12. Decisions

Every decision below is settled. Each one can be read on its own: what it
covers, what v1 does, the rule v2 follows, why, and what changes for existing
vaults.

### D1. `build.out` and `--out`

**What it is.** `build.out` in `skillwright.md`, or `--out` on the command
line, says where `build` writes its output.

**Today.** It is required. The repo's own `plugin/` builds beside its root
file:

```yaml
build:
  out: .        # writes plugin/.claude-plugin/, plugin/.agents/, ...
```

**Decided.** Both are dropped. Output always lands in `<root>/dist/<m>/`, such
as `plugin/dist/osdlc/.claude-plugin/`. A repo that wants the catalog at its
own root copies it there, for example in CI.

**Why.** The build stops at one fixed place, and nothing else needs a setting.
Output that can land anywhere needs the output-overlaps-source checks and the
`stale-output` warning; a fixed place needs neither. `bump --changed-since`
uses its own temporary folder.

**Existing vaults.** Remove `out:`. The repo's `plugin/` built trees move from
`plugin/.claude-plugin/` and its siblings to `plugin/dist/osdlc/`. The
`plugin` workflow diffs that folder, and `plugin-release` copies from it.

### D2. The scope of a call

**What it is.** How far `/[[name]]` looks for the skill it calls.

**Today.** v1 looks a called name up across the whole vault. The repo's
`author` skill calls `/[[setup]]` in its own plugin, which renders as
`/skillwright:setup` on Claude Code, `$skillwright:setup` on Codex and
`/skill:skillwright-setup` on Pi. If two skills share the name, v1 stops with
`skill-call-ambiguous` and asks for the path of the one meant.

**Decided.** A call looks across the whole marketplace. `/[[review]]` finds
`review` in any plugin of the marketplace. A name two plugins share is
refused, and the error names both. Links and embeds stay within the plugin.

**Why.** A call copies no file, so it does not break containment. Calls across
plugins are common, such as `/sdlc:task-work` from a `craft` skill, and an
own-plugin scope could not express them. The cost is that a call can name a
skill in a plugin the user did not install; the harness then reports an
unknown command at run time.

**Existing vaults.** None, unless a call names a skill in another marketplace
of the same vault. That call is now refused.

### D3. Calls and plain links to a skill

**What it is.** What `/[[setup]]` and `[[setup]]` mean when `setup` is a skill.

**Today.** v1 has two forms. `/[[setup]]` is a call: it renders as
`/skillwright:setup` on Claude Code. `[[setup]]` without the `/` is a mention:
it renders as the skill's name, `skillwright:setup`, and runs nothing. A
skill's link to its own note ships as a file link, `[setup](SKILL.md)`
([#53](https://github.com/osdlc/skillwright/issues/53) asked for the name).
v1 also read calls inside code spans and fenced blocks.

**Decided.** The syntax decides, not the target. v2 keeps the call and makes a
plain link a real link (section 4 rules 6 to 8).

- `/[[setup]]` is a call, as in v1, with v1's `/` positions and its `\/[[`
  escape. A call with words keeps them. A fragment on a call is an error.
- `[[setup]]` is a file link to the skill's installed `SKILL.md`, under the
  normal link scope (same plugin). `[[setup#Usage]]` links to that section.
  A skill's link to its own note is a file link to its own file, which settles
  #53 in that form.
- A plain link to another skill's or command's note warns
  `Did you mean to call /[[setup]]?`. A self-link does not warn.
- There is no mention form. Nothing renders a bare skill name.
- Code is fully literal, so a call in a code span is not read.

**Why `/` and not `@`.**

- `/` matches how Claude Code runs a skill or command, `/name`, so the source
  reads like its output.
- v1 and existing vaults already use it, so nothing has to be rewritten.
- Its cost is the clash with paths, such as `dir/[[x]]`. The call-position
  rule handles that: a `/` inside a word or path is text.
- `@` was considered and rejected. In Claude Code and Cursor it means mention
  or attach, `@file` or `@agent`, which is what a plain link already does. It
  also clashes with email addresses and GitHub `@user` mentions, and every
  existing vault would have to be rewritten.
- Codex runs skills with `$` either way. The build writes each harness's own
  form from `/[[name]]`.

**Existing vaults.** A v1 mention, `[[name]]`, now ships as a file link and
warns; write `/[[name]]` if a call was meant, or keep the link. A call inside a
code span or fenced block now ships as written; move it into prose. The repo's
`plugin/` has 10 calls, all `/[[...]]`. The three heading self-calls, such as
`# /[[author]]`, are in prose and need no change. The other seven sit in code
spans, such as `` `/[[setup]]` ``, and v1 read them as calls. They move into
prose, keeping the `/`: `/[[setup]]` without the backticks. A call renders in
backticks, so the output is the same. The `author` skill's text on links and
calls (`plugins/skillwright/skills/author/author.md`, lines 59 to 73) is
rewritten to describe v2; its syntax examples stay in code and ship as written.

### D4. `build.assets` and `build.hooks`

**What it is.** Two lists in `skillwright.md` that copy extra files into a
plugin's pack.

**Today.**

```yaml
build:
  assets:
    - {from: shared/LICENSE, to: LICENSE, plugin: tools}
  hooks:
    - {from: plugin/hooks.json, plugin: tools}
```

`LICENSE` lands at the root of the `tools` pack. `hooks.json` is translated
into each harness's hooks file.

**Decided.** Both are dropped. The author puts the file in the plugin:
`plugins/tools/assets/LICENSE` ships as `<pack>/assets/LICENSE`, and
`plugins/tools/hooks/hooks.json` is the hooks file.

**Why.** The plugin folder already says where things go. `hooks/hooks.json` is
the path Claude Code reads by default (harness-reference: `Claude Code Plugin
manifest fields`). Keeping the lists would mean two settings with their own
path checks, and `plugin: tools` would need a marketplace prefix to be
unambiguous. The cost is that a file shared by several plugins is copied into
each.

**Existing vaults.** Move each file into its plugin. The repo's `plugin/` uses
neither.

### D5. `build.exclude`

**What it is.** A list of globs in `skillwright.md` for files that never ship.

**Today.**

```yaml
build:
  exclude: ["**/*.draft.md"]
```

`skills/review/notes.draft.md` stays in the vault and is not shipped.

**Decided.** `build.exclude` is dropped. A file that must not ship goes in a
`docs/` folder, such as `skills/review/docs/notes.md`. `docs/` never ships.

**Why.** `docs/` already does the job, and the layout stays the only rule. An
excluded file would also need an exemption from the folder rules of section 3,
which refuse `notes.draft.md` beside `review.md`. That would be a second way to
say what a folder may hold.

**Existing vaults.** Move excluded files into `docs/`. The repo's `plugin/`
uses no `exclude`.

### D6. Plugin notes are required

**What it is.** Whether `plugins/<p>/<p>.md` must exist.

**Today.** A plugin folder builds without a note. Its name is the folder's
name, and its version is `0.1.0` with a warning.

**Decided.** The note is required. `plugins/tools/` with no `tools.md` is an
error that names the missing path. `init` writes the note.

**Why.** Every plugin has one place for its `version`. An optional note gives
the version two sources, the note or a default, and `bump` needs a note to
write to anyway.

**Existing vaults.** Add a note to any plugin without one. The repo's
`plugin/` already has `plugins/skillwright/skillwright.md`.

### D7. `.mcp.json`

**What it is.** A plugin's MCP server configuration. Claude Code reads
`.mcp.json` at the plugin root (harness-reference: `Claude Code MCP config
file`). Claude Code, Codex and Cursor also read an `mcpServers` manifest field.

**Today.** skillwright has no MCP kind. An author ships the file through the
flat assets copy: `plugins/tools/assets/.mcp.json` lands at `<pack>/.mcp.json`
for every harness.

**Decided.** `.mcp.json` has no home in v2. A `.mcp.json` in a plugin is
refused by the folder rules. An MCP server kind comes later and writes each
harness's own file.

**Why.** A copied `.mcp.json` is right for Claude Code and may be wrong for the
other harnesses, whose files and keys differ. A file that is right for one
harness and wrong for others breaks installs quietly.

**Existing vaults.** A plugin that ships `.mcp.json` today cannot build with
v2 until the MCP kind lands. The repo's `plugin/` has none.

### D8. Embedding a file that is neither a note nor an image

**What it is.** `![[name]]` where the target is, say, a script or a CSV file.

**Today.** `![[run.sh]]` ships as a plain link, `[run.sh](scripts/run.sh)`,
and the file is copied. The `!` changes nothing.

**Decided.** A text file is inlined as a fenced code block (section 5 rules 9
to 14). With `scripts/run.sh` holding `echo hi`, `![[scripts/run.sh]]` ships as:

````text
```bash
echo hi
```
````

- The info string comes from the extension: `.sh` gives `bash`, `.py` gives
  `python`, and an extension not in the table gives none.
- The fence is one backtick longer than the longest backtick run that starts a
  line in the file, and never fewer than three. The content is never escaped
  or changed, so a file with its own ```` ``` ```` block and `[[` text ships
  intact.
- Inside a list item, the block is indented to match the item.
- A binary file (not valid UTF-8) is refused. A fragment on a non-note file,
  `![[run.sh#main]]`, is refused.
- A plain `[[run.sh]]` stays a file link.

**Existing vaults.** An `![[file]]` of a text file that shipped as a link now
ships the file's text. Write `[[file]]` to keep the link. An embed of a binary
file is now refused. The repo's `plugin/` has none.

## Later

The layout is deliberately the recommended practice and nothing more. More
flexible structures may come in a later version of this spec.

These commands are out of v2 and may return in later milestones: `publish` to
a skills folder, `harness install`, `update`, `uninstall`, `link` and
`unlink`, `adopt`, `new`, `rename`, `move`, `build --watch`, and the other
current commands (`list`, `status`, `delete`, `convert`, `author`,
`unpublish`).
