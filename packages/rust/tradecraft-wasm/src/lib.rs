//! A thin wasm-bindgen surface over `tradecraft-core`.
//!
//! The TypeScript side is a projection of this one Rust implementation, reached
//! over wasm — never a second copy of the compiler. Nothing here decides
//! anything: parsing a skill note, resolving links, choosing a target layout
//! all live in the core, and this crate exists only to let JavaScript reach
//! them.
//!
//! # The exports, and what each reaches
//!
//! | Export | Core entry point | The verbs over it |
//! |---|---|---|
//! | `discoverComponents` | `discover::discover_components` | the vault scan every other verb needs |
//! | `exportSkills` | `export::export_skills` | `build`, and `check` through `mode` |
//! | `loadMarketplace` | `model::load::load` | any host that wants the model rather than files |
//! | `knownTargets` | `harness::KNOWN_TARGETS` | every verb that takes a target name |
//! | `parseRootFile` | `settings::parse_root_file` | a host reading `skillwright.md` |
//! | `rootFileConfig` | `settings::root_file_config` | a host building from `skillwright.md` |
//!
//! `build` and `check` share an export, because a mode is a key on the
//! request rather than an export of its own. There is deliberately no
//! `checkSkills` beside `exportSkills`.
//!
//! # Boundary choices
//!
//! - **The filesystem is an argument, not an ambient.** Every export takes the
//!   host's `Vfs` object first. `wasm32-unknown-unknown` has no filesystem, so
//!   there is nothing to reach for; see `js_vfs` for why that is a synchronous
//!   call into JS and what happens when the host throws.
//! - **The request and report cross as JSON strings**, deserialized and
//!   serialized by the same `serde_json` the core is compiled against, through
//!   the core's own derives. The wrappers this crate owns — `WireExportRequest`
//!   and `WireLoadRequest` — `flatten` the core's own request rather than
//!   re-declaring its fields, so there is no second copy of a request shape
//!   here to drift from the core's. `WireDiscoverRequest` is the one exception
//!   and it re-declares nothing either: `discover_components` takes loose
//!   parameters rather than a struct, so there is no core shape to flatten.
//! - **The vault is read in the v2 layout** (`skillwright.md`, `marketplaces/`):
//!   discovery and the load read only `marketplaces/`, so nothing a build
//!   writes to `dist/` is ever read back.
//! - **A `CoreError` becomes a thrown JS `Error`** whose `.name` is the variant,
//!   mirroring what `yaml-splice-wasm` does for `EditError`. `Refused` is
//!   a guard working, and a caller that wants to tell it from a genuine I/O
//!   failure reads `.name` rather than pattern-matching on a message.
//!
//! An export has no per-unit refusal: its rails fire before anything is
//! staged, so every refusal it can produce is a throw.

mod js_vfs;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tradecraft_core::CoreError;
use tradecraft_core::discover::discover_components as core_discover_components;
use tradecraft_core::export::{ExportRequest, export_skills as core_export_skills};
use tradecraft_core::harness::KNOWN_TARGETS;
use tradecraft_core::model::load::{LoadRequest, load as core_load};
use tradecraft_core::settings::{
    parse_root_file as core_parse_root_file, root_file_config as core_root_file_config,
};
use wasm_bindgen::prelude::*;

pub use js_vfs::{HostVfs, JsVfs};

/// Every name `targets` and `build.harnesses` accept, in report order:
/// `tradecraft_core::harness::KNOWN_TARGETS`, verbatim.
///
/// Exported so a host validating a name off a command line checks the same
/// list the core checks a note's frontmatter against. Without it a host has to
/// keep its own copy, and a name added here would be accepted in a note and
/// rejected on the command line by the same build.
///
/// Returns a serialized array of strings, like every other export here: one
/// wire form, and no `Array` marshalling for a list that changes at compile
/// time.
#[wasm_bindgen(js_name = knownTargets)]
pub fn known_targets() -> Result<String, JsValue> {
    report_json("known targets", &KNOWN_TARGETS)
}

// --- the root file -----------------------------------------------------------

/// Read a `skillwright.md`'s text into a serialized `RootFile`, the reader the
/// CLI uses. It takes text and no vfs: finding the file is the host's.
///
/// # Errors
///
/// An `Error` carrying the core's message: frontmatter that is not a YAML
/// mapping, a key the root file refuses, or a known key of the wrong shape.
/// An unknown key is a warning on the `RootFile`.
#[wasm_bindgen(js_name = parseRootFile)]
pub fn parse_root_file(text: &str) -> Result<String, JsValue> {
    let root = core_parse_root_file(text).map_err(|e| throwable(&e.to_string()))?;
    report_json("root file", &root)
}

/// The `MarketplaceConfig` a `skillwright.md`'s text describes: its shared
/// defaults. It names no marketplace; each marketplace's note `<m>.md` does.
///
/// Returns a JS object, not a JSON string. `MarketplaceConfig` is a request
/// type, and the generator spells each type for one direction, so this stays
/// outside it. `core.ts` types the result, which goes back in as a request.
///
/// # Errors
///
/// As [`parse_root_file`].
#[wasm_bindgen(js_name = rootFileConfig)]
pub fn root_file_config(text: &str) -> Result<JsValue, JsValue> {
    let config = core_root_file_config(text).map_err(|e| throwable(&e.to_string()))?;
    js_sys::JSON::parse(&report_json("marketplace config", &config)?)
}

// --- discovery ---------------------------------------------------------------

/// What crosses as `request_json` for [`discover_components`].
///
/// The one wire shape in this crate that does not `flatten` a core struct,
/// because there is no core struct to flatten: `discover_components` takes its
/// vault root as a loose parameter. So this is not a second copy of anything —
/// it is the parameter list, spelled once, in the only place it has to cross a
/// wire.
#[derive(Debug, Deserialize)]
struct WireDiscoverRequest {
    /// Absolute path to the vault root, the folder holding `skillwright.md`
    /// and `marketplaces/`.
    vault_root: PathBuf,
}

/// Every component under the vault's `marketplaces/`, vetted and sorted, and
/// every note the scan could not read.
///
/// `vfs` is the host's filesystem object; `request_json` is a
/// `WireDiscoverRequest`. Returns a serialized
/// `tradecraft_core::DiscoveryReport` — the core's own return value, an
/// object with `components` and `skipped`. Both halves cross: a host that saw
/// only the roster could not tell a vault with three skills from a vault with
/// three skills and a note nobody can read.
///
/// Every other verb here is vault-wide because of this scan.
///
/// # Errors
///
/// Only what is not a fact about one note: `refused: layout` (the root is not
/// a vault root, or the vault breaks the layout; every problem in one
/// message), a symlink in the tree, and a duplicate name. Everything wrong
/// with a single note is reported in `skipped` and does not throw — one
/// half-written draft must not block every build out of a shared vault.
#[wasm_bindgen(js_name = discoverComponents)]
pub fn discover_components(vfs: JsValue, request_json: &str) -> Result<String, JsValue> {
    let vfs = JsVfs::adopt(vfs).map_err(|e| core_error_to_js(&e))?;
    let wire: WireDiscoverRequest = serde_json::from_str(request_json)
        .map_err(|e| throwable(&format!("request is not a valid discovery request: {e}")))?;
    let report =
        core_discover_components(&vfs, &wire.vault_root).map_err(|e| core_error_to_js(&e))?;

    report_json("discovery report", &report)
}

// --- the marketplace ---------------------------------------------------------

/// What crosses as `request_json` for [`export_skills`]: the core's
/// [`ExportRequest`], flattened.
#[derive(Deserialize)]
struct WireExportRequest {
    /// Flattened, not re-declared: every field the core's own derive reads
    /// arrives here untouched, so a field added there needs no change here.
    #[serde(flatten)]
    request: ExportRequest,
}

/// Render the vault into one marketplace tree per harness — and, per the request's
/// `mode`, either swap the drifted roots into place or report the drift and
/// write nothing.
///
/// `vfs` is the host's filesystem object; `request_json` is a serialized
/// `ExportRequest`. Returns a serialized `ExportReport`.
///
/// # `build` and `check` are one export, because they are one path
///
/// `mode` is `"build"`, or `"check"` to render into the overlay, compare, and
/// write nothing under any outcome. The key is optional and absent means
/// `"build"`.
///
/// `check`'s verdict cannot disagree with `build`'s: there is one render and
/// one comparison behind both. See `tradecraft_core::export`'s module doc.
///
/// The exit code is the caller's, not this layer's: `ExportReport.changed` is
/// the fact, and a `check` that exits 1 on drift is the CLI reading it.
///
/// # Errors
///
/// * `Refused` — an output root inside `marketplaces/`, `empty-marketplace`
///   (a marketplace with no plugin), and everything discovery and
///   compilation refuse.
/// * `SwapFailed` — the commit failed part-way; the message says what the
///   rollback restored. Its own `.name`, because a host has to be able to tell
///   "nothing happened" from "something happened and was undone".
///
/// There is no per-unit refusal to look for inside the report: an export's
/// rails all fire before anything is staged.
#[wasm_bindgen(js_name = exportSkills)]
pub fn export_skills(vfs: JsValue, request_json: &str) -> Result<String, JsValue> {
    let vfs = JsVfs::adopt(vfs).map_err(|e| core_error_to_js(&e))?;
    let wire: WireExportRequest = serde_json::from_str(request_json)
        .map_err(|e| throwable(&format!("request is not a valid ExportRequest: {e}")))?;
    let report = core_export_skills(&vfs, &wire.request).map_err(|e| core_error_to_js(&e))?;

    report_json("export report", &report)
}

/// What crosses as `request_json` for [`load_marketplace`]: the core's
/// [`LoadRequest`], flattened.
#[derive(Deserialize)]
struct WireLoadRequest {
    /// Flattened, not re-declared, as on [`WireExportRequest`].
    #[serde(flatten)]
    request: LoadRequest,
}

/// Load the vault into the marketplace model — every plugin and its members,
/// with each body's links and calls resolved — and write nothing.
///
/// `vfs` is the host's filesystem object; `request_json` is a serialized
/// `LoadRequest`. Returns a serialized
/// `Marketplace`: the same model `exportSkills` translates into files, so a
/// host that wants to show a vault's structure reads it here rather than
/// re-deriving it from a built tree.
///
/// # Errors
///
/// Everything the load refuses, thrown as on `exportSkills`: a vault outside
/// the layout, an unreadable member note, a call or link that does not resolve, an override key a
/// requested harness does not read, an empty marketplace.
#[wasm_bindgen(js_name = loadMarketplace)]
pub fn load_marketplace(vfs: JsValue, request_json: &str) -> Result<String, JsValue> {
    let vfs = JsVfs::adopt(vfs).map_err(|e| core_error_to_js(&e))?;
    let wire: WireLoadRequest = serde_json::from_str(request_json)
        .map_err(|e| throwable(&format!("request is not a valid LoadRequest: {e}")))?;
    let market = core_load(&vfs, &wire.request).map_err(|e| core_error_to_js(&e))?;

    report_json("marketplace model", &market)
}

// --- the shared half of every export ---------------------------------------

/// A core report as the JSON string that crosses back.
///
/// `what` names the report in the failure message, because an unserializable
/// report is a bug in this crate and the message is what says which one.
fn report_json<T: Serialize>(what: &str, report: &T) -> Result<String, JsValue> {
    serde_json::to_string(report).map_err(|e| throwable(&format!("serializing the {what}: {e}")))
}

// --- CoreError -> thrown JS Error ------------------------------------------

fn core_error_to_js(e: &CoreError) -> JsValue {
    let err = js_sys::Error::new(&e.to_string());
    err.set_name(e.variant_name());
    err.into()
}

/// A boundary failure that never reached the core — bad JSON in, unserializable
/// report out. Deliberately left with JavaScript's own `Error` name, because it
/// is not any `CoreError` variant and claiming one would be a lie.
fn throwable(message: &str) -> JsValue {
    js_sys::Error::new(message).into()
}

/// The wire, and only the wire.
///
/// What these pin is the half of the boundary that fails silently: a
/// `#[serde(flatten)]` envelope whose optional key is spelled wrong parses
/// fine and drops the value, so a `mode: "check"` that never reached the core
/// would write a marketplace a caller asked it not to. The exports themselves
/// need a JS engine, so they are tested end to end against the artifact
/// `build.sh` produces, not here.
#[cfg(test)]
mod tests {
    use std::path::Path;

    use tradecraft_core::{ExportMode, MemVfs, Vfs};

    use super::*;

    #[test]
    fn an_export_request_flattens_and_its_absences_are_the_core_defaults() {
        let wire: WireExportRequest = serde_json::from_str(
            r#"{
                "vault_root": "/vault",
                "out_root": "/out",
                "config": { "name": "market" }
            }"#,
        )
        .unwrap();

        assert_eq!(wire.request.vault_root, PathBuf::from("/vault"));
        assert_eq!(wire.request.out_root, PathBuf::from("/out"));
        assert_eq!(wire.request.config.name.as_deref(), Some("market"));
        // Every absence, at its documented default.
        assert_eq!(wire.request.mode, ExportMode::Build);
        assert_eq!(wire.request.marketplace, None);
        assert!(wire.request.config.plugins.is_empty());
    }

    #[test]
    fn check_mode_crosses_in_the_flattened_request() {
        let wire: WireExportRequest = serde_json::from_str(
            r#"{
                "vault_root": "/vault",
                "marketplace": "house",
                "out_root": "/vault/dist/house",
                "mode": "check",
                "config": { "name": "market" }
            }"#,
        )
        .unwrap();

        assert_eq!(wire.request.mode, ExportMode::Check);
        assert_eq!(wire.request.marketplace.as_deref(), Some("house"));
    }

    #[test]
    fn a_discovery_request_requires_its_vault_root() {
        let wire: WireDiscoverRequest =
            serde_json::from_str(r#"{ "vault_root": "/vault" }"#).unwrap();
        assert_eq!(wire.vault_root, PathBuf::from("/vault"));

        let err = serde_json::from_str::<WireDiscoverRequest>(r#"{}"#).unwrap_err();
        assert!(err.to_string().contains("vault_root"), "{err}");
    }

    /// A v2 vault: one marketplace `m`, one plugin `rev`, one skill `a`, and
    /// a built tree in `dist/m/` holding a stale note.
    fn vault() -> MemVfs {
        let vfs = MemVfs::new();
        let file = |path: &str, text: &str| {
            let path = Path::new(path);
            vfs.create_dir_all(path.parent().unwrap()).unwrap();
            vfs.write_file(path, text.as_bytes()).unwrap();
        };
        let skill = "---\ndescription: Use it.\n---\nBody.\n";
        file("/vault/skillwright.md", "---\n---\n");
        file("/vault/marketplaces/m/m.md", "---\n---\n");
        file("/vault/marketplaces/m/plugins/rev/rev.md", "---\n---\n");
        file("/vault/marketplaces/m/plugins/rev/skills/a/a.md", skill);
        file(
            "/vault/dist/m/.claude-plugin/packs/rev/skills/stale.md",
            skill,
        );
        vfs
    }

    /// `discoverComponents` leaves an unset identity field out, so a skill's
    /// `identity` is `{}`.
    #[test]
    fn a_discovered_skill_s_identity_is_an_empty_object() {
        let vfs = vault();
        let report = core_discover_components(&vfs, Path::new("/vault")).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&report_json("discovery report", &report).unwrap()).unwrap();
        let skill = json["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == "skill")
            .unwrap();
        assert_eq!(skill["identity"], serde_json::json!({}), "{json}");
    }

    #[test]
    fn discovery_and_the_load_never_read_the_built_tree() {
        let vfs = vault();
        let report = core_discover_components(&vfs, Path::new("/vault")).unwrap();
        let found = serde_json::to_string(&report).unwrap();
        assert!(found.contains("plugins/rev/skills/a/a.md"), "{found}");
        assert!(!found.contains("stale"), "{found}");

        let wire: WireLoadRequest =
            serde_json::from_str(r#"{ "vault_root": "/vault", "config": {} }"#).unwrap();
        let market = core_load(&vfs, &wire.request).unwrap();
        let loaded = serde_json::to_string(&market).unwrap();
        assert!(loaded.contains("rev"), "{loaded}");
        assert!(!loaded.contains("stale"), "{loaded}");
    }
}
