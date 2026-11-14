# skillwright

A command-line tool that builds a vault of markdown skill notes into plugin marketplaces for coding-agent tools (Claude Code, Codex, Cursor and Pi), and checks the output for drift.

## Install

```sh
brew install osdlc/tap/skillwright
npm i -g @osdlc/skillwright
cargo install skillwright
```

## Commands

| Command | Does |
|---|---|
| `init <marketplace> <plugin>` | Write a new vault in the current folder |
| `build` | Build every marketplace into `dist/<m>/` |
| `check` | Compare the vault with `dist/<m>/`; exit 1 on drift |
| `bump [plugin]` | Raise a plugin's or a marketplace's `version` |

See the [CLI README](packages/rust/skillwright/README.md) for options and exit codes.

## What is here

- `packages/rust/`: the skillwright CLI and the tradecraft crates.
- `packages/ts/tradecraft-wasm/`: the WebAssembly build of the core, for TypeScript.
- `plugin/`: the agent plugin, with skills for using the skillwright CLI. See [plugin/README.md](plugin/README.md).
- `docs/`: [decisions](docs/decisions/README.md), [standards](docs/standards/README.md) and [terms](docs/terms/README.md).

## Documentation

- [Getting started](docs/guide/getting-started.md)
- [`skillwright.md` reference](docs/reference/skillwright-md.md)
- [Field reference](docs/reference/fields.md), for each kind of note
- [Structure spec](docs/spec/structure-v2.md), versioned

Releases are built from this repository. See
[RELEASING.md](packages/rust/skillwright/RELEASING.md).

## Licence

MIT OR Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
