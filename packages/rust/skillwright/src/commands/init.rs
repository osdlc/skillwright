//! `skillwright init <m> <p>`: write a new vault in the current folder.
//!
//! ```text
//! ./
//! ├── skillwright.md
//! └── marketplaces/<m>/
//!     ├── <m>.md
//!     └── plugins/<p>/<p>.md
//! ```
//!
//! Both names follow the Agent Skills name rule and may not be a structure
//! folder's name. A folder that already holds `skillwright.md` is refused.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use tradecraft_core::layout::{MARKETPLACES_DIR, PLUGINS_DIR};
use tradecraft_core::{ROOT_FILE, compose_root_file, name_problem};

use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_REFUSED, blocked, usage_error};

/// The resolved arguments of `skillwright init`.
pub struct InitFlags {
    /// The marketplace's name.
    pub marketplace: String,
    /// The plugin's name.
    pub plugin: String,
}

/// Run `skillwright init` and return the exit code.
pub fn run_init(flags: &InitFlags, ctx: &mut CommandContext<'_>) -> i32 {
    let root_file = ctx.cwd.join(ROOT_FILE);
    if root_file.symlink_metadata().is_ok() {
        return already_exists(&root_file, ctx);
    }
    for (name, what) in [
        (&flags.marketplace, "marketplace name"),
        (&flags.plugin, "plugin name"),
    ] {
        if let Some(problem) = name_problem(name, what) {
            return usage_error(ctx.err, &problem);
        }
    }

    let (m, p) = (&flags.marketplace, &flags.plugin);
    let market_dir = ctx.cwd.join(MARKETPLACES_DIR).join(m);
    let plugin_dir = market_dir.join(PLUGINS_DIR).join(p);
    let notes: [(PathBuf, String); 3] = [
        (root_file, compose_root_file()),
        (
            market_dir.join(format!("{m}.md")),
            format!("---\n---\n\n# {m}\n"),
        ),
        (
            plugin_dir.join(format!("{p}.md")),
            format!("---\ndescription: The {p} plugin.\nversion: 0.1.0\n---\n\n# {p}\n"),
        ),
    ];
    for (path, _) in &notes {
        if path.symlink_metadata().is_ok() {
            return already_exists(path, ctx);
        }
    }
    if let Err(err) = std::fs::create_dir_all(&plugin_dir) {
        return blocked(
            ctx.err,
            &format!("{} was not created: {err}", plugin_dir.display()),
        );
    }
    for (at, (path, text)) in notes.iter().enumerate() {
        // `create_new` never follows a link, and closes the gap after the check.
        let opened = OpenOptions::new().write(true).create_new(true).open(path);
        let written = opened.and_then(|mut file| {
            file.write_all(text.as_bytes()).inspect_err(|_| {
                let _ = std::fs::remove_file(path);
            })
        });
        if let Err(err) = written {
            // Take back the notes this run wrote, so nothing was written.
            for (done, _) in &notes[..at] {
                let _ = std::fs::remove_file(done);
            }
            if err.kind() == ErrorKind::AlreadyExists {
                return already_exists(path, ctx);
            }
            return blocked(
                ctx.err,
                &format!("{} was not written, so nothing was: {err}", path.display()),
            );
        }
    }
    for (path, _) in &notes {
        let rel = path.strip_prefix(&ctx.cwd).unwrap_or(path);
        let _ = writeln!(ctx.out, "wrote {}", rel.display());
    }
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
