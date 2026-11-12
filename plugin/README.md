# skillwright plugin

The `skillwright` agent plugin. Its skills teach a coding agent to use the skillwright CLI:

- `setup` installs the CLI and writes a new vault with `skillwright init`.
- `author` writes skills, plugins and marketplaces as markdown notes, then builds and checks them
  for Claude Code, Codex, Cursor and Pi.

The plugin ships in the `osdlc` marketplace as `skillwright@osdlc`.

## Layout

This folder is a skillwright vault:

```text
plugin/
├── skillwright.md                        the root file
├── marketplaces/osdlc/
│   ├── osdlc.md                          the marketplace note
│   └── plugins/skillwright/
│       ├── skillwright.md                the plugin note
│       └── skills/<s>/<s>.md             the skills: the source
└── dist/osdlc/                           the built output
```

`dist/osdlc/` holds one tree per harness: `.claude-plugin/`, `.agents/`, `.cursor-plugin/` and
`.pi/`. It is committed. Do not edit it; rebuild instead. `plugin/` is not installable from its own
root; install from `osdlc/marketplace`.

## Build

The plugin is built with the skillwright CLI in this repository, as the `plugin` workflow does.
From the repository root:

```sh
cd packages/rust
cargo run --locked -p skillwright -- build --yes --vault ../../plugin
cargo run --locked -p skillwright -- check --vault ../../plugin
```

Or use an installed `skillwright` from `plugin/`: `skillwright build --yes`, then
`skillwright check`.

The `plugin` workflow runs the same build and fails if `plugin/` differs from what is committed.
Commit the rebuilt files with the change to the notes. The build must also print no warning:
`skillwright/tests/repo_plugin.rs` checks that, and `scripts/plugin-notes.test.ts` checks that the
notes teach no removed command or flag.

## Install

```sh
claude plugin marketplace add osdlc/marketplace
claude plugin install skillwright@osdlc
```

For another harness, copy the matching `packs/skillwright` folder from `dist/osdlc/`, or install
from the marketplace with that harness's own tooling.

## Release

1. Change the notes, rebuild, and raise `version` in
   `marketplaces/osdlc/plugins/skillwright/skillwright.md` (`skillwright bump skillwright`). Merge
   the pull request.
2. Push the tag `plugin/vX.Y.Z` on main, where `X.Y.Z` is that `version`.
3. The `plugin-release` workflow opens a pull request on `osdlc/marketplace` that replaces only
   this plugin's pack and manifest entry. Merging it publishes the plugin.
