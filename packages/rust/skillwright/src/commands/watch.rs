//! `skillwright build --watch` — the same render/write path `build` uses,
//! re-run every time something inside the vault, or any declared `assets:`
//! or `hooks:` source, changes on disk.
//!
//! This module owns no rendering logic of its own. It resolves the vault
//! root once (the same [`resolve_run`] every other verb calls), arms a
//! [`notify`] watcher over it and over each asset and hooks source. A
//! directory is watched recursively; a file through its parent directory,
//! because an editor's atomic save replaces the file a direct watch would be
//! holding. On every debounced batch of filesystem events it calls straight
//! back into [`crate::commands::marketplace::run_export`] — one rebuild, one status
//! line, the identical write path a plain `skillwright build` takes. A batch
//! that turns out to touch nothing publishable still writes nothing and
//! still reports "0 changed", exactly as running `build` twice in a row
//! would.
//!
//! A rebuild that fails — a half-saved note that does not parse yet — prints
//! its error and keeps watching: the next save is the fix, and a loop that
//! died on the first mistake would make the author restart it every time.
//! Only the initial build's failure ends the command, because that one says
//! the configuration itself is wrong.
//!
//! # Debouncing
//!
//! One save is several OS events — an editor's atomic write is a temp file
//! created, written, and renamed over the target, which a naive watcher
//! would see as three or four separate events. `DEBOUNCE` collapses a
//! burst into one rebuild. The loop needs no per-path classification,
//! only "something changed, rebuild". A read is not a change: see
//! `is_change`.
//!
//! # No confirmation prompt
//!
//! [`run_export`]'s write path asks for confirmation unless `--yes` was
//! given (the CLI spec's rule for any writing verb). A watch loop has
//! nobody at a keyboard between saves to answer that prompt, so every
//! rebuild this module triggers passes `yes: true` regardless of what the
//! user typed — the same way `--dry-run` on `build` already overrides what
//! would otherwise be asked. What the user passed for `--dry-run` is left
//! alone: a `build --watch --dry-run` is a legitimate "tell me what would
//! change on every save, write nothing" loop.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use tradecraft_core::built_paths_for;

use crate::commands::marketplace::{ExportFlags, ExportVerb, out_root, run_export};
use crate::config::resolve_configured_path;
use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, blocked};
use crate::resolve::{refuse_workspace, resolve_run};

/// How long a burst of filesystem events is treated as one save. Long enough
/// to swallow an editor's create/write/rename trio; short enough that the
/// "a few seconds" rebuild budget still has room in it once the render
/// itself is accounted for.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// Watch the vault named by `flags` and rebuild on every change. Runs until
/// interrupted (Ctrl-C ends the process; there is no other stop signal in
/// the CLI surface) — see [`run_watch_for`] for the bounded form a test
/// drives so the suite itself does not hang forever.
pub fn run_watch(flags: &ExportFlags, ctx: &mut CommandContext<'_>) -> i32 {
    run_watch_for(flags, ctx, None)
}

/// The real loop. An initial build runs first, unconditionally, so the
/// output reflects the vault's current state the moment the watch starts
/// rather than only after the first subsequent save. After that, one
/// rebuild runs per debounced batch of events.
///
/// `max_rebuilds` bounds how many EVENT-TRIGGERED rebuilds (not counting the
/// initial one) the loop performs before returning `EXIT_OK`, so a test can
/// touch a file and wait for exactly one rebuild instead of watching a loop
/// that would otherwise run until the test process is killed. Production's
/// `run_watch` passes `None` and never returns on its own.
pub fn run_watch_for(
    flags: &ExportFlags,
    ctx: &mut CommandContext<'_>,
    max_rebuilds: Option<usize>,
) -> i32 {
    let run = match resolve_run(&flags.common, &ctx.cwd, ctx.err) {
        Ok(run) => run,
        Err(code) => return code,
    };
    if let Err(code) = refuse_workspace("watch", &run, ctx.err) {
        return code;
    }
    // The output can be inside the vault, and what a build writes there is
    // not a change.
    let out = match out_root(flags, &run, &ctx.cwd, ctx.err) {
        Ok(out) => out,
        Err(code) => return code,
    };

    // Every rebuild this loop triggers writes unattended — see the module
    // doc's "No confirmation prompt" section.
    let rebuild_flags = ExportFlags {
        out: flags.out.clone(),
        skills_root: flags.skills_root.clone(),
        marketplace: flags.marketplace.clone(),
        common: flags.common.clone(),
        json: flags.json,
        dry_run: flags.dry_run,
        yes: true,
        quiet: flags.quiet,
    };

    if !flags.quiet {
        let _ = writeln!(
            ctx.err,
            "watch: building {} once before watching for changes",
            run.vault.display()
        );
    }
    let code = run_export(ExportVerb::Build, &rebuild_flags, ctx);
    if code != EXIT_OK {
        return code;
    }
    // `Some(0)` asks for the initial build and nothing else — most useful to
    // a test that only wants to pin the initial-build behaviour without
    // arming a watcher (and blocking on an event that will never come).
    if max_rebuilds == Some(0) {
        return EXIT_OK;
    }

    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher =
        match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let _ = tx.send(res);
        }) {
            Ok(watcher) => watcher,
            Err(err) => {
                return blocked(
                    ctx.err,
                    &format!("watch failed: could not start a filesystem watcher: {err}"),
                );
            }
        };
    for (path, mode) in watch_targets(&run) {
        if let Err(err) = watcher.watch(&path, mode) {
            return blocked(
                ctx.err,
                &format!("watch failed: could not watch {}: {err}", path.display()),
            );
        }
        if !flags.quiet {
            let _ = writeln!(
                ctx.err,
                "watch: watching {} for changes (Ctrl-C to stop)",
                path.display()
            );
        }
    }

    let mut rebuilds = 0usize;
    loop {
        // Read each time: a rebuild's note edit can move the packs folder.
        let built: Vec<String> = run
            .effective_configs(flags.marketplace.as_deref())
            .iter()
            .flat_map(built_paths_for)
            .collect();
        match next_batch(&rx, DEBOUNCE, &out, &built) {
            // The sender lives inside the watcher's OS callback; a closed
            // channel means the watcher itself was torn down.
            None => return EXIT_OK,
            Some(false) => continue,
            Some(true) => {}
        }

        let started = Instant::now();
        let code = run_export(ExportVerb::Build, &rebuild_flags, ctx);
        if code != EXIT_OK {
            // `run_export` already wrote the reason; see the module doc on
            // why a failed rebuild does not end the loop.
            let _ = writeln!(
                ctx.err,
                "watch: rebuild failed (exit {code}); still watching"
            );
        } else if !flags.quiet {
            let _ = writeln!(
                ctx.err,
                "watch: rebuilt in {}ms",
                started.elapsed().as_millis()
            );
        }

        rebuilds += 1;
        if max_rebuilds.is_some_and(|max| rebuilds >= max) {
            return EXIT_OK;
        }
    }
}

/// Wait for the next batch of events and say whether any of it was a change.
/// Blocks for the batch's first event, then takes whatever else arrives
/// within `debounce`, so one save is one rebuild rather than one per OS
/// event. `None` when the channel has closed.
fn next_batch(
    rx: &mpsc::Receiver<notify::Result<notify::Event>>,
    debounce: Duration,
    out: &Path,
    built: &[String],
) -> Option<bool> {
    let is_change = |event: &notify::Event| is_change(event) && !is_build_output(event, out, built);
    let first = rx.recv().ok()?;
    let mut changed = first.as_ref().is_ok_and(is_change);
    let deadline = Instant::now() + debounce;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(next) => changed |= next.as_ref().is_ok_and(is_change),
            Err(_) => break,
        }
    }
    Some(changed)
}

/// Whether an event can mean a file's content changed. A read is not one:
/// Linux's watcher reports every open and read-only close, and each rebuild
/// reads the whole vault, so counting them would rebuild forever.
fn is_change(event: &notify::Event) -> bool {
    use notify::event::{AccessKind, AccessMode, EventKind};
    match event.kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) => false,
        _ => true,
    }
}

/// Whether every path in `event` is something a build writes under `out`,
/// `built` being those paths. Only those: with `out` at the vault root, a
/// `build.hooks` source can sit in a dot folder beside them.
fn is_build_output(event: &notify::Event, out: &Path, built: &[String]) -> bool {
    !event.paths.is_empty()
        && event.paths.iter().all(|path| {
            path.strip_prefix(out).is_ok_and(|rest| {
                let rest = rest.to_string_lossy().replace('\\', "/");
                built.iter().any(|fragment| {
                    rest == fragment.trim_end_matches('/') || rest.starts_with(fragment.as_str())
                })
            })
        })
}

/// Every path the watcher arms, each with its recursion: the vault itself,
/// then each `assets:` and `hooks:` source resolved the way the build
/// resolves it, with the recursion rule in the module doc. A path named
/// twice is armed once, the deeper recursion winning.
fn watch_targets(run: &crate::resolve::ResolvedRun) -> Vec<(PathBuf, RecursiveMode)> {
    let mut targets: BTreeMap<PathBuf, RecursiveMode> = BTreeMap::new();
    targets.insert(run.vault.clone(), RecursiveMode::Recursive);
    // A workspace file above feeds this build too.
    if let Some(parent) =
        crate::config::workspace_above(&run.vault).and_then(|file| Some(file.parent()?.to_owned()))
    {
        targets.insert(parent, RecursiveMode::NonRecursive);
    }
    let build = &run.config.build;
    let sources = build
        .assets
        .iter()
        .map(|a| &a.from)
        .chain(build.hooks.iter().map(|h| &h.from));
    for from in sources {
        let source = resolve_configured_path(from, &run.config_dir);
        let (path, mode) = if source.is_dir() {
            (source, RecursiveMode::Recursive)
        } else {
            match source.parent() {
                Some(parent) => (parent.to_path_buf(), RecursiveMode::NonRecursive),
                None => continue,
            }
        };
        if targets.get(&path) != Some(&RecursiveMode::Recursive) {
            targets.insert(path, mode);
        }
    }
    targets.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::CommonFlags;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::thread;

    /// The same vault the CLI/host parity test builds from
    /// (`marketplace_parity.rs`) — two marketplaces, real notes, already
    /// proven to render through `run_export`. Copied into a tempdir rather
    /// than watched in place: this suite writes into it to simulate a saved
    /// edit, and the checked-in fixture is shared with other tests.
    fn fixture_source() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/freeform-vault")
            .canonicalize()
            .unwrap()
    }

    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let dest = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &dest);
            } else {
                fs::copy(entry.path(), &dest).unwrap();
            }
        }
    }

    /// A private copy of the fixture vault this test suite is free to write
    /// into.
    fn fixture_vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        copy_tree(&fixture_source(), dir.path());
        dir
    }

    fn flags_for(vault: &Path, out: &Path) -> ExportFlags {
        ExportFlags {
            out: Some(out.display().to_string()),
            skills_root: None,
            // The fixture holds two marketplaces (`acme`, `labs`); named
            // the same way `marketplace_parity.rs` does, or the render
            // refuses `ambiguous-marketplace`.
            marketplace: Some("acme".to_string()),
            common: CommonFlags {
                vault: Some(vault.display().to_string()),
                config: None,
                namespace: None,
            },
            json: false,
            dry_run: false,
            yes: false,
            quiet: true,
        }
    }

    #[test]
    fn a_read_is_not_a_change_but_a_write_is() {
        use notify::event::{AccessKind, AccessMode, CreateKind, EventKind, ModifyKind};
        let event = |kind| notify::Event::new(kind);
        assert!(!is_change(&event(EventKind::Access(AccessKind::Open(
            AccessMode::Any
        )))));
        assert!(!is_change(&event(EventKind::Access(AccessKind::Close(
            AccessMode::Read
        )))));
        assert!(is_change(&event(EventKind::Access(AccessKind::Close(
            AccessMode::Write
        )))));
        assert!(is_change(&event(EventKind::Modify(ModifyKind::Any))));
        assert!(is_change(&event(EventKind::Create(CreateKind::File))));
    }

    #[test]
    fn a_batch_of_reads_does_not_rebuild_but_a_batch_with_a_write_does() {
        use notify::event::{AccessKind, AccessMode, EventKind, ModifyKind};
        let event = |kind| Ok(notify::Event::new(kind));
        let open = EventKind::Access(AccessKind::Open(AccessMode::Any));
        let read = EventKind::Access(AccessKind::Close(AccessMode::Read));
        let debounce = Duration::from_millis(50);
        let (tx, rx) = mpsc::channel();

        tx.send(event(open)).unwrap();
        tx.send(event(read)).unwrap();
        assert_eq!(
            next_batch(&rx, debounce, Path::new("/out"), &[]),
            Some(false)
        );

        tx.send(event(open)).unwrap();
        tx.send(event(EventKind::Modify(ModifyKind::Any))).unwrap();
        tx.send(event(read)).unwrap();
        assert_eq!(
            next_batch(&rx, debounce, Path::new("/out"), &[]),
            Some(true)
        );

        drop(tx);
        assert_eq!(next_batch(&rx, debounce, Path::new("/out"), &[]), None);
    }

    #[test]
    fn the_initial_build_runs_before_any_event_is_seen() {
        let vault_dir = fixture_vault();
        let out_dir = tempfile::tempdir().unwrap();
        let flags = flags_for(vault_dir.path(), out_dir.path());

        let mut out: Vec<u8> = Vec::new();
        let mut err: Vec<u8> = Vec::new();
        let mut ctx = CommandContext::new(vault_dir.path(), &mut out, &mut err);
        let code = run_watch_for(&flags, &mut ctx, Some(0));
        drop(ctx);

        assert_eq!(code, EXIT_OK, "{}", String::from_utf8_lossy(&err));
        assert!(
            out_dir
                .path()
                .join(".claude-plugin/packs/core/skills/review/SKILL.md")
                .exists(),
            "the initial build did not write the rendered skill"
        );
    }

    #[test]
    fn only_a_write_a_build_makes_under_the_output_is_not_a_change() {
        let event =
            |path: &str| notify::Event::new(notify::EventKind::Any).add_path(PathBuf::from(path));
        let out = Path::new("/vault");
        let built = tradecraft_core::built_paths();
        assert!(is_build_output(
            &event("/vault/.claude-plugin/packs/x"),
            out,
            &built
        ));
        assert!(is_build_output(
            &event("/vault/.claude-plugin/packs"),
            out,
            &built
        ));
        assert!(is_build_output(
            &event("/vault/.skillwright-stage/a"),
            out,
            &built
        ));
        assert!(!is_build_output(
            &event("/vault/plugins/p/skills/s.md"),
            out,
            &built
        ));
        // A source in a dot folder beside the output.
        assert!(!is_build_output(
            &event("/vault/.claude/hooks.json"),
            out,
            &built
        ));
        assert!(!is_build_output(
            &event("/vault/.agents/skills/s.md"),
            out,
            &built
        ));
        assert!(!is_build_output(&event("/elsewhere/.pi/x"), out, &built));
        // A moved packs folder, as the run resolves it.
        assert!(!is_build_output(&event("/vault/moved/x"), out, &built));
        let moved = ["moved/".to_owned()];
        assert!(is_build_output(&event("/vault/moved/x"), out, &moved));
    }

    #[test]
    fn asset_sources_are_watched_beside_the_vault() {
        use crate::config::{AssetSetting, RootFile};
        use crate::resolve::ResolvedRun;

        let root = tempfile::tempdir().unwrap();
        let vault = root.path().join("plugin");
        fs::create_dir_all(&vault).unwrap();
        fs::create_dir_all(root.path().join("conventions")).unwrap();
        fs::create_dir_all(root.path().join("bin")).unwrap();
        fs::write(root.path().join("bin/tool"), "launcher\n").unwrap();
        let asset = |from: &str| AssetSetting {
            from: from.to_owned(),
            to: from.to_owned(),
            plugin: None,
        };
        let run = ResolvedRun {
            config: {
                let mut config = RootFile::default();
                config.build.assets = vec![asset("conventions"), asset("bin/tool")];
                config
            },
            config_dir: root.path().to_path_buf(),
            config_path: None,
            namespace: None,
            vault: vault.clone(),
        };

        let targets = watch_targets(&run);
        assert!(targets.contains(&(vault, RecursiveMode::Recursive)));
        assert!(targets.contains(&(root.path().join("conventions"), RecursiveMode::Recursive)));
        // A file asset is watched through its parent, so an editor's
        // rename-over save is still seen.
        assert!(targets.contains(&(root.path().join("bin"), RecursiveMode::NonRecursive)));
        assert_eq!(targets.len(), 3);
    }

    #[test]
    fn a_workspace_file_above_is_watched() {
        use crate::config::RootFile;
        use crate::resolve::ResolvedRun;

        let root = tempfile::tempdir().unwrap();
        let vault = root.path().join("marketplaces/m");
        fs::create_dir_all(&vault).unwrap();
        fs::write(
            root.path().join("skillwright.md"),
            "---\nworkspace: true\n---\n",
        )
        .unwrap();
        let run = ResolvedRun {
            config: RootFile::default(),
            config_dir: vault.clone(),
            config_path: None,
            namespace: None,
            vault,
        };
        assert!(
            watch_targets(&run).contains(&(root.path().to_path_buf(), RecursiveMode::NonRecursive))
        );
    }

    #[test]
    fn a_saved_change_triggers_exactly_one_rebuild() {
        let vault_dir = fixture_vault();
        let out_dir = tempfile::tempdir().unwrap();
        // Beside the vault, not in it: the temp file is not a vault event.
        let scratch = tempfile::tempdir().unwrap();
        let flags = flags_for(vault_dir.path(), out_dir.path());
        let cwd = vault_dir.path().to_path_buf();
        let watch = thread::spawn(move || {
            let mut out: Vec<u8> = Vec::new();
            let mut err: Vec<u8> = Vec::new();
            let mut ctx = CommandContext::new(&cwd, &mut out, &mut err);
            let code = run_watch_for(&flags, &mut ctx, Some(1));
            drop(ctx);
            (code, String::from_utf8_lossy(&err).into_owned())
        });

        let skill_path = vault_dir
            .path()
            .join("marketplaces/acme/plugins/core/skills/review.md");
        let edited = fs::read_to_string(&skill_path)
            .unwrap()
            .replace("Reviews a diff.", "Reviews a diff, now edited.");
        let staged = scratch.path().join("review.md");
        // The watcher arms only after the initial build, so one early save
        // can go unseen: save again until the watch returns. Each save is a
        // rename, so a rebuild never reads a half-written note (a truncated
        // note renders no skill and the build removes its output). Both temp
        // dirs sit under the same temp root, so the rename stays on one
        // filesystem.
        let ceiling = Instant::now() + Duration::from_secs(30);
        while !watch.is_finished() {
            assert!(
                Instant::now() < ceiling,
                "no rebuild followed the saved edit within 30s"
            );
            fs::write(&staged, &edited).unwrap();
            fs::rename(&staged, &skill_path).unwrap();
            thread::sleep(Duration::from_millis(200));
        }
        let (code, log) = watch.join().unwrap();
        assert_eq!(code, EXIT_OK, "{log}");
        assert!(
            !log.contains("rebuild failed"),
            "the rebuild after the save failed: {log}"
        );

        let rendered = fs::read_to_string(
            out_dir
                .path()
                .join(".claude-plugin/packs/core/skills/review/SKILL.md"),
        )
        .expect("the rebuilt output should carry the edited skill");
        assert!(
            rendered.contains("Reviews a diff, now edited."),
            "the rebuild did not pick up the saved change: {rendered}\nwatch log: {log}"
        );
    }
}
