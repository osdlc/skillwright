# Contributing

Thanks for your interest in `@osdlc/tradecraft-wasm`.

## Issues

Report bugs and ask for features in the
[issue tracker](https://github.com/osdlc/skillwright/issues). For a bug, include the
library version, your runtime and its version, your operating system, and the
request you passed. Report security problems privately. See
[SECURITY.md](./SECURITY.md).

## Pull requests

This package lives in [osdlc/skillwright](https://github.com/osdlc/skillwright) under
`packages/ts/tradecraft-wasm`. Open pull requests there. They are reviewed and
merged there.

## Where a change goes

Every decision about a publish, an export, an adoption or a status is made by
the `tradecraft-core` Rust crate, in `packages/rust/tradecraft-core`.
`packages/rust/tradecraft-wasm` exposes it to JavaScript. This package is the
typed wrapper over that, plus the Node filesystem adapter. A change to what
the core does goes into the crate, not here.

## Running the tests

The repository is a pnpm workspace. Install from the repository root, then
work in the package directory. The tests run on Vitest under Node 22 or
later. They load the built WebAssembly from `wasm/`, which is not committed:
build it first with `pnpm run build:wasm` (see
[Building the WebAssembly](#building-the-webassembly)).

```sh
pnpm install
cd packages/ts/tradecraft-wasm
pnpm run build:wasm
pnpm test
```

`pnpm run smoke` runs `tests/smoke.mjs`, a short check of the built package
under Node. Run `pnpm run build` first. It is not published.
`pnpm run coverage` reports test coverage.

Other checks:

```sh
pnpm run typecheck
pnpm run lint
pnpm run format:check
```

## Building the WebAssembly

`wasm/` is built from the crates by `packages/rust/tradecraft-wasm/build.sh`
(`pnpm run build:wasm`). It is gitignored. Rebuild it whenever
`tradecraft-core`, `tradecraft-wasm` or a crate they depend on changes. The
script needs:

- the Rust toolchain pinned in `packages/rust/rust-toolchain.toml`, with the
  `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`);
- `wasm-bindgen` 0.2.126 (`cargo install wasm-bindgen-cli --version 0.2.126`);
- `wasm-opt` from binaryen `version_131`.

In CI, the `packages` job in `.github/workflows/ts.yml` builds `wasm/` on
every pull request before it builds and tests the package, so the tests run
against the pull request's own Rust. It uploads the build as the
`tradecraft-wasm` artifact, kept for 7 days. The compiler can emit different
bytes on macOS and on Linux; that artifact is the Linux build.

`build.sh` refuses a `wasm-bindgen` other than the pinned one. A different
`wasm-opt` is a warning on a local build and an error on CI.

## Regenerating the types

`src/core.generated.ts` is a copy of
`packages/rust/tradecraft-typegen/generated/core.generated.ts`. Do not edit
it. After a change to a wire type in Rust:

```sh
cd packages/rust
cargo run -p tradecraft-typegen
cp tradecraft-typegen/generated/core.generated.ts ../ts/tradecraft-wasm/src/core.generated.ts
```

`cargo test -p tradecraft-typegen` fails when the crate's file is behind the
Rust source. A test in this package fails when its copy differs from the
crate's. The `typegen` job in `.github/workflows/rust.yml` also compares the two.

## Code style

- TypeScript, ES modules, and explicit `.ts` import paths.
- Prettier formats the code and oxlint checks it. Run `pnpm run format` and
  `pnpm run lint` before you commit.
- Add a test for each behaviour change or bug fix.
- This repository does not publish the package yet. Note a change users can
  see in the pull request description.
