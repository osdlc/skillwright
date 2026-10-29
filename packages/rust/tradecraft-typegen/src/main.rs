//! Writes `generated/core.generated.ts`, inside this crate, from the Rust
//! source.
//!
//! `cargo run -p tradecraft-typegen` regenerates. `--check` reports drift and
//! writes nothing, which is what a run inside an editor or a hook wants; CI
//! regenerates and diffs instead, so a red run hands the reviewer the patch.
//!
//! The output path is resolved from `CARGO_MANIFEST_DIR`, and the crates it
//! reads from `cargo metadata` on the workspace above it, so the working
//! directory does not matter.

use tradecraft_typegen::{Crates, OUTPUT, emit, generate, read};

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let check_only = std::env::args().any(|arg| arg == "--check");
    match run(check_only) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("tradecraft-typegen: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(check_only: bool) -> Result<String, String> {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates = Crates::locate(&workspace_manifest())?;
    let generated = generate(&crates)?;
    let output = crate_dir.join(OUTPUT);

    // A Windows checkout can carry the file with CRLF line ends.
    let current = std::fs::read_to_string(&output)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    if current == generated {
        return Ok(format!("{OUTPUT} is up to date"));
    }
    if check_only {
        return Err(format!(
            "{OUTPUT} does not match the Rust source. Run `cargo run -p tradecraft-typegen`."
        ));
    }
    if let Some(dir) = output.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    std::fs::write(&output, &generated)
        .map_err(|e| format!("writing {}: {e}", output.display()))?;
    Ok(format!(
        "wrote {OUTPUT}\n{}",
        emit::summary(&read(&crates)?).join("\n")
    ))
}

/// The workspace `Cargo.toml`, one level above this crate.
fn workspace_manifest() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate sits one level below the workspace directory")
        .join("Cargo.toml")
}
