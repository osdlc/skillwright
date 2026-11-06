//! `skillwright init`: write a `skillwright.md` in the current folder.
//!
//! The file holds the two things a build cannot go without: the marketplace
//! name and `build.out`. There is no default output location, so `--out` is
//! asked for on a terminal and required otherwise, and a blank one is refused.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::Path;

use tradecraft_core::authoring::slug_refusal;
use tradecraft_core::discover::slugify;
use tradecraft_core::{ROOT_FILE, compose_root_file};

use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_REFUSED, blocked, usage_error};
use crate::prompt::{ask, is_interactive};

/// The resolved flags of `skillwright init`.
pub struct InitFlags {
    /// `--out`: the value `build.out` gets.
    pub out: Option<String>,
    /// `--name`: the marketplace name. Absent means the folder's name.
    pub name: Option<String>,
}

/// Run `skillwright init` and return the exit code.
pub fn run_init(flags: &InitFlags, ctx: &mut CommandContext<'_>) -> i32 {
    let file = ctx.cwd.join(ROOT_FILE);
    if file.symlink_metadata().is_ok() {
        return already_exists(&file, ctx);
    }

    let name = flags.name.clone().unwrap_or_else(|| {
        ctx.cwd
            .file_name()
            .map(|name| slugify(&name.to_string_lossy()))
            .unwrap_or_default()
    });
    if let Some(refusal) = slug_refusal(&name, "marketplace name") {
        return usage_error(ctx.err, &format!("{refusal}; pass a valid --name"));
    }

    let out = match &flags.out {
        Some(out) if out.trim().is_empty() => {
            return usage_error(ctx.err, "an output folder is required");
        }
        Some(out) => out.clone(),
        None if is_interactive() => match ask("Output folder for builds:", ctx.err) {
            Some(out) => out,
            None => return usage_error(ctx.err, "an output folder is required"),
        },
        None => {
            return usage_error(
                ctx.err,
                "init needs --out when not running interactively; there is no default output folder",
            );
        }
    };

    // `create_new` never follows a link, and closes the gap after the check.
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file)
        .and_then(|mut opened| opened.write_all(compose_root_file(&name, &out).as_bytes()));
    match written {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::AlreadyExists => return already_exists(&file, ctx),
        Err(err) => {
            return blocked(
                ctx.err,
                &format!("{} was not written: {err}", file.display()),
            );
        }
    }
    let _ = writeln!(
        ctx.out,
        "wrote {ROOT_FILE}: marketplace {name}, output {out}"
    );
    EXIT_OK
}

fn already_exists(file: &Path, ctx: &mut CommandContext<'_>) -> i32 {
    let _ = writeln!(
        ctx.err,
        "refused: {} already exists, so nothing was written",
        file.display()
    );
    EXIT_REFUSED
}
