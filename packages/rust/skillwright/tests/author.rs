//! `skillwright author`, driven in-process through a fake [`AuthorHost`]:
//! nothing here starts an app or reads the real Obsidian configuration.

mod common;

use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use common::with_machine_env;
use skillwright::cli::run_with;
use skillwright::commands::author::{AuthorFlags, AuthorHost, Launch, Os, Probe, run_author_with};
use skillwright::config::Editor;
use skillwright::ctx::CommandContext;
use skillwright::resolve::CommonFlags;

struct FakeHost {
    probe: Probe,
    answers: RefCell<Vec<Option<usize>>>,
    questions: RefCell<Vec<String>>,
    launched: RefCell<Vec<Launch>>,
}

impl FakeHost {
    fn new(probe: Probe) -> Self {
        Self {
            probe,
            answers: RefCell::new(Vec::new()),
            questions: RefCell::new(Vec::new()),
            launched: RefCell::new(Vec::new()),
        }
    }

    fn answering(self, answers: &[Option<usize>]) -> Self {
        *self.answers.borrow_mut() = answers.iter().rev().copied().collect();
        self
    }
}

impl AuthorHost for FakeHost {
    fn probe(&self) -> Probe {
        self.probe.clone()
    }

    fn launch(&self, launch: &Launch) -> Result<(), String> {
        self.launched.borrow_mut().push(launch.clone());
        Ok(())
    }

    fn choose(
        &self,
        question: &str,
        options: &[String],
        _default: usize,
        _err: &mut dyn Write,
    ) -> Option<usize> {
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
            obsidian_installed: true,
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
    skillwright::commands::author::obsidian_open_uri(path)
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
fn a_missing_obsidian_json_reads_as_not_registered() {
    let world = World::new("vault");
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(&world, None, Some(Editor::Obsidian), false),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stderr.contains("cannot read"), "{}", r.stderr);
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
    let host = FakeHost::new(world.probe(false));
    let r = run(
        &world,
        &flags(&world, Some("skills/a note.md"), None, false),
        &host,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let launched = host.launched.borrow();
    assert!(launched[0].terminal);
    assert_eq!(launched[0].argv[0], "vim");
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
fn off_a_terminal_with_nothing_saved_the_first_available_wins() {
    let world = World::new("vault");
    let mut probe = world.probe(false);
    probe.obsidian_installed = false;
    probe.code = false;
    let host = FakeHost::new(probe);
    let r = run(&world, &flags(&world, None, None, false), &host);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(argv(&host)[0][0], "vim");
    assert!(
        r.stderr.contains("no editor chosen; using $EDITOR (vim)"),
        "{}",
        r.stderr
    );
    assert!(!world.machine.exists(), "nothing is saved off a terminal");
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

#[test]
fn a_project_file_may_not_set_author() {
    let world = World::new("vault");
    fs::write(
        world.root.join("skillwright.yaml"),
        "vault: vault\nauthor:\n  editor: code\n",
    )
    .unwrap();
    let mut f = flags(&world, None, Some(Editor::Code), true);
    f.common.vault = None;
    let host = FakeHost::new(world.probe(false));
    let r = run(&world, &f, &host);
    assert_eq!(r.code, 2);
    assert!(
        r.stderr.contains("`author` is a per-user setting"),
        "{}",
        r.stderr
    );
}

#[test]
fn the_cli_parses_author() {
    let world = World::new("vault");
    let argv: Vec<String> = [
        "author",
        "--with",
        "system",
        "--dry-run",
        "--vault",
        &world.vault.display().to_string(),
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = with_machine_env(Some(&world.machine), || {
        let mut ctx = CommandContext::new(&world.root, &mut out, &mut err);
        run_with(&argv, &mut ctx)
    });
    let stderr = String::from_utf8(err).unwrap();
    assert_eq!(code, 0, "{stderr}");
    let stdout = String::from_utf8(out).unwrap();
    assert!(stdout.contains("editor  system"), "{stdout}");
}
