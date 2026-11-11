# Structure v2: what changes for v1 users (draft)

What [structure v2](structure-v2.md) changes for a vault built with today's
tool (v1): the commands and features it removes, the decisions it amends, what
each of its decisions changes for an existing vault, and the one known
consumer migration. How v2 is built and tracked is in [the plan](v2-plan.md).
"Section" numbers refer to structure-v2.md.

## Commands removed

v2 keeps `init`, `build`, `check` and `bump`. Every other command leaves v2
and may return in a later milestone:

| Command | Goes with it |
|---|---|
| `publish`, `unpublish` | `build.destinations` |
| `harness install`, `update`, `uninstall`, `link`, `unlink` | |
| `adopt` | `--from`, `--from-git`, `build.scan_locations` |
| `new`, `rename`, `move`, `delete`, `convert` | |
| `build --watch` | |
| `list`, `status`, `author` | |

## Features removed or changed

### The root and the root file

- A vault with no root file, read with every default.
- The walk-up for the nearest `skillwright.md`, and its reserved-name rule
  (`reserved-name`). The walk-up stays, but it now stops only at a folder
  that holds both `skillwright.md` and `marketplaces/` (section 1 rule 2).
- `--config`. `--vault` names the root.
- The root file as a marketplace. Its marketplace fields move to
  `marketplaces/<m>/<m>.md`.
- `workspace: true`, and `marketplaces/<m>/skillwright.md` as a member file.
  There is one root file, and every marketplace sits under it.
- The `name` key on any note. The folder name is the name, and a `name` key is
  refused.
- `build.namespace` and prefixed keys (`sw-version`, an `sw:` block). A
  prefixed key is now an unknown key.

### The layout

- A root `skills/`, `agents/`, `commands/` or `hooks/` as a plugin named after
  the marketplace, and the root file's `version` as that plugin's version.
- A root `plugins/` as the plugins of a single marketplace.
- Marketplaces below the top, and components promoted to a marketplace.
- Flat entry notes (`skills/x.md`) and `SKILL.md` entries.
- Optional plugin notes (decision D6).
- `build.patterns` and every custom folder glob.
- `build.exclude` (decision D5). A file that must not ship goes in `docs/`.
- Loose files in a component folder. Each folder now holds only the entries
  its row in section 3's "What a folder may hold" table lists. Anything else
  is an error.
- New: six reserved names (`marketplaces`, `plugins`, `skills`, `agents`,
  `commands`, `hooks`) for any marketplace, plugin or component.
- New: a `.mcp.json` in a plugin folder is an error (decision D7), not a
  shipped asset.
- New: the body of `<m>.md` and `<p>.md` is not interpreted and does not ship.
- Hook notes, `hooks/<h>/<h>.md`. Hooks come only from `hooks/hooks.json`. A
  folder or any other file in `hooks/` is an error.

### Output

- `build.out`, `--out` and the "no default output" rule. Output is fixed at
  `dist/<m>/`.
- `<harness>: {packs: <path>}`, with its overlap checks and the
  `stale-output` warning.
- `build.assets` and `build.hooks` (decision D4).
- The plugin `assets/` folder copied flat to the pack root (`assets/bin/tool`
  shipping as `bin/tool`). A plugin's `assets/` now ships as `assets/`. The
  launcher moves to `scripts/`.
- Bare skill names as Cursor's default. Cursor now writes a plugin's skill as
  `<p>-<s>`, folder and `name`, as Pi already does. `cursor: {naming: bare}`
  keeps today's plain names (section 8 rule 6). A call to a Cursor skill
  renders `/<p>-<s>`, not `/<s>`.
- New: `check` with no `dist/<m>/` reports drift (exit 1).
- New: `build` with no `--marketplace` builds every marketplace. The working
  directory no longer picks one.

### Links, calls and embeds

| v1 | v2 |
|---|---|
| A plain `[[skill]]` rendered the skill's name (a mention). | An ordinary file link that warns `Did you mean to call /[[skill]]?` (D3). |
| A link resolved vault-wide: a basename match across the vault, the `own_folder_candidate` tiebreak, and `link-ambiguous` across folders. | It resolves within the plugin in two steps (section 4 rule 3). |
| A linked file from anywhere was copied to `references/vault/` or `assets/vault/`. | Every reachable file already ships, so nothing is copied for a link. |
| An embed or link to nothing was reported, and its words shipped. | An error (section 4 rule 11). |
| An embed of a file that is neither a note nor an image became a plain link. | A text file is inlined as a fenced code block (D8). |
| Calls inside code spans and fenced blocks were interpreted. | Code is literal (section 4 rule 12). |
| A fragment on a call or a mention was checked and then dropped. | On a call, `/[[setup#Usage]]`, an error. On a plain link it is kept. |
| A note with malformed frontmatter embedded its body. | An error that names both notes (section 5 rule 4). |
| A hand-written call string was refused in prose and code. | Refused in prose only. In code it ships as written. |
| `\[[x]]` shipped with its backslash, as `\[[x]]`. | It ships as `[[x]]`: the backslash is removed (section 4 rule 13). |
| Two skills sharing a called name: `skill-call-ambiguous`, with the path of the one meant as the fix. | An error that names both plugins. There is no path form of a call; rename one. |

### Plugin-root paths

- `build.launcher_path` (decision D9). Authors now write
  `${PLUGIN_ROOT}/<path>`. Every such path in a component's Markdown is
  checked and replaced, not one configured path. A path that names nothing
  shipped, left as written in v1, is now an error.
- `${CLAUDE_PLUGIN_ROOT}` as the source form, in notes and in hooks files. It
  is now an error that says to write `${PLUGIN_ROOT}` (decision D9).

### Decisions amended

| Decision | Amendment |
|---|---|
| [D-WNTQ](../decisions/D-WNTQ-harness-profile-values-from-config.md) | `pi: {naming}` stays settable, `cursor: {naming}` is added, and `packs` is not settable. |
| [D-KVMB](../decisions/D-KVMB-bare-keys-by-default.md) | `build.namespace` and prefixed keys are gone. An unknown key still warns, with two exceptions: the removed keys `workspace` (root file) and `packs` (harness block) are refused. A key the table lists is refused where it does not belong: a key that is not inherited in the root file, `name` on any note, `build:` outside the root file, and the removed `build:` keys. |
| [D-GPYR](../decisions/D-GPYR-convention-first.md) | Its patterns layer is gone. |

## What each decision changes

Each decision's rule is in section 12. This part says what v1 does and what
an existing vault must change.

### D1. `build.out` and `--out`

v1 requires it. The repo's own `plugin/` builds beside its root file:

```yaml
build:
  out: .        # writes plugin/.claude-plugin/, plugin/.agents/, ...
```

Existing vaults:

1. Remove `out:`.
2. The repo's `plugin/` built trees move from `plugin/.claude-plugin/` and its
   siblings to `plugin/dist/osdlc/`.
3. The `plugin` workflow diffs that folder, and `plugin-release` copies from
   it.

### D2. The scope of a call

v1 looks a called name up across the whole vault. The repo's `author` skill
calls `/[[setup]]` in its own plugin, `skillwright`:

| Harness | Renders as |
|---|---|
| Claude Code | `/skillwright:setup` |
| Codex | `$skillwright:setup` |
| Pi | `/skill:skillwright-setup` |

If two skills share the name, v1 stops with `skill-call-ambiguous` and asks for
the path of the one meant.

Existing vaults: none, unless a call names a skill in another marketplace of
the same vault. That call is now an error.

### D3. Calls and plain links to a skill

v1 has two forms:

| Source | v1 renders | Runs the skill? |
|---|---|---|
| `/[[setup]]` | `/skillwright:setup` on Claude Code | Yes: a call |
| `[[setup]]` | `skillwright:setup`, the skill's name | No: a mention |

A skill's link to its own note ships as a file link, `[setup](SKILL.md)`
([#53](https://github.com/osdlc/skillwright/issues/53) asked for the name).
v1 also interprets calls inside code spans and fenced blocks. v1 vaults
already write calls as `/[[name]]`, which is one reason v2 keeps the `/`.

Existing vaults:

1. A v1 mention, `[[name]]`, now ships as a file link and warns. Write
   `/[[name]]` if a call was meant, or keep the link.
2. A call inside a code span or fenced block now ships as written. Move it
   into prose.
3. The repo's `plugin/` has 10 calls, all `/[[...]]`:

   | Where | Count | Change |
   |---|---|---|
   | Heading self-calls, such as `# /[[author]]` | 3 | None: they are in prose. |
   | Code spans in skill notes, such as `` `/[[setup]]` `` | 6 | Move into prose, keeping the `/`. A call renders in backticks, so the output is the same. |
   | The plugin note's body | 1 | None: a container note's body is not interpreted and does not ship. |

4. The `author` skill's "Links" section
   (`plugins/skillwright/skills/author/author.md`, lines 57 to 75) is
   rewritten to describe v2. That includes its last bullet, v1's vault-wide
   `link-ambiguous` rule. Its syntax examples stay in code and ship as
   written.

### D4. `build.assets` and `build.hooks`

v1:

```yaml
build:
  assets:
    - {from: shared/LICENSE, to: LICENSE, plugin: tools}
  hooks:
    - {from: plugin/hooks.json, plugin: tools}
```

`LICENSE` lands at the root of the `tools` pack. `hooks.json` is translated
into each harness's hooks file.

Existing vaults:

1. Move each file into its plugin.
2. A built file is written there by the author's build step, with its source
   in `src/` or outside the vault.
3. The `sdlc` plugin uses `build.assets`; see "Consumer migration" below.

### D5. `build.exclude`

v1:

```yaml
build:
  exclude: ["**/*.draft.md"]
```

`skills/review/notes.draft.md` stays in the vault and is not shipped.

Existing vaults: move excluded files into `docs/`. The repo's `plugin/` uses
no `exclude`.

### D6. Plugin notes are required

v1 builds a plugin folder without a note. Its name is the folder's name, and
its version is `0.1.0` with a warning.

Existing vaults: add a note to any plugin without one. The repo's `plugin/`
already has `plugins/skillwright/skillwright.md`.

### D7. `.mcp.json`

v1 has no MCP kind. An author ships the file through the flat assets copy:
`plugins/tools/assets/.mcp.json` lands at `<pack>/.mcp.json` for every
harness.

Existing vaults: remove the `.mcp.json`, or keep the plugin on v1 until the
MCP kind lands. The repo's `plugin/` has none.

### D8. Embedding a file that is neither a note nor an image

v1 ships `![[run.sh]]` as a plain link, `[run.sh](scripts/run.sh)`, and copies
the file. The `!` changes nothing.

Existing vaults:

1. An `![[file]]` of a text file that shipped as a link now ships the file's
   text. Write `[[file]]` to keep the link.
2. An embed of a binary file is now refused.
3. The repo's `plugin/` has none.

### D9. Plugin-root paths replace `build.launcher_path`

v1, in steps:

1. `build.launcher_path` names one launcher, such as `bin/tool`, shipped
   through the flat assets copy.
2. A skill calls it as `${CLAUDE_PLUGIN_ROOT}/bin/tool`. Claude Code keeps the
   call.
3. For Codex, Cursor and Pi, the build rewrites that one path to
   `../../bin/tool`, relative to the skill's folder.
4. Any other `${CLAUDE_PLUGIN_ROOT}/...` path is left as written. That is dead
   text on those three harnesses.
5. If the plugin does not ship the launcher, the call is also left as written,
   with no warning.
6. On Pi, a separate step renames `${CLAUDE_PLUGIN_ROOT}/skills/<s>` to
   `skills/<p>-<s>`, but keeps the variable, which Pi does not expand.
7. A Markdown file in a skill's `references/` is rewritten relative to the
   skill's folder, not its own, so its `../..` is one level short.
8. Hooks write `${CLAUDE_PLUGIN_ROOT}`, swapped for each harness's own
   variable. v2 keeps the swap, with `${PLUGIN_ROOT}` as the source.

Existing vaults:

1. Remove `launcher_path`.
2. Move the launcher from `assets/bin/tool` to `scripts/tool`, and call it as
   `${PLUGIN_ROOT}/scripts/tool`.
3. Replace every `${CLAUDE_PLUGIN_ROOT}` in notes and hooks files with
   `${PLUGIN_ROOT}`.
4. A plugin-root path that names nothing shipped now fails the build.
5. The repo's `plugin/` uses neither the key nor the variable.

## Consumer migration: the sdlc plugin

The `sdlc` plugin in the owner's dev repo builds with
`solutions/ontological/agent-pants.yaml`. It ships about 25 `assets:` entries
from outside the vault to arbitrary pack paths, such as `cli/sdlc`,
`cli/sdlc.js`, `dashboard-spa/dist`, `conventions/`, `lib/model/entities` and
`lib/services/docs/*.eta`. v2 drops `build.assets` (D4), so it moves to the
`src/` and external-build pattern (section 3 rule 16):

1. Its own build step, such as a moon task, writes each file into the plugin's
   `scripts/`, `references/` or `assets/` before `skillwright build`.
2. Its pack paths move under those three folders:

   | Today | v2, for example |
   |---|---|
   | `cli/sdlc.js` | `scripts/sdlc.js` |
   | `conventions/` | `references/conventions/` |

3. Its runtime path lookups, and its skills' `${PLUGIN_ROOT}/...` paths,
   change to the new pack paths.

This is work in the dev repo, not in skillwright. A skill may run a
plugin-shared script through `${PLUGIN_ROOT}/scripts/...` (spec section 4
rule 19).
