//! `harness install|update|uninstall pi`.
//!
//! Pi has no marketplace. A built pack is a Pi package, and `pi install
//! <path>` loads a local package in place, without copying it; the install is
//! recorded in the `packages` list of `~/.pi/agent/settings.json`, or of the
//! project's `.pi/settings.json` with `-l` (harness-reference
//! `facts/pi/plugins/install.md` and `install-location.md`). So:
//!
//! - install runs `pi install <root>/.pi/packs/<plugin>` for each wanted pack
//!   the settings file does not list yet;
//! - update does the same: a local package is read in place, so a pack
//!   already installed is already the build;
//! - uninstall refuses with the manual step. harness-reference records no
//!   Pi command that removes a package, and this file runs none it does not
//!   record.
//!
//! `--git-url` is refused: Pi installs a git package from its repository
//! root (`facts/pi/plugins/marketplace.md`), and a built marketplace's packs
//! sit below that root.

use std::path::{Path, PathBuf};

use config_layers::needs::Need;
use serde::Serialize;
use serde_json::Value;
use tradecraft_core::harness::profile;

use super::adapter::{Scope, same_source, str_at};
use super::cli_runner::Argv;
use super::reconcile::{Execution, ReconcileError, execute};
use super::{HarnessFlags, HarnessVerb, Outcome, approve_commands, built_root, read_json};
use crate::config::{NEEDS, resolve_against_cwd};
use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, blocked, usage_error};
use crate::resolve::resolve_run;

/// The CLI target name.
pub const NAME: &str = profile::PI.name;

/// The scopes Pi records an install at.
pub const SCOPES: &[Scope] = &[Scope::User, Scope::Project];

#[derive(Serialize)]
struct PiDocument<'a> {
    harness: &'a str,
    scope: &'a str,
    settings: String,
    packages: Vec<String>,
    outcome: &'static str,
    commands: &'a [Vec<String>],
    dry_run: bool,
}

/// The settings file Pi records an install at `scope` in.
fn settings_file(scope: Scope, home: &Path, project: &Path) -> PathBuf {
    match scope {
        Scope::Project => project.join(".pi").join("settings.json"),
        _ => home.join(".pi").join("agent").join("settings.json"),
    }
}

/// The package sources a settings file lists. An entry is a source string,
/// or an object whose `source` (or `path`) is one; anything else is skipped.
fn listed_packages(file: &Path) -> Result<Vec<String>, String> {
    let Some(doc) = read_json(file)? else {
        return Ok(Vec::new());
    };
    let entries = doc.get("packages").and_then(Value::as_array);
    Ok(entries
        .into_iter()
        .flatten()
        .filter_map(|entry| match entry {
            Value::String(source) => Some(source.clone()),
            other => str_at(other, &["source", "path"]).map(str::to_string),
        })
        .collect())
}

/// Whether `listed` names the pack at `pack`: the same path as written, or
/// read against `base`, the directory `pi install` ran in.
fn names_pack(listed: &[String], pack: &Path, base: &Path) -> bool {
    let wanted = pack.display().to_string();
    listed.iter().any(|source| {
        same_source(source, &wanted)
            || same_source(
                &resolve_against_cwd(source, base).display().to_string(),
                &wanted,
            )
    })
}

fn install(pack: &Path, scope: Scope) -> Argv {
    let mut argv = vec![
        "pi".to_string(),
        "install".to_string(),
        pack.display().to_string(),
    ];
    if scope == Scope::Project {
        argv.push("-l".to_string());
    }
    argv
}

/// Run `harness <verb> pi` and return the exit code.
pub fn run_pi(verb: HarnessVerb, flags: &HarnessFlags, ctx: &mut CommandContext<'_>) -> i32 {
    let verb_name = verb.name();
    let argv_verb = ["harness", verb_name, NAME];
    if !SCOPES.contains(&flags.scope) {
        return usage_error(
            ctx.err,
            &format!(
                "pi supports --scope user|project only, not {}",
                flags.scope.as_str()
            ),
        );
    }
    if flags.git_url.is_some() {
        return usage_error(
            ctx.err,
            "pi installs a git package from its repository root, and a built marketplace's packs \
             sit below it; install from the built folder, without --git-url",
        );
    }
    if matches!(verb, HarnessVerb::Uninstall { marketplace: true }) {
        return usage_error(
            ctx.err,
            "pi has no marketplace to remove; drop --marketplace",
        );
    }

    let run = match resolve_run(&flags.common, &ctx.cwd, ctx.err) {
        Ok(run) => run,
        Err(code) => return code,
    };
    let root = match built_root(&run, flags, ctx, &argv_verb) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let Some(plugins) = super::configured_plugins(&run, flags) else {
        let need = Need {
            config: "marketplace.default_plugin",
            unless_input: Some("plugin"),
        };
        return usage_error(ctx.err, &NEEDS.message(&need, &run.config_dir, &argv_verb));
    };
    let packs_dir = root.join(
        profile::PI
            .plugin
            .as_ref()
            .map_or(".pi/packs", |format| format.packs),
    );
    let packs: Vec<PathBuf> = plugins.iter().map(|p| packs_dir.join(p)).collect();
    if !matches!(verb, HarnessVerb::Uninstall { .. })
        && let Some(missing) = packs.iter().find(|pack| !pack.is_dir())
    {
        return blocked(
            ctx.err,
            &format!(
                "{} is not built; run `skillwright build` first",
                missing.display()
            ),
        );
    }

    let settings = settings_file(flags.scope, &ctx.home, &run.config_dir);
    let listed = match listed_packages(&settings) {
        Ok(listed) => listed,
        Err(message) => return blocked(ctx.err, &message),
    };
    let (installed, missing): (Vec<&PathBuf>, Vec<&PathBuf>) = packs
        .iter()
        .partition(|pack| names_pack(&listed, pack, &run.config_dir));

    let (outcome, commands) = match verb {
        HarnessVerb::Uninstall { .. } => {
            if !installed.is_empty() {
                let list: Vec<String> = installed.iter().map(|p| p.display().to_string()).collect();
                return blocked(
                    ctx.err,
                    &format!(
                        "harness-reference records no pi command that removes a package, so \
                         skillwright runs none; remove {} from `packages` in {}",
                        list.join(", "),
                        settings.display()
                    ),
                );
            }
            (Outcome::Exists, Vec::new())
        }
        _ if missing.is_empty() => (Outcome::Exists, Vec::new()),
        _ => (
            Outcome::Added,
            missing
                .iter()
                .map(|pack| install(pack, flags.scope))
                .collect(),
        ),
    };

    let err = &mut *ctx.err;
    let mut approve = |commands: &[Vec<String>]| approve_commands(flags, &argv_verb, commands, err);
    let exec = Execution {
        runner: ctx.runner,
        cwd: &run.config_dir,
        dry_run: flags.dry_run,
        approve: &mut approve,
    };
    let report = match execute(outcome, commands, exec) {
        Ok(report) => report,
        Err(ReconcileError::Usage(message)) => return usage_error(ctx.err, &message),
        Err(ReconcileError::Blocked(message)) => return blocked(ctx.err, &message),
        Err(ReconcileError::Declined(code)) => return code,
    };

    if flags.json {
        let document = PiDocument {
            harness: NAME,
            scope: flags.scope.as_str(),
            settings: settings.display().to_string(),
            packages: packs.iter().map(|p| p.display().to_string()).collect(),
            outcome: report.outcome.as_str(),
            commands: &report.commands,
            dry_run: flags.dry_run,
        };
        match serde_json::to_string_pretty(&document) {
            Ok(text) => {
                let _ = writeln!(ctx.out, "{text}");
            }
            Err(e) => return blocked(ctx.err, &format!("the report would not serialize: {e}")),
        }
    } else {
        if report.commands.is_empty() {
            let _ = writeln!(ctx.out, "{NAME}  unchanged · exists");
        }
        let verb_word = if flags.dry_run { "would run" } else { "ran" };
        for argv in &report.commands {
            let _ = writeln!(ctx.out, "{NAME}  {verb_word} · {}", argv.join(" "));
        }
    }
    if !flags.quiet {
        let tally = match report.commands.len() {
            0 => "nothing to change".to_string(),
            1 => format!("1 command · {}", report.outcome.as_str()),
            n => format!("{n} commands · {}", report.outcome.as_str()),
        };
        let suffix = if flags.dry_run { " (dry run)" } else { "" };
        let _ = writeln!(ctx.err, "{tally}{suffix}");
    }
    EXIT_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scopes_are_the_profiles() {
        let scopes: Vec<&str> = SCOPES.iter().map(|s| s.as_str()).collect();
        assert_eq!(scopes, profile::PI.install_cli().scopes);
    }

    #[test]
    fn a_listed_package_is_a_string_or_an_object_with_a_source() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(
            &file,
            r#"{"packages": ["/m/.pi/packs/a", {"source": "./rel/b"}, 3]}"#,
        )
        .unwrap();
        let listed = listed_packages(&file).unwrap();
        assert_eq!(listed, ["/m/.pi/packs/a", "./rel/b"]);
        assert!(names_pack(
            &listed,
            Path::new("/m/.pi/packs/a"),
            Path::new("/x")
        ));
        assert!(names_pack(&listed, Path::new("/x/rel/b"), Path::new("/x")));
        assert!(!names_pack(
            &listed,
            Path::new("/m/.pi/packs/c"),
            Path::new("/x")
        ));
        assert!(
            listed_packages(&dir.path().join("none.json"))
                .unwrap()
                .is_empty()
        );
    }
}
