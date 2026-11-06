//! `harness link|unlink <harness>`: the dev loop.
//!
//! `link` symlinks each built skill folder of the harness's packs into that
//! harness's user skill folder, so a `build` (or `build --watch`) reaches the
//! harness with no install step. `unlink` removes only the links `link` would
//! make: a link pointing somewhere else, and a real file or folder, are never
//! touched. `link` refuses to put a symlink where a real folder or another
//! link already is.
//!
//! The user skill folders, from harness-reference `facts/<harness>/skills/`:
//! Codex reads `~/.agents/skills`, Cursor `~/.cursor/skills`, and Pi
//! `~/.pi/agent/skills` (`user-dirs.md`). Claude Code reads `.claude/skills`
//! (`project-dirs.md`) under `~/.claude`, the user destination `publish`
//! already writes to.
//!
//! A linked skill loads on its own, not inside its plugin, so the harness
//! calls it by its folder name. A body that calls a sibling as
//! `/plugin:skill` still names the plugin form.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tradecraft_core::harness::profile;

use super::adapter::Scope;
use super::{Harness, HarnessFlags, HarnessVerb, built_root};
use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_REFUSED, blocked, usage_error};
use crate::prompt::{confirm_write, plan_flow};
use crate::resolve::resolve_run;

/// The user skill folder `harness` reads, under `home`.
fn user_skills(harness: &str, home: &Path) -> PathBuf {
    match harness {
        "claude" => home.join(".claude").join("skills"),
        "codex" => home.join(".agents").join("skills"),
        "cursor" => home.join(".cursor").join("skills"),
        _ => home.join(".pi").join("agent").join("skills"),
    }
}

/// Where `harness`'s packs land under the built root.
fn packs(harness: &str) -> Option<&'static str> {
    profile::PROFILES
        .iter()
        .find(|p| p.name == harness)
        .and_then(|p| p.plugin.as_ref())
        .map(|format| format.packs)
}

/// One planned link and what was found where it goes.
#[derive(Serialize)]
struct LinkRow {
    path: String,
    target: String,
    /// `linked`, `would-link`, `unlinked`, `would-unlink`, `unchanged`,
    /// `absent` or `refused`.
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Serialize)]
struct LinkDocument<'a> {
    harness: &'a str,
    verb: &'a str,
    folder: String,
    links: &'a [LinkRow],
    dry_run: bool,
}

/// What is at `link` now, as `link` and `unlink` read it.
enum Found {
    Nothing,
    /// A symlink to `target`.
    Ours,
    /// A symlink to somewhere else.
    OtherLink(PathBuf),
    /// A real file or folder.
    Real,
}

fn found_at(link: &Path, target: &Path) -> Result<Found, String> {
    let meta = match std::fs::symlink_metadata(link) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Found::Nothing),
        Err(e) => return Err(format!("{} could not be read: {e}", link.display())),
    };
    if !meta.file_type().is_symlink() {
        return Ok(Found::Real);
    }
    let points_to = std::fs::read_link(link)
        .map_err(|e| format!("{} could not be read: {e}", link.display()))?;
    Ok(if points_to == target {
        Found::Ours
    } else {
        Found::OtherLink(points_to)
    })
}

#[cfg(unix)]
fn make_link(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn make_link(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

fn remove_link(link: &Path) -> std::io::Result<()> {
    // A directory symlink on Windows is removed as a directory.
    std::fs::remove_file(link).or_else(|_| std::fs::remove_dir(link))
}

/// The built skill folders of `plugins` (every pack when empty), by the
/// folder name each one links as.
fn built_skills(
    packs_dir: &Path,
    plugins: &[String],
) -> Result<BTreeMap<String, Vec<PathBuf>>, String> {
    let names: Vec<String> = if plugins.is_empty() {
        let entries = std::fs::read_dir(packs_dir).map_err(|e| {
            format!(
                "{} could not be read ({e}); run `skillwright build` first",
                packs_dir.display()
            )
        })?;
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .collect();
        names.sort();
        names
    } else {
        plugins.to_vec()
    };
    let mut skills: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for plugin in names {
        let dir = packs_dir.join(&plugin).join("skills");
        let entries = std::fs::read_dir(&dir).map_err(|e| {
            format!(
                "{} could not be read ({e}); run `skillwright build` first",
                dir.display()
            )
        })?;
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if !path.join("SKILL.md").is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            skills.entry(name).or_default().push(path);
        }
    }
    Ok(skills)
}

/// Run `harness link|unlink <harness>` and return the exit code.
pub fn run_link(
    verb: HarnessVerb,
    harness: Harness,
    flags: &HarnessFlags,
    ctx: &mut CommandContext<'_>,
) -> i32 {
    let name = harness.name();
    let verb_name = verb.name();
    let argv_verb = ["harness", verb_name, name];
    if flags.scope != Scope::User {
        return usage_error(
            ctx.err,
            &format!("{verb_name} writes to the user skill folder only; drop --scope"),
        );
    }
    if flags.git_url.is_some() {
        return usage_error(
            ctx.err,
            &format!("{verb_name} links the built folder; drop --git-url"),
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
    let root = root.canonicalize().unwrap_or(root);
    let Some(packs) = packs(name) else {
        return blocked(ctx.err, &format!("{name} has no pack layout"));
    };
    let skills = match built_skills(&root.join(packs), &flags.plugins) {
        Ok(skills) => skills,
        Err(message) => return blocked(ctx.err, &message),
    };
    let folder = user_skills(name, &ctx.home);

    let linking = verb == HarnessVerb::Link;
    let mut rows: Vec<LinkRow> = Vec::new();
    let mut actions: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (skill, targets) in &skills {
        let link = folder.join(skill);
        let target = &targets[0];
        let mut row = LinkRow {
            path: link.display().to_string(),
            target: target.display().to_string(),
            status: "unchanged",
            reason: None,
        };
        if targets.len() > 1 {
            row.status = "refused";
            row.reason = Some(format!(
                "{} built skills share this folder name",
                targets.len()
            ));
            rows.push(row);
            continue;
        }
        let found = match found_at(&link, target) {
            Ok(found) => found,
            Err(message) => return blocked(ctx.err, &message),
        };
        match (linking, found) {
            (true, Found::Nothing) | (false, Found::Ours) => {
                row.status = match (linking, flags.dry_run) {
                    (true, true) => "would-link",
                    (true, false) => "linked",
                    (false, true) => "would-unlink",
                    (false, false) => "unlinked",
                };
                actions.push((link, target.clone()));
            }
            (true, Found::Ours) => {}
            (false, Found::Nothing) => row.status = "absent",
            (true, Found::Real) => {
                row.status = "refused";
                row.reason =
                    Some("a real file or folder is there; skillwright never replaces one".into());
            }
            (true, Found::OtherLink(other)) => {
                row.status = "refused";
                row.reason = Some(format!("a link to {} is there", other.display()));
            }
            (false, Found::Real | Found::OtherLink(_)) => {
                row.status = "unchanged";
                row.reason = Some("not a link this build made; left alone".into());
            }
        }
        rows.push(row);
    }

    if !flags.dry_run && !actions.is_empty() {
        if !flags.yes {
            let mut plan = vec![argv_verb[1..].join(" "), String::new()];
            plan.extend(plan_flow(&[
                (
                    if linking { "link" } else { "unlink" },
                    actions
                        .iter()
                        .map(|(link, target)| format!("{} -> {}", link.display(), target.display()))
                        .collect(),
                ),
                ("in", vec![folder.display().to_string()]),
            ]));
            let gate = confirm_write(&argv_verb.join(" "), &plan, ctx.err);
            if gate != EXIT_OK {
                return gate;
            }
        }
        if linking && let Err(e) = std::fs::create_dir_all(&folder) {
            return blocked(
                ctx.err,
                &format!("{} could not be created: {e}", folder.display()),
            );
        }
        for (link, target) in &actions {
            let done = if linking {
                make_link(target, link)
            } else {
                remove_link(link)
            };
            if let Err(e) = done {
                return blocked(
                    ctx.err,
                    &format!("{verb_name} {} failed: {e}", link.display()),
                );
            }
        }
    }

    if flags.json {
        let document = LinkDocument {
            harness: name,
            verb: verb_name,
            folder: folder.display().to_string(),
            links: &rows,
            dry_run: flags.dry_run,
        };
        match serde_json::to_string_pretty(&document) {
            Ok(text) => {
                let _ = writeln!(ctx.out, "{text}");
            }
            Err(e) => return blocked(ctx.err, &format!("the report would not serialize: {e}")),
        }
    } else {
        for row in &rows {
            let detail = match &row.reason {
                Some(reason) => reason.clone(),
                None => row.target.clone(),
            };
            let _ = writeln!(ctx.out, "{}  {} · {detail}", row.path, row.status);
        }
    }
    if !flags.quiet {
        let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
        for row in &rows {
            *tally.entry(row.status).or_default() += 1;
        }
        let parts: Vec<String> = tally.iter().map(|(s, n)| format!("{n} {s}")).collect();
        let summary = if parts.is_empty() {
            "no built skills".to_string()
        } else {
            parts.join(", ")
        };
        let _ = writeln!(ctx.err, "{} skills — {summary}", rows.len());
    }
    if rows.iter().any(|r| r.status == "refused") {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
}
