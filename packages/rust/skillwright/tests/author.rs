//! `skillwright author`, driven in-process through a fake [`AuthorHost`]:
//! nothing here starts an app or reads the real Obsidian configuration.

mod common;

use std::cell::{Cell, RefCell};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use common::with_machine_env;
use skillwright::commands::author::{AuthorFlags, AuthorHost, Launch, Os, Probe, run_author_with};
use skillwright::config::{Editor, MachineAuthor};
use skillwright::ctx::CommandContext;
use skillwright::resolve::CommonFlags;

type Hook = Box<dyn FnOnce()>;

struct FakeHost {
    probe: Probe,
    answers: RefCell<Vec<Option<usize>>>,
    questions: RefCell<Vec<String>>,
    launched: RefCell<Vec<Launch>>,
    /// Answers to the running check, in order; once used up, "running".
    running: RefCell<Vec<bool>>,
    checks: Cell<usize>,
    /// Makes `launch` fail with this message.
    launch_error: RefCell<Option<String>>,
    /// Runs while the person is "thinking" at a question.
    while_asking: RefCell<Option<Hook>>,
}

impl FakeHost {
    fn new(probe: Probe) -> Self {
        Self {
            probe,
            answers: RefCell::new(Vec::new()),
            questions: RefCell::new(Vec::new()),
            launched: RefCell::new(Vec::new()),
            running: RefCell::new(Vec::new()),
            checks: Cell::new(0),
            launch_error: RefCell::new(None),
            while_asking: RefCell::new(None),
        }
    }

    fn answering(self, answers: &[Option<usize>]) -> Self {
        *self.answers.borrow_mut() = answers.iter().rev().copied().collect();
        self
    }

    fn running(self, running: &[bool]) -> Self {
        *self.running.borrow_mut() = running.iter().rev().copied().collect();
        self
    }

    fn while_asking(self, hook: impl FnOnce() + 'static) -> Self {
        *self.while_asking.borrow_mut() = Some(Box::new(hook));
        self
    }
}

impl AuthorHost for FakeHost {
    fn probe(&self) -> Probe {
        self.probe.clone()
    }

    fn launch(&self, launch: &Launch) -> Result<(), String> {
        self.launched.borrow_mut().push(launch.clone());
        match self.launch_error.borrow().clone() {
            Some(message) => Err(message),
            None => Ok(()),
        }
    }

    fn obsidian_running(&self, os: Os) -> bool {
        assert_eq!(os, self.probe.os);
        self.checks.set(self.checks.get() + 1);
        self.running.borrow_mut().pop().unwrap_or(true)
    }

    fn choose(
        &self,
        question: &str,
        options: &[String],
        _default: usize,
        _err: &mut dyn Write,
    ) -> Option<usize> {
        if let Some(hook) = self.while_asking.borrow_mut().take() {
            hook();
        }
        self.questions
            .borrow_mut()
            .push(format!("{question} {}", options.join(" | ")));
        self.answers
            .borrow_mut()
            .pop()
            .expect("an answer was scripted")
    }
}

/// A temp directory holding a vault, a fake home, and a machine file path.
struct World {
    _dir: tempfile::TempDir,
    root: PathBuf,
    vault: PathBuf,
    machine: PathBuf,
    obsidian_json: PathBuf,
}

impl World {
    fn new(vault_name: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let vault = root.join(vault_name);
        fs::create_dir_all(vault.join("skills")).unwrap();
        fs::write(vault.join("skills/a note.md"), "# a\n").unwrap();
        let home = root.join("home");
        let obsidian_json = home.join("Library/Application Support/obsidian/obsidian.json");
        fs::create_dir_all(obsidian_json.parent().unwrap()).unwrap();
        Self {
            machine: root.join("machine/config.yaml"),
            _dir: dir,
            root,
            vault,
            obsidian_json,
        }
    }

    fn register(&self, vault: &Path) {
        let json = serde_json::json!({
            "vaults": {"0123456789abcdef": {"path": vault, "ts": 1, "open": true}}
        });
        fs::write(&self.obsidian_json, json.to_string()).unwrap();
    }

    /// An `obsidian.json` that lists another folder and has keys of its own.
    fn register_elsewhere(&self) -> serde_json::Value {
        let json = serde_json::json!({
            "vaults": {
                "0123456789abcdef": {"path": self.root.join("elsewhere"), "ts": 1, "open": true}
            },
            "frame": "hidden",
            "updateDisabled": true,
            "nested": {"a": [1, 2, {"b": null}]}
        });
        fs::write(&self.obsidian_json, json.to_string()).unwrap();
        json
    }

    fn backup(&self) -> PathBuf {
        self.obsidian_json.with_file_name("obsidian.json.bak")
    }

    fn save_editor(&self, editor: &str) {
        fs::create_dir_all(self.machine.parent().unwrap()).unwrap();
        fs::write(
            &self.machine,
            format!("# mine\nauthor:\n  editor: {editor}\n"),
        )
        .unwrap();
    }

    fn probe(&self, interactive: bool) -> Probe {
        Probe {
            os: Os::Mac,
            interactive,
            obsidian_config: Some(self.obsidian_json.clone()),
            code: true,
            terminal_editor: Some(("EDITOR".to_string(), "vim".to_string())),
        }
    }
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn flags(world: &World, path: Option<&str>, with: Option<Editor>, dry_run: bool) -> AuthorFlags {
    AuthorFlags {
        path: path.map(str::to_string),
        with,
        dry_run,
        common: CommonFlags {
            vault: Some(world.vault.display().to_string()),
            ..Default::default()
        },
        json: false,
        quiet: false,
    }
}

fn run(world: &World, flags: &AuthorFlags, host: &FakeHost) -> Run {
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = with_machine_env(Some(&world.machine), || {
        let mut ctx = CommandContext::new(&world.root, &mut out, &mut err);
        run_author_with(flags, &mut ctx, host)
    });
    Run {
        code,
        stdout: String::from_utf8(out).unwrap(),
        stderr: String::from_utf8(err).unwrap(),
    }
}

fn argv(host: &FakeHost) -> Vec<Vec<String>> {
    host.launched
        .borrow()
        .iter()
        .map(|l| l.argv.clone())
        .collect()
}

fn encoded(path: &Path) -> String {
    skillwright::commands::author::obsidian::open_uri(path)
}

#[test]
fn a_registered_vault_opens_in_obsidian() {
    let world = World::new("vault");
    world.register(&world.vault);
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        argv(&host),
        vec![vec!["open".to_string(), encoded(&world.vault)]]
    );
    assert!(r.stdout.contains("editor  obsidian"), "{}", r.stdout);
}

#[test]
fn a_note_inside_a_registered_ancestor_opens_by_its_own_path() {
    let world = World::new("vault");
    world.register(&world.root);
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(
            &world,
            Some("skills/a note.md"),
            Some(Editor::Obsidian),
            false,
        ),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let note = world.vault.join("skills/a note.md");
    assert_eq!(argv(&host), vec![vec!["open".to_string(), encoded(&note)]]);
    assert!(
        r.stderr.contains("opens inside the Obsidian vault"),
        "{}",
        r.stderr
    );
}

#[test]
fn paths_with_spaces_are_percent_encoded() {
    let world = World::new("My Vault");
    world.register(&world.vault);
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), true),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.contains("%2FMy%20Vault"), "{}", r.stdout);
    assert!(!r.stdout.contains("My Vault%"), "{}", r.stdout);
}

#[test]
fn an_unregistered_folder_off_a_terminal_recommends_and_falls_through() {
    let world = World::new("vault");
    world.register(&world.root.join("elsewhere"));
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stderr.contains("Obsidian does not know"), "{}", r.stderr);
    assert!(
        r.stderr.contains("\"Open folder as vault\""),
        "{}",
        r.stderr
    );
    assert!(r.stderr.contains(".obsidian"), "{}", r.stderr);
    assert_eq!(
        argv(&host),
        vec![vec!["code".to_string(), world.vault.display().to_string()]]
    );
    assert!(host.questions.borrow().is_empty());
}

#[test]
fn malformed_obsidian_json_reads_as_not_registered() {
    let world = World::new("vault");
    fs::write(&world.obsidian_json, "{not json").unwrap();
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stderr.contains("unexpected content"), "{}", r.stderr);
    assert!(r.stderr.contains("treating the folder as not registered"));
    assert_eq!(argv(&host)[0][0], "code");
}

#[test]
fn without_obsidian_json_obsidian_is_not_available() {
    let world = World::new("vault");
    let mut probe = world.probe(false);
    probe.obsidian_config = None;
    let host = FakeHost::new(probe);
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 2, "{}", r.stderr);
    assert!(
        r.stderr.contains("obsidian.json was not found"),
        "{}",
        r.stderr
    );
    assert!(argv(&host).is_empty());
}

#[test]
fn an_unregistered_folder_on_a_terminal_offers_the_vault_picker() {
    let world = World::new("vault");
    let host = FakeHost::new(world.probe(true)).answering(&[Some(0)]);
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        argv(&host),
        vec![vec![
            "open".to_string(),
            "obsidian://choose-vault".to_string()
        ]]
    );
}

#[test]
fn cancelling_at_the_vault_question_opens_nothing() {
    let world = World::new("vault");
    let host = FakeHost::new(world.probe(true)).answering(&[Some(2)]);
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 2);
    assert!(argv(&host).is_empty());
}

#[test]
fn the_saved_choice_is_honoured() {
    let world = World::new("vault");
    world.save_editor("editor");
    let host = FakeHost::new(world.probe(true));
    let r = run(
        &world,
        &flags(&world, Some("skills/a note.md"), None, false),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let launched = host.launched.borrow();
    assert!(launched[0].terminal);
    assert_eq!(launched[0].argv[..3], ["sh", "-c", "vim \"$@\""]);
}

#[test]
fn with_overrides_the_saved_choice() {
    let world = World::new("vault");
    world.save_editor("editor");
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(&world, Some("skills/a note.md"), Some(Editor::Code), false),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let note = world.vault.join("skills/a note.md").display().to_string();
    assert_eq!(
        argv(&host),
        vec![vec![
            "code".to_string(),
            world.vault.display().to_string(),
            "--goto".to_string(),
            note
        ]]
    );
}

#[test]
fn off_a_terminal_the_fallback_skips_the_terminal_editor() {
    let world = World::new("vault");
    let mut probe = world.probe(false);
    probe.obsidian_config = None;
    probe.code = false;
    let host = FakeHost::new(probe);
    let r = run(&world, &flags(&world, None, None, false), &host);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(argv(&host)[0][0], "open");
    assert!(
        r.stderr
            .contains("no editor chosen; using the system default (open)"),
        "{}",
        r.stderr
    );
    assert!(!world.machine.exists(), "nothing is saved off a terminal");
}

#[test]
fn off_a_terminal_an_unregistered_folder_skips_the_terminal_editor() {
    let world = World::new("vault");
    let mut probe = world.probe(false);
    probe.code = false;
    let host = FakeHost::new(probe);
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        argv(&host),
        vec![vec!["open".to_string(), world.vault.display().to_string()]]
    );
}

#[test]
fn with_editor_off_a_terminal_is_blocked() {
    let world = World::new("vault");
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Editor), true),
        &host,
    );
    assert_eq!(r.code, 2, "{}", r.stderr);
    assert!(
        r.stderr.contains("a terminal editor needs a terminal"),
        "{}",
        r.stderr
    );
    assert!(argv(&host).is_empty());
}

#[test]
fn a_saved_terminal_editor_off_a_terminal_falls_back() {
    let world = World::new("vault");
    world.save_editor("editor");
    let mut probe = world.probe(false);
    probe.obsidian_config = None;
    let host = FakeHost::new(probe);
    let r = run(&world, &flags(&world, None, None, false), &host);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(argv(&host)[0][0], "code");
    assert!(r.stderr.contains("needs a terminal"), "{}", r.stderr);
}

#[test]
fn end_of_input_at_the_first_question_cancels_and_saves_nothing() {
    let world = World::new("vault");
    let host = FakeHost::new(world.probe(true)).answering(&[None]);
    let r = run(&world, &flags(&world, None, None, false), &host);
    assert_eq!(r.code, 2, "{}", r.stderr);
    assert!(r.stderr.contains("nothing opened"), "{}", r.stderr);
    assert!(argv(&host).is_empty());
    assert!(!world.machine.exists());
}

#[test]
fn end_of_input_at_the_vault_question_cancels() {
    let world = World::new("vault");
    let host = FakeHost::new(world.probe(true)).answering(&[None]);
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 2, "{}", r.stderr);
    assert!(argv(&host).is_empty());
}

#[cfg(unix)]
#[test]
fn a_vault_registered_through_a_symlink_opens_by_the_stored_path() {
    let world = World::new("vault");
    let link = world.root.join("link");
    std::os::unix::fs::symlink(&world.vault, &link).unwrap();
    world.register(&link);
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(
            &world,
            Some("skills/a note.md"),
            Some(Editor::Obsidian),
            false,
        ),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        argv(&host),
        vec![vec![
            "open".to_string(),
            encoded(&link.join("skills/a note.md"))
        ]]
    );
}

#[test]
fn a_relative_path_outside_the_vault_from_cwd_falls_back_to_the_vault() {
    let world = World::new("vault");
    // `<cwd>/skills` exists but is outside the vault; `<vault>/skills` wins.
    fs::create_dir_all(world.root.join("skills")).unwrap();
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(&world, Some("skills"), Some(Editor::System), false),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        argv(&host),
        vec![vec![
            "open".to_string(),
            world.vault.join("skills").display().to_string()
        ]]
    );
}

#[test]
fn a_bad_author_block_warns_and_breaks_nothing() {
    let world = World::new("vault");
    fs::create_dir_all(world.machine.parent().unwrap()).unwrap();
    fs::write(&world.machine, "author:\n  editor: vscode\n  colour: red\n").unwrap();
    let host = FakeHost::new(world.probe(true)).answering(&[Some(1)]);
    let f = flags(&world, None, None, false);
    let r = run(&world, &f, &host);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stderr.contains("`author.editor`"), "{}", r.stderr);
    assert!(r.stderr.contains("`author.colour`"), "{}", r.stderr);
    assert_eq!(argv(&host)[0][0], "code");
    // The save replaces the whole block.
    let saved = fs::read_to_string(&world.machine).unwrap();
    assert!(saved.contains("editor: code"), "{saved}");
    assert!(
        !saved.contains("vscode") && !saved.contains("colour"),
        "{saved}"
    );

    // A scalar `author:`.
    fs::write(&world.machine, "author: code\n").unwrap();
    let host = FakeHost::new(world.probe(false));
    let r = run(&world, &f, &host);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stderr.contains("is not a mapping"), "{}", r.stderr);
}

#[test]
fn a_concurrent_edit_is_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    fs::write(&path, "namespace: sw\n").unwrap();
    let author = MachineAuthor::read(&path);
    fs::write(&path, "namespace: other\n").unwrap();
    let err = author.save(Editor::Code).unwrap_err();
    assert!(err.contains("changed since skillwright read it"), "{err}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "namespace: other\n");

    // A file created after a read that found none is not overwritten either.
    let fresh = dir.path().join("new.yaml");
    let author = MachineAuthor::read(&fresh);
    fs::write(&fresh, "namespace: mine\n").unwrap();
    assert!(author.save(Editor::Code).is_err());
    assert_eq!(fs::read_to_string(&fresh).unwrap(), "namespace: mine\n");
}

#[cfg(unix)]
#[test]
fn a_symlinked_machine_file_is_written_through() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("dotfiles/config.yaml");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, "# mine\nnamespace: sw\n").unwrap();
    let link = dir.path().join("config.yaml");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    MachineAuthor::read(&link).save(Editor::Code).unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let text = fs::read_to_string(&real).unwrap();
    assert!(text.starts_with("# mine\nnamespace: sw\n"), "{text}");
    assert!(text.contains("editor: code"), "{text}");
}

#[test]
fn off_a_terminal_obsidian_comes_first() {
    let world = World::new("vault");
    world.register(&world.vault);
    let host = FakeHost::new(world.probe(false));
    let r = run(&world, &flags(&world, None, None, false), &host);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(argv(&host)[0][1], encoded(&world.vault));
}

#[test]
fn the_first_run_question_is_saved_to_the_machine_file() {
    let world = World::new("vault");
    fs::create_dir_all(world.machine.parent().unwrap()).unwrap();
    fs::write(&world.machine, "# keep me\nnamespace: sw\n").unwrap();
    let host = FakeHost::new(world.probe(true)).answering(&[Some(1)]);
    let r = run(&world, &flags(&world, None, None, false), &host);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(host.questions.borrow()[0].contains("Obsidian (recommended)"));
    assert_eq!(argv(&host)[0][0], "code");
    let saved = fs::read_to_string(&world.machine).unwrap();
    assert!(saved.starts_with("# keep me\nnamespace: sw\n"), "{saved}");
    assert!(saved.contains("author:"), "{saved}");
    assert!(saved.contains("editor: code"), "{saved}");

    // The next run uses it without asking.
    let host = FakeHost::new(world.probe(true));
    let r = run(&world, &flags(&world, None, None, false), &host);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(host.questions.borrow().is_empty());
    assert_eq!(argv(&host)[0][0], "code");
}

#[test]
fn a_dry_run_launches_nothing_and_saves_nothing() {
    let world = World::new("vault");
    world.register(&world.vault);
    let host = FakeHost::new(world.probe(true));
    let r = run(&world, &flags(&world, None, None, true), &host);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(argv(&host).is_empty());
    assert!(host.questions.borrow().is_empty());
    assert!(!world.machine.exists());
    assert!(r.stdout.contains("obsidian://open?path="), "{}", r.stdout);
    assert!(
        r.stderr.contains("dry run: nothing launched"),
        "{}",
        r.stderr
    );
}

#[test]
fn json_reports_the_command() {
    let world = World::new("vault");
    let host = FakeHost::new(world.probe(false));
    let mut f = flags(&world, None, Some(Editor::System), true);
    f.json = true;
    let r = run(&world, &f, &host);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let doc: serde_json::Value = serde_json::from_str(&r.stdout).unwrap();
    assert_eq!(doc["editor"], "system");
    assert_eq!(doc["launched"], false);
    assert_eq!(doc["command"][0], "open");
}

#[test]
fn json_reports_a_failed_launch() {
    let world = World::new("vault");
    let host = FakeHost::new(world.probe(false));
    *host.launch_error.borrow_mut() = Some("xdg-open: not found".to_string());
    let mut f = flags(&world, None, Some(Editor::System), false);
    f.json = true;
    let r = run(&world, &f, &host);
    assert_eq!(r.code, 2, "{}", r.stderr);
    assert!(
        r.stderr
            .contains("could not open the editor: xdg-open: not found")
    );
    let doc: serde_json::Value = serde_json::from_str(&r.stdout).unwrap();
    assert_eq!(doc["launched"], false);
    assert_eq!(doc["error"], "xdg-open: not found");
}

#[test]
fn json_reports_a_launch_that_worked() {
    let world = World::new("vault");
    let host = FakeHost::new(world.probe(false));
    let mut f = flags(&world, None, Some(Editor::System), false);
    f.json = true;
    let r = run(&world, &f, &host);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let doc: serde_json::Value = serde_json::from_str(&r.stdout).unwrap();
    assert_eq!(doc["launched"], true);
    assert!(doc.get("error").is_none());
}

#[test]
fn a_machine_file_that_cannot_be_located_saves_nothing() {
    let author = MachineAuthor::unavailable("cannot resolve the machine directories".into());
    assert!(author.editor.is_none());
    assert_eq!(author.warnings.len(), 1);
    assert!(author.save(Editor::Code).is_err());
}

#[test]
fn an_unavailable_with_is_blocked() {
    let world = World::new("vault");
    let mut probe = world.probe(false);
    probe.code = false;
    let host = FakeHost::new(probe);
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Code), false),
        &host,
    );
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("`code` is not on PATH"), "{}", r.stderr);
}

#[test]
fn a_path_outside_the_vault_is_a_usage_error() {
    let world = World::new("vault");
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(&world, Some("home"), Some(Editor::Code), false),
        &host,
    );
    assert_eq!(r.code, 3);
    assert!(r.stderr.contains("is not inside the vault"), "{}", r.stderr);
}

const ADD: &str = "Add it to Obsidian's vault list and open it (Obsidian is closed) (recommended)";

#[test]
fn adding_is_offered_only_while_obsidian_is_closed() {
    let world = World::new("vault");
    world.register_elsewhere();
    let host = FakeHost::new(world.probe(true))
        .running(&[true])
        .answering(&[Some(2)]);
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 2, "{}", r.stderr);
    assert_eq!(host.checks.get(), 1);
    let question = &host.questions.borrow()[0];
    assert!(!question.contains("Add it"), "{question}");
    assert!(question.starts_with("What now? Open the vault picker now"));
    assert!(r.stderr.contains("Obsidian is running"), "{}", r.stderr);

    let host = FakeHost::new(world.probe(true))
        .running(&[false])
        .answering(&[Some(3)]);
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 2, "{}", r.stderr);
    assert_eq!(
        host.questions.borrow()[0],
        format!(
            "What now? {ADD} | Open the vault picker now | Use another editor this time | Cancel"
        )
    );
    assert!(!world.backup().exists());
}

#[test]
fn malformed_obsidian_json_is_never_offered_for_adding() {
    let world = World::new("vault");
    fs::write(&world.obsidian_json, "{not json").unwrap();
    let host = FakeHost::new(world.probe(true))
        .running(&[false, false])
        .answering(&[Some(0)]);
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(host.checks.get(), 0);
    assert!(!host.questions.borrow()[0].contains("Add it"));
    assert_eq!(argv(&host)[0][1], "obsidian://choose-vault");
    assert_eq!(
        fs::read_to_string(&world.obsidian_json).unwrap(),
        "{not json"
    );
    assert!(!world.backup().exists());
}

#[test]
fn adding_keeps_everything_else_backs_up_and_opens_the_vault() {
    let world = World::new("vault");
    let before = world.register_elsewhere();
    let original = fs::read(&world.obsidian_json).unwrap();
    let host = FakeHost::new(world.probe(true))
        .running(&[false, false])
        .answering(&[Some(0)]);
    let r = run(
        &world,
        &flags(
            &world,
            Some("skills/a note.md"),
            Some(Editor::Obsidian),
            false,
        ),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    // Checked when offering, and again right before writing.
    assert_eq!(host.checks.get(), 2);
    assert_eq!(fs::read(world.backup()).unwrap(), original);

    let mut after: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&world.obsidian_json).unwrap()).unwrap();
    let vaults = after["vaults"].as_object_mut().unwrap();
    assert_eq!(vaults.len(), 2);
    let id = vaults
        .keys()
        .find(|k| *k != "0123456789abcdef")
        .unwrap()
        .clone();
    assert_eq!(id.len(), 16);
    assert!(
        id.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "{id}"
    );
    let added = vaults.remove(&id).unwrap();
    assert_eq!(added["path"], world.vault.display().to_string());
    assert!(added["ts"].as_i64().unwrap() > 1_600_000_000_000);
    assert_eq!(added.as_object().unwrap().len(), 2);
    assert_eq!(after, before, "every other key and vault is untouched");

    let note = world.vault.join("skills/a note.md");
    assert_eq!(argv(&host), vec![vec!["open".to_string(), encoded(&note)]]);
    assert!(
        r.stderr.contains(&format!(
            "added {} to {} as vault {id}; the previous file is in {}",
            world.vault.display(),
            world.obsidian_json.display(),
            world.backup().display()
        )),
        "{}",
        r.stderr
    );
}

#[test]
fn obsidian_started_before_the_write_means_nothing_is_written() {
    let world = World::new("vault");
    world.register_elsewhere();
    let original = fs::read(&world.obsidian_json).unwrap();
    let host = FakeHost::new(world.probe(true))
        .running(&[false, true])
        .answering(&[Some(0)]);
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 2, "{}", r.stderr);
    assert!(r.stderr.contains("Obsidian is running now"), "{}", r.stderr);
    assert_eq!(fs::read(&world.obsidian_json).unwrap(), original);
    assert!(!world.backup().exists());
    assert!(argv(&host).is_empty());
}

#[test]
fn a_file_changed_while_asking_is_not_overwritten() {
    let world = World::new("vault");
    world.register_elsewhere();
    let json = world.obsidian_json.clone();
    let host = FakeHost::new(world.probe(true))
        .running(&[false, false])
        .answering(&[Some(0)])
        .while_asking(move || fs::write(&json, r#"{"vaults":{}}"#).unwrap());
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 2, "{}", r.stderr);
    assert!(
        r.stderr.contains("changed since skillwright read it"),
        "{}",
        r.stderr
    );
    assert_eq!(
        fs::read_to_string(&world.obsidian_json).unwrap(),
        r#"{"vaults":{}}"#
    );
    assert!(!world.backup().exists());
    assert!(argv(&host).is_empty());
}

#[test]
fn a_dry_run_prints_the_planned_write_and_writes_nothing() {
    let world = World::new("vault");
    world.register_elsewhere();
    let original = fs::read(&world.obsidian_json).unwrap();
    let host = FakeHost::new(world.probe(true)).running(&[false]);
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), true),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(host.questions.borrow().is_empty());
    assert!(argv(&host).is_empty());
    assert!(
        r.stderr.contains(&format!(
            "dry run: would copy {} to {}, then add vaults.<16 random hex>",
            world.obsidian_json.display(),
            world.backup().display()
        )),
        "{}",
        r.stderr
    );
    assert!(r.stdout.contains(&encoded(&world.vault)), "{}", r.stdout);
    assert_eq!(fs::read(&world.obsidian_json).unwrap(), original);
    assert!(!world.backup().exists());
}

#[test]
fn off_a_terminal_nothing_is_added() {
    let world = World::new("vault");
    world.register_elsewhere();
    let original = fs::read(&world.obsidian_json).unwrap();
    for dry_run in [false, true] {
        let host = FakeHost::new(world.probe(false)).running(&[false, false]);
        let r = run(
            &world,
            &flags(&world, None, Some(Editor::Obsidian), dry_run),
            &host,
        );
        assert_eq!(r.code, 0, "{}", r.stderr);
        assert_eq!(host.checks.get(), 0);
        assert!(host.questions.borrow().is_empty());
        assert_eq!(fs::read(&world.obsidian_json).unwrap(), original);
        assert!(!world.backup().exists());
        assert!(r.stdout.contains("editor  code"), "{}", r.stdout);
    }
}

#[test]
fn the_picker_route_prints_steps_and_no_path_line() {
    let world = World::new("vault");
    for (os, mac_hint) in [(Os::Mac, true), (Os::Linux, false)] {
        let mut probe = world.probe(true);
        probe.os = os;
        let host = FakeHost::new(probe).answering(&[Some(0)]);
        let r = run(
            &world,
            &flags(&world, None, Some(Editor::Obsidian), false),
            &host,
        );
        assert_eq!(r.code, 0, "{}", r.stderr);
        assert!(!r.stdout.contains("path"), "{}", r.stdout);
        assert!(
            r.stderr
                .contains("In the vault picker, click \"Open\" next to \"Open folder as vault\"."),
            "{}",
            r.stderr
        );
        assert_eq!(r.stderr.contains("Cmd+Shift+G"), mac_hint, "{}", r.stderr);
        assert!(
            r.stderr.contains(&format!("{}.", world.vault.display())),
            "{}",
            r.stderr
        );
    }
}
