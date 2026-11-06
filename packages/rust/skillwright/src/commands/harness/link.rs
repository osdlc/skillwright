//! `harness link|unlink <harness>`: the dev loop.
//!
//! `link` symlinks each built skill folder of the harness's packs into that
//! harness's user skill folder, so a `build` (or `build --watch`) reaches the
//! harness with no install step. `unlink` removes only links that point into
//! the build's packs: a link pointing somewhere else, and a real file or
//! folder, are never touched. `link` refuses to put a symlink where a real
//! folder or another link already is.
//!
//! Only Claude Code and Codex are linked. harness-reference
//! `facts/<harness>/skills/discovery-walk.md` records that those two follow a
//! symlinked skill folder; for Cursor and Pi it is an open question, which
//! `probes/skill-symlink` would settle.
//!
//! The user skill folders: Codex reads `~/.agents/skills`
//! (`facts/codex/skills/user-dirs.md`), which Cursor and Pi read as well, so
//! `link codex` warns. Claude Code reads `.claude/skills`
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

/// The user skill folder `harness` reads, under `home`, or why `harness`
/// is not linked.
fn user_skills(harness: &str, home: &Path) -> Result<PathBuf, String> {
    match harness {
        "claude" => Ok(home.join(".claude").join("skills")),
        "codex" => Ok(home.join(".agents").join("skills")),
        other => Err(format!(
            "harness link supports claude and codex only. harness-reference does not record \
             whether {other} follows a symlinked skill folder (an open question in \
             facts/{other}/skills/discovery-walk.md; probes/skill-symlink would settle it)"
        )),
    }
}

/// What linking into `harness`'s folder also does to other harnesses.
fn shared_folder_warning(harness: &str) -> Option<&'static str> {
    (harness == "codex").then_some(
        "~/.agents/skills is also read by Cursor and Pi (harness-reference \
         facts/cursor/skills/user-dirs.md, facts/pi/skills/user-dirs.md), so they load these \
         Codex builds too",
    )
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
    /// `absent`, `refused` or `skipped` (not tried, after an earlier failure).
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
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    warnings: &'a [String],
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

#[cfg(not(windows))]
fn remove_link(link: &Path) -> std::io::Result<()> {
    std::fs::remove_file(link)
}

#[cfg(windows)]
fn remove_link(link: &Path) -> std::io::Result<()> {
    // A directory symlink on Windows is removed as a directory.
    std::fs::remove_file(link).or_else(|_| std::fs::remove_dir(link))
}

/// Removes `link` only if it is still a symlink to `target`, read again
/// right before the removal. `Ok(false)` when something else is there now.
fn remove_if_ours(link: &Path, target: &Path) -> Result<bool, String> {
    match found_at(link, target)? {
        Found::Ours => remove_link(link)
            .map(|()| true)
            .map_err(|e| format!("unlink {} failed: {e}", link.display())),
        _ => Ok(false),
    }
}

/// `path` with its longest existing prefix resolved through symlinks, so it
/// compares with link targets made from a canonical root even when the rest
/// of it is gone.
fn canonical_lossy(path: &Path) -> PathBuf {
    let mut rest: Vec<&std::ffi::OsStr> = Vec::new();
    let mut base = path;
    loop {
        if let Ok(found) = base.canonicalize() {
            return rest.iter().rev().fold(found, |acc, part| acc.join(part));
        }
        match (base.parent(), base.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name);
                base = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
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

/// Symlinks in `folder` not named in `skip` whose target is under one of
/// `under`: links an earlier build made for a skill that is no longer
/// built. Each comes back with its target.
fn stale_links(
    folder: &Path,
    under: &[PathBuf],
    skip: &BTreeMap<String, Vec<PathBuf>>,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{} could not be read: {e}", folder.display())),
    };
    let mut stale = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        if name.to_str().is_some_and(|n| skip.contains_key(n)) {
            continue;
        }
        let link = entry.path();
        let is_link = std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink());
        if !is_link {
            continue;
        }
        let Ok(target) = std::fs::read_link(&link) else {
            continue;
        };
        if under.iter().any(|root| target.starts_with(root)) {
            stale.push((link, target));
        }
    }
    stale.sort();
    Ok(stale)
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
    let folder = match user_skills(name, &ctx.home) {
        Ok(folder) => folder,
        Err(message) => return usage_error(ctx.err, &message),
    };
    if flags.scope != Scope::User {
        return usage_error(
            ctx.err,
            &format!("{verb_name} writes to the user skill folder only; drop --scope"),
        );
    }
    if flags.git_url.is_some() || flags.git_ref.is_some() {
        return usage_error(
            ctx.err,
            &format!("{verb_name} links the built folder; drop --git-url and --git-ref"),
        );
    }
    let run = match resolve_run(&flags.common, &ctx.cwd, ctx.err) {
        Ok(run) => run,
        Err(code) => return code,
    };
    let root = match built_root(&run, flags, ctx, &argv_verb) {
        Ok(root) => canonical_lossy(&root),
        Err(code) => return code,
    };
    let Some(packs) = packs(name) else {
        return blocked(ctx.err, &format!("{name} has no pack layout"));
    };
    let packs_dir = root.join(packs);
    let linking = verb == HarnessVerb::Link;
    let skills = match built_skills(&packs_dir, &flags.plugins) {
        Ok(skills) => skills,
        // Unlink still removes links into a build that is gone.
        Err(_) if !linking => BTreeMap::new(),
        Err(message) => return blocked(ctx.err, &message),
    };
    let warnings: Vec<String> = if linking {
        shared_folder_warning(name)
            .map(str::to_string)
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };

    let mut rows: Vec<LinkRow> = Vec::new();
    // Each planned change, by the index of its row.
    let mut actions: Vec<(usize, PathBuf, PathBuf)> = Vec::new();
    let planned = |linking: bool| match (linking, flags.dry_run) {
        (true, true) => "would-link",
        (true, false) => "linked",
        (false, true) => "would-unlink",
        (false, false) => "unlinked",
    };
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
                row.status = planned(linking);
                actions.push((rows.len(), link, target.clone()));
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
    if !linking {
        let under: Vec<PathBuf> = if flags.plugins.is_empty() {
            vec![packs_dir.clone()]
        } else {
            flags.plugins.iter().map(|p| packs_dir.join(p)).collect()
        };
        let stale = match stale_links(&folder, &under, &skills) {
            Ok(stale) => stale,
            Err(message) => return blocked(ctx.err, &message),
        };
        for (link, target) in stale {
            actions.push((rows.len(), link.clone(), target.clone()));
            rows.push(LinkRow {
                path: link.display().to_string(),
                target: target.display().to_string(),
                status: planned(false),
                reason: Some("a link into the build to a skill no longer built".into()),
            });
        }
    }

    let mut failure: Option<String> = None;
    if !flags.dry_run && !actions.is_empty() {
        if !flags.yes {
            let mut plan = vec![argv_verb[1..].join(" "), String::new()];
            plan.extend(plan_flow(&[
                (
                    if linking { "link" } else { "unlink" },
                    actions
                        .iter()
                        .map(|(_, link, target)| {
                            format!("{} -> {}", link.display(), target.display())
                        })
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
        for (row, link, target) in &actions {
            let row = &mut rows[*row];
            if failure.is_some() {
                row.status = "skipped";
                row.reason = Some("not tried after an earlier failure".into());
                continue;
            }
            let done = if linking {
                make_link(target, link)
                    .map(|()| true)
                    .map_err(|e| format!("link {} failed: {e}", link.display()))
            } else {
                remove_if_ours(link, target)
            };
            match done {
                Ok(true) => {}
                Ok(false) => {
                    row.status = "unchanged";
                    row.reason = Some("changed since it was read; left alone".into());
                }
                Err(message) => {
                    row.status = "refused";
                    row.reason = Some(message.clone());
                    failure = Some(message);
                }
            }
        }
    }

    if flags.json {
        let document = LinkDocument {
            harness: name,
            verb: verb_name,
            folder: folder.display().to_string(),
            links: &rows,
            warnings: &warnings,
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
    for warning in &warnings {
        let _ = writeln!(ctx.err, "warning: {warning}");
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
    if let Some(message) = failure {
        return blocked(ctx.err, &message);
    }
    if rows.iter().any(|r| r.status == "refused") {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn unlink_leaves_a_real_folder_that_replaced_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::create_dir_all(&target).unwrap();
        let link = dir.path().join("link");

        // A real empty folder where the link was: left alone, not removed.
        std::fs::create_dir_all(&link).unwrap();
        assert!(!remove_if_ours(&link, &target).unwrap());
        assert!(link.is_dir());
        // Without the Windows fallback, removing a link never removes a folder.
        assert!(remove_link(&link).is_err());
        assert!(link.is_dir());

        // A real file: left alone.
        std::fs::remove_dir(&link).unwrap();
        std::fs::write(&link, "mine").unwrap();
        assert!(!remove_if_ours(&link, &target).unwrap());
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "mine");

        // Still a link to the target: removed.
        std::fs::remove_file(&link).unwrap();
        make_link(&target, &link).unwrap();
        assert!(remove_if_ours(&link, &target).unwrap());
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert!(target.is_dir());
    }

    #[test]
    fn canonical_lossy_resolves_the_part_that_exists() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().canonicalize().unwrap();
        assert_eq!(
            canonical_lossy(&dir.path().join("gone/deeper")),
            real.join("gone/deeper")
        );
    }
}
