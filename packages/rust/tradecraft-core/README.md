# tradecraft-core

The library behind the [`skillwright`](../skillwright) CLI. It reads a vault of
markdown skill notes, checks each note against the
[Agent Skills](https://agentskills.io) specification, and publishes or builds
the notes into the on-disk formats of coding-agent tools: Claude Code, Codex,
Cursor and Pi.

The markdown files are the system of record and git is their history. There
is no database. Rendering for each tool is done by
[`agent-adapt`](https://crates.io/crates/agent-adapt).

## Install

```sh
cargo add tradecraft-core
```

## Quick start

Every function that reads or writes files takes a `&dyn Vfs` first.
`NativeVfs` works over `std::fs`. `MemVfs` is an in-memory tree.
`PreviewVfs` wraps either one and keeps writes in memory, which is how a dry
run works.

```rust
use std::path::Path;
use tradecraft_core::{MemVfs, Namespace, PublishMode, PublishRequest, Vfs, publish_skill};

let vfs = MemVfs::new();
vfs.create_dir_all(Path::new("/vault/skills"))?;
vfs.write_file(
    Path::new("/vault/skills/code-review.md"),
    b"---\ndescription: Reviews the current diff.\n---\nReview the diff.\n",
)?;

// The note, the destination and the target are required. The other fields
// start empty; set any of them on the value.
let mut request = PublishRequest::new("/vault/skills/code-review.md", "/project", "claude-code");
request.mode = PublishMode::Write;
let report = publish_skill(&vfs, &request, &Namespace::default())?;
// .skillwright/manifest.json, .claude/skills/code-review/SKILL.md
assert_eq!(report.files.len(), 2);
# Ok::<(), tradecraft_core::CoreError>(())
```

`examples/publish.rs` is the same code as a program:
`cargo run -p tradecraft-core --example publish`.

## Load a vault and build it

The CLI does the same steps.

1. `load_marketplace(&vfs, start, home)` finds the nearest `skillwright.md` at
   or above `start`, folds in the workspace above it when the file is a
   member (`marketplaces/<m>/skillwright.md`), and checks `build.namespace`.
   It returns a `LoadedMarketplace`: the vault folder `dir`, the merged
   `RootFile`, the `namespace`, `out_root` (`build.out` resolved with `~`
   expanded), and `inputs`, the config, harnesses, assets and hooks. `home`
   is the home directory; the library reads no environment. `Ok(None)` means
   no root file. A root file with `workspace: true` builds nothing itself;
   load each member by passing its folder as `start`.
2. `export_skills(&vfs, &request, &loaded.namespace)` builds, or checks, the
   marketplace. Build the request from the result with
   `ExportRequest::new(loaded.dir, out_root, inputs.config)`, then set
   `harnesses`, `assets` and `hooks` from `inputs`. Leave out `inputs` and a
   build gets no assets and no hooks.

The steps inside are public if a host needs them apart: `find_root_file`,
`workspace_above`, `RootFile::inherit`, `marketplace_inputs` and
`resolve_path` for any other path read from the file.

To rename or delete a marketplace folder, plan it with
`plan_rename_marketplace` or `plan_delete_container(…, EntityKind::Marketplace)`,
passing the entry `marketplaces/<m>/skillwright.md`. The returned
`AuthoringOp` moves or trashes the whole folder. The library only plans.
Apply the op with `skillwright::apply::{apply, Vault}` from the `skillwright`
crate, which also relinks notes outside the folder that link into it; that
crate pulls in the CLI's dependencies. A path written in the root file, and a
link that names `skillwright.md` itself, are not rewritten. The CLI's `rename`
refuses to run at a workspace root.

## What it covers

| Module | Does |
|---|---|
| `discover` | Walk a vault and vet every publishable note |
| `vault` | Read a skill as the vault stores it: a note plus a sibling folder of peer files |
| `compile` | Expand embeds, rewrite links and collect the files a skill references |
| `projection` | Render one skill into one tool's files, through `agent-adapt` |
| `publish` | Write a note onto a destination, idempotently, or report what a write would do |
| `export` | Build the vault into one marketplace tree per tool, or check the tree for drift |
| `paths` | Load the root file with its workspace and resolve it into a build's inputs |
| `status` | Report a note's publish state at a destination |
| `adopt` | Import published `SKILL.md` bundles as vault notes |
| `authoring` | Plan a new, renamed, moved, deleted or converted note |
| `guard` | Refuse any write into a source root or the vault |
| `vfs` | The filesystem seam |

The `keys` module lists the frontmatter keys this library owns, per kind.
They are stripped on publish, and an unknown key on a skill travels under
`metadata:`. Keys are bare by default. `Namespace::new("sw")` restores the
prefixed forms, flat (`sw-version`) or nested (`sw:`).

## Runtime names

Some names the library writes are fixed:

- `.skillwright/manifest.json`, the record of what a publish wrote at a
  destination;
- `.skillwright-stage`, the folder a build stages its output in before it
  swaps it into place;
- the `<!-- Generated by skillwright from … -->` marker on generated files;
- `skillwright.md`, the root file the `settings` module parses.

## Generated output

Each harness's packs folder and marketplace manifest are generated output,
replaced on every build. `HarnessProfile::owned_paths` names them, for example
`.agents/packs/` and `.agents/plugins/marketplace.json`. A build leaves
everything else in `.agents/`, `.pi/`, `.claude-plugin/` and `.cursor-plugin/`
alone.

## Launcher path

A plugin may ship a launcher script as an asset and name it in
`MarketplaceConfig::launcher_path` (`build.launcher_path` in
`skillwright.md`), for example `bin/tool`. A skill body calls it as
`${CLAUDE_PLUGIN_ROOT}/bin/tool`. The Claude Code tree keeps that call, since
Claude Code expands the variable. The Codex, Cursor and Pi trees get the
launcher's path relative to the skill's folder instead, such as
`../../bin/tool` from `skills/<name>/SKILL.md`, the way a skill names its own
files. With no `launcher_path`, nothing is rewritten. The build adds no `PATH`
fallback: a skill that needs a tool on `PATH` says so in its own text.

## WebAssembly

The library does no I/O except through `Vfs`, so it runs under
`wasm32-unknown-unknown` with a filesystem supplied by the host. The
`tradecraft-wasm` crate in this repository is that wrapper.

## Stability

The crate is 0.x. A minor version may break the API, and each break is listed
in the [CHANGELOG](CHANGELOG.md).

The request types (`PublishRequest`, `StatusRequest`, `ExportRequest`,
`LoadRequest`, `AdoptRequest`, `ComponentRequest`, `VaultScope`, `AssetSpec`,
`HookFileSpec`, `NewNote` and `CompileSource`) and the config types
(`MarketplaceConfig`, `PluginOverrides`) are `#[non_exhaustive]`, so a new
field is not a breaking change. Build a request with its `new` constructor,
which takes the required inputs, and set the other fields on the value. Build a
config with `Default::default()` and set its fields. `ReasonToken` is
`#[non_exhaustive]` too, so a `match` on it needs a `_` arm.

## Platforms

The crate is tested on Linux and macOS. Windows is not supported yet. The
crate builds there, but CI does not run its tests on Windows, because about
300 of them fail: the path guard refuses the in-memory `/vault` roots the
tests use, which are not absolute paths on Windows. Paths cross the `Vfs` seam in `/` form. The minimum supported Rust version is 1.89.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT), at your option.
