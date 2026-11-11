# tradecraft-core

The library behind the [`skillwright`](../skillwright) CLI. It reads a vault of
markdown skill notes, checks each note against the
[Agent Skills](https://agentskills.io) specification, and builds the notes
into plugin marketplaces for coding-agent tools: Claude Code, Codex, Cursor
and Pi.

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
use tradecraft_core::{
    ExportRequest, MarketplaceConfig, MemVfs, Namespace, Vfs, export_skills,
};

let vfs = MemVfs::new();
vfs.create_dir_all(Path::new("/vault/skills"))?;
vfs.write_file(
    Path::new("/vault/skills/code-review.md"),
    b"---\ndescription: Reviews the current diff.\n---\nReview the diff.\n",
)?;

// The vault, the output folder and the marketplace config are required.
// The other fields start empty; set any of them on the value.
let mut config = MarketplaceConfig::default();
config.name = Some("house".to_owned());
let mut request = ExportRequest::new("/vault", "/market", config);
request.harnesses.insert("claude".to_owned());
let report = export_skills(&vfs, &request, &Namespace::default())?;
assert!(report.changed);
assert!(vfs.exists(Path::new(
    "/market/.claude-plugin/packs/house/skills/code-review/SKILL.md"
)));
# Ok::<(), tradecraft_core::CoreError>(())
```

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
   build gets no assets and no hooks. `out_root` is an `Option`, because
   `build.out` is optional in the root file: when it is `None`, pass an
   output folder of your own, or refuse the build as the CLI does.

```rust
use std::path::Path;
use tradecraft_core::{CoreError, ExportRequest, MemVfs, Vfs, export_skills, load_marketplace};

let vfs = MemVfs::new();
vfs.create_dir_all(Path::new("/vault/skills"))?;
vfs.write_file(
    Path::new("/vault/skillwright.md"),
    b"---\nname: house\nbuild:\n  out: dist\n---\n",
)?;
vfs.write_file(
    Path::new("/vault/skills/review.md"),
    b"---\ndescription: Reviews the current diff.\n---\nReview the diff.\n",
)?;

let loaded = load_marketplace(&vfs, Path::new("/vault"), Path::new("/home"))?
    .ok_or_else(|| CoreError::Refused("no skillwright.md".to_owned()))?;
let out_root = loaded
    .out_root
    .clone()
    .ok_or_else(|| CoreError::Refused("set build.out in skillwright.md".to_owned()))?;
let inputs = loaded.inputs;
let mut request = ExportRequest::new(loaded.dir, out_root, inputs.config);
request.harnesses = inputs.harnesses;
request.assets = inputs.assets;
request.hooks = inputs.hooks;
let report = export_skills(&vfs, &request, &loaded.namespace)?;
assert!(report.changed);
# Ok::<(), CoreError>(())
```

The steps inside are public if a host needs them apart: `find_root_file`,
`workspace_above` (and `workspace_file`, the path it reads),
`RootFile::inherit`, `marketplace_inputs` and `resolve_path` for any other
path read from the file.

## What it covers

| Module | Does |
|---|---|
| `discover` | Walk a vault and vet every publishable note |
| `vault` | Read a skill as the vault stores it: a note plus a sibling folder of peer files |
| `compile` | Expand embeds, rewrite links and collect the files a skill references |
| `projection` | Render one skill into one tool's files, through `agent-adapt` |
| `export` | Build the vault into one marketplace tree per tool, or check the tree for drift |
| `paths` | Load the root file with its workspace and resolve it into a build's inputs |
| `field_edit` | Edit frontmatter fields and keep every other byte |
| `guard` | Refuse any write into a source root or the vault |
| `vfs` | The filesystem seam |

The `keys` module lists the frontmatter keys this library owns, per kind.
They are stripped on build, and an unknown key on a skill travels under
`metadata:`. Keys are bare by default. `Namespace::new("sw")` restores the
prefixed forms, flat (`sw-version`) or nested (`sw:`).

## Runtime names

Some names the library writes are fixed:

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

Most public types are `#[non_exhaustive]`, so a new field or variant is not a
breaking change:

- The request types (`ExportRequest`, `LoadRequest`, `ComponentRequest`,
  `VaultScope`, `AssetSpec`, `HookFileSpec`, `CompileSource`). Build one with
  its `new` constructor, which takes the required inputs, and set the other
  fields on the value.
- The config and settings types (`MarketplaceConfig`, `PluginOverrides`,
  `RootFile`, `BuildSettings`, `MarketplaceFields`). Build one with
  `Default::default()` and set its fields.
- The output types (`ExportReport`, `DiscoveryReport`,
  `DiscoveredComponent`, `DiscoveryWarning`, `SkippedNote`). Read them by
  field. `DiscoveryWarning::new` builds a line for a host's own report.
- The enums (`CoreError`, `ExportUnit`, `UnitStatus`, `StaleKind`,
  `ReasonToken`). A `match` on one needs a `_` arm. The status enums have
  `as_str`, the word they serialize as.
  `CoreError::variant_name` names an error's variant, and
  `CoreError::reason_token` gives the reason token a host reports for it.

`LinkIndex` (for `CompileSource::links`) and `FieldValue` (for
`FieldEdit::set`) are re-exported, so a consumer needs no `markdown-link` or
`yaml-splice` dependency of its own.

## Platforms

The crate is tested on Linux and macOS. Windows is not supported yet. The
crate builds there, but CI does not run its tests on Windows, because about
300 of them fail: the path guard refuses the in-memory `/vault` roots the
tests use, which are not absolute paths on Windows. Paths cross the `Vfs` seam in `/` form. The minimum supported Rust version is 1.89.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT), at your option.
