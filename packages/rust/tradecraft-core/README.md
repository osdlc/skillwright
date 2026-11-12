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
`ExportMode::Check` renders and compares without writing anything.

A vault holds `skillwright.md` and `marketplaces/` at its root. Each
marketplace, plugin and component is a folder with a note of the same name:

```text
<root>/
├── skillwright.md
└── marketplaces/<m>/
    ├── <m>.md
    └── plugins/<p>/
        ├── <p>.md
        └── skills/<s>/<s>.md   (also agents/, commands/)
```

```rust
use std::path::Path;
use tradecraft_core::{MemVfs, VaultRequest, Vfs, build_vault};

let vfs = MemVfs::new();
let plugin = "/vault/marketplaces/house/plugins/tools";
for (path, text) in [
    ("/vault/skillwright.md", "---\n---\n"),
    ("/vault/marketplaces/house/house.md", "---\n---\n"),
    (&format!("{plugin}/tools.md"), "---\nversion: 1.0.0\n---\n"),
    (
        &format!("{plugin}/skills/code-review/code-review.md"),
        "---\ndescription: Reviews the current diff.\n---\nReview the diff.\n",
    ),
] {
    let path = Path::new(path);
    vfs.create_dir_all(path.parent().unwrap())?;
    vfs.write_file(path, text.as_bytes())?;
}

// Only the vault is required. The other fields start empty; set any of them
// on the value. `scope` picks one marketplace; without it, every one is
// built. `options.harnesses` replaces the root file's `build.harnesses`. A
// build writes `dist/<m>/`. Everything else comes from `skillwright.md` and
// the vault's notes.
let mut request = VaultRequest::new("/vault");
request.options.harnesses.insert("claude".to_owned());
let report = build_vault(&vfs, &request)?;
assert!(report.marketplaces[0].report.changed);
assert!(vfs.exists(Path::new(
    "/vault/dist/house/.claude-plugin/packs/tools/skills/code-review/SKILL.md"
)));
# Ok::<(), tradecraft_core::CoreError>(())
```

## Load a vault and build it

The CLI does the same steps.

1. `find_vault_root(&vfs, start)` walks up from `start` to the first folder
   that holds both `skillwright.md` and `marketplaces/`. For a root the user
   names, `check_vault_root(&vfs, dir)` checks it instead.
2. `load_vault(&vfs, root, &scope)` checks the layout, parses the
   root file, and reads each marketplace's note.
   `BuildScope::Marketplace(name)` loads one marketplace. It returns a `LoadedVault`: the root
   file as `settings` and one `VaultMarketplace` per marketplace.
3. `build_vault(&vfs, &VaultRequest::new(root))` builds every marketplace
   into its `<root>/dist/<m>/`, or checks them with `options.mode` set to
   `ExportMode::Check`. It plans every marketplace before it writes any, and
   writes all of them or none: a failed swap rolls back every marketplace
   already swapped. A host that asks before it writes calls the three steps
   itself: `plan(&vfs, &loaded, &options)`, then `VaultPlan::report()`, then
   `VaultPlan::commit(&vfs)`. `owned_paths` names what a build replaces.
4. `VaultRequest::for_marketplace(root, name)` is the request for one
   marketplace. A check of one marketplace may set `options.compare_with` to
   compare with another built folder in place of `dist/<m>/`.

```rust
use std::path::Path;
use tradecraft_core::{CoreError, MemVfs, VaultRequest, Vfs, build_vault, find_vault_root};

let vfs = MemVfs::new();
let plugin = "/vault/marketplaces/house/plugins/tools";
for (path, text) in [
    ("/vault/skillwright.md", "---\nlicense: MIT\n---\n"),
    ("/vault/marketplaces/house/house.md", "---\n---\n"),
    (&format!("{plugin}/tools.md"), "---\nversion: 1.0.0\n---\n"),
    (
        &format!("{plugin}/skills/review/review.md"),
        "---\ndescription: Reviews the current diff.\n---\nReview the diff.\n",
    ),
] {
    let path = Path::new(path);
    vfs.create_dir_all(path.parent().unwrap())?;
    vfs.write_file(path, text.as_bytes())?;
}

let root = find_vault_root(&vfs, Path::new(plugin))?;
let report = build_vault(&vfs, &VaultRequest::new(root))?;
assert!(report.marketplaces.iter().all(|m| m.report.changed));
assert!(vfs.exists(Path::new("/vault/dist/house/.claude-plugin/marketplace.json")));
# Ok::<(), CoreError>(())
```

The steps inside are public if a host needs them apart: `read_layout`,
`read_root_file`, `parse_root_file` and `dist_dir`. `resolve_path` resolves a
path a user typed, as a shell would.

## What it covers

| Module | Does |
|---|---|
| `layout` | Find the vault root, check every folder, and place a path by its position |
| `discover` | Read every note of a vault by its position and vet it |
| `vault` | Read a skill as the vault stores it: `<name>/<name>.md` and the folder it owns |
| `compile` | Expand embeds, rewrite links and collect the files a skill references |
| `projection` | Render one skill into one tool's files, through `agent-adapt` |
| `export` | Build the vault into one marketplace tree per tool, or check the tree for drift |
| `paths` | Load a vault: its root file and its marketplaces |
| `field_edit` | Edit frontmatter fields and keep every other byte |
| `guard` | Containment resolved through links |
| `vfs` | The filesystem seam |

The `keys` module lists the frontmatter keys this library owns, per kind.
They are stripped on build, and an unknown key on a skill travels under
`metadata:`. There is no key prefix: `sw-version` is an unknown key.

## Runtime names

Some names the library writes are fixed:

- `.skillwright-stage`, the folder a build stages its output in before it
  swaps it into place;
- the `<!-- Generated by skillwright from … -->` marker on generated files;
- `skillwright.md`, the root file the `settings` module parses;
- `dist/<m>/`, the folder `build_vault` builds marketplace `<m>` into.

## Generated output

Each harness's packs folder and marketplace manifest are generated output,
replaced on every build. `HarnessProfile::owned_paths` names them, for example
`.agents/packs/` and `.agents/plugins/marketplace.json`. A build leaves
everything else in `.agents/`, `.pi/`, `.claude-plugin/` and `.cursor-plugin/`
alone.

## WebAssembly

The library does no I/O except through `Vfs`, so it runs under
`wasm32-unknown-unknown` with a filesystem supplied by the host. The
`tradecraft-wasm` crate in this repository is that wrapper.

## Stability

The crate is 0.x. A minor version may break the API, and each break is listed
in the [CHANGELOG](CHANGELOG.md).

Most public types are `#[non_exhaustive]`, so a new field or variant is not a
breaking change:

- The request types (`VaultRequest`, `LoadRequest`,
  `CompileSource`).
  Build one with
  its `new` constructor, which takes the required inputs, and set the other
  fields on the value.
- The settings types (`RootFile`, `SharedDefaults`, `BuildSettings`).
  Build one with
  `Default::default()` and set its fields.
- The output types (`ExportReport`, `VaultReport`, `DiscoveryReport`,
  `DiscoveredComponent`, `DiscoveryWarning`, `SkippedNote`, `LoadedVault`,
  `VaultMarketplace`, `Layout`). Read them by
  field. `DiscoveryWarning::new` builds a line for a host's own report.
- The enums (`CoreError`, `ExportUnit`, `UnitStatus`, `StaleKind`). A
  `match` on one needs a `_` arm. `UnitStatus` and `StaleKind` have `as_str`,
  the word they serialize as. `CoreError::variant_name` names an error's
  variant.

`read_version` and `set_version` read and edit a note's `version` and keep
every other byte.

## Platforms

The crate is tested on Linux and macOS. Windows is not supported yet. The
crate builds there, but CI does not run its tests on Windows, because about
300 of them fail: the path guard refuses the in-memory `/vault` roots the
tests use, which are not absolute paths on Windows. Paths cross the `Vfs` seam in `/` form. The minimum supported Rust version is 1.89.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT), at your option.
