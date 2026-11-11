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
//! | `validateNamespace` | `Namespace::new` | every verb, once, at configuration load |
//! | `discoverComponents` | `discover::discover_components` | the vault scan every other verb needs |
//! | `exportSkills` | `export::export_skills` | `build`, and `check` through `mode` |
//! | `loadMarketplace` | `model::load::load` | any host that wants the model rather than files |
//! | `knownTargets` | `publish::KNOWN_TARGETS` | every verb that takes a target name |
//! | `canonicalTarget` | `publish::canonical_target` | a host narrowing a run by target |
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
//!   A namespace crosses as a `String` stem, because the core gives
//!   `Namespace` no `Deserialize`: every stem goes through [`Namespace::new`].
//! - **A `CoreError` becomes a thrown JS `Error`** whose `.name` is the variant,
//!   mirroring what `yaml-splice-wasm` does for `EditError`. `Refused` and
//!   `Conflict` in particular are the safety rails working, and a caller that
//!   wants to tell them from a genuine I/O failure reads `.name` rather than
//!   pattern-matching on a message.
//!
//! An export has no per-unit refusal: its rails fire before anything is
//! staged, so every refusal it can produce is a throw.

mod js_vfs;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tradecraft_core::CoreError;
use tradecraft_core::Namespace;
use tradecraft_core::discover::discover_components as core_discover_components;
use tradecraft_core::export::MarketplaceConfig;
use tradecraft_core::export::{ExportRequest, export_skills as core_export_skills};
use tradecraft_core::model::load::{LoadRequest, load as core_load};
use tradecraft_core::namespace::InvalidStem;
use tradecraft_core::publish::{KNOWN_TARGETS, canonical_target as core_canonical_target};
use tradecraft_core::settings::{
    parse_root_file as core_parse_root_file, root_file_config as core_root_file_config,
};
use tradecraft_core::vfs::Vfs;
use tradecraft_core::{SourceView, VaultScope, source_view, vault_config};
use wasm_bindgen::prelude::*;

pub use js_vfs::{HostVfs, JsVfs};

/// Check that `stem` can be a frontmatter namespace.
///
/// Exists so a host can fail a bad stem where the user set it — at
/// configuration load — instead of once per note, without re-implementing
/// [`Namespace::new`]'s rules in its own language. Those rules decide which of
/// the author's keys get *deleted* at build, so a TypeScript copy that
/// drifted by one case would accept a stem the core refuses or reads
/// differently.
///
/// Returns nothing on success. There is deliberately nothing to return: the
/// derived forms (`prefix`, `block_key`) stay inside the core, so no host is
/// ever tempted to spell `stem + "-"` for itself.
///
/// # Errors
///
/// An `Error` whose `.name` is `InvalidStem` and whose message is
/// [`InvalidStem`]'s own — the offending stem and the reason, unmodified, so a
/// host prints it rather than re-words it.
#[wasm_bindgen(js_name = validateNamespace)]
pub fn validate_namespace(stem: &str) -> Result<(), JsValue> {
    Namespace::new(stem)
        .map(|_| ())
        .map_err(|e| invalid_stem_to_js(&e))
}

/// Every target name a note's targets key may name,
/// sorted — `tradecraft_core::publish::KNOWN_TARGETS`, verbatim.
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

/// The one harness a target name folds onto, or `undefined` for a name
/// outside [`KNOWN_TARGETS`] — `tradecraft_core::publish::canonical_target`,
/// verbatim.
///
/// Exported because the fold is not derivable from [`known_targets`] alone:
/// that list carries both spellings of a harness (`claude` and
/// `claude-code`) and says nothing about which pairs name the same one.
///
/// # Why this one does not cross as JSON
///
/// Exports that return a core structure serialize it, because that is the
/// only way a struct crosses. This answers with one name out of a
/// compile-time vocabulary — there is no structure, and no error either, so
/// there is nothing for a `Result<String, JsValue>` to carry. Wrapping it
/// would add a `JSON.parse` to a value that is already a string.
#[wasm_bindgen(js_name = canonicalTarget)]
pub fn canonical_target(name: &str) -> Option<String> {
    core_canonical_target(name).map(ToOwned::to_owned)
}

// --- the root file -----------------------------------------------------------

/// Read a `skillwright.md`'s text into a serialized `RootFile`, the reader the
/// CLI uses. It takes text and no vfs: finding the file is the host's.
///
/// # Errors
///
/// An `Error` carrying the core's message: frontmatter that is not a YAML
/// mapping, or a known key of the wrong shape. An unknown key is a warning on
/// the `RootFile`.
#[wasm_bindgen(js_name = parseRootFile)]
pub fn parse_root_file(text: &str) -> Result<String, JsValue> {
    let root = core_parse_root_file(text).map_err(|e| throwable(&e.to_string()))?;
    report_json("root file", &root)
}

/// The `MarketplaceConfig` a `skillwright.md`'s text describes. `folder_name`
/// is the root file's folder, the marketplace's name when the file gives none,
/// so a host does not rebuild that default.
///
/// Returns a JS object, not a JSON string. `MarketplaceConfig` is a request
/// type, and the generator spells each type for one direction, so this stays
/// outside it. `core.ts` types the result, which goes back in as a request.
///
/// # Errors
///
/// As [`parse_root_file`].
#[wasm_bindgen(js_name = rootFileConfig)]
pub fn root_file_config(text: &str, folder_name: &str) -> Result<JsValue, JsValue> {
    let config = core_root_file_config(text, folder_name).map_err(|e| throwable(&e.to_string()))?;
    js_sys::JSON::parse(&report_json("marketplace config", &config)?)
}

// --- discovery ---------------------------------------------------------------

/// What crosses as `request_json` for [`discover_components`].
///
/// The one wire shape in this crate that does not `flatten` a core struct,
/// because there is no core struct to flatten: `discover_components` takes its
/// vault root and skills root as loose parameters. So this is not a second
/// copy of anything — it is the parameter list, spelled once, in the only
/// place it has to cross a wire.
#[derive(Debug, Deserialize)]
struct WireDiscoverRequest {
    /// Absolute path to the vault.
    vault_root: PathBuf,
    /// The content root to scan. Absent means the vault root, exactly as the
    /// core reads it.
    #[serde(default)]
    skills_root: Option<PathBuf>,
    /// Absolute path to the folder a build writes to. The scan leaves out what
    /// a build owns there. Absent means nothing is left out.
    #[serde(default)]
    out_root: Option<PathBuf>,
    /// Says where the packs folders are, under what the vault's own notes say.
    /// Absent means the vault's root file.
    #[serde(default)]
    config: Option<MarketplaceConfig>,
    /// The vault's frontmatter namespace, as a stem. Absent means
    /// [`Namespace::default`].
    #[serde(default)]
    namespace: Option<String>,
}

/// The vault as `wire` asks to scan it. Refused when the request names no
/// config and the vault's root file or workspace file does not parse.
fn discovery_view<'v>(
    vfs: &'v dyn Vfs,
    wire: &WireDiscoverRequest,
    ns: &Namespace,
) -> Result<SourceView<'v>, CoreError> {
    let config = match &wire.config {
        Some(config) => config.clone(),
        None => vault_config(vfs, &wire.vault_root)?,
    };
    let mut scope = VaultScope::new(&wire.vault_root);
    scope.skills_root = wire.skills_root.as_deref();
    Ok(source_view(
        SourceView::over(vfs),
        scope,
        wire.out_root.as_deref(),
        &config,
        ns,
    ))
}

/// Every component under a vault's skills root, vetted and sorted, and every
/// note the scan could not read.
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
/// Only what is not a fact about one note: a missing skills root
/// (`RootNotFound`), a symlink in the tree, a duplicate name, a bundle
/// containing another component's entry, and `refused: malformed-root-file`
/// when the vault's root file, or the workspace file above it, cannot say
/// what it excludes (its frontmatter is not YAML, or its `workspace`,
/// `build`, `build.exclude` or `build.patterns` has the wrong shape). A bad
/// value in any other key of the root file does not throw here; a build says
/// what is wrong with it. Everything wrong with a single note
/// is reported in `skipped` and does not throw — one half-written draft must
/// not block every build out of a shared vault.
#[wasm_bindgen(js_name = discoverComponents)]
pub fn discover_components(vfs: JsValue, request_json: &str) -> Result<String, JsValue> {
    let vfs = JsVfs::adopt(vfs).map_err(|e| core_error_to_js(&e))?;
    let wire: WireDiscoverRequest = serde_json::from_str(request_json)
        .map_err(|e| throwable(&format!("request is not a valid discovery request: {e}")))?;
    let ns = namespace_of(wire.namespace.as_deref())?;

    let view = discovery_view(&vfs, &wire, &ns).map_err(|e| core_error_to_js(&e))?;
    let report =
        core_discover_components(&view, &wire.vault_root, wire.skills_root.as_deref(), &ns)
            .map_err(|e| core_error_to_js(&e))?;

    report_json("discovery report", &report)
}

// --- the marketplace ---------------------------------------------------------

/// What crosses as `request_json` for [`export_skills`]: the core's
/// [`ExportRequest`], flattened, plus the namespace stem this layer adds.
#[derive(Deserialize)]
struct WireExportRequest {
    /// Flattened, not re-declared: every field the core's own derive reads
    /// arrives here untouched, so a field added there needs no change here.
    #[serde(flatten)]
    request: ExportRequest,
    /// The vault's frontmatter namespace, as a stem. Absent means
    /// [`Namespace::default`].
    #[serde(default)]
    namespace: Option<String>,
}

/// Render the vault into one marketplace tree per harness — and, per the request's
/// `mode`, either swap the drifted roots into place or report the drift and
/// write nothing.
///
/// `vfs` is the host's filesystem object; `request_json` is a serialized
/// `ExportRequest` optionally carrying a `namespace` stem (see
/// `WireExportRequest`). Returns a serialized `ExportReport`.
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
/// * `Refused` — the rail (an output root that is, contains, or sits inside the
///   vault), `empty-marketplace` (a vault with nothing publishable), and
///   everything discovery and compilation refuse.
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
    let ns = namespace_of(wire.namespace.as_deref())?;

    let report = core_export_skills(&vfs, &wire.request, &ns).map_err(|e| core_error_to_js(&e))?;

    report_json("export report", &report)
}

/// What crosses as `request_json` for [`load_marketplace`]: the core's
/// [`LoadRequest`], flattened, plus the namespace stem this layer adds.
#[derive(Deserialize)]
struct WireLoadRequest {
    /// Flattened, not re-declared, as on [`WireExportRequest`].
    #[serde(flatten)]
    request: LoadRequest,
    /// Absolute path to the folder a build writes to. The load leaves out what
    /// a build owns there. Absent means nothing is left out.
    #[serde(default)]
    out_root: Option<PathBuf>,
    /// The vault's frontmatter namespace, as a stem. Absent means
    /// [`Namespace::default`].
    #[serde(default)]
    namespace: Option<String>,
}

/// The vault as `wire` asks to load it.
fn load_view<'v>(vfs: &'v dyn Vfs, wire: &WireLoadRequest, ns: &Namespace) -> SourceView<'v> {
    let request = &wire.request;
    let mut scope = VaultScope::new(&request.vault_root);
    scope.skills_root = request.skills_root.as_deref();
    scope.marketplace = request.marketplace.as_deref();
    source_view(
        SourceView::over(vfs),
        scope,
        wire.out_root.as_deref(),
        &request.config,
        ns,
    )
}

/// Load the vault into the marketplace model — every plugin and its members,
/// with each body's links and calls resolved — and write nothing.
///
/// `vfs` is the host's filesystem object; `request_json` is a serialized
/// `LoadRequest` optionally carrying a `namespace` stem. Returns a serialized
/// `Marketplace`: the same model `exportSkills` translates into files, so a
/// host that wants to show a vault's structure reads it here rather than
/// re-deriving it from a built tree.
///
/// # Errors
///
/// Everything the load refuses, thrown as on `exportSkills`: an unreadable
/// member note, a call or link that does not resolve, an override key a
/// requested harness does not read, an empty marketplace.
#[wasm_bindgen(js_name = loadMarketplace)]
pub fn load_marketplace(vfs: JsValue, request_json: &str) -> Result<String, JsValue> {
    let vfs = JsVfs::adopt(vfs).map_err(|e| core_error_to_js(&e))?;
    let wire: WireLoadRequest = serde_json::from_str(request_json)
        .map_err(|e| throwable(&format!("request is not a valid LoadRequest: {e}")))?;
    let ns = namespace_of(wire.namespace.as_deref())?;

    let view = load_view(&vfs, &wire, &ns);
    let market = core_load(&view, &wire.request, &ns).map_err(|e| core_error_to_js(&e))?;

    report_json("marketplace model", &market)
}

// --- the shared half of every export ---------------------------------------

/// The stem off the wire as exactly one [`Namespace`].
///
/// One function rather than a copy per export: a second place where a prefix could
/// be spelled is the only way the read-once guarantee breaks.
fn namespace_of(stem: Option<&str>) -> Result<Namespace, JsValue> {
    match stem {
        Some(stem) => Namespace::new(stem).map_err(|e| invalid_stem_to_js(&e)),
        None => Ok(Namespace::default()),
    }
}

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

/// A configured stem that cannot be a namespace, as a thrown `Error`.
///
/// `.name` is `InvalidStem` — the core's type, not a `CoreError` variant, and
/// not one either: nothing was read, no note is involved, and a host that
/// matched this against the `CoreError` names would be told a run failed
/// when in fact it never started. The message is `InvalidStem`'s `Display`
/// verbatim, which already names the stem and the reason.
fn invalid_stem_to_js(e: &InvalidStem) -> JsValue {
    let err = js_sys::Error::new(&e.to_string());
    err.set_name("InvalidStem");
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

    use tradecraft_core::{ExportMode, MemVfs};

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
        assert_eq!(wire.request.skills_root, None);
        assert_eq!(wire.namespace, None);
        assert!(wire.request.config.plugins.is_empty());
    }

    #[test]
    fn check_mode_and_a_stem_cross_beside_the_flattened_request() {
        let wire: WireExportRequest = serde_json::from_str(
            r#"{
                "vault_root": "/vault",
                "skills_root": "/vault/skills",
                "out_root": "/out",
                "mode": "check",
                "namespace": "x",
                "config": { "name": "market" }
            }"#,
        )
        .unwrap();

        assert_eq!(wire.request.mode, ExportMode::Check);
        assert_eq!(wire.namespace.as_deref(), Some("x"));
        assert_eq!(
            wire.request.skills_root,
            Some(PathBuf::from("/vault/skills"))
        );
    }

    #[test]
    fn a_discovery_request_defaults_its_optional_fields() {
        let wire: WireDiscoverRequest =
            serde_json::from_str(r#"{ "vault_root": "/vault" }"#).unwrap();
        assert_eq!(wire.skills_root, None);
        assert_eq!(wire.out_root, None);
        assert!(wire.config.is_none());
        assert_eq!(wire.namespace, None);

        // And the one required field is required.
        let err = serde_json::from_str::<WireDiscoverRequest>(r#"{}"#).unwrap_err();
        assert!(err.to_string().contains("vault_root"), "{err}");
    }

    /// `discoverComponents` leaves an unset identity field out, so a skill's
    /// `identity` is `{}`.
    #[test]
    fn a_discovered_skill_s_identity_is_an_empty_object() {
        let vfs = MemVfs::new();
        let path = Path::new("/vault/skills/a.md");
        vfs.create_dir_all(path.parent().unwrap()).unwrap();
        vfs.write_file(path, b"---\ndescription: Use it.\n---\nBody.\n")
            .unwrap();
        let ns = Namespace::bare();
        let report = core_discover_components(&vfs, Path::new("/vault"), None, &ns).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&report_json("discovery report", &report).unwrap()).unwrap();
        let skill = &json["components"][0];
        assert_eq!(skill["kind"], "skill", "{json}");
        assert_eq!(skill["identity"], serde_json::json!({}), "{json}");
    }

    /// A vault that moves its packs folder inside itself, with a note left in
    /// that folder.
    fn vault_with_moved_output() -> MemVfs {
        let vfs = MemVfs::new();
        let file = |path: &str, text: &str| {
            let path = Path::new(path);
            vfs.create_dir_all(path.parent().unwrap()).unwrap();
            vfs.write_file(path, text.as_bytes()).unwrap();
        };
        let skill = "---\ndescription: Use it.\n---\nBody.\n";
        file("/vault/skillwright.md", "---\nname: m\n---\n");
        file(
            "/vault/marketplaces/m/skillwright.md",
            "---\nname: m\nclaude: {packs: built}\n---\n",
        );
        file("/vault/marketplaces/m/plugins/rev/skills/a.md", skill);
        file("/vault/built/rev/.claude-plugin/plugin.json", "{}");
        file("/vault/built/rev/skills/stale.md", skill);
        vfs
    }

    #[test]
    fn discovery_with_an_out_root_leaves_out_a_moved_packs_folder() {
        let vfs = vault_with_moved_output();
        let ns = Namespace::bare();
        let found = |json: &str| {
            let wire: WireDiscoverRequest = serde_json::from_str(json).unwrap();
            let view = discovery_view(&vfs, &wire, &ns).unwrap();
            let root = wire.vault_root.as_path();
            let report = core_discover_components(&view, root, None, &ns).unwrap();
            serde_json::to_string(&report).unwrap()
        };

        let all = found(r#"{ "vault_root": "/vault" }"#);
        assert!(all.contains("stale"), "{all}");

        let source = found(r#"{ "vault_root": "/vault", "out_root": "/vault" }"#);
        assert!(source.contains("plugins/rev/skills/a.md"), "{source}");
        assert!(!source.contains("stale"), "{source}");
    }

    #[test]
    fn a_load_with_an_out_root_leaves_out_a_moved_packs_folder() {
        let vfs = vault_with_moved_output();
        let ns = Namespace::bare();
        let load = |json: &str| {
            let wire: WireLoadRequest = serde_json::from_str(json).unwrap();
            core_load(&load_view(&vfs, &wire, &ns), &wire.request, &ns)
        };

        let err = load(r#"{ "vault_root": "/vault", "config": {} }"#).unwrap_err();
        assert!(err.to_string().contains("stale"), "{err}");

        let market =
            load(r#"{ "vault_root": "/vault", "out_root": "/vault", "config": {} }"#).unwrap();
        let source = serde_json::to_string(&market).unwrap();
        assert!(source.contains("rev"), "{source}");
        assert!(!source.contains("stale"), "{source}");
    }
}
