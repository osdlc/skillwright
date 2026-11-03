//! `skillwright author [path]` — open the vault, or a folder or note inside
//! it, in an editor. Experimental, and macOS and Linux only.
//!
//! The editor comes from `--with`, else the machine file's `author.editor`,
//! else a one-time question (saved to the machine file). Off a terminal there
//! is no question: the first available editor that needs no terminal wins,
//! and stderr says which.
//!
//! Obsidian only opens folders it already knows as vaults. [`obsidian`]
//! reads its `obsidian.json` to find out, and never writes it: an unknown
//! folder gets a recommendation and, on a terminal, an offer to open
//! Obsidian's own vault picker.
//!
//! Everything that touches the machine — what is installed, the terminal,
//! launching, asking — goes through [`AuthorHost`], so tests never start an
//! app.

pub mod obsidian;

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::{Editor, MachineAuthor, machine_config_path, resolve_against_cwd};
use crate::ctx::CommandContext;
use crate::exit::{EXIT_BLOCKED, EXIT_OK, blocked, usage_error};
use crate::prompt::{is_interactive, plan_flow};
use crate::resolve::{CommonFlags, resolve_run};
use obsidian::{Registration, canonical};

/// The order editors are offered and fallen back to.
const ORDER: [Editor; 4] = [
    Editor::Obsidian,
    Editor::Code,
    Editor::Editor,
    Editor::System,
];

/// The resolved flags of `skillwright author`.
pub struct AuthorFlags {
    /// A folder or note inside the vault; `None` means the vault root.
    pub path: Option<String>,
    /// `--with`: the editor for this run.
    pub with: Option<Editor>,
    /// `--dry-run`: print the action; launch nothing and save nothing.
    pub dry_run: bool,
    /// The global `--vault`, `--config` and `--namespace` flags.
    pub common: CommonFlags,
    /// `--json`: print the report as one JSON document.
    pub json: bool,
    /// `--quiet`: drop notes from stderr.
    pub quiet: bool,
}

/// The operating system, as far as opening files goes. Windows is refused
/// before anything is probed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// macOS.
    Mac,
    /// Linux and the other freedesktop systems.
    Linux,
}

impl Os {
    /// The OS this binary was built for.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Linux
        }
    }

    /// The command that opens a file, folder or URI in its default app.
    pub fn opener(self) -> &'static str {
        match self {
            Self::Mac => "open",
            Self::Linux => "xdg-open",
        }
    }
}

/// The facts [`probe`] reads: the OS and the environment variables.
/// Separate so a test can fake both.
pub struct EnvFacts {
    /// The OS.
    pub os: Os,
    /// The environment variables that matter: `HOME`, `PATH`, `VISUAL`,
    /// `EDITOR`, `XDG_CONFIG_HOME`.
    pub vars: BTreeMap<String, String>,
}

impl EnvFacts {
    /// The facts of this process.
    pub fn from_process() -> Self {
        let vars = ["HOME", "PATH", "VISUAL", "EDITOR", "XDG_CONFIG_HOME"]
            .into_iter()
            .filter_map(|name| std::env::var(name).ok().map(|v| (name.to_string(), v)))
            .collect();
        Self {
            os: Os::current(),
            vars,
        }
    }

    fn var(&self, name: &str) -> Option<&str> {
        self.vars
            .get(name)
            .map(String::as_str)
            .filter(|v| !v.trim().is_empty())
    }

    fn home(&self) -> Option<PathBuf> {
        self.var("HOME").map(PathBuf::from)
    }
}

/// What is available on this machine, read once per run.
#[derive(Debug, Clone)]
pub struct Probe {
    /// The OS.
    pub os: Os,
    /// True when stdin and stdout are a terminal.
    pub interactive: bool,
    /// Obsidian's `obsidian.json`, when it exists. Obsidian counts as
    /// available exactly when it does.
    pub obsidian_config: Option<PathBuf>,
    /// `code` is on `PATH`.
    pub code: bool,
    /// The terminal editor: the variable that named it and its value.
    pub terminal_editor: Option<(String, String)>,
}

impl Probe {
    /// True when `editor` can be launched here. A terminal editor needs a
    /// terminal.
    pub fn has(&self, editor: Editor) -> bool {
        match editor {
            Editor::Obsidian => self.obsidian_config.is_some(),
            Editor::Code => self.code,
            Editor::Editor => self.terminal_editor.is_some() && self.interactive,
            Editor::System => true,
        }
    }

    /// The available editors, in the order they are offered.
    pub fn available(&self) -> Vec<Editor> {
        ORDER.into_iter().filter(|e| self.has(*e)).collect()
    }

    /// How `editor` reads in a question or a note.
    pub fn describe(&self, editor: Editor) -> String {
        match (editor, &self.terminal_editor) {
            (Editor::Obsidian, _) => "Obsidian".to_string(),
            (Editor::Code, _) => "VS Code".to_string(),
            (Editor::Editor, Some((var, value))) => format!("${var} ({value})"),
            (Editor::Editor, None) => "$VISUAL / $EDITOR".to_string(),
            (Editor::System, _) => format!("the system default ({})", self.os.opener()),
        }
    }

    fn unavailable_reason(&self, editor: Editor) -> &'static str {
        match editor {
            Editor::Obsidian => "Obsidian's obsidian.json was not found; open Obsidian once first",
            Editor::System => "no system opener",
            Editor::Code => "`code` is not on PATH",
            Editor::Editor if self.terminal_editor.is_none() => {
                "neither $VISUAL nor $EDITOR is set"
            }
            Editor::Editor => "a terminal editor needs a terminal, and this run has none",
        }
    }
}

/// Read [`Probe`] from `facts`. It looks at the filesystem and nothing else.
pub fn probe(facts: &EnvFacts, interactive: bool) -> Probe {
    let terminal_editor = ["VISUAL", "EDITOR"]
        .into_iter()
        .find_map(|var| facts.var(var).map(|v| (var.to_string(), v.to_string())));
    Probe {
        os: facts.os,
        interactive,
        obsidian_config: obsidian::config_path(facts).filter(|p| p.is_file()),
        code: find_on_path("code", facts.var("PATH")).is_some(),
        terminal_editor,
    }
}

/// The first executable called `name` in `path_var`'s directories.
pub fn find_on_path(name: &str, path_var: Option<&str>) -> Option<PathBuf> {
    path_var?
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(name))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// One process to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    /// Program first.
    pub argv: Vec<String>,
    /// Hand it the terminal and wait for it: a terminal editor.
    pub terminal: bool,
}

/// What `author` needs from the machine.
pub trait AuthorHost {
    /// What is installed, and whether a person is at a terminal.
    fn probe(&self) -> Probe;
    /// Start `launch` and wait for it to exit.
    ///
    /// # Errors
    ///
    /// When it cannot start or exits non-zero.
    fn launch(&self, launch: &Launch) -> Result<(), String>;
    /// Ask `question` with numbered `options`, writing to `err`. `None`
    /// means the person gave no usable answer, end of input included.
    fn choose(
        &self,
        question: &str,
        options: &[String],
        default: usize,
        err: &mut dyn Write,
    ) -> Option<usize>;
}

/// The real machine.
pub struct SystemHost;

impl AuthorHost for SystemHost {
    fn probe(&self) -> Probe {
        probe(&EnvFacts::from_process(), is_interactive())
    }

    // Long-lived and interactive: a terminal editor inherits stdio, which
    // `Runner` (it captures output) cannot do.
    fn launch(&self, launch: &Launch) -> Result<(), String> {
        let (program, args) = launch.argv.split_first().ok_or("empty command")?;
        let mut command = Command::new(program);
        command.args(args);
        if !launch.terminal {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        }
        let status = command
            .status()
            .map_err(|e| format!("{program} could not be started: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{program} exited with {status}"))
        }
    }

    fn choose(
        &self,
        question: &str,
        options: &[String],
        default: usize,
        err: &mut dyn Write,
    ) -> Option<usize> {
        ask(
            question,
            options,
            default,
            &mut std::io::stdin().lock(),
            err,
        )
    }
}

/// [`AuthorHost::choose`] over any input. An empty line takes `default`; end
/// of input, a read error or an answer out of range is `None`.
pub fn ask(
    question: &str,
    options: &[String],
    default: usize,
    input: &mut dyn BufRead,
    err: &mut dyn Write,
) -> Option<usize> {
    let _ = writeln!(err, "{question}");
    for (index, option) in options.iter().enumerate() {
        let _ = writeln!(err, "  {}) {option}", index + 1);
    }
    let _ = write!(err, "Choose [{}]: ", default + 1);
    let _ = err.flush();
    let mut answer = String::new();
    match input.read_line(&mut answer) {
        Ok(0) | Err(_) => return None,
        Ok(_) => {}
    }
    let answer = answer.trim();
    if answer.is_empty() {
        return Some(default);
    }
    let number: usize = answer.parse().ok()?;
    (1..=options.len()).contains(&number).then(|| number - 1)
}

/// Run `skillwright author` on the real machine.
pub fn run_author(flags: &AuthorFlags, ctx: &mut CommandContext<'_>) -> i32 {
    run_author_with(flags, ctx, &SystemHost)
}

/// [`run_author`] with the machine supplied.
pub fn run_author_with(
    flags: &AuthorFlags,
    ctx: &mut CommandContext<'_>,
    host: &dyn AuthorHost,
) -> i32 {
    if cfg!(windows) {
        return usage_error(
            ctx.err,
            "`skillwright author` is experimental and not supported on Windows.",
        );
    }
    let run = match resolve_run(&flags.common, &ctx.cwd, ctx.err) {
        Ok(run) => run,
        Err(code) => return code,
    };
    let vault = canonical(&run.vault);
    let target = match resolve_target(flags.path.as_deref(), &ctx.cwd, &vault) {
        Ok(target) => target,
        Err(message) => return usage_error(ctx.err, &message),
    };
    let probe = host.probe();
    // A dry run asks nothing and saves nothing.
    let ask = probe.interactive && !flags.dry_run;
    let mut notes: Vec<String> = Vec::new();

    let mut editor = match pick_editor(flags, &probe, ask, host, ctx.err, &mut notes) {
        Ok(editor) => editor,
        Err(code) => return code,
    };

    let mut launch = None;
    if editor == Editor::Obsidian {
        // `has(Obsidian)` holds, so the config exists.
        let config = probe.obsidian_config.as_deref().unwrap_or(Path::new(""));
        match obsidian::registration(config, &target) {
            Registration::Registered { vault: owner, path } => {
                if canonical(&owner) != vault {
                    notes.push(format!(
                        "{} opens inside the Obsidian vault {}",
                        target.display(),
                        owner.display()
                    ));
                }
                launch = Some(plan_launch(Editor::Obsidian, &probe, &vault, &path));
            }
            other => {
                flush_notes(&mut notes, flags.quiet, ctx.err);
                if let Registration::Unknown { reason } = &other {
                    let _ = writeln!(ctx.err, "{reason}; treating the folder as not registered.");
                }
                for line in recommendation(&vault, ctx) {
                    let _ = writeln!(ctx.err, "{line}");
                }
                let options = [
                    "Open the vault picker now".to_string(),
                    "Use another editor this time".to_string(),
                    "Cancel".to_string(),
                ];
                let choice = if ask {
                    host.choose("What now?", &options, 0, ctx.err)
                } else {
                    Some(1)
                };
                match choice {
                    Some(0) => {
                        launch = Some(Launch {
                            argv: vec![
                                probe.os.opener().to_string(),
                                obsidian::PICKER_URI.to_string(),
                            ],
                            terminal: false,
                        });
                    }
                    Some(1) => {
                        let Some(next) = probe
                            .available()
                            .into_iter()
                            .find(|e| *e != Editor::Obsidian)
                        else {
                            return blocked(ctx.err, "no other editor is available.");
                        };
                        notes.push(format!("opening with {} instead", probe.describe(next)));
                        editor = next;
                    }
                    _ => return blocked(ctx.err, "cancelled; nothing opened."),
                }
            }
        }
    }

    let launch = launch.unwrap_or_else(|| plan_launch(editor, &probe, &vault, &target));
    flush_notes(&mut notes, flags.quiet, ctx.err);
    report(flags, editor, &target, &launch, ctx);
    if flags.dry_run {
        if !flags.quiet {
            let _ = writeln!(ctx.err, "dry run: nothing launched");
        }
        return EXIT_OK;
    }
    match host.launch(&launch) {
        Ok(()) => EXIT_OK,
        Err(message) => blocked(ctx.err, &format!("could not open the editor: {message}")),
    }
}

/// `path`, absolute or relative, as a canonical path inside the vault. A
/// relative path is tried against `cwd` first, and that wins only when it
/// lands inside the vault; otherwise it is tried against the vault.
fn resolve_target(path: Option<&str>, cwd: &Path, vault: &Path) -> Result<PathBuf, String> {
    let Some(path) = path else {
        return Ok(vault.to_path_buf());
    };
    let mut candidates = vec![resolve_against_cwd(path, cwd)];
    if Path::new(path).is_relative() {
        candidates.push(vault.join(path));
    }
    let found: Vec<PathBuf> = candidates
        .iter()
        .filter(|c| c.exists())
        .map(|c| canonical(c))
        .collect();
    if let Some(inside) = found.iter().find(|c| c.starts_with(vault)) {
        return Ok(inside.clone());
    }
    match found.first() {
        Some(outside) => Err(format!(
            "{} is not inside the vault {}",
            outside.display(),
            vault.display()
        )),
        None => Err(format!("not found: {path}")),
    }
}

/// `--with`, else the saved choice, else a question (saved), else the first
/// available editor.
fn pick_editor(
    flags: &AuthorFlags,
    probe: &Probe,
    ask: bool,
    host: &dyn AuthorHost,
    err: &mut dyn Write,
    notes: &mut Vec<String>,
) -> Result<Editor, i32> {
    if let Some(editor) = flags.with {
        if probe.has(editor) {
            return Ok(editor);
        }
        return Err(blocked(
            err,
            &format!(
                "--with {}: {}",
                editor.as_str(),
                probe.unavailable_reason(editor)
            ),
        ));
    }
    let saved = MachineAuthor::read(&machine_config_path());
    notes.extend(saved.warnings.iter().map(|w| format!("warning: {w}")));
    match saved.editor {
        Some(editor) if probe.has(editor) => return Ok(editor),
        Some(editor) => notes.push(format!(
            "the saved editor `{}` is not available here: {}",
            editor.as_str(),
            probe.unavailable_reason(editor)
        )),
        None => {}
    }
    let available = probe.available();
    let Some(&first) = available.first() else {
        return Err(blocked(
            err,
            "no editor found: install Obsidian or VS Code, or set $VISUAL or $EDITOR.",
        ));
    };
    if !ask {
        notes.push(if probe.interactive {
            format!(
                "dry run: a real run asks which editor to use; showing the first available, {}.",
                probe.describe(first)
            )
        } else {
            format!(
                "no editor chosen; using {}. Pass --with, or run `skillwright author` in a terminal to choose one.",
                probe.describe(first)
            )
        });
        return Ok(first);
    }
    flush_notes(notes, flags.quiet, err);
    let options: Vec<String> = available
        .iter()
        .enumerate()
        .map(|(index, editor)| {
            let name = probe.describe(*editor);
            if index == 0 {
                format!("{name} (recommended)")
            } else {
                name
            }
        })
        .collect();
    let Some(index) = host.choose(
        "Which editor should `skillwright author` open?",
        &options,
        0,
        err,
    ) else {
        let _ = writeln!(err, "no editor chosen; nothing opened.");
        return Err(EXIT_BLOCKED);
    };
    let editor = available[index];
    match saved.save(editor) {
        Ok(path) => notes.push(format!(
            "saved author.editor: {} in {}",
            editor.as_str(),
            path.display()
        )),
        Err(message) => notes.push(format!("could not save the choice: {message}")),
    }
    Ok(editor)
}

/// The process that opens `target` in `editor`. For Obsidian, `target` is
/// the path as Obsidian stores the vault.
pub fn plan_launch(editor: Editor, probe: &Probe, vault: &Path, target: &Path) -> Launch {
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let opener = probe.os.opener().to_string();
    let argv = match editor {
        Editor::Obsidian => vec![opener, obsidian::open_uri(target)],
        Editor::Code if target.is_file() => vec![
            "code".to_string(),
            path(vault),
            "--goto".to_string(),
            path(target),
        ],
        Editor::Code => vec!["code".to_string(), path(vault)],
        // As git runs it: the shell reads the value, so quotes and
        // arguments in it work.
        Editor::Editor => {
            let value = probe
                .terminal_editor
                .as_ref()
                .map(|(_, value)| value.clone())
                .unwrap_or_default();
            vec![
                "sh".to_string(),
                "-c".to_string(),
                format!("{value} \"$@\""),
                value,
                path(target),
            ]
        }
        Editor::System => vec![opener, path(target)],
    };
    Launch {
        argv,
        terminal: editor == Editor::Editor,
    }
}

/// What to tell a person whose folder Obsidian does not know.
fn recommendation(vault: &Path, ctx: &CommandContext<'_>) -> Vec<String> {
    let dot = vault.join(".obsidian");
    let ignored = ctx.runner.run(
        &[
            "git".to_string(),
            "check-ignore".to_string(),
            "-q".to_string(),
            ".obsidian/".to_string(),
        ],
        vault,
    );
    let git_line = match ignored.code {
        Some(0) => format!("{} is gitignored here.", dot.display()),
        Some(1) => format!(
            "{} is not gitignored here; add `.obsidian/` to .gitignore to keep it out of commits.",
            dot.display()
        ),
        _ => format!("{} would sit outside any git repository.", dot.display()),
    };
    vec![
        format!(
            "Obsidian does not know {} as a vault. skillwright does not register vaults.",
            vault.display()
        ),
        format!(
            "To add it: in Obsidian choose \"Open folder as vault\" and pick {}.",
            vault.display()
        ),
        format!(
            "Obsidian keeps its settings for the vault in {}.",
            dot.display()
        ),
        git_line,
    ]
}

fn flush_notes(notes: &mut Vec<String>, quiet: bool, err: &mut dyn Write) {
    for note in notes.drain(..) {
        if !quiet {
            let _ = writeln!(err, "{note}");
        }
    }
}

/// Quote an argument for display when it would not read as one word.
fn shell_word(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=?%~@+,".contains(c))
    {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

fn report(
    flags: &AuthorFlags,
    editor: Editor,
    target: &Path,
    launch: &Launch,
    ctx: &mut CommandContext<'_>,
) {
    if flags.json {
        let doc = serde_json::json!({
            "editor": editor.as_str(),
            "path": target.to_string_lossy(),
            "command": launch.argv,
            "launched": !flags.dry_run,
        });
        let _ = writeln!(ctx.out, "{doc}");
        return;
    }
    let command: Vec<String> = launch.argv.iter().map(|a| shell_word(a)).collect();
    for line in plan_flow(&[
        ("editor", vec![editor.as_str().to_string()]),
        ("path", vec![target.display().to_string()]),
        ("run", vec![command.join(" ")]),
    ]) {
        let _ = writeln!(ctx.out, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::obsidian::{open_uri, owning_vault, parse_vaults, percent_encode};
    use super::*;

    #[test]
    fn percent_encode_matches_encode_uri_component_on_paths() {
        assert_eq!(
            percent_encode("/Users/me/My Vault/a+b.md"),
            "%2FUsers%2Fme%2FMy%20Vault%2Fa%2Bb.md"
        );
        assert_eq!(percent_encode("é"), "%C3%A9");
    }

    #[test]
    fn parse_reads_every_vault_path() {
        let json = r#"{"vaults":{"0123456789abcdef":{"path":"/a","ts":1,"open":true},"fedcba9876543210":{"path":"/b/c","ts":2}}}"#;
        let mut vaults = parse_vaults(json).unwrap();
        vaults.sort();
        assert_eq!(vaults, vec![PathBuf::from("/a"), PathBuf::from("/b/c")]);
    }

    #[test]
    fn parse_refuses_the_wrong_shape() {
        assert!(parse_vaults("not json").is_err());
        assert!(parse_vaults(r#"{"vaults":[]}"#).is_err());
        assert!(parse_vaults(r#"{"vaults":{"x":{"ts":1}}}"#).is_err());
        assert!(parse_vaults("{}").is_err());
    }

    #[test]
    fn owning_vault_matches_whole_components_and_prefers_the_innermost() {
        let vaults = vec![PathBuf::from("/notes"), PathBuf::from("/notes/inner")];
        assert_eq!(
            owning_vault(&vaults, Path::new("/notes/inner/a.md")),
            Some((
                PathBuf::from("/notes/inner"),
                PathBuf::from("/notes/inner/a.md")
            ))
        );
        assert_eq!(
            owning_vault(&vaults, Path::new("/notes")),
            Some((PathBuf::from("/notes"), PathBuf::from("/notes")))
        );
        assert_eq!(owning_vault(&vaults, Path::new("/notes-other")), None);
    }

    #[cfg(unix)]
    #[test]
    fn owning_vault_keeps_the_stored_path_of_a_symlinked_vault() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir_all(real.join("skills")).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let target = real.join("skills").canonicalize().unwrap();
        let (vault, path) = owning_vault(std::slice::from_ref(&link), &target).unwrap();
        assert_eq!(vault, link);
        assert_eq!(path, link.join("skills"));
        assert!(open_uri(&path).ends_with("link%2Fskills"));
    }

    #[test]
    fn find_on_path_needs_an_executable_file() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("code");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let path_var = dir.path().to_string_lossy().into_owned();
        #[cfg(unix)]
        {
            assert_eq!(find_on_path("code", Some(&path_var)), None);
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(find_on_path("code", Some(&path_var)), Some(bin.clone()));
        assert_eq!(find_on_path("code", None), None);
    }

    #[test]
    fn probe_finds_obsidian_by_its_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let facts = |os| EnvFacts {
            os,
            vars: [
                ("HOME".to_string(), home.to_string_lossy().into_owned()),
                ("EDITOR".to_string(), "vim".to_string()),
                ("VISUAL".to_string(), " ".to_string()),
            ]
            .into_iter()
            .collect(),
        };
        let mac = probe(&facts(Os::Mac), false);
        assert_eq!(mac.obsidian_config, None);
        assert!(!mac.code);
        assert_eq!(
            mac.terminal_editor,
            Some(("EDITOR".to_string(), "vim".to_string()))
        );
        // Off a terminal the terminal editor is not offered.
        assert_eq!(mac.available(), vec![Editor::System]);
        assert_eq!(
            probe(&facts(Os::Mac), true).available(),
            vec![Editor::Editor, Editor::System]
        );

        let mac_json = home.join("Library/Application Support/obsidian/obsidian.json");
        std::fs::create_dir_all(mac_json.parent().unwrap()).unwrap();
        std::fs::write(&mac_json, "{}").unwrap();
        assert_eq!(
            probe(&facts(Os::Mac), false).obsidian_config,
            Some(mac_json)
        );

        assert_eq!(probe(&facts(Os::Linux), false).obsidian_config, None);
        let flatpak = home.join(".var/app/md.obsidian.Obsidian/config/obsidian/obsidian.json");
        std::fs::create_dir_all(flatpak.parent().unwrap()).unwrap();
        std::fs::write(&flatpak, "{}").unwrap();
        assert_eq!(
            probe(&facts(Os::Linux), false).obsidian_config,
            Some(flatpak)
        );
        let native = home.join(".config/obsidian/obsidian.json");
        std::fs::create_dir_all(native.parent().unwrap()).unwrap();
        std::fs::write(&native, "{}").unwrap();
        assert_eq!(
            probe(&facts(Os::Linux), false).obsidian_config,
            Some(native)
        );
    }

    #[test]
    fn ask_cancels_at_end_of_input_and_defaults_on_an_empty_line() {
        let options = ["a".to_string(), "b".to_string()];
        let mut err = Vec::new();
        assert_eq!(ask("q", &options, 0, &mut &b""[..], &mut err), None);
        assert_eq!(ask("q", &options, 1, &mut &b"\n"[..], &mut err), Some(1));
        assert_eq!(ask("q", &options, 0, &mut &b"2\n"[..], &mut err), Some(1));
        assert_eq!(ask("q", &options, 0, &mut &b"3\n"[..], &mut err), None);
    }

    #[test]
    fn plan_launch_builds_each_command() {
        let probe = Probe {
            os: Os::Mac,
            interactive: true,
            obsidian_config: None,
            code: true,
            terminal_editor: Some((
                "VISUAL".to_string(),
                r#""/Applications/Sublime Text.app/bin/subl" -w"#.to_string(),
            )),
        };
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("a b.md");
        std::fs::write(&note, "").unwrap();
        let vault = dir.path();
        let s = |p: &Path| p.to_string_lossy().into_owned();

        let launch = plan_launch(Editor::Code, &probe, vault, &note);
        assert_eq!(
            launch.argv,
            vec!["code".to_string(), s(vault), "--goto".to_string(), s(&note)]
        );
        let launch = plan_launch(Editor::Code, &probe, vault, vault);
        assert_eq!(launch.argv, vec!["code".to_string(), s(vault)]);

        let launch = plan_launch(Editor::Editor, &probe, vault, &note);
        assert!(launch.terminal);
        assert_eq!(
            launch.argv,
            vec![
                "sh".to_string(),
                "-c".to_string(),
                r#""/Applications/Sublime Text.app/bin/subl" -w "$@""#.to_string(),
                r#""/Applications/Sublime Text.app/bin/subl" -w"#.to_string(),
                s(&note)
            ]
        );

        let launch = plan_launch(Editor::Obsidian, &probe, vault, &note);
        assert_eq!(launch.argv[0], "open");
        assert!(launch.argv[1].starts_with("obsidian://open?path=%2F"));
        assert!(launch.argv[1].ends_with("a%20b.md"));
    }

    #[cfg(unix)]
    #[test]
    fn the_editor_command_runs_a_quoted_program_path() {
        // A program whose path has a space, named in quotes with an argument.
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("my editor");
        let out = dir.path().join("out");
        std::fs::write(
            &bin,
            format!("#!/bin/sh\nprintf '%s|' \"$@\" > '{}'\n", out.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let probe = Probe {
            os: Os::Linux,
            interactive: true,
            obsidian_config: None,
            code: false,
            terminal_editor: Some(("EDITOR".to_string(), format!("'{}' -w", bin.display()))),
        };
        let note = dir.path().join("a b.md");
        let launch = plan_launch(Editor::Editor, &probe, dir.path(), &note);
        let status = Command::new(&launch.argv[0])
            .args(&launch.argv[1..])
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            format!("-w|{}|", note.display())
        );
    }
}
