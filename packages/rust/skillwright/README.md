# skillwright

A command-line tool for coding-agent skills. You write each skill once, as a
markdown note in a vault (a folder of notes). `skillwright` publishes the notes
into the on-disk formats of Claude Code, Codex, Cursor and Pi, builds them into
plugin marketplaces, reports drift, and authors and reorganises the vault.

The work is done by [`tradecraft-core`](../tradecraft-core). This crate reads
the configuration, calls the core and prints the result.

## Install

```sh
cargo install skillwright
```

This installs the `skillwright` binary. Prebuilt binaries for macOS and Linux
(arm64 and x86_64) are on each `skillwright/vX.Y.Z` release of
[osdlc/skillwright](https://github.com/osdlc/skillwright/releases), and these install
them:

```sh
cargo binstall skillwright
brew install osdlc/tap/skillwright
npm install --global @osdlc/skillwright   # or: npx @osdlc/skillwright --help
curl -fsSL https://github.com/osdlc/skillwright/releases/download/skillwright/v<version>/install.sh | sh
```

`install.sh` checks the binary against the release's `SHA256SUMS` and installs
it to `~/.local/bin`, or to the directory given with `--to`. The crate also has a library half, which
the binary and its tests use:

```sh
cargo add skillwright
```

## Quick start

Make a vault with one skill note:

```sh
mkdir -p my-vault/skills
cat > my-vault/skills/code-review.md <<'EOF'
---
description: Reviews a diff for correctness bugs.
---
Read the diff. Report each bug with the line and a fix.
EOF
```

List what the vault holds:

```console
$ skillwright list --vault my-vault
skills/code-review.md  skill  code-review  -  every destination · Reviews a diff for correctness bugs.
1 skill
```

Publish the note into a Claude Code project:

```console
$ skillwright publish skills/code-review.md --vault my-vault --to my-project --target claude-code --yes
publishing skills/code-review.md -> /path/to/my-project (claude-code)
.skillwright/manifest.json  written
.claude/skills/code-review/SKILL.md  written · first-write
2 written
```

`--dry-run` shows the plan and writes nothing. `skillwright --help` and
`skillwright <verb> --help` list every verb and option.

## Verbs

| Verb | Does |
|---|---|
| `publish` | Publish notes to their destinations |
| `build` | Render the vault into its marketplace roots and swap them into place |
| `check` | Report the marketplace's drift against the vault. Writes nothing; exits 1 on drift |
| `list` | List every publishable note in the vault |
| `adopt` | Bring skills already on disk, or in a repository with `--from-git`, into the vault as notes |
| `status` | Report each unit's publish state at each configured destination |
| `bump` | Raise a plugin's `sw-version` by one semver level, or with `--changed-since` every plugin that changed since a baseline |
| `new` | Author a new marketplace, plugin or skill note |
| `rename` | Rename a component, plugin or marketplace, and rewrite every link to it |
| `move` | Move a component to another plugin or marketplace |
| `delete` | Move a component's note to the vault's trash |
| `convert` | Change a note's kind and move it into that kind's folder |
| `harness install <claude\|codex\|cursor\|pi>` | Register the built marketplace with a harness and install its plugins, through the harness's own `plugin` command. On Pi, `pi install` each built pack |
| `harness update <claude\|codex\|cursor\|pi>` | Install what is missing, then update each plugin whose version changed |
| `harness uninstall <claude\|codex\|cursor\|pi>` | Remove the marketplace's plugins from a harness, and with `--marketplace` its registration too. On Pi, print the settings entry to remove |
| `harness link <claude\|codex>` | Dev mode: symlink each built skill into the harness's user skill folder. Never replaces a real folder. Cursor and Pi are refused until harness-reference records that they follow symlinked skill folders |
| `harness unlink <claude\|codex>` | Remove the links into the build, including links to skills no longer built, and nothing else |
| `init` | Declared, and exits `not implemented` |
| `unpublish` | Declared, and exits `not implemented` |

`build` and `check` are one path in two modes. The `harness` verbs change
nothing when run a second time.

stdout carries the report and nothing else. Progress, tallies, prompts and
error messages go to stderr. `--json` prints the report as one JSON document.

## Options

Every verb accepts these after the verb name, for example
`skillwright build --vault ~/notes`:

| Option | Does |
|---|---|
| `--vault <path>` | The vault root, overriding the configuration |
| `--config <path>` | Read this project file instead of the nearest one above the working directory |
| `--namespace <stem>` | The frontmatter namespace stem, the `sw` of `sw-kind` |
| `--json` | Print the report on stdout as one JSON document |
| `--quiet` | Drop progress and the tally from stderr |
| `--dry-run` | Show the plan; write nothing (verbs that write) |
| `--yes` | Accept the plan without a prompt (verbs that write) |

Each verb's own options:

| Verb | Its own options |
|---|---|
| `publish` | `[note...]`, `--to <root>` (repeatable), `--target <target>`, `--all`, `--overwrite <path>` (repeatable) |
| `build` / `check` | `--out <path>`, `--skills-root <path>`, `--marketplace <name>`, `--default-plugin <name>`, `--watch` (`build` only) |
| `list` | `--skills-root <path>`, `--model` (print the loaded marketplace model as JSON) |
| `status` | `[note]`, `--check` |
| `bump` | `[plugin]`, `--major`, `--minor`, `--patch`, `--changed-since <ref\|dir>` |
| `adopt` | `[source...]`, `--from <location>` (repeatable), `--from-git <ref>` (repeatable), `--plugin <name>`, `--into <path>`, `--set-targets <target>` (repeatable), `--only <name>` (repeatable), `--overwrite <path>` (repeatable) |
| `harness install` / `harness update` | `--plugin <name>` (repeatable), `--scope <user\|project\|local>`, `--path <path>`, `--git-url <url>`, `--git-ref <ref>` |
| `harness uninstall` | the same, plus `--marketplace` |
| `harness link` / `harness unlink` | `--plugin <name>` (repeatable; default every built plugin), `--path <path>` |

`adopt` also accepts `--as <name>` and `--promote`. Both exit with
`not implemented`.

## Exit codes

| Code | Means |
|---|---|
| 0 | The run completed and every outcome was written or unchanged |
| 1 | The run completed and something was refused, or `check` found drift |
| 2 | A run-level blocker. Nothing was attempted |
| 3 | Usage error |
| 4 | No configuration, and none supplied |
| 130 | Interrupted |

## Configuration

Two files, both optional. A flag overrides the project file, which overrides
the machine file, except for `paths:` (below). `skillwright` never writes
either one.

- **The project file** is `skillwright.yaml`. It is committed, and found by
  walking up from the working directory, or named with `--config`. It holds
  the vault path, the frontmatter namespace, the destinations and the
  `marketplace:` block.
- **The machine file** is `<config-home>/skillwright/config.yaml`, set by
  `SKILLWRIGHT_MACHINE_CONFIG` when that is set. It has the same shape. It
  supplies a whole top-level key when the project file leaves that key out
  or empty. Nested values are not merged: a project `marketplace:` block
  replaces the machine one entirely.

`paths:` is the exception. Names from both files merge, and when both define
the same name the machine file's value wins.

`<config-home>` comes from [`tiered-dirs`](https://crates.io/crates/tiered-dirs): for example
`~/Library/Application Support` on macOS and `~/.config` on Linux.
`SKILLWRIGHT_ROOT` and the other `SKILLWRIGHT_*` variables move it.

Relative paths in a config file resolve against that file's directory,
including relative entries under `paths:` in the machine file. A flag's path
resolves against the working directory.

```yaml
vault: notes
namespace: sw
destinations:
  - root: ~/code/my-project
    target: claude-code
marketplace:
  name: house
  default_plugin: tools
```

### Bump

`bump --changed-since <ref|dir>` compares the vault's render with a baseline
and bumps each plugin whose skills or files differ while its `sw-version`
has not moved. A directory is read as an already-built marketplace, such as
the committed output; one that holds no plugin manifest for any configured
harness is refused. Anything else is a git ref: it is checked out into a
temporary worktree with the repository's hooks off, built there with the
ref's own copy of the project file and the ref's own vault, and removed
afterwards. Git LFS files are not fetched, so they compare as pointers. A
ref whose vault resolves outside its checkout is refused. A run also removes
the baselines an interrupted earlier run left behind.

A drifted plugin that is not bumped is `skipped`, with one reason:
`version-moved` (its version already changed since the baseline, so a second
run bumps nothing), `not-in-baseline` (the baseline does not hold it) or
`manifest-only` (only its manifest changed). A plugin whose note has no
`sw-version` is bumped on any change.

`bump.patch` picks how the patch number is written. Only the project file
may set `bump:`; a machine file that sets it is refused.

```yaml
bump:
  patch: timestamp # or increment, the default
```

With `timestamp`, every bump sets the patch to the UTC time as
`YYYYMMDDHHMMSS` (`1.4.20261105143000`), or to one more than the current
patch when that is larger, so each release of a plugin gets a new,
increasing version even twice in one second. The patch is a 64-bit number;
a consumer that stores version parts as 32-bit integers cannot hold it.

### Hooks

`hooks:` names one hooks file per plugin:

```yaml
hooks:
  - from: plugin/hooks.json   # relative to this file
    plugin: tools             # absent means marketplace.default_plugin
```

The file is a Claude Code `hooks.json`, with event names from Claude Code's
hooks reference. `build` renders it into each pack whose harness carries
hooks: the nested shape for Claude Code and Codex, and Cursor's
`{ "version": 1, "hooks": {…} }` shape for Cursor. Pi gets none. A hook a
harness cannot carry is left out of that harness's file. An unknown event or a
malformed entry fails the build, and the error names the file and the entry.

A command names the plugin's files as `${CLAUDE_PLUGIN_ROOT}/…`. Each pack
uses its harness's own variable instead: `CLAUDE_PLUGIN_ROOT` for Claude Code,
`PLUGIN_ROOT` for Codex, `CURSOR_PLUGIN_ROOT` for Cursor. Two placeholders are
filled per pack: `{{harness}}` (the harness name) and `{{event}}` (the event
the hook is under, as the file spells it). Any other `{{…}}` fails the build.

Every pack of the plugin also gets `hooks/gap-report.json`. Per harness, it
lists how the hooks are delivered, each event's support, each hook left out
and why, and the plugin-root variable with its source. `check` reports a stale
or hand-edited report as drift.

### Launcher

`marketplace.launcher_path` names a launcher script the plugin ships as an
asset, relative to the plugin's root:

```yaml
assets:
  - from: scripts/tool
    to: bin/tool
marketplace:
  launcher_path: bin/tool
```

Skills call it as `${CLAUDE_PLUGIN_ROOT}/bin/tool`. Claude Code expands that
variable, so its pack keeps the call. For Codex, Cursor and Pi, `build`
rewrites it to the launcher's path relative to the skill's folder, such as
`../../bin/tool`. Without `launcher_path`, nothing is rewritten. There is no
`PATH` fallback: a skill that needs a tool on `PATH` says so in its own text.

## Platforms

The crate is tested on Linux and macOS. Windows is not supported yet. The
tool builds there, but CI does not run its tests on Windows, because about 35
of them fail, most through the `tradecraft-core` path guard. `adopt --from-git` and the
`harness` verbs run `git` and each harness's own command, so those must be on
`PATH`. The minimum supported Rust version is 1.89.

The prebuilt macOS binaries are not signed or notarized yet. Installs through
`brew`, `npm`, `cargo binstall` or `install.sh` are not blocked by Gatekeeper,
because those tools do not mark the download as quarantined. A binary
downloaded from the release page with a web browser is quarantined, and macOS
refuses to run it until you allow it in System Settings, Privacy & Security.

## Changes

Each release is listed in the [changelog](CHANGELOG.md).

This crate was published as `tradecraft` up to 0.3.7. skillwright is in
alpha and does not read the old names, so a project that used them must be
renamed by hand: the project file, manifest folder, frontmatter keys,
environment variables and machine file all take the `skillwright` and `sw`
spellings. Old `tc-` keys are not errors: they publish as ordinary keys, and a
note with `tc-kind: plugin` reads as a skill.

`publish` applies a note's `sw-overrides`; `build` and `check` do not.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT), at your option.
