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
   `marketplaces/` folder. That folder is the root. A vault looks like this:

   ```text
   <root>/
   ├── skillwright.md                  the root file: shared settings
   ├── dist/                           build output
   └── marketplaces/
       └── <m>/
           ├── <m>.md                  the marketplace
           └── plugins/
               └── <p>/
                   ├── <p>.md          the plugin
                   └── skills/<s>/<s>.md
   ```

2. `--vault <path>` names the root. Without it, skillwright walks up from the
   working directory and stops at the first folder that holds both
   `skillwright.md` and `marketplaces/`. Any other `skillwright.md` is passed
   over (see Appendix A).
3. v2 does not support a vault inside a larger notes vault. At the root, only
   `skillwright.md` and `marketplaces/` are read, and `dist/` is written
   (section 9). Anything else at the root is left alone.

## 2. The root file

A complete root file, with every key it may hold:

```yaml
---
# Shared defaults. Every marketplace and plugin inherits these unless it
# sets its own.
author: Ada Lovelace            # or {name, email, url}
license: MIT                    # an SPDX expression
repository: https://github.com/acme/skills
homepage: https://acme.dev/skills
targets: [claude, codex]        # which harnesses get each component; absent means all
claude: {}                      # harness blocks: fields only that harness reads (section 8)
codex: {}
cursor: {}
pi: {naming: bare}

# Tool settings.
build:
  harnesses: [claude, codex, cursor, pi]   # which harnesses to build; absent means all
  namespace: sw                            # read sw-prefixed keys; absent means bare keys
  bump:
    patch: increment                       # or timestamp
---
```

1. `skillwright.md` exists only at the root. Its body is not read, and it is the
   only configuration file.
2. Only the keys above are allowed. The shared defaults are the keys the key
   table marks as inherited.
3. A key that belongs to one marketplace, such as `name`, `description`,
   `owner`, `version`, `display_name`, `keywords` or `category`, is refused.
   The error names the marketplace note it belongs in.
4. These v1 `build:` keys are refused (section 11): `out`, `patterns`,
   `exclude`, `assets`, `hooks`, `destinations`, `scan_locations` and
   `launcher_path`. Any other unknown `build:` key warns.

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
   entry form: no flat `<name>.md` directly in a kind folder (`skills/`,
   `agents/`, `commands/`, `hooks/`), and no `SKILL.md`.
2. Position alone decides what a note is. A folder under `marketplaces/` is a
   marketplace. A folder under `marketplaces/<m>/plugins/` is a plugin. A folder
   under a plugin's `skills/`, `agents/`, `commands/` or `hooks/` is a component
   of that kind. Frontmatter never decides kind, plugin or marketplace.
3. Each marketplace folder must hold `<m>.md`, and each plugin folder `<p>.md`.
   A missing note is an error that names the path it expects.
4. The folder's name is the note's name. The build writes it as the `name`
   each harness reads, such as the `name` in `SKILL.md`. Authors do not write
   `name`. A `name` key that equals the folder's name is accepted, so a copied
   `SKILL.md` still works; any other value is refused.
5. A name is lowercase words joined by hyphens, at most 64 characters.
6. Names are unique within a plugin, across all kinds. A skill and a command
   with one name in one plugin are refused, naming both. The same name in two
   plugins, or two marketplaces, is allowed.
7. Every note at a component position is a component. A skill, command or
   agent needs a `description`, because harnesses use it to pick the
   component. A missing `description` is an error that `build` and `check`
   report.
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
    component in the plugin. A pack is the folder a harness installs for one
    plugin (section 9). Each shared folder ships under the same name at the
    pack root. Nothing is flattened: `assets/x` ships as `assets/x`.

    ```text
    plugins/tools/
    ├── scripts/deploy.sh        shared: ships as <pack>/scripts/deploy.sh
    ├── commands/deploy/
    │   └── deploy.md            one file: <pack>/commands/deploy.md
    └── skills/review/
        ├── review.md            ships as <pack>/skills/review/SKILL.md
        └── scripts/lint.sh      the skill's own: ships inside its folder
    ```
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
written there ships exactly as written (rule 12). A plugin-root path,
`${PLUGIN_ROOT}/<path>`, is not link syntax and is read in prose and
code alike (rules 15 to 26).

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
   match in a step is an error that names each match. From `author.md`:

   ```text
   marketplaces/acme/plugins/tools/
   ├── references/style.md          reached second: a shared folder
   └── skills/
       ├── author/
       │   ├── author.md            the linking note
       │   └── references/guide.md  reached first: its own folder
       └── setup/setup.md           reached second: another component
   marketplaces/acme/plugins/other/
   └── skills/x/x.md                refused: another plugin
   ```
4. A link with a path resolves relative to the linking file. Its result must
   still fall in a row the table allows.
5. An installed path is written relative to where the linking file lands in the
   pack. From a skill's entry note, the plugin's `references/x.md` becomes
   `../../references/x.md`. Every harness built for keeps a pack whole on
   install, so the path holds after install (section 9).

   ```text
   .claude-plugin/packs/tools/
   ├── references/x.md              from SKILL.md: ../../references/x.md
   └── skills/author/
       ├── SKILL.md                 the linking file lands here
       └── references/guide.md      from SKILL.md: references/guide.md
   ```
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
    no call, link or embed. Plugin-root paths are the one exception (rule
    15). A call written in code ships as written, so write calls in prose,
    where they render in backticks.
13. After the build, no `[[` may remain in prose. A leftover is an error that
    names the file and the text. The check skips code spans, fenced blocks and
    `\[[`-escaped text. In prose, `\[[x]]` ships as `[[x]]`.
14. Every error in this section names the linking file, the link as written and
    the rule it broke.

### Plugin-root paths

A component names a file of its plugin as `${PLUGIN_ROOT}/<path>`, such as
`${PLUGIN_ROOT}/scripts/tool`. The build replaces the string with each
harness's own form (decision D9).

15. Every `${PLUGIN_ROOT}/<path>` in a Markdown file a component ships is
    read: its note, the Markdown files in its `references/`, the plugin's
    shared Markdown files, and text an embed inlines. Prose and code alike,
    because this is a path, not link syntax. Only the braced form is read:
    `$PLUGIN_ROOT` is text.
16. The path runs to the first character that cannot continue a path: a
    letter, digit, `.`, `_`, `-` or `/` continues it. So
    `${PLUGIN_ROOT}/scripts/tool.js` is the path `scripts/tool.js`, never
    `scripts/tool`.
17. The path is a path in the plugin's pack, laid out as section 9 places
    files for Claude Code: `scripts/tool` is the plugin's shared
    `scripts/tool`, and `skills/setup/scripts/run.sh` is the `setup` skill's
    `scripts/run.sh`. It must name a file the build ships. If it does not, the
    build fails, and the error names the file holding the reference and the
    reference as written.
18. In Claude Code's pack it is written as `${CLAUDE_PLUGIN_ROOT}/<path>`,
    which Claude Code expands in skill, command and agent text.
19. In Codex's, Cursor's and Pi's packs it is written as the path of that same
    file in that harness's pack, relative to the folder the text lands in.
    None of the three expands a variable in skill text. From a skill's
    `SKILL.md`, `${PLUGIN_ROOT}/scripts/tool` becomes `../../scripts/tool`;
    from that skill's `references/guide.md` it becomes
    `../../../scripts/tool`. On Pi, a path into a skill follows the skill's
    folder name there, `skills/<p>-<s>/`. Embedded text lands in the file that
    embeds it, so the path is written from that file's folder. Files in
    `scripts/`, `references/` and `assets/` are all rewritten this way.
20. If a harness's pack does not carry the target, such as a command's file
    on Codex, the reference is refused for that harness, as a link to a
    component that is not carried is (rule 6).
21. `${PLUGIN_ROOT}` not followed by `/<path>` is an error in Markdown. It
    cannot be rewritten to a path for Codex, Cursor or Pi, and a vault writes
    one text for all harnesses.
22. `${CLAUDE_PLUGIN_ROOT}` anywhere in source, Markdown or a hooks file, is
    an error that says to write `${PLUGIN_ROOT}`. There is one syntax.
23. A hooks file, `hooks/hooks.json` or a hook note, also writes
    `${PLUGIN_ROOT}`. Each hook command's `${PLUGIN_ROOT}` becomes the
    harness's own variable, as in v1: `${CLAUDE_PLUGIN_ROOT}` for Claude Code,
    `${PLUGIN_ROOT}` for Codex, `${CURSOR_PLUGIN_ROOT}` for Cursor. Pi carries
    no hooks. A hook runs as a shell command from the project folder, where a
    pack-relative path would not resolve, and each of those harnesses sets its
    variable for hooks. A bare `${PLUGIN_ROOT}` is allowed in a hook, since
    the variable is kept. A path after it is checked by rule 17.
24. Files that are not Markdown or a hooks file, such as a script in
    `scripts/`, ship byte for byte and are never rewritten. A script should
    find its own files relative to itself, such as `$(dirname "$0")/lib.sh`,
    not through a harness variable.
25. A code example that shows a shell `${PLUGIN_ROOT}` is read as a
    plugin-root path, and rewritten or refused. To show the shell variable,
    write it unbraced, `$PLUGIN_ROOT/x`, which the shell reads the same way
    and the build leaves alone. There is no escape form.
26. A skill's own file named by a plain relative path, such as `scripts/x.py`
    in its `SKILL.md`, is text. It ships as written and is not checked. The
    Agent Skills standard, Claude Code, Cursor and Pi resolve it from the
    skill's folder (section 9). Codex is not recorded; the Codex probe in the
    build order covers it. Use `${PLUGIN_ROOT}/<path>` for a file the build
    should check, and for any file outside the skill's folder.

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

1. A key inherits only if the key table marks it as inherited. Those keys are
   `author`, `license`, `repository`, `homepage`, `targets` and the harness
   blocks, the same set the root file takes (section 2). Harness blocks inherit
   key by key.
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

   ```text
   dist/acme/
   ├── .claude-plugin/
   │   ├── marketplace.json
   │   └── packs/tools/             one pack per plugin
   ├── .agents/
   │   ├── plugins/marketplace.json
   │   └── packs/tools/
   ├── .cursor-plugin/
   │   ├── marketplace.json
   │   └── packs/tools/
   └── .pi/packs/tools/             Pi has no marketplace manifest
   ```

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
   For example, a plugin's source and its Claude Code pack:

   ```text
   marketplaces/acme/plugins/tools/    dist/acme/.claude-plugin/packs/tools/
   ├── tools.md                     →  ├── .claude-plugin/plugin.json
   ├── CHANGELOG.md                 →  ├── CHANGELOG.md
   ├── scripts/tool                 →  ├── scripts/tool          byte for byte
   ├── skills/review/                  ├── skills/review/
   │   ├── review.md                →  │   ├── SKILL.md         compiled
   │   └── references/guide.md      →  │   └── references/guide.md  compiled
   ├── commands/deploy/deploy.md    →  ├── commands/deploy.md    Claude Code only
   ├── hooks/hooks.json             →  ├── hooks/hooks.json
   └── agents/helper/helper.md         (nothing: no build carries an agent)
   ```
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
2. The root file has `workspace: true`, or a key the key table does not mark
   as inherited (section 2 rule 3).
3. A marketplace folder has no `<m>.md`. If it holds a `skillwright.md`, the
   error says the marketplace note is now `<m>.md`.
4. A kind folder (`skills/`, `agents/`, `commands/`, `hooks/`) or a `plugins/`
   folder is anywhere other than its place in section 3, such as at the root or
   directly in a marketplace.

   ```text
   <root>/
   ├── skills/                      refused: a kind folder at the root
   └── marketplaces/acme/
       ├── skills/                  refused: directly in a marketplace
       └── plugins/tools/skills/    accepted
   ```
5. A `marketplaces/` folder is anywhere below the top.
6. A component's note is not `<name>/<name>.md`, such as `skills/x.md` or
   `skills/x/SKILL.md`.

   ```text
   plugins/tools/skills/
   ├── review/review.md             accepted
   ├── lint.md                      refused: a flat note
   └── setup/SKILL.md               refused: write setup/setup.md
   ```
7. The root file has `build.out`, `build.patterns`, `build.exclude`,
   `build.assets`, `build.hooks`, `build.scan_locations` or
   `build.launcher_path`, or a `packs` key in a harness block.
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

### D9. Plugin-root paths replace `build.launcher_path`

**What it is.** How a component or a hook names a file of its plugin, such as
a launcher script, so that the path works on every harness.

**Today.** `build.launcher_path` names one launcher, such as `bin/tool`,
shipped through the flat assets copy. A skill calls it as
`${CLAUDE_PLUGIN_ROOT}/bin/tool`. Claude Code keeps the call. For Codex,
Cursor and Pi, the build rewrites that one path to `../../bin/tool`, relative
to the skill's folder. Any other `${CLAUDE_PLUGIN_ROOT}/...` path is left as
written, which is dead text on those three harnesses. If the plugin does not
ship the launcher, the call is also left as written, with no warning. On Pi, a
separate step renames `${CLAUDE_PLUGIN_ROOT}/skills/<s>` to `skills/<p>-<s>`
but keeps the variable, which Pi does not expand. A Markdown file in a skill's
`references/` is rewritten relative to the skill's folder, not its own, so its
`../..` is one level short. Hooks write `${CLAUDE_PLUGIN_ROOT}`, swapped for
each harness's own variable.

**Decided.** `build.launcher_path` is dropped and refused. Authors write
`${PLUGIN_ROOT}/<path>` (section 4 rules 15 to 26).

- In a component's Markdown, prose and code alike, the path is checked to name
  a file the pack ships. Claude Code's pack gets
  `${CLAUDE_PLUGIN_ROOT}/<path>`. Codex, Cursor and Pi get the path relative
  to the folder the text lands in.
- `scripts/`, `references/` and `assets/` files are rewritten the same way,
  whether a skill's own or the plugin's shared copy. A path has one meaning,
  so it has one rule.
- `${PLUGIN_ROOT}` with no path after it is an error in Markdown. There is no
  path to rewrite it to for three harnesses.
- A target the harness does not carry is refused for that harness, as a link
  to it is. A dead path breaks the skill at run time with no sign at build
  time.
- Hooks write `${PLUGIN_ROOT}` too and keep v1's swap to each harness's own
  variable. Their paths are checked. A hook runs from the project folder, so
  a relative path would not resolve.
- `${CLAUDE_PLUGIN_ROOT}` in source is an error that says to write
  `${PLUGIN_ROOT}`. One syntax means one thing to search for and check.
- Scripts ship byte for byte. A script finds its own files relative to itself.

**Why a build-time string replacement.** No harness but Claude Code expands
a plugin-root variable in skill text, so the variable cannot be left for the
harness to fill in.

- Only Claude Code substitutes a plugin-root variable in skill text
  (harness-reference: `Claude Code Plugin root variable`).
- Cursor expands its variables only in `mcp.json`, and it does not expand
  `${PLUGIN_ROOT}` (harness-reference: `Cursor Plugin root variable`).
- Codex documents `PLUGIN_ROOT` only for hooks.
- So the build replaces the string: Claude Code gets its own variable, and the
  other harnesses get a relative path, which each resolves from the folder
  the text is in.

**Why `${PLUGIN_ROOT}`.**

- It is the open Agent Plugins standard's name (harness-reference:
  `Agent Plugins Plugin root variable`).

- It reads as the shell variable it becomes, so the source looks like a
  working command.
- It names no harness. The vault is written once for every harness, and its
  text should not favour one.
- It is the name Codex already uses for hooks, so on Codex a hook ships as
  written.
- Keeping `${CLAUDE_PLUGIN_ROOT}` was rejected. It names one harness, and the
  source would read as if Claude Code were the target while three of four
  packs get something else.
- A template token such as `{{plugin}}` was rejected. It reads as neither a
  path nor a variable, it is close to the `{{harness}}` and `{{event}}` hook
  placeholders, and a source file using it is not runnable as written.

**The cost.** A code example that shows a shell `${PLUGIN_ROOT}`, such as
documentation of a Codex hook, is read as a plugin-root path. It is rewritten,
or refused when it names no shipped file. The simplest fix needs no new
syntax: write the variable unbraced, `$PLUGIN_ROOT`, which the shell reads the
same way and the build leaves alone. So v2 has no escape form for it.

**Why.** One setting covered one file, and every other plugin path was left
broken on three harnesses without a word. The plugin's folders are fixed in
v2, so the build knows where every file lands and can rewrite any path. The
check turns a typo or a moved file into a build error instead of a failure at
run time.

**Existing vaults.** Remove `launcher_path`. Move the launcher from
`assets/bin/tool` to `scripts/tool` and call it as
`${PLUGIN_ROOT}/scripts/tool`. Replace every `${CLAUDE_PLUGIN_ROOT}` in notes
and hooks files with `${PLUGIN_ROOT}`. A plugin-root path that names nothing
shipped now fails the build. The repo's `plugin/` uses neither the key nor
the variable.

## Later

The layout is deliberately the recommended practice and nothing more. More
flexible structures may come in a later version of this spec.

These commands are out of v2 and may return in later milestones: `publish` to
a skills folder, `harness install`, `update`, `uninstall`, `link` and
`unlink`, `adopt`, `new`, `rename`, `move`, `build --watch`, and the other
current commands (`list`, `status`, `delete`, `convert`, `author`,
`unpublish`).

## Appendix A: Edge cases

1. **A component named `skillwright`.** A skill, agent, command or hook is a
   component (section 3). A skill named `skillwright` has a note called
   `skillwright.md`, so the walk up from inside it meets a second
   `skillwright.md`. It is never taken for the root, because only the root has
   `marketplaces/` beside it. A `marketplaces/` folder anywhere below the root
   is refused (section 11), so the test needs no reserved names.

   ```text
   <root>/
   ├── skillwright.md                      the root file: marketplaces/ is beside it
   └── marketplaces/acme/plugins/tools/
       └── skills/skillwright/
           └── skillwright.md              a skill's note: no marketplaces/ beside it
   ```
