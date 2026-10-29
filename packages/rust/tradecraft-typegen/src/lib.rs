//! The generator behind `generated/core.generated.ts`: the TypeScript
//! declarations for the `tradecraft-wasm` requests and responses.
//!
//! A library as well as a binary so the tests can call [`generate`] and assert
//! against what it produces, rather than shelling out to the binary and
//! reading a file.
//!
//! Reads the core's source rather than deriving from it (`schemars`, `ts-rs`)
//! because a derive would put a boundary concern — how a type looks to
//! JavaScript — inside `tradecraft-core`, which deliberately carries no wire
//! dependency.

pub mod emit;
pub mod parse;

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the generated file lands, relative to this crate's directory.
pub const OUTPUT: &str = "generated/core.generated.ts";

/// The source directories of the crates the generator reads.
///
/// `tradecraft-core` and `tradecraft-wasm` are workspace members, but
/// `agent-adapt` is a crates.io dependency, so its source sits in the cargo
/// registry. [`Crates::locate`] asks `cargo metadata` for all three.
#[derive(Debug, Clone)]
pub struct Crates {
    /// The `tradecraft-core` crate directory.
    pub tradecraft_core: PathBuf,
    /// The `tradecraft-wasm` crate directory.
    pub tradecraft_wasm: PathBuf,
    /// The `agent-adapt` crate directory, as resolved by the lockfile.
    pub agent_adapt: PathBuf,
}

impl Crates {
    /// Finds the three crates in the workspace whose manifest is
    /// `workspace_manifest`, through `cargo metadata --locked`.
    ///
    /// Runs the cargo that invoked this build (`$CARGO`) when there is one.
    pub fn locate(workspace_manifest: &Path) -> Result<Self, String> {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let output = Command::new(cargo)
            .args([
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--manifest-path",
            ])
            .arg(workspace_manifest)
            .output()
            .map_err(|e| format!("running cargo metadata: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("reading cargo metadata: {e}"))?;
        let dir = |name: &str| -> Result<PathBuf, String> {
            metadata["packages"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| p["name"] == name)
                .and_then(|p| p["manifest_path"].as_str())
                .and_then(|m| Path::new(m).parent())
                .map(Path::to_path_buf)
                .ok_or_else(|| format!("cargo metadata lists no `{name}` package"))
        };
        Ok(Self {
            tradecraft_core: dir("tradecraft-core")?,
            tradecraft_wasm: dir("tradecraft-wasm")?,
            agent_adapt: dir("agent-adapt")?,
        })
    }
}

/// The generated TypeScript for `crates`.
pub fn generate(crates: &Crates) -> Result<String, String> {
    emit::emit(&read(crates)?)
}

/// The parsed wasm surface: the exports, and every type they carry.
pub fn read(crates: &Crates) -> Result<parse::Model, String> {
    let adapt = crates.agent_adapt.join("src");
    parse::read(
        &crates.tradecraft_core.join("src"),
        &crates.tradecraft_wasm.join("src/lib.rs"),
        // `Diagnostic` and the types it reaches: `DiagnosticScope` carries a
        // `RuntimeId` and a `ConceptKind`, which live beside it in agent-adapt
        // rather than in the core.
        &[
            adapt.join("capabilities.rs"),
            adapt.join("diagnostic.rs"),
            adapt.join("runtime.rs"),
        ],
    )
}
