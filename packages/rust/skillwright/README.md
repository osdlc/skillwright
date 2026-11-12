# skillwright

A command-line tool for coding-agent skills. You write each skill once, as a
markdown note in a vault (a folder of notes). `skillwright` builds the vault
into plugin marketplaces for Claude Code, Codex, Cursor and Pi, and reports
when the built output has drifted from the notes.

The work is done by [`tradecraft-core`](../tradecraft-core). This crate finds
the vault, calls the core and prints the result.

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

Make a vault with one marketplace, `house`, and one plugin, `tools`:

```console
$ mkdir my-skills && cd my-skills
$ skillwright init house tools
wrote skillwright.md
wrote marketplaces/house/house.md
wrote marketplaces/house/plugins/tools/tools.md
```

Add a skill to the plugin. Each skill is a folder with a note of the same
name:

```sh
mkdir -p marketplaces/house/plugins/tools/skills/code-review
cat > marketplaces/house/plugins/tools/skills/code-review/code-review.md <<'NOTE'
---
description: Reviews a diff for correctness bugs.
---

Read the diff. Report each bug with the line and a fix.
NOTE
```

Build it, then check that the output matches the notes:

```console
$ skillwright build --yes
claude/tools  written · 1 skill
claude  written · marketplace manifest
...
7 checked — 7 written
$ skillwright check
...
7 checked — 7 ok
```

`build` writes `dist/house/`, one tree per harness. `--dry-run` shows what
would change and writes nothing. `skillwright --help` and
`skillwright <command> --help` list every command and option.

## Commands

| Command | Does |
|---|---|
| `init <marketplace> <plugin>` | Write a new vault in the current folder: `skillwright.md`, the marketplace note and the plugin note. Refuses a folder that already holds `skillwright.md`. |
| `build` | Build every marketplace into `dist/<m>/` |
| `check` | Run every rule `build` runs, compare the result with `dist/<m>/`, and exit 1 on drift. Writes nothing. A missing `dist/<m>/` is drift. |
| `bump [plugin]` | Raise a plugin's `version` in `<p>.md`, or with `--marketplace` and no plugin the marketplace's `version` in `<m>.md`. With `--changed-since`, every plugin that changed since a baseline. |

Any other command is a usage error, exit 3.

`build` and `check` are one path in two modes. `build` and `bump` print what
they will write and ask first. `--yes` skips the question. Off a terminal,
without `--yes`, they stop with a usage error, exit 3, and write nothing.

stdout carries the report and nothing else. Progress, tallies, prompts and
error messages go to stderr. `--json` prints the report as one JSON document.

## Options

`build`, `check` and `bump` take these after the command name, for example
`skillwright build --vault ~/skills --yes`. `init` takes only its two names.

| Option | Taken by | Does |
|---|---|---|
| `--vault <path>` | all three | The vault root: the folder holding `skillwright.md` and `marketplaces/`. Without it, the first such folder at or above the working directory. |
| `--marketplace <name>` | all three | Work on one marketplace. `build` and `check` without it cover every marketplace. |
| `--harness <name>` | all three | Build only this harness: `claude`, `codex`, `cursor` or `pi`. Repeat it for several. It replaces `build.harnesses` for this run. |
| `--json` | all three | Print the report on stdout as one JSON document |
| `--quiet` | all three | Drop progress and the tally from stderr |
| `--dry-run` | `build`, `bump` | Show what would be written; write nothing |
| `--yes` | `build`, `bump` | Write without asking |
| `--major`, `--minor`, `--patch` | `bump` | Which part of the version rises. `--patch` is the default. |
| `--changed-since <ref\|dir>` | `bump` | See [Bump](#bump) |

## Exit codes

| Code | Means |
|---|---|
| 0 | The run completed and every outcome was written or unchanged |
| 1 | The run completed and something was refused, or `check` found drift |
| 2 | A run-level blocker, such as a vault not in the layout or a refused key in `skillwright.md`. Nothing was attempted |
| 3 | Usage error, including an unknown command, and `build` or `bump` off a terminal without `--yes` |
| 4 | No vault: no folder at or above the working directory holds both `skillwright.md` and `marketplaces/`, and no `--vault` was given |

## Configuration

The only configuration file is `skillwright.md`, the root file, at the vault
root beside `marketplaces/`. It is committed with the vault. No command writes
it after `init`.

```markdown
---
author: Ada Lovelace
license: MIT
repository: https://github.com/acme/skills
build:
  harnesses: [claude, codex]
---
```

Its top-level keys are shared defaults: `author`, `license`, `repository`,
`homepage`, `targets` and the harness blocks. Every marketplace, plugin and
component takes each one it does not set. `build:` holds `harnesses`, which
harnesses to build, and `bump`, how `bump` writes a patch number. An unknown
key warns. A removed key, such as `build.out` or `workspace`, is refused. See
the [`skillwright.md` reference](../../../docs/reference/skillwright-md.md),
and the [field reference](../../../docs/reference/fields.md) for every
frontmatter key.

The vault has one fixed layout:

```text
<root>/
├── skillwright.md
├── dist/                          build output
└── marketplaces/<m>/
    ├── <m>.md                     the marketplace note
    └── plugins/<p>/
        ├── <p>.md                 the plugin note, required
        ├── skills/<s>/<s>.md      also agents/<a>/<a>.md and commands/<c>/<c>.md
        ├── hooks/hooks.json
        ├── references/, scripts/, assets/
        └── CHANGELOG.md
```

Output is fixed at `dist/<m>/`. Each harness's packs folder and marketplace
manifest there (`.agents/packs/`, `.agents/plugins/marketplace.json`, and so
on) are generated, and replaced on every build, so do not edit them. A build
leaves everything else in `dist/<m>/` alone. Copying the output elsewhere,
committing it or publishing a marketplace from it is up to you.

### Bump

`bump --changed-since <ref|dir>` compares the vault's build with a baseline
and bumps each plugin whose skills or files differ while its `version` has not
moved. A directory is read as one already-built marketplace, such as a
committed `dist/<m>/`; with several marketplaces, name one with
`--marketplace`. Anything else is a git ref: its `skillwright.md` and
`marketplaces/` are checked out into a temporary folder with the repository's
hooks off, built there, and removed afterwards. Git LFS files are not fetched,
so they compare as pointers. A run also removes the baselines an interrupted
earlier run left behind.

A drifted plugin that is not bumped is `skipped`, with one reason:
`version-moved` (its version already changed since the baseline, so a second
run bumps nothing), `not-in-baseline` (the baseline does not hold it) or
`manifest-only` (only its manifest changed). A plugin whose note has no
`version` is bumped on any change, starting from `0.1.0`.

`build.bump.patch` picks how the patch number is written:

```yaml
build:
  bump:
    patch: timestamp # or increment, the default
```

With `timestamp`, every bump sets the patch to the UTC time as
`YYYYMMDDHHMMSS` (`1.4.20261105143000`), or to one more than the current
patch when that is larger, so each release of a plugin gets a new,
increasing version even twice in one second. The patch is a 64-bit number;
a consumer that stores version parts as 32-bit integers cannot hold it.

### Hooks

A plugin's hooks come from `plugins/<p>/hooks/hooks.json`, a Claude Code
`hooks.json`. `build` writes it into the Claude Code and Codex packs. The
Cursor pack gets no hooks for now, with a warning, and Pi gets none.

A hook a harness cannot carry is left out of that harness's file. An unknown
event or a malformed entry fails the build, and the error names the file and
the entry.

A command names the plugin's files as `${PLUGIN_ROOT}/…`, and each such path
must name a file the pack ships. Each pack uses its harness's own variable:
`CLAUDE_PLUGIN_ROOT` for Claude Code and `PLUGIN_ROOT` for Codex.
`${CLAUDE_PLUGIN_ROOT}` in the source is refused. Two placeholders are filled
per pack: `{{harness}}` (the harness name) and `{{event}}` (the event the hook
is under, as the file spells it). Any other `{{…}}` fails the build.

## Platforms

The crate is tested on Linux and macOS. Windows is not supported yet. The
tool builds there, but CI does not run its tests on Windows, because about 35
of them fail, most through the `tradecraft-core` path guard.
`bump --changed-since <ref>` runs `git`, so it must be on `PATH`. The minimum supported Rust version is 1.89.

The prebuilt macOS binaries are not signed or notarized yet. Installs through
`brew`, `npm`, `cargo binstall` or `install.sh` are not blocked by Gatekeeper,
because those tools do not mark the download as quarantined. A binary
downloaded from the release page with a web browser is quarantined, and macOS
refuses to run it until you allow it in System Settings, Privacy & Security.

## Changes

Each release is listed in the [changelog](CHANGELOG.md).

This crate was published as `tradecraft` up to 0.3.7.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT), at your option.
