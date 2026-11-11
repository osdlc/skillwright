---
description: "Author coding-agent skills, plugins and marketplaces as markdown notes in a skillwright vault: the fixed folder layout, the frontmatter (`description`, `version`, `targets` and the harness blocks), links, calls and embeds between notes, plugin-root paths, and building, checking, bumping and shipping the `dist/` output. Use when asked to write a new skill, add a plugin or marketplace, link or embed files, bump a plugin version, or build the vault for Claude Code, Codex, Cursor and Pi."
---

# /[[author]]

Scope: writes vault notes and builds them. If `skillwright --version` fails, or there is no vault
yet, run /[[setup]] first.

## The vault

A vault has one fixed layout. Where a note sits decides what it is:

```text
<root>/
├── skillwright.md                  the root file: shared defaults and build settings
├── dist/                           build output, one folder per marketplace
└── marketplaces/<m>/
    ├── <m>.md                      the marketplace
    └── plugins/<p>/
        ├── <p>.md                  the plugin
        ├── CHANGELOG.md            optional, copied into each pack
        ├── references/             shared docs
        ├── scripts/                shared code
        ├── assets/                 shared output files, such as templates
        ├── skills/<s>/<s>.md       a skill, with optional references/, scripts/, assets/
        ├── agents/<a>/<a>.md       an agent (no pack ships agents yet)
        ├── commands/<c>/<c>.md     a command (Claude Code only)
        └── hooks/hooks.json        the plugin's hooks
```

- Every note is `<name>.md` in a folder of the same name. The folder's name is the note's name.
- Each marketplace folder needs `<m>.md`, and each plugin folder `<p>.md`. The body of these
  two notes is for you; it does not ship.
- A name is lowercase letters, digits and single hyphens, at most 64 characters. `marketplaces`,
  `plugins`, `skills`, `agents`, `commands` and `hooks` are reserved.
- A folder holds only the entries the layout lists. Anything else, such as a `README.md` in
  `skills/`, is an error.
- `src/`, `docs/`, `tests/` and `fixtures/` in a plugin or component folder never ship. Put
  source, drafts and notes for yourself there.
- skillwright runs no build step of its own. A generated file, such as a bundled script, is made by
  your own build step into `scripts/`, `references/` or `assets/` before `skillwright build`.

To add a skill, create `marketplaces/<m>/plugins/<p>/skills/<s>/<s>.md`. To add a plugin, create
`plugins/<p>/<p>.md` with a `description` and a `version`.

## Frontmatter

- `description` is required on a skill, command or agent. Write it in the third person: what the
  component does and when to use it. The limit is 1024 characters.
- There is no `name` key. A note that sets one is refused; the folder names it.
- `version` is a plugin's semver, in `<p>.md`. Without one the plugin ships as `0.1.0`, with a
  warning. A marketplace's `version` is optional.
- `owner` goes in `<m>.md`. `keywords` and `category` go in `<p>.md`.
- `targets` limits a component to some harnesses: `claude`, `codex`, `cursor` or `pi`. Absent
  means all four.
- `claude:`, `codex:`, `cursor:` and `pi:` hold fields only that harness reads. `cursor:` and
  `pi:` take `naming: bare` on a plugin to keep plain skill names; by default those two harnesses
  name a skill `<p>-<s>`.
- `author`, `license`, `repository`, `homepage`, `targets` and the harness blocks inherit: the
  note, then the plugin, then the marketplace, then `skillwright.md`. Set them once, high up.
- `build:` is read only in `skillwright.md`.
- A key skillwright does not know warns. On a skill it ships under `metadata:`; on a plugin or
  marketplace note it is ignored. There is no key prefix: `sw-version` is an unknown key.

## Links, calls and embeds

The syntax decides what a token is. Each form, as you write it in a note:

| You write | It is | The build |
|---|---|---|
| `/[[setup]]` | a call | writes the string that runs the skill on each harness, in backticks |
| `/[[setup\|the setup skill]]` | a call with words | keeps the words and adds the call after them |
| `[[guide]]`, `[text](references/guide.md)` | a link | checks the target ships, rewrites the path |
| `![[guide]]`, `![[guide#Usage]]` | an embed | inlines the note, or one section of it |
| `![[run.sh]]` | an embed of a text file | inlines it as a fenced code block |
| `\[[x]]` | escaped brackets | ships as `[[x]]`, not interpreted |

- A call renders as `/plugin:skill` on Claude Code, `$plugin:skill` on Codex, `/plugin-skill`
  on Cursor and `/skill:plugin-skill` on Pi. Write the call form, never one harness's string: a
  hand-written call string in prose is an error.
- A call looks the name up among the skills and commands of the whole marketplace. A name no
  plugin has, or two plugins share, is an error. A call cannot take a `#fragment`.
- A plain `[[setup]]` to another skill is a file link to its `SKILL.md`, and warns: did you mean
  to call it? Use the `/` form to run a skill.
- A name-only link looks first in the linking skill's own folder, then in the plugin's shared
  `references/`, `scripts/` and `assets/` and its components. It never leaves the plugin. Two
  matches in one step are an error; write a path, such as `[[../setup/references/notes.md]]`.
- A link or embed to a missing file, into a never-ship folder, or outside the plugin is an error.
  An external link (`https://…`) ships as written and is not checked.
- Code is literal. Nothing in a code span or fenced block is a call, link or embed, so write calls
  in prose. A call in code ships exactly as written.
- After the build no `[[` may remain in prose. Write `\[[` for literal brackets.

## Plugin-root paths

To name a file of the plugin, such as a shared script, write the dollar sign, then `PLUGIN_ROOT`
in braces, then the path: the braced form of `$PLUGIN_ROOT/scripts/tool.js`. This skill shows it
unbraced, because the build would rewrite the braced form here.

```sh
node $PLUGIN_ROOT/scripts/tool.js --check
```

- The braced form is checked and rewritten in prose and in code alike. The path must name a file
  the plugin ships, or the build fails.
- Claude Code gets its own variable, `CLAUDE_PLUGIN_ROOT` in braces, then the path. Codex, Cursor
  and Pi get a path relative to the file, such as `../../scripts/tool.js`.
- Never write Claude Code's own variable braced, in notes or in `hooks/hooks.json`; it is an
  error. Hooks use the braced `PLUGIN_ROOT` form too.
- To show the shell variable as text, write it unbraced, `$PLUGIN_ROOT` or
  `$CLAUDE_PLUGIN_ROOT`. The build leaves the unbraced form alone.
- A script in `scripts/` ships byte for byte. It should find its own files relative to itself,
  such as `$(dirname "$0")/lib.sh`.

## Steps

1. Edit the notes.
2. Run `skillwright check`. It runs every rule `build` runs and writes nothing. It reports each
   error and warning with the note and the rule, and exits 1 on drift between the vault and
   `dist/`. Fix every error and warning in the notes.
3. Build: `skillwright build --dry-run`, then `skillwright build --yes`. `--marketplace <m>`
   builds one marketplace; without it every marketplace builds.
4. Run `skillwright check` again. Exit 0 means `dist/` matches the vault.

## The dist/ output

`build` writes each marketplace to `dist/<m>/`, one tree per harness:

| Harness | Marketplace manifest | Packs |
|---|---|---|
| Claude Code | `.claude-plugin/marketplace.json` | `.claude-plugin/packs/<p>/` |
| Codex | `.agents/plugins/marketplace.json` | `.agents/packs/<p>/` |
| Cursor | `.cursor-plugin/marketplace.json` | `.cursor-plugin/packs/<p>/` |
| Pi | none | `.pi/packs/<p>/` |

- Never edit `dist/` by hand. A hand edit is drift; change the note and build again.
- `skillwright check` in CI fails when someone changed a note without rebuilding.
- Before a release, raise the plugin's version: `skillwright bump <p>` (`--minor`, `--major`), or
  `skillwright bump --changed-since <ref>` to bump every plugin whose output changed since a git
  ref. `bump --marketplace <m>` with no plugin raises the marketplace's own version.
- skillwright stops at `dist/`. Shipping is your step. Commit `dist/<m>/` to keep it beside the
  notes. To publish, copy its trees to the root of a marketplace repository, or copy one pack and
  its manifest entry into an existing marketplace repository.
