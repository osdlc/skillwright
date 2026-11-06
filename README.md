# skillwright

A command-line tool that publishes a vault of markdown skill notes into the formats of coding-agent tools (Claude Code, Codex, Cursor and Pi), checks them for drift, and authors and reorganises the vault.

## Install

```sh
brew install osdlc/tap/skillwright
npm i -g @osdlc/skillwright
cargo install skillwright
```

## What is here

- `packages/rust/`: the skillwright CLI and the tradecraft crates.
- `packages/ts/tradecraft-wasm/`: the WebAssembly build of the core, for TypeScript.
- `packages/ts/markdown-db/`: the validator for [markdown-db](docs/standards/S-GFFY-markdown-db.md) corpora.
- `plugin/`: the agent plugin, with skills for using the skillwright CLI. See [plugin/README.md](plugin/README.md).
- `harness-reference/`: reference notes on agent harnesses.
- `docs/`: [decisions](docs/decisions/README.md), [standards](docs/standards/README.md) and [terms](docs/terms/README.md).

## Documentation

- [Getting started](docs/guide/getting-started.md)
- [`skillwright.md` reference](docs/reference/skillwright-md.md)
- [Field reference](docs/reference/fields.md), for each kind of note
- [Structure spec](docs/spec/structure-v1.md), versioned
- [Migrating a vault](docs/guide/migrating.md)

Releases are built from this repository. See
[RELEASING.md](packages/rust/skillwright/RELEASING.md).

## Licence

MIT OR Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
