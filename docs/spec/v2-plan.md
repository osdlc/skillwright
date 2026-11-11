# Structure v2: implementation plan

## Goal

Build [structure v2](structure-v2.md) in skillwright as one PR. v2 reads one
fixed vault layout and has four commands: `init`, `build`, `check` and
`bump`. It writes each marketplace to `dist/<m>/` for Claude Code, Codex,
Cursor and Pi, with scoped links, calls, embeds and plugin-root paths. The
plan fixes what must hold and in what order: the commit order, the tests each
commit must have and the gates. How to write the code is the implementor's
choice.

## Read first

1. [structure-v2.md](structure-v2.md): the spec. Every rule in it needs a
   test.
2. [v2-changes.md](v2-changes.md): what v2 removes and changes for v1 users.
   It explains why many v1 tests are deleted, not ported.

"Section" numbers below refer to structure-v2.md.

## Settled and open

The whole spec is settled. There are no open questions and no blocking
probes.

These decisions change today's behaviour, or fix a detail the spec leaves to
the build. The commits must cover them:

| Decision | Where written | Commit |
|---|---|---|
| A removed command is clap's own unknown-command error: a usage error, exit 3 (`EXIT_USAGE`), with no "removed in v2" hint. | Spec section 10 | 1 |
| The flags are `--vault`, `--marketplace`, `--json`, `--quiet`, `--dry-run` and `--yes`, plus `bump`'s own. `--config` and `--namespace` go. | Spec section 10 rule 5 | 1 to 3 |
| `--yes` stays. On `origin/main`, `build` and `bump` both ask before writing (`confirm_write` in `commands/marketplace.rs` and `commands/bump.rs`), and off a terminal without `--yes` they exit 3. So the `plugin` workflow keeps `build --yes` and only drops `--config`. | Spec section 10 rule 6 | 2 |
| The wasm exports of removed features go: `publishSkill`, `adoptSkills`, `statusOf`, `publishableKinds` and `validateNamespace`. | This plan | 1 and 3 |
| Hooks come only from `hooks/hooks.json`. Hook notes are removed; richer hook authoring is [#152](https://github.com/osdlc/skillwright/issues/152). | Spec section 3 | 2 and 4 |
| The Cursor pack ships no hooks, with a warning, until a probe confirms Cursor sets `${CURSOR_PLUGIN_ROOT}` for hook commands. That probe is tracked in [#152](https://github.com/osdlc/skillwright/issues/152) and is not part of v2. | Spec section 4 rule 23 | 4 |
| Cursor's default naming becomes prefixed through a constant in its profile row, as Pi's is, not through an edit to harness knowledge. Knowledge records what Cursor allows (plain names); the default is skillwright's choice. | This plan | 4 |
| `\[[x]]` ships as `[[x]]`, with the backslash removed. Today's code keeps it. | Spec section 4 rule 13 | 5 |
| A hand-written call is detected in the four forms that name the plugin: `/p:s`, `$p:s`, `/p-s` and `/skill:p-s`, for a skill or command of the marketplace. A bare `/s` or `/skill:s` is not detected. There is no escape form; code ships as written. | Spec section 4 rule 8 | 5 |

## Probes

| Probe | Question | Role |
|---|---|---|
| `plugin-layout` ([#148](https://github.com/osdlc/skillwright/issues/148)) | Do models on Codex, Cursor and Pi resolve relative paths in skill text from the file they are written in, for references and scripts, both skill-local (`scripts/x`) and plugin-shared (`../../scripts/x`)? | Evidence for section 4 rule 19 only. It blocks no commit and is not in the definition of done. Method and fixture: [plugin-layout](../../harness-reference/corpus/methods/plugin-layout/plugin-layout.md). Record each run as a study result linked to it. If models fail, rule 19 is revisited in a follow-up; v2 ships as specified. |

## Build order

v2 is one PR, built as six ordered commits. Each commit is written test-first:

1. Write its tests.
2. See them fail for the right reason.
3. Make the change.
4. Run the gates.

### The gates

Each commit passes every gate alone, so the branch can be bisected and
reviewed commit by commit.

Rust, from `packages/rust` (the `rust` workflow):

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`. This
  builds the examples too.
- `cargo test --workspace --locked`
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`
- `cargo run --locked -q -p tradecraft-typegen -- --check`, and
  `packages/ts/tradecraft-wasm/src/core.generated.ts` equals
  `tradecraft-typegen/generated/core.generated.ts`.

TypeScript, from the repository root (the `ts` workflow):

- `pnpm install --frozen-lockfile`
- `pnpm run format:check`. Prettier checks these docs too.
- `pnpm run typecheck:scripts` and `pnpm run test:scripts`
- `pnpm run build:wasm` in `packages/ts/tradecraft-wasm`, then
  `pnpm run build`, `pnpm run typecheck`, `pnpm run lint` and
  `pnpm run test` from the root.

The repo's plugin (the `plugin` workflow): the build of `plugin/` matches
what is committed, and `check` passes.

### What each commit migrates

So that each commit passes alone, each one migrates what it breaks, in the
same commit:

- the fixture vaults it breaks, under `skillwright/tests/fixtures/` and
  `tradecraft-core/tests/fixtures/`;
- the goldens under `skillwright/tests/fixtures/golden/`, re-blessed with
  `TRADECRAFT_BLESS=1 cargo test -p skillwright --test build_golden`;
- `docs/reference/fields.md`, when the key table changes, regenerated with
  `TRADECRAFT_BLESS=1 cargo test -p tradecraft-core --test field_reference`;
- `core.generated.ts`, when a wasm export or its types change: run
  `cargo run -p tradecraft-typegen` and copy the file into
  `packages/ts/tradecraft-wasm/src/`;
- the TypeScript wrappers and tests in `packages/ts/tradecraft-wasm`;
- the repo's `plugin/`: its source, its rebuilt output, its two workflows,
  `scripts/plugin-marketplace.ts` and `plugin/README.md`'s build commands.

### 1. Cut to four commands

Delete every command but `init`, `build`, `check` and `bump`, with their core
code, tests, examples and wasm exports ("The removed commands" in the code
inventory). Do it first, so no later commit ports a command v2 drops.

Tests first:

- Each removed command, such as `publish` or `harness install`, is clap's
  unknown-command error with exit 3. The message has no "removed" hint.
- `build --watch` is an unknown flag, exit 3.
- `build.destinations` and `build.scan_locations` in the root file are
  refused (section 11 case 7).
- The existing `build`, `check` and `bump` tests still pass unchanged.

### 2. Layout reader, `init` and `bump`

Root discovery, the fixed-position `classify`, the one loader, the
marketplace note `<m>.md`, required plugin notes (D6), the folder rules of
section 3, and `init` and `bump` on the new layout. Delete the
nearest-`skillwright.md` walk-up, `--config`, `--skills-root`, reserved notes,
workspaces, patterns and `build.exclude` (D5), `SKILL.md` and flat entries,
and hook notes.

The root file's marketplace fields stop being read here; commit 3 refuses
them.

Tests first, layout:

- A v2 fixture vault lists every component with its marketplace and plugin.
- A root holding `README.md` and `notes/` beside `skillwright.md` and
  `marketplaces/` loads; neither is read (section 1 rule 3).
- `--vault` on a folder with `skillwright.md` but no `marketplaces/` is
  refused.
- A `skillwright` skill's note is not taken for a root (Appendix A1). A
  component named `skills` is refused.
- A loose file in a skill folder is refused, naming the allowed entries. A
  `README.md` in `marketplaces/` is refused.
- `src/` in a command or agent folder is refused; in a skill or plugin folder
  it is accepted.
- A `.mcp.json` in a plugin is an error. `assets/.eslintrc.json` is skipped.
- `references/docs/x.md` is accepted; `skills/review/docs/x.md` is a
  never-ship folder (commit 4 tests what ships).
- A name with digits, `v2-tools`, is accepted; `a--b` is refused.
- A skill and a command named `deploy` in one plugin are refused, naming
  both. The same name in two plugins is accepted (section 3 rule 7).
- `hooks/hooks.json` is accepted. A folder such as `hooks/lint/`, or any
  other file in `hooks/`, is an error.

Section 11, one test per case, each error naming the path and printing the
expected layout:

| Case | Test |
|---|---|
| 1 | No root at or above the working directory; the error names a `skillwright.md` passed over. |
| 3 | A marketplace folder with no `<m>.md`; with a `skillwright.md` in it, the error says the note is now `<m>.md`. Also a plugin folder with no `<p>.md` (section 3 rule 3). |
| 4 | `skills/` at the root, and directly in a marketplace. |
| 5 | `marketplaces/` below the top. |
| 6 | `skills/x.md` and `skills/x/SKILL.md`. |
| 7 | `build.patterns` and `build.exclude`. |
| 8 | A plugin's `bin/`, and a `notes.md` beside a skill's entry. |
| all | Two flat notes are reported once, listing both paths. |

Tests first, `init` and `bump`:

- `init acme tools` in an empty folder writes the three notes, and a `build`
  of it succeeds.
- `init` refuses a folder with `skillwright.md`, and a name such as `Tools`.
- `bump tools` writes `tools.md`. `bump --marketplace acme` writes `acme.md`.
  With plugin `osdlc` in marketplace `osdlc`, `bump osdlc` bumps the plugin.
- `bump` with a plugin name two marketplaces share asks for
  `--marketplace`.
- `build.bump.patch: timestamp` sets the patch to the UTC time. A patch
  already larger than that time becomes one more than itself (section 10
  rule 4).
- `bump --changed-since` bumps only a changed plugin.

The repo's `plugin/`:

- Its marketplace fields move from `plugin/skillwright.md` to
  `plugin/marketplaces/osdlc/osdlc.md`. `build.out` stays until commit 4.
- `plugin/plugins/skillwright/` moves to
  `plugin/marketplaces/osdlc/plugins/skillwright/`. Its skills are already
  `skills/<s>/<s>.md`; only the parent folders move.
- Both workflows run `--vault ../../plugin` in place of
  `--config ../../plugin/skillwright.md`. `build` keeps `--yes`.
- `scripts/plugin-marketplace.ts` reads the plugin's version from
  `marketplaces/osdlc/plugins/skillwright/skillwright.md`. Today it reads
  `vault/plugins/skillwright/skillwright.md`, which does not exist, so the
  release's version step is already broken. Its test changes with it.

### 3. Root file, keys and cascade

The root file's key set, the closed key table, the
root-marketplace-plugin-component cascade, and the `name` key. Delete
`build.namespace`, `--namespace`, prefixed keys and the `validateNamespace`
export.

Tests first:

- One table test per inherited key across the four levels.
- A plugin's `claude: {author: Bob}` beats its plain `author: Ada` for Claude
  Code only (section 7 rule 5).
- A key that is not inherited is refused in the root file, and the error
  names the note kinds that accept it: `owner` names `<m>.md`, `keywords`
  names `<p>.md`, `version` names both.
- An unknown root key warns. An unknown `build:` key warns.
- `build:` outside the root file is refused.
- A `name` key on any note is refused, and the error says to delete it.
- `sw-version` on a skill is an unknown key: it warns and is emitted under
  `metadata:`. An unknown key on a container warns and is ignored.
- A skill with malformed frontmatter is an error naming the note.
- A skill with no `description` is an error even when its plugin note has
  one (section 7 rule 5 covers inherited keys only).
- A plugin's `claude: {description: x}` does not set its skills'
  `description` (section 7 rule 4).
- A key in a plugin's block that a skill's kind does not accept stays with the
  plugin (section 7 rule 6).
- An unknown key in a harness block warns and is left out of that harness's
  output. A known key with a wrong value is refused (section 8 rule 2).
- A plugin with no `version` builds as `0.1.0` with a warning. A marketplace
  with no `version` writes none, with no warning.

Section 11:

| Case | Test |
|---|---|
| 2 | `workspace: true` in the root file is refused. So is `owner`. |
| 7 | `build.namespace` is refused. |

The repo's `plugin/`: its notes lose their `name` keys.

### 4. Bundling and output

Fixed `dist/<m>/`, default packs folders, `check` against `dist/`, the pack
contents of section 9, Cursor's naming default, and hooks. Delete
`build.out`, `--out`, the `packs` override, the overlap checks,
`stale-output`, `build.assets` and `build.hooks` (D4), and the flat `assets/`
copy.

This commit owns hooks translation. Its hook fixtures write `${PLUGIN_ROOT}`,
the source form from here on, and `${CLAUDE_PLUGIN_ROOT}` in a hooks file is
refused here. Commit 5 adds the check that a hook's path names a shipped
file.

Tests first, one test per row of the "What a pack holds" table:

- A plugin's `assets/logo.png` lands at `<pack>/assets/logo.png`.
- A skill's `scripts/run.sh` keeps its exec bit.
- A Markdown file in `references/` is compiled. `CHANGELOG.md` is copied
  byte for byte.
- `references/docs/x.md` ships; `skills/review/docs/x.md` does not.
- An agent ships nowhere.
- The Pi pack always holds `package.json`.
- An in-scope symlink in a skill's `scripts/` ships as a copy (section 9 rule
  11).
- Two sources on one path are refused, naming both.

Hooks:

| Harness | `hooks/hooks.json` with `${PLUGIN_ROOT}/scripts/hook.sh` |
|---|---|
| Claude Code | its hooks file, with `${CLAUDE_PLUGIN_ROOT}/scripts/hook.sh` |
| Codex | its hooks file, with `${PLUGIN_ROOT}/scripts/hook.sh` |
| Cursor | no hooks file and no manifest hooks entry; one warning naming `hooks/hooks.json` |
| Pi | nothing |

Also: no source but `hooks/hooks.json` adds hooks. `${CLAUDE_PLUGIN_ROOT}` in
a hooks file is an error that says to write `${PLUGIN_ROOT}`.

Output tests:

- A two-marketplace build writes two trees under `dist/`. `--marketplace`
  builds one.
- A file in `dist/acme/` beside the packs folders survives a build (section
  9 rule 4). A `README.md` at the root is untouched.
- `check` reports drift after a hand edit there, and when `dist/` is missing.
- A skill with `targets: [claude]` is only in the Claude Code pack.

Naming tests, for the `tools` plugin's `review` skill:

| Setting | Cursor and Pi | Claude Code and Codex |
|---|---|---|
| none | `skills/tools-review/SKILL.md`, `name: tools-review` | `skills/review/` |
| `cursor: {naming: bare}`, `pi: {naming: bare}` | `skills/review/` | `skills/review/` |
| marketplace `pi: {naming: bare}` | `skills/review/` on Pi, for every plugin | unchanged |
| that, plus the plugin's `pi: {naming: prefixed}` | `skills/tools-review/` on Pi | unchanged |
| another `naming` value | refused | refused |
| `naming` on the skill | refused | refused |

Also: a prefixed name over 64 characters is refused. With `bare`, a `review`
skill in two plugins fails the build for that harness, naming both plugins.
Plugin `a-b`'s skill `c` and plugin `a`'s skill `b-c` fail for Cursor and Pi.

Section 11:

| Case | Test |
|---|---|
| 7 | `build.out`, `build.assets` and `build.hooks` are refused, and so is `packs` in a harness block. |

The repo's `plugin/`:

- `plugin/skillwright.md` drops `build:`. It keeps `author` and `license`.
- The built trees move from `plugin/.claude-plugin/`, `plugin/.agents/`,
  `plugin/.cursor-plugin/` and `plugin/.pi/` to `plugin/dist/osdlc/`.
- The rebuild renames the Cursor pack's skills to `skills/skillwright-author/`
  and so on, with matching `name` fields and call strings. Nothing is renamed
  by hand. The Pi pack already uses these names.
- The `plugin` workflow's drift check covers `plugin/dist/osdlc/`.
- `scripts/plugin-marketplace.ts` copies from `plugin/dist/osdlc/`. Its
  `HARNESSES` roots and its test change with it.

### 5. Links, transclusion and plugin-root paths

Scoped resolution, the link table, the real-path check, missing targets as
errors, calls and plain links (D2, D3), hand-written calls, literal code, the
leftover check, the embed rules of section 5, and plugin-root paths (D9).
Delete `build.launcher_path`.

Tests first, through `build`, one test per rule in sections 4 and 5. All use
plugin `tools`.

Scope:

- A symlink in a skill folder that points out of the marketplace is refused.
- The plugin's `references/style.md` renders as `../../references/style.md`
  from a skill.
- `[[guide]]` from `author` prefers its own `references/guide.md` over the
  shared one.
- `[[guide]]` in the shared `references/style.md` starts at step 2, finds the
  shared `references/guide.md`, not `skills/author/references/guide.md`, and
  ships as `[guide](guide.md)` (Appendix A3).
- `[[layout]]` matches `references/design/layout.md` at any depth. With
  `references/old/layout.md` too, it is an error naming both (Appendix A4).
- `[[notes]]` naming a file inside another component is a link to nothing;
  `[[../setup/references/notes.md]]` works.
- `/[[setup]]` is tokenized as a call: it never becomes a link and never
  fires the "Did you mean to call" warning.
- A link to a file in another plugin is refused.
- A link into `docs/` is refused with the same error as `[[docs/x]]`,
  `[t](docs/x.md)` and `![[docs/x]]` (section 4 rule 10).

Markdown links (the table at the top of section 4):

| Source | Result |
|---|---|
| `[t](references/x.md)` | checked, and rewritten to its installed path |
| `[t](references/missing.md)` | an error naming the link |
| `[t](/abs/x.md)`, `[t](file:x.md)` | refused |
| `[t](https://x.io)`, `<https://x.io>`, `[t](mailto:a@b)` | shipped as written, not checked |
| `[t](#usage)` | shipped as written, not checked |

Plain links:

- `[[setup]]` from `author` renders as `[setup](../setup/SKILL.md)` on Claude
  Code and `[setup](../tools-setup/SKILL.md)` on Cursor and Pi, and warns
  `Did you mean to call /[[setup]]?`.
- `[[setup#Usage]]` keeps its fragment.
- A skill's link to its own note is `[author](SKILL.md)`, with no warning.
- A link to a command is a file link on Claude Code and fails the build for
  Codex, Cursor and Pi. A link to an agent fails the build.

Calls:

| Source | Claude Code | Codex | Cursor | Pi |
|---|---|---|---|---|
| `/[[setup]]` | `` `/tools:setup` `` | `` `$tools:setup` `` | `` `/tools-setup` `` | `` `/skill:tools-setup` `` |
| `/[[setup]]`, `naming: bare` | same | same | `` `/setup` `` | `` `/skill:setup` `` |
| `/[[setup\|words]]` | ``words (`/tools:setup`)`` | ``words (`$tools:setup`)`` | ``words (`/tools-setup`)`` | ``words (`/skill:tools-setup`)`` |
| `/[[deploy]]`, a command | `` `/tools:deploy` `` | error | error | error |
| `/[[helper]]`, an agent | error | error | error | error |
| `/[[deploy]]`, `deploy` with `targets: [claude]` (Appendix A5) | `` `/tools:deploy` `` | error naming the file and the call | the same | the same |

Also:

- `/[[setup#Usage]]` is an error.
- `/[[review]]` finds a skill in another plugin; a name two plugins share is
  an error naming both plugins. A name found only in another marketplace is
  an error.
- A `/` that starts a line, or follows whitespace or one of
  `( " ' * > _ | [ =`, starts a call: one test per character.
- `dir/[[x]]` is a link, not a call. In `[[A]]/[[B]]`, `[[B]]` is a link.
- `` `/[[x]]` `` in a code span ships as written. `\/[[x]]` ships as
  `/[[x]]` and passes the leftover check.
- A call in a container note's body does nothing (Appendix A12).

Hand-written calls (section 4 rule 8):

| In prose | Result |
|---|---|
| `/tools:setup`, `$tools:setup`, `/tools-setup`, `/skill:tools-setup` | an error that says to write `/[[setup]]`, one test per form |
| `/tools:deploy`, a command | the same error |
| `/setup`, `/skill:setup` | shipped as written |
| any of the four forms in a code span or fenced block | shipped as written |
| `\/tools:setup` | not an escape; the same error |
| `a/tools:setup`, `x$tools:setup` | not a call: the sigil continues a word or path |

Code and leftovers:

- `[[x]]`, `/[[x]]` and `![[x]]` in a code span or fenced block ship as
  written.
- `\[[x]]` in prose ships as `[[x]]`, and `\/[[x]]` ships as `/[[x]]`. Today's
  code keeps the backslash in `\[[x]]`, so this commit changes it.
- A leftover `[[` in prose is refused. One in code or after `\` is not.

Embeds:

- The worked example of section 5 ships as shown.
- A heading embed inlines the section with its subsections.
- A block embed inlines the block without its marker.
- A missing heading is `fragment-unresolved`.
- A nested embed's link is written relative to the outer file.
- A cycle prints its loop. A note that embeds itself is a cycle. Two embeds
  of different sections of one note are not.
- A call inside embedded text renders as it would in place (section 5 rule
  19).
- An embedded note's frontmatter is dropped. Malformed frontmatter is an error
  naming both notes.
- An image embed is an image link.

D8:

- `![[run.sh]]` ships as a `bash` fenced block. An unknown extension gets no
  info string.
- A file holding its own ```` ``` ```` block and `[[` text is fenced with four
  backticks and ships byte for byte, with its `[[` not interpreted. So is a
  file whose fence is indented three spaces.
- An embed inside a list item is indented to the item.
- A binary file is refused. `![[run.sh#main]]` is refused. `[[run.sh]]` is a
  link.

Plugin-root paths (D9):

| Source | Claude Code | Codex, Cursor, Pi |
|---|---|---|
| `${PLUGIN_ROOT}/scripts/tool` in a skill's prose or code span | `${CLAUDE_PLUGIN_ROOT}/scripts/tool` | `../../scripts/tool` |
| the same, from the skill's `references/guide.md` | `${CLAUDE_PLUGIN_ROOT}/scripts/tool` | `../../../scripts/tool` |
| `Run ${PLUGIN_ROOT}/scripts/tool.` | path `scripts/tool`, full stop kept | the same |
| `${PLUGIN_ROOT}/commands/deploy.md` | `${CLAUDE_PLUGIN_ROOT}/commands/deploy.md` | error |
| `$PLUGIN_ROOT/x` | as written | as written |

Also:

- A path into another skill follows `skills/<p>-<s>/` on Cursor and Pi.
- `${PLUGIN_ROOT}/scripts/tool.js` is not `scripts/tool`.
- A path naming nothing shipped fails, naming the file and the reference.
- A lone `${PLUGIN_ROOT}` in Markdown is an error. `${CLAUDE_PLUGIN_ROOT}` in
  a note or a code span is an error that says to write `${PLUGIN_ROOT}`.
- A plain `scripts/x.py` in a `SKILL.md` ships as written and is not checked.
- An embedded reference is written from the embedding file.
- A script in `scripts/` holding the variable ships byte for byte.
- `![[run.sh]]`, where `run.sh` holds `${PLUGIN_ROOT}/scripts/tool`, a lone
  `${PLUGIN_ROOT}` and a `${CLAUDE_PLUGIN_ROOT}`, inlines the file unchanged
  with no error.
- A hook command's `${PLUGIN_ROOT}/scripts/missing.sh` fails the build. A
  lone `${PLUGIN_ROOT}` in a hook is allowed.

Section 11:

| Case | Test |
|---|---|
| 7 | `build.launcher_path` is refused. |

The repo's `plugin/`: code is now literal, so the six `/[[...]]` calls in
code spans in the skill notes move into prose, keeping the `/`
(`` `/[[setup]]` `` becomes `/[[setup]]`). The output is the same. The three
heading self-calls, such as `# /[[author]]`, stay. The plugin note's
`` `/[[setup]]` `` stays too: a container note's body is not read.

### 6. Docs and the repo's plugin skills

Changes:

1. The reference docs: `docs/reference/skillwright-md.md` is rewritten; its
   Patterns, Several marketplaces and Publishing one note by path sections
   go. `docs/reference/fields.md` is already current from the commits that
   changed the key table.
2. The CLI README's verb and option tables shrink to the four commands. Its
   Configuration section is rewritten.
3. `packages/ts/tradecraft-wasm/README.md` lists the remaining exports.
4. `plugin/README.md` describes the v2 layout, `plugin/dist/osdlc/` and the
   v2 build commands.
5. The `publish`, `setup` and `author` skills rewritten for v2 (the table in
   "The repo's own `plugin/`"). The `author` skill's syntax examples stay in
   code, where they ship as written.

Tests first:

- The `plugin` workflow's drift check passes on the rebuilt
  `plugin/dist/osdlc/`.

## Definition of done

The PR is done when all of these hold:

- [ ] Every gate under "The gates" passes on every commit.
- [ ] Every numbered rule in the spec, and every case in its Appendix A, is
      covered by at least one test.
- [ ] `plugin/` is in the v2 layout and builds to `plugin/dist/osdlc/`, and
      the `plugin` workflow's drift check passes.
- [ ] `docs/reference/skillwright-md.md`, `docs/reference/fields.md`, the CLI
      README and the wasm package README describe v2.

## Code inventory

This inventory is a starting map of where v2 touches the code, read at
`2d11033c` (`origin/main` after
[#143](https://github.com/osdlc/skillwright/pull/143)), with paths under
`packages/rust/` unless they start at the repository root. It is not a set of
instructions: the implementor decides how, as long as the commit order, the
tests and the gates above hold. The number in brackets is the commit.

### tradecraft-core

- `paths.rs` [2]:
  - Go: `find_root_file`, `is_component_note`, `is_reserved_note`,
    `is_vault_root_file`, `is_marketplace_entry`, `is_flat_root_file_name`,
    `workspace_above`, `RootRead`, `read_workspace_above`, `workspace_file`,
    `read_root`, `read_root_both`.
  - Change: the walk-up now stops at a folder holding both `skillwright.md`
    and `marketplaces/`.
  - `marketplace_inputs` and `load_marketplace` become one loader, in steps:
    read the root file, list `marketplaces/*/`, read each `<m>.md`.
- `patterns.rs` [2]: almost all of it goes.
  - Go: `Patterns::canonical`, `plugin_folder`, `assets_folder`,
    `sibling_files`, `source_folders`, `warnings`, `warnings_with`,
    `keep_conventional`, `workspace_exclude_warnings`, and with D5 the exclude
    list and the glob matcher (`glob_match`, `glob_problem`).
  - Stays: the never-ship rule, which can move into `discover.rs`. It now
    applies only directly in a plugin or component folder.
- `settings.rs`:
  - Go: `FolderPatterns`, `folder_patterns`, `globs`, `warn_nested` [2];
    `DestinationSetting` and the `destinations` and `scan_locations` keys
    [1]; the `patterns` key [2]; `namespace` [3]; `out` [4], `AssetSetting`
    and `HookSetting` (D4) [4]; `launcher_path` [5]. Each removed key joins
    the refused list in its commit.
  - Change [3]: `RootFile::inherit` becomes the root-defaults step of the
    cascade. `compose_root_file` writes a root with no marketplace fields.
    `root_file_config` and `RootFile::to_config` shrink. An unknown root-file
    key warns. A key that is not inherited, and `workspace`, are refused.
  - Change [2]: `MarketplaceFields` is read from `<m>.md`, not the root file.
- `namespace.rs` [3]: the prefixed stem goes, with `InvalidStem` and
  `StemProblem`. Either delete `Namespace` and its parameter everywhere, or
  keep only `Namespace::bare()`. It is threaded through `discover.rs`,
  `export.rs`, `frontmatter.rs`, `keys.rs`, `paths.rs`, `source_view.rs`,
  `vault.rs`, `model/`, `harness/translator.rs` and `lib.rs`'s re-exports.
- `frontmatter.rs` [3]: the prefixed-namespace reading goes: the flat and
  nested shapes and the `mixed-frontmatter-shape` refusal. A `name` key is
  refused with "delete it".
- `keys.rs`: the `Hook` rows, which describe hook-note frontmatter, go [2].
  The `name` row becomes a refusal [3]. `ADOPTABLE` goes [1].
- `projection.rs`: stays; `harness/translator.rs` renders through it. Parts
  only removed commands use go [1]: `content_hash` (adopt) and `file_hash`
  (publish).
- `discover.rs` [2]:
  - `classify` becomes a fixed-depth match.
  - Go: `conventional`, `folder_placement_with`, `RESERVED_NAME_REASON`,
    `SKILL_ENTRY`, `is_skill_entry`, the `SKILL.md` branch of `is_entry`, and
    `placement`'s `container_level` (promoted components).
  - `container_notes`, `container_notes_with` and `read_container` read fixed
    paths.
  - `under_a_kind_folder` and `refuse_stray` turn into the layout check of
    section 11.
  - New: the six reserved names, the per-folder entry table, and the
    `.mcp.json` error.
- `model/cascade.rs`:
  - Go [2]: `only_marketplace`, `sole_marketplace`, `marketplaces_in`, and
    the "a note under no marketplace takes the one marketplace" rule.
  - [2]: `marketplace_note` reads `<m>.md`. `belongs_to` becomes a path
    prefix test.
  - New [3]: a marketplace with no `version` writes no version and no
    warning.
- `model/load.rs`:
  - Go or shrink [2]: `standalone` (publish of one note), `members_folder`,
    `has_folder`, the marketplace's-own-plugin branch of `missing_versions`,
    the loading of hook notes (`hooks/<h>/<h>.md`), and `plugin_folder`.
  - Go [4]: `load_assets`, `load_hook_files` (D4), `folder_assets` (the flat
    copy; replaced by copying the three shared folders under their own
    names), `asset_covers` (its last use was the launcher check).
  - Go [5]: the `plugin.launcher` set from `build.launcher_path`.
  - [2]: `marketplace_name` and `resolve_marketplace` read the folder.
    Container note bodies are no longer read.
  - `apply_naming` already fills `bare_on` and `prefixed_on` from the
    `cursor` and `pi` blocks; it needs no change. New [4]: a `naming` key on
    a component is refused, saying to set it on the plugin.
- `source_view.rs` [2, 4]: the workspace folding in `effective_config`,
  `effective_configs`, `marketplace_folder` and `marketplace_notes` goes [2].
  `output_overlaps_vault`, `output_in`, `output_paths` and
  `check_pack_location` go [4]. Output cannot overlap source when only
  `marketplaces/` is read.
- `compile.rs` [5]:
  - Resolution: `CompileSource::resolve` changes from vault-wide to the two
    steps of section 4 rule 3. A file with no component starts at step 2. A
    shared file lands at the pack root, and its link is written relative to
    where the linking file lands. `own_folder_candidate` and the
    `references/vault/` and `assets/vault/` placements go.
    `vault_link_index` indexes one plugin, not the vault.
  - Containment: a real-path check is added beside `refuse_never_shipped`.
  - Calls (D3): `TokenKind::Call`, `TokenKind::Escape` (`\/[[`), `sigil_at`,
    `replace_call`, `render_call` and `format_call` stay. The call positions
    no longer include a backtick. `check_call_fragment` turns from "checked,
    then dropped" into an error. The call index (`skill_index` in
    `model/load.rs`) adds commands for harnesses whose packs contain them.
  - Plain links: `skill_mention` and the `mention` path of `render_or_mark`
    go. A plain link to a skill is a file link, plus the new "Did you mean to
    call" warning.
  - Code is literal: `scan_code_calls`, `code_token`, `TokenKind::Unconverted`
    with its `unconverted-call` warning, and the in-code branch of
    `format_call` go.
  - Leftovers: `validate_no_obsidian_syntax` skips code spans, fenced blocks,
    `\[[`-escaped text and text a `\/[[` escape produced.
  - Hand-written calls: `literal_skill_calls` and `literal-skill-call` stay,
    with three changes. They skip code spans and fenced blocks. The
    `LiteralForm::Bare` form (`/skill`, for the note's own plugin) goes, and a
    `/p-s` form for Cursor is added. The `\/` and `\$` escape goes. The
    error says to write `/[[name]]`.
  - Missing targets: in `replace_token`, an embed or link to nothing becomes
    an error instead of an entry in `unresolved`.
  - Embeds: a non-note, non-image embed is inlined as a fenced block (D8).
    New code: the extension table, the fence length (counting fences indented
    up to three spaces), list indentation and the UTF-8 check. Malformed
    frontmatter in an embedded note becomes an error. Embed expansion,
    `select_fragment` and the cycle check stay.
- `harness/profile.rs` [4]:
  - `PluginFormat::packs` stays a fixed value. `owned_paths_at` and the config
    override path go.
  - The Cursor row: `naming` changes from `Source::Knowledge` to
    `Source::Constant(SkillNaming::PluginPrefixed)`, as the Pi row's is, with
    a comment saying why (see "Settled and open"). `naming_choice` is already
    `true`. The test that checks each row's naming against harness knowledge
    (`skills/plugin-namespace`) then treats Cursor like Pi.
  - The Cursor row: `carries_hooks` becomes `false` and
    `hooks_manifest_entry` becomes `None`. The build warns once per plugin
    with a `hooks/hooks.json`.
  - `CliInvocation` stays as the per-harness switch, now for every
    plugin-root path (D9) [5].
- `harness/translator.rs` and `harness/manifest.rs`:
  - `naming_of` and `plugin_block` need no change: the block already starts
    from the marketplace's, which holds the root file's defaults, and
    overlays the plugin's.
  - `refuse_prefixed_collisions` already refuses every duplicate, bare ones
    too, where the profile's default prefixes. With Cursor's default changed,
    it covers Cursor with no new code. Its 64-character check does too.
- `model/mod.rs` and `harness/translator.rs` (D9) [5]:
  - `Segment::LauncherCall` and `push_text`'s one-path split become a
    plugin-root path segment that holds any path after `${PLUGIN_ROOT}/`,
    checked against the files the pack ships. A trailing `.` is not part of
    the path.
  - A source `${CLAUDE_PLUGIN_ROOT}` and a lone `${PLUGIN_ROOT}` are refused
    there.
  - `write_body` writes `${CLAUDE_PLUGIN_ROOT}/<path>` for Claude Code. For
    the other harnesses it writes the path relative to the folder the text
    lands in, so a resource under `references/` gets its own depth, not the
    skill's.
  - `rewrite_sibling_paths` goes: Pi's `skills/<p>-<s>/` folder is where the
    rewrite lands. `rewrite_launcher_calls` goes with its test.
    `relative_path` stays.
- `hooks_file.rs` [4, 5]: `localize`, the variable swap, stays. Its
  `CANONICAL_ROOT` becomes `${PLUGIN_ROOT}`, and a source
  `${CLAUDE_PLUGIN_ROOT}` is refused [4]. It gains the path check [5].
- `export.rs` [4, 5]: the `stale-output` warning and moved-packs handling go
  [4]. `MarketplaceConfig::launcher_path` and the launcher tests go [5]; their
  cases move to commit 5's plugin-root path tests.
- `vault.rs` [2]: `bundle_root` keeps only the `<name>/<name>.md` shape.

### The removed commands [1]

- `tradecraft-core/src/publish.rs`:
  - Go: everything that writes to a destination: `publish_skill`'s
    single-note arms, the destination manifest, the rails, `publishes_to`,
    `overrides_for`, `override_states` and `accepted_override_keys`.
  - Stay, because `export.rs` and `model/cascade.rs` call them: the plugin
    arm, `plugin_format_carries_commands`, `plugin_format_carries_hooks`,
    `kind_has_renderer`, `canonical_target`, `KNOWN_TARGETS` and
    `accepted_keys`. Moving those into `export.rs` or `harness/` is a
    follow-up, not v2.
- `tradecraft-core/src/status.rs`: `status_of` and `units_depending_on` go.
  `version_bump_if_unmoved`, which `export.rs` uses for `bump`, stays.
- `tradecraft-core/src/authoring.rs`: all of it.
- `tradecraft-core/src/adopt.rs`: `adopt_skills`, `AdoptRequest`,
  `AdoptReport`, `AdoptedSkill`, `detect_format` and `rewrite_to_wikilinks`.
- `tradecraft-core/src/scan.rs`: `scan_roots`, `scan_root` and
  `ScanOptions`. Only `adopt` and one `publish.rs` test call them. The parts
  of `corpus.rs` that only the scan uses go with it.
- `skillwright/src/commands/`: `publish.rs`, `status.rs`, `list.rs`,
  `authoring.rs`, `adopt.rs`, `watch.rs`, `author/` and `harness/`.
- `skillwright/src/apply.rs` (the authoring writer), `git_source.rs` and
  `dirs.rs` (the `--from-git` cache), and the `author.editor` machine setting
  in `config.rs`.
- `skillwright/src/cli.rs`: every command but `init`, `build`, `check` and
  `bump`, with its flags, `--watch`, the `unpublish` stub and `WAITS_ON_CORE`.
- `skillwright/Cargo.toml` and `Cargo.lock`: dependencies only the removed
  commands use, such as `notify` (the watcher), `tiered-dirs` (the
  `--from-git` cache), `markdown-relink` and `markdown-corpus` (the authoring
  writer). Check each against what stays before removing it.
- `tradecraft-wasm/src/lib.rs`: `publishSkill`, `adoptSkills`, `statusOf`
  and `publishableKinds` go. `exportSkills`, `discoverComponents`,
  `loadMarketplace`, `parseRootFile`, `rootFileConfig`, `knownTargets` and
  `canonicalTarget` stay. `validateNamespace` goes in commit 3.
- `packages/ts/tradecraft-wasm` (repository root): the wrappers in
  `src/core.ts` and the re-exports in `src/index.ts` for those exports,
  `tests/status-of.test.ts` and `examples/publish-a-skill.test.ts`.
  `InvalidNamespaceError` and `validateNamespace` go in commit 3.
- Examples: `tradecraft-core/examples/publish.rs` and
  `skillwright/examples/list.rs`. `cargo clippy --all-targets` builds them,
  so they go with their commands.
- Tests, deleted:
  - `skillwright/tests/`: `authoring.rs`, `author.rs`, `harness.rs`,
    `apply_api.rs`, `git_source.rs` and `user_folders.rs` (publish to the
    user's skill folders).
  - The publish, status, list, adopt and authoring cases in
    `skillwright/tests/cli.rs` and `consumer_flows.rs`.
  - The publish, adopt and status cases in
    `tradecraft-core/tests/release_api.rs`, and the constructors of removed
    types in `tradecraft-core/tests/constructors.rs`.

### skillwright (CLI)

- `resolve.rs` [2]:
  - `resolve_run` keeps a walk-up, but one that stops at a folder holding both
    `skillwright.md` and `marketplaces/`. It loses `--config`,
    `--skills-root` and the workspace fold.
  - `workspace_runs`, `refuse_workspace`, `workspace_warnings`,
    `member_workspace_warnings`, `is_workspace`, `configured_out` and
    `resolve_skills_root` go or shrink to "every marketplace, or the one
    `--marketplace` names". The working directory no longer picks one.
- `cli.rs`: `ReadOnlyGlobals` loses `config` [2] and `namespace` [3]. The
  root help's `GLOBAL_OPTIONS` list shrinks with them. `--marketplace`
  stays on `build`, `check` and `bump`. Unknown commands keep going through
  `report_parse_error`, which maps clap's errors to `EXIT_USAGE`.
- `config.rs` [2]: `workspace_above` and `workspace_members` go.
- `commands/init.rs` [2]: takes `<m> <p>` and writes the v2 skeleton. Its
  `--out` and `--name` go.
- `commands/marketplace.rs` (`build` and `check`): loses the workspace runs
  [2] and `--out` [4]. `check` treats a missing `dist/<m>/` as drift [4]. The
  `confirm_write` prompt and `--yes` stay.
- `commands/bump.rs` [2]: the marketplace's-own-plugin version rule goes.
  `bump <p>` names a plugin, and `bump --marketplace <m>` with no name bumps
  the marketplace. `BuiltRef::build` checks out `skillwright.md` and
  `marketplaces/` only.
- `resolve.rs` and `commands/bump.rs` [3]: `ResolvedRun::namespace`,
  `core_namespace` and `bump`'s `namespace` field go with `Namespace`.
  `report.rs` and `skillwright/tests/report.rs` hold no namespace and need no
  change.

### tradecraft-wasm

- `lib.rs`: the pattern and workspace options of `discovery_view` and
  `load_view` go [2]. `rootFileConfig` reads no marketplace fields [2, 3].
  The `out_root` argument of `exportSkills` goes [4].

### tradecraft-typegen

- `generated/core.generated.ts` is regenerated in each commit that changes a
  wasm export or an exported type, and copied to
  `packages/ts/tradecraft-wasm/src/core.generated.ts`.
  `tests/every_export_is_covered.rs` checks both.

### Tests and fixtures still in use

- `skillwright/tests/fixtures/`: `vault`, `identity-vault`,
  `agent-skills-vault` and `freeform-vault` move to the v2 layout [2].
  `freeform-vault` holds flat notes (`skills/review.md`), so keep only what
  is still valid. `excluded-vault` tests `build.exclude` (D5) and goes [2].
  `golden/` is re-blessed in each commit that changes output.
- `tradecraft-core/tests/fixtures/agent-skills-vault` moves to the v2
  layout [2].
- `tradecraft-core/tests/fixtures/hooks-file/` [4]: `hooks.json` writes
  `${PLUGIN_ROOT}`, and `expected/cursor-hooks.json` goes.
- `skillwright/tests/bump.rs` and the `init` cases in `tests/cli.rs` move to
  the new layout [2].
- `skillwright/tests/marketplace_parity.rs`: its fixture moves [2] and its
  output root becomes `dist/<m>/` [4].
- `skillwright/tests/build_golden.rs`: unchanged, re-blessed.
- `tradecraft-core/tests/release_api.rs`: workspace cases go [2], namespace
  cases go [3].
- `tradecraft-core/tests/field_reference.rs`: re-blessed whenever the key
  table changes.
- `packages/ts/tradecraft-wasm/tests/load-marketplace.test.ts` and the
  other TypeScript tests read v1 vaults (`marketplaces/house/skillwright.md`,
  `build.out`); they move with commits 2 to 4.
- Tests for every other removed feature are deleted, not ported: the
  `patterns.rs` and `settings.rs` workspace tests, the reserved-name tests in
  `paths.rs`, and the workspace cases in `tests/cli.rs` and
  `tests/consumer_flows.rs`.

### Docs

- `docs/reference/skillwright-md.md` is rewritten [6]. Its Patterns, Several
  marketplaces and Publishing one note by path sections go.
- The CLI README's verb and option tables shrink to the four commands. Its
  Configuration section is rewritten [6].
- `docs/reference/fields.md` is regenerated in each commit that changes the
  key table.

### The repo's own `plugin/`

Files, all from the repository root:

| File | Change | Commit |
|---|---|---|
| `plugin/skillwright.md` | Marketplace fields (`name`, `owner`) move out; `build.out` goes. It keeps `author` and `license`. | 2, 4 |
| `plugin/marketplaces/osdlc/osdlc.md` | New: the marketplace note. | 2 |
| `plugin/plugins/skillwright/` | Moves to `plugin/marketplaces/osdlc/plugins/skillwright/`. Its skills are already `skills/<s>/<s>.md`. | 2 |
| `plugin/.claude-plugin/`, `plugin/.agents/`, `plugin/.cursor-plugin/`, `plugin/.pi/` | Move to `plugin/dist/osdlc/`, rebuilt. | 4 |
| `.github/workflows/plugin.yml` | `build --yes --vault ../../plugin` and `check --vault ../../plugin`, in place of `--config ../../plugin/skillwright.md`. The drift check covers `plugin/dist/osdlc/`. | 2, 4 |
| `.github/workflows/plugin-release.yml` | The same build command. It copies the pack and manifest entry from `plugin/dist/osdlc/` into `osdlc/marketplace`. | 2, 4 |
| `scripts/plugin-marketplace.ts` and `scripts/plugin-marketplace.test.ts` | The version path (broken on `origin/main`) [2]; the `HARNESSES` roots under `plugin/dist/osdlc/` [4]. | 2, 4 |
| `plugin/README.md` | The build commands [2]; the layout and output [4]; the text [6]. The `plugin` workflow's error points here. | 2, 4, 6 |
| The skill notes | Code-span calls move into prose [5]; the skills are rewritten for v2 [6]. | 5, 6 |

Install from `osdlc/marketplace` is unchanged. `plugin/` itself is no longer
installable from its own root. Nothing documents that route today.

Its skills teach v1 commands and need rewriting for v2 [6]:

| Skill | Teaches today | v2 |
|---|---|---|
| `publish` | `publish`, `status --check`, `harness install`, `update`, `uninstall`, `link`, `unlink` | Rewrite around `build`, `check` and `bump`, or drop the skill and fold `build` and `check` into `author`. |
| `setup` | `init --out`, "the nearest `skillwright.md`", `build.destinations`, `skillwright list` | `init <m> <p>`, the two-file root test, no destinations, `check` to verify. |
| `author` | `skillwright list` in its steps, `build.namespace`, v1 links and mentions, "the backslash ships too" for `\[[` | `check` in its steps, no namespace, the v2 link, call and embed rules (D3). |

## Tracking

### Pull requests

The PRs this work depended on are done:
[#136](https://github.com/osdlc/skillwright/pull/136),
[#138](https://github.com/osdlc/skillwright/pull/138),
[#139](https://github.com/osdlc/skillwright/pull/139) and
[#143](https://github.com/osdlc/skillwright/pull/143) are merged;
[#134](https://github.com/osdlc/skillwright/pull/134),
[#135](https://github.com/osdlc/skillwright/pull/135),
[#137](https://github.com/osdlc/skillwright/pull/137) and
[#140](https://github.com/osdlc/skillwright/pull/140) are closed. v2 itself
is one PR, built as the six commits of the build order.

### Open issues

Issues still open that v2 touches. These are closed and need nothing:
[#53](https://github.com/osdlc/skillwright/issues/53),
[#75](https://github.com/osdlc/skillwright/issues/75),
[#78](https://github.com/osdlc/skillwright/issues/78),
[#79](https://github.com/osdlc/skillwright/issues/79),
[#80](https://github.com/osdlc/skillwright/issues/80),
[#81](https://github.com/osdlc/skillwright/issues/81),
[#82](https://github.com/osdlc/skillwright/issues/82),
[#91](https://github.com/osdlc/skillwright/issues/91),
[#108](https://github.com/osdlc/skillwright/issues/108),
[#118](https://github.com/osdlc/skillwright/issues/118) and
[#123](https://github.com/osdlc/skillwright/issues/123).

"Deferred" means the issue belongs to a later milestone for a command v2
leaves out. It stays open.

| Issue | Verdict | Reason |
|---|---|---|
| [#152](https://github.com/osdlc/skillwright/issues/152) richer hook authoring | Later | Hook notes, and Cursor's hook variable. v2 reads only `hooks/hooks.json` and ships no Cursor hooks. |
| [#148](https://github.com/osdlc/skillwright/issues/148) plugin-layout probe | Evidence | Not in the definition of done. A failure reopens section 4 rule 19 in a follow-up. |
| [#144](https://github.com/osdlc/skillwright/issues/144) published links: labels, shared-note paths, missing files | Changed, in v2 | The flattening part is moot: shared notes outside a plugin are gone. Missing-file links become build errors (section 4). The `.md` label part stays. |
| [#141](https://github.com/osdlc/skillwright/issues/141) bump: scan once, in-memory baseline | Keep | `bump` stays. The reads it lists get simpler. |
| [#77](https://github.com/osdlc/skillwright/issues/77) `resolve_run` should call `load_marketplace` | Changed | Still one loader, written fresh in v2. Close when it lands. |
| [#76](https://github.com/osdlc/skillwright/issues/76) one root-file and workspace loader | Changed | No workspace fold. Parse-once still applies. |
| [#83](https://github.com/osdlc/skillwright/issues/83) path-guard and globset | Changed | The output-overlap check goes. Link containment by real path is new work for path-guard. With D5 the glob half goes. |
| [#50](https://github.com/osdlc/skillwright/issues/50) compile scans bodies twice | Keep | Compile stays. |
| [#47](https://github.com/osdlc/skillwright/issues/47) leftover `[[` check gaps | Keep | Compile stays. |
| [#46](https://github.com/osdlc/skillwright/issues/46) valid Markdown refused as leftover `[[` | Changed | v2's leftover check skips code spans, fenced blocks and escaped text. Indented code, math, `<pre>` and autolinks are still open. |
| [#142](https://github.com/osdlc/skillwright/issues/142) publish collision checks | Deferred: publish | All three asks are about `publish`. Inside a pack, v2's own rule refuses two sources on one path (section 9). |
| [#87](https://github.com/osdlc/skillwright/issues/87) rename of a bundle that half-lands | Deferred: rename | `rename` leaves v2. |
| [#49](https://github.com/osdlc/skillwright/issues/49) rename reads the vault three times | Deferred: rename | `rename` leaves v2. |
| [#45](https://github.com/osdlc/skillwright/issues/45) rename misses an unreadable note | Deferred: rename | `rename` leaves v2. |
| [#43](https://github.com/osdlc/skillwright/issues/43) rename can make a link ambiguous | Deferred: rename | `rename` leaves v2. When it returns, links resolve within one plugin, so the risk is smaller. |
| [#51](https://github.com/osdlc/skillwright/issues/51) adopt in reverse | Deferred: adopt | Reading harness files into notes is `adopt`'s job. Nothing in v2 needs it. |
| [#94](https://github.com/osdlc/skillwright/issues/94) install natively first | Deferred: install and adopt | Both halves need commands v2 leaves out: `harness install` for the native path, `adopt` for the fallback. |
| [#42](https://github.com/osdlc/skillwright/issues/42) markdown-db and markdown-contract | Keep | Not about skillwright. Already labelled to-close. |
| [#4](https://github.com/osdlc/skillwright/issues/4) gaps migrating a marketplace | Keep | Harness output gaps. Already labelled to-close. |
