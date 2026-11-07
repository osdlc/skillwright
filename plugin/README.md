# skillwright plugin

The `skillwright` agent plugin. Its skills teach a coding agent to use the skillwright CLI:

- `setup` installs the CLI and writes a `skillwright.md`.
- `author` writes skills, plugins and marketplaces as markdown notes in a vault.
- `publish` builds and publishes them to Claude Code, Codex, Cursor and Pi.
- `migrate` moves a vault to the `skillwright.md` structure and bare frontmatter keys.

The plugin ships in the `osdlc` marketplace as `skillwright@osdlc`.

## Layout

- `skillwright.md` is the root file. The folder holding it is the vault.
- `plugins/skillwright/` holds the plugin's notes. This is the source.
- `.claude-plugin/`, `.agents/`, `.cursor-plugin/` and `.pi/` are the built output. They are
  committed. Do not edit them; rebuild instead.

## Build

The plugin is built with the skillwright CLI in this repository. From the repository root:

```sh
cd packages/rust
cargo run -p skillwright -- build --yes --config ../../plugin/skillwright.md
cargo run -p skillwright -- check --config ../../plugin/skillwright.md
```

Or use an installed `skillwright` and run it from `plugin/`: `skillwright build --yes`.

The `plugin` workflow runs the same build and fails if `plugin/` differs from what is committed.
Commit the rebuilt files with the change to the notes.

## Install

```sh
claude plugin marketplace add osdlc/marketplace
claude plugin install skillwright@osdlc
```

For another harness, copy the matching `packs/skillwright` folder from this directory, or install
from the marketplace with that harness's own tooling.

## Release

1. Change the notes, rebuild, and raise `version` in `plugins/skillwright/skillwright.md`
   (`skillwright bump skillwright`). Merge the pull request.
2. Push the tag `plugin/vX.Y.Z` on main, where `X.Y.Z` is that `version`.
3. The `plugin-release` workflow opens a pull request on `osdlc/marketplace` that replaces only
   this plugin's pack and manifest entry. Merging it publishes the plugin.
