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
//! | `buildVault` | `export::build_vault` | `build` and `check` of every marketplace, through `mode` |
//! | `loadMarketplace` | `model::load::load`, through `wire::marketplace` | any host that wants the structure rather than files |
//! | `knownTargets` | `harness::KNOWN_TARGETS` | every verb that takes a target name |
//! | `parseRootFile` | `settings::parse_root_file` | a host reading `skillwright.md` |
//!
//! `build` and `check` share an export, because a mode is a key on the
//! request rather than an export of its own. There is deliberately no
//! `checkVault` beside `buildVault`.
//!
//! No request carries configuration. The core reads the root file's shared
//! defaults and `build.harnesses` from `skillwright.md` through the host's
//! vfs; a request's `harnesses` replaces that list.
//!
//! # Boundary choices
//!
//! - **The filesystem is an argument, not an ambient.** Every export takes the
//!   host's `Vfs` object first. `wasm32-unknown-unknown` has no filesystem, so
//!   there is nothing to reach for; see `js_vfs` for why that is a synchronous
//!   call into JS and what happens when the host throws.
//! - **The request and report cross as JSON strings**, deserialized and
//!   serialized by the same `serde_json` the core is compiled against, through
//!   the core's own derives. `buildVault` and `loadMarketplace` read the
//!   core's `VaultRequest` and `LoadRequest` directly. `WireDiscoverRequest`
//!   is the one request shape this crate owns, because `discover_components`
//!   takes loose parameters rather than a struct. Every request refuses an
//!   unknown key.
//! - **`loadMarketplace` answers in this crate's own types** (`wire`), not
//!   the core's model. The model carries what the translators work from, such
//!   as each body as segments; the wire form carries the structure a host
//!   shows, and nothing it would have to depend on beyond that.
//! - **The vault is read in its layout** (`skillwright.md`, `marketplaces/`):
//!   discovery and the load read only `marketplaces/`, so nothing a build
//!   writes to `dist/` is ever read back.
//! - **A `CoreError` becomes a thrown JS `Error`** whose `.name` is the variant,
//!   mirroring what `yaml-splice-wasm` does for `EditError`. `Refused` is
//!   a guard working, and a caller that wants to tell it from a genuine I/O
//!   failure reads `.name` rather than pattern-matching on a message. A
//!   refusal's `.token` is its reason token.
//!
//! A build refuses before anything is staged, so its refusals throw. A check
//! lists each marketplace that refused in its report.

mod js_vfs;
mod wire;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tradecraft_core::CoreError;
use tradecraft_core::discover::discover_components as core_discover_components;
use tradecraft_core::export::{VaultRequest, build_vault as core_build_vault};
use tradecraft_core::harness::KNOWN_TARGETS;
use tradecraft_core::model::load::LoadRequest;
use tradecraft_core::settings::parse_root_file as core_parse_root_file;
use wasm_bindgen::prelude::*;
use wire::marketplace as wire_marketplace;

use js_vfs::JsVfs;

/// Every name `build.harnesses` and a request's `harnesses` accept, in report order:
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
/// CLI uses. It takes text and no vfs: finding the file is the host's. A
/// build reads the file itself; this is for a host that shows it.
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

// --- discovery ---------------------------------------------------------------

/// What crosses as `request_json` for [`discover_components`].
///
/// The one wire shape this crate owns: `discover_components` takes its vault
/// root as a loose parameter, so there is no core struct to read.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// Build or check every marketplace of the vault, or the one the request's
/// `marketplace` names, each into its `<vault_root>/dist/<m>/`.
///
/// `vfs` is the host's filesystem object; `request_json` is a serialized
/// `VaultRequest`. Returns a serialized `VaultReport`: one `ExportReport` per
/// marketplace, and in a check, each marketplace that refused under `refused`.
///
/// Every marketplace is planned before any is written, and a build writes
/// all of them or none: a failed swap rolls back every marketplace already
/// swapped.
///
/// A check of one marketplace may name `compare_with`, another built folder
/// to compare with in place of `dist/<m>/`.
///
/// # Errors
///
/// A build: the first marketplace's refusal, with nothing written, or
/// `SwapFailed`. `Refused` (`compare-with`) for a build that names
/// `compare_with`, or a request for several marketplaces that does.
#[wasm_bindgen(js_name = buildVault)]
pub fn build_vault(vfs: JsValue, request_json: &str) -> Result<String, JsValue> {
    let vfs = JsVfs::adopt(vfs).map_err(|e| core_error_to_js(&e))?;
    let request: VaultRequest = serde_json::from_str(request_json)
        .map_err(|e| throwable(&format!("request is not a valid VaultRequest: {e}")))?;
    let report = core_build_vault(&vfs, &request).map_err(|e| core_error_to_js(&e))?;

    report_json("vault report", &report)
}

/// Load one marketplace of the vault — every plugin and its members, with
/// each body's links and calls resolved — and write nothing. The request's
/// `marketplace` names it, and is required when the vault holds more than
/// one.
///
/// `vfs` is the host's filesystem object; `request_json` is a serialized
/// `LoadRequest`. Returns a serialized `wire::WireMarketplace`: the
/// structure of the model `buildVault` translates into files, so a host
/// that wants to show a vault reads it here rather than re-deriving it from a
/// built tree.
///
/// # Errors
///
/// Everything the load refuses, thrown as on `buildVault`: a vault outside
/// the layout, `ambiguous-marketplace` (several and none named), an
/// unreadable member note, a call or link that does not resolve, an empty
/// marketplace.
#[wasm_bindgen(js_name = loadMarketplace)]
pub fn load_marketplace(vfs: JsValue, request_json: &str) -> Result<String, JsValue> {
    let vfs = JsVfs::adopt(vfs).map_err(|e| core_error_to_js(&e))?;
    let request: LoadRequest = serde_json::from_str(request_json)
        .map_err(|e| throwable(&format!("request is not a valid LoadRequest: {e}")))?;
    let market = wire_marketplace(&vfs, &request).map_err(|e| core_error_to_js(&e))?;

    report_json("marketplace", &market)
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

/// A refusal also carries its token as `.token`.
fn core_error_to_js(e: &CoreError) -> JsValue {
    let err = js_sys::Error::new(&e.to_string());
    err.set_name(e.variant_name());
    if let Some(token) = e.token() {
        // Setting a property on a fresh `Error` cannot fail.
        let _ = js_sys::Reflect::set(&err, &"token".into(), &token.as_str().into());
    }
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
/// What these pin is the half of the boundary that fails silently: an
/// optional key spelled wrong would, without `deny_unknown_fields`, parse fine
/// and drop the value, so a `mode: "check"` that never reached the core would
/// write a marketplace a caller asked it not to. The exports themselves
/// need a JS engine, so they are tested end to end against the artifact
/// `build.sh` produces, not here.
#[cfg(test)]
mod tests {
    use std::path::Path;

    use tradecraft_core::{BuildScope, ExportMode, MemVfs, Vfs};

    use super::*;

    #[test]
    fn a_vault_request_s_absences_are_the_core_defaults() {
        let request: VaultRequest = serde_json::from_str(r#"{ "vault_root": "/vault" }"#).unwrap();
        assert_eq!(request.vault_root, PathBuf::from("/vault"));
        assert_eq!(request.options.mode, ExportMode::Build);
        assert_eq!(request.scope, BuildScope::Vault);
        assert!(request.options.harnesses.is_empty());
        assert_eq!(request.options.compare_with, None);
    }

    #[test]
    fn check_mode_crosses_in_the_request() {
        let request: VaultRequest = serde_json::from_str(
            r#"{
                "vault_root": "/vault",
                "marketplace": "house",
                "mode": "check"
            }"#,
        )
        .unwrap();

        assert_eq!(request.options.mode, ExportMode::Check);
        assert_eq!(request.scope, BuildScope::Marketplace("house".to_owned()));
    }

    #[test]
    fn a_discovery_request_requires_its_vault_root() {
        let wire: WireDiscoverRequest =
            serde_json::from_str(r#"{ "vault_root": "/vault" }"#).unwrap();
        assert_eq!(wire.vault_root, PathBuf::from("/vault"));

        let err = serde_json::from_str::<WireDiscoverRequest>(r#"{}"#).unwrap_err();
        assert!(err.to_string().contains("vault_root"), "{err}");
    }

    /// An unknown key is refused, not dropped: a removed key such as `config`
    /// or `defaults`, or a typo of an optional key, would otherwise parse and
    /// be lost.
    #[test]
    fn every_request_refuses_an_unknown_key() {
        for key in ["config", "defaults"] {
            let json = format!(r#"{{ "vault_root": "/vault", "{key}": {{}} }}"#);
            let vault = serde_json::from_str::<VaultRequest>(&json).unwrap_err();
            assert!(vault.to_string().contains(key), "{vault}");
            let load = serde_json::from_str::<LoadRequest>(&json).unwrap_err();
            assert!(load.to_string().contains(key), "{load}");
            let discover = serde_json::from_str::<WireDiscoverRequest>(&json).unwrap_err();
            assert!(discover.to_string().contains(key), "{discover}");
        }
    }

    /// A vault: one marketplace `m`, one plugin `rev`, one skill `a`, and
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
            .find(|c| c["position"]["kind"] == "skill")
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

        let request: LoadRequest = serde_json::from_str(r#"{ "vault_root": "/vault" }"#).unwrap();
        let market = wire_marketplace(&vfs, &request).unwrap();
        let loaded = serde_json::to_string(&market).unwrap();
        assert!(loaded.contains("rev"), "{loaded}");
        assert!(!loaded.contains("stale"), "{loaded}");
    }

    /// The load answers in the wire types: no body segments, no folder, no
    /// agent-adapt content.
    #[test]
    fn the_loaded_marketplace_carries_the_wire_shape_only() {
        let vfs = vault();
        let request: LoadRequest = serde_json::from_str(r#"{ "vault_root": "/vault" }"#).unwrap();
        let market = wire_marketplace(&vfs, &request).unwrap();
        let json = serde_json::to_value(&market).unwrap();
        let keys =
            |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(
            keys(&json),
            ["name", "plugins", "uncarried", "warnings", "checked"]
        );
        let skill = &json["plugins"]["rev"]["skills"][0];
        assert_eq!(
            keys(skill),
            [
                "kind",
                "name",
                "path",
                "blocks",
                "description",
                "calls",
                "resources"
            ]
        );
        assert_eq!(skill["description"], "Use it.");
    }
}
