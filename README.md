# skillwright

## Install

```sh
brew install osdlc/tap/skillwright
npm i -g @osdlc/skillwright
cargo install skillwright
```

## What is here

- `packages/rust/`: the CLI and core crates. They arrive in a later PR, with their history.
- `packages/ts/tradecraft-wasm/`: the WebAssembly build of the core, for TypeScript.
- `plugin/`: the agent plugin.
- `harness-reference/`: reference notes on agent harnesses.
- `site/`: the project site.
- `talks/`: talk material.

Releases are currently built from osdlc/osdlc until the move to this repository
completes. See
[RELEASING.md](https://github.com/osdlc/osdlc/blob/main/packages/rust/skillwright/RELEASING.md).
