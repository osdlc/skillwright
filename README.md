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

Releases are built from this repository. See
[RELEASING.md](packages/rust/skillwright/RELEASING.md).

## Licence

MIT OR Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
