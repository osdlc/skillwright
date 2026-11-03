//! `skillwright author [path]` — open the vault, or a folder or note inside
//! it, in an editor. Experimental.
//!
//! The editor comes from `--with`, else the machine file's `author.editor`,
//! else a one-time question (saved to the machine file). Off a terminal there
//! is no question: the first available editor wins and stderr says which.
//!
//! Obsidian only opens folders it already knows as vaults. This verb reads
//! Obsidian's `obsidian.json` to find out, and never writes it: an unknown
//! folder gets a recommendation and, on a terminal, an offer to open
//! Obsidian's own vault picker.
//!
//! Everything that touches the machine — what is installed, the terminal,
//! launching, asking — goes through [`AuthorHost`], so tests never start an
//! app.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::commands::harness::cli_runner::{ProcessRunner, Runner};
use crate::config::{
    Editor, machine_author_editor, machine_config_path, resolve_against_cwd,
    save_machine_author_editor,
};
use crate::ctx::CommandContext;
use crate::exit::{EXIT_BLOCKED, EXIT_OK, blocked, usage_error};
use crate::prompt::{is_interactive, plan_flow};
use crate::resolve::{CommonFlags, resolve_run};

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

/// The operating system, as far as finding editors goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// macOS.
    Mac,
    /// Linux and the other freedesktop systems.
    Linux,
    /// Windows.
    Windows,
    /// Anything else: no known opener.
    Other,
}

impl Os {
    /// The OS this binary was built for.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(windows) {
            Self::Windows
        } else if cfg!(unix) {
            Self::Linux
        } else {
            Self::Other
        }
    }

    /// The command that opens a file, folder or URI in its default app.
    pub fn opener(self) -> Option<Vec<String>> {
        let argv: &[&str] = match self {
            Self::Mac => &["open"],
            Self::Linux => &["xdg-open"],
            // `start`'s first quoted argument is a window title.
            Self::Windows => &["cmd", "/C", "start", ""],
            Self::Other => return None,
        };
        Some(argv.iter().map(|s| (*s).to_string()).collect())
    }
}

/// The facts [`probe`] reads: the OS, the environment variables, and the
/// system-wide app folders. Separate so a test can fake all three.
pub struct EnvFacts {
    /// The OS.
    pub os: Os,
    /// The environment variables that matter: `HOME`, `PATH`, `VISUAL`,
    /// `EDITOR`, `XDG_CONFIG_HOME`, `APPDATA`, `LOCALAPPDATA`.
    pub vars: BTreeMap<String, String>,
    /// System-wide app folders: `/Applications` on macOS,
    /// `/var/lib/flatpak/app` on Linux.
    pub app_dirs: Vec<PathBuf>,
}

impl EnvFacts {
    /// The facts of this process.
    pub fn from_process() -> Self {
        let os = Os::current();
        let vars = [
            "HOME",
            "PATH",
            "VISUAL",
            "EDITOR",
            "XDG_CONFIG_HOME",
            "APPDATA",
            "LOCALAPPDATA",
        ]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok().map(|v| (name.to_string(), v)))
        .collect();
        let app_dirs = match os {
            Os::Mac => vec![PathBuf::from("/Applications")],
            Os::Linux => vec![PathBuf::from("/var/lib/flatpak/app")],
            _ => Vec::new(),
        };
        Self { os, vars, app_dirs }
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
    /// Obsidian is installed.
    pub obsidian_installed: bool,
    /// Where Obsidian keeps its vault list, `obsidian.json`.
    pub obsidian_config: Option<PathBuf>,
    /// `code` is on `PATH`.
    pub code: bool,
    /// The terminal editor: the variable that named it and its value.
    pub terminal_editor: Option<(String, String)>,
}

impl Probe {
    /// True when `editor` can be launched here.
    pub fn has(&self, editor: Editor) -> bool {
        match editor {
            Editor::Obsidian => self.obsidian_installed && self.os.opener().is_some(),
            Editor::Code => self.code,
            Editor::Editor => self.terminal_editor.is_some(),
            Editor::System => self.os.opener().is_some(),
        }
    }

    /// The available editors, in the order they are offered.
    pub fn available(&self) -> Vec<Editor> {
        ORDER.into_iter().filter(|e| self.has(*e)).collect()
    }

    /// How `editor` reads in a question or a note.
    pub fn describe(&self, editor: Editor) -> String {
        match editor {
            Editor::Obsidian => "Obsidian".to_string(),
            Editor::Code => "VS Code".to_string(),
            Editor::Editor => match &self.terminal_editor {
                Some((var, value)) => format!("${var} ({value})"),
                None => "$VISUAL / $EDITOR".to_string(),
            },
            Editor::System => match self.os {
                Os::Mac => "the system default (open)".to_string(),
                Os::Linux => "the system default (xdg-open)".to_string(),
                Os::Windows => "the system default (start)".to_string(),
                Os::Other => "the system default".to_string(),
            },
        }
    }
}

/// Read [`Probe`] from `facts`. Pure: it looks at the filesystem and
/// nothing else.
pub fn probe(facts: &EnvFacts, interactive: bool) -> Probe {
    let home = facts.home();
    let on_path = |name: &str| find_on_path(name, facts.var("PATH"), facts.os).is_some();
    let obsidian_installed = match facts.os {
        Os::Mac => facts
            .app_dirs
            .iter()
            .cloned()
            .chain(home.iter().map(|h| h.join("Applications")))
            .any(|dir| dir.join("Obsidian.app").exists()),
        Os::Linux => {
            on_path("obsidian")
                || facts
                    .app_dirs
                    .iter()
                    .cloned()
                    .chain(home.iter().map(|h| h.join(".local/share/flatpak/app")))
                    .any(|dir| dir.join(FLATPAK_ID).exists())
        }
        Os::Windows => facts
            .var("LOCALAPPDATA")
            .is_some_and(|l| Path::new(l).join("Programs").join("Obsidian").exists()),
        Os::Other => false,
    };
    let terminal_editor = ["VISUAL", "EDITOR"]
        .into_iter()
        .find_map(|var| facts.var(var).map(|v| (var.to_string(), v.to_string())));
    Probe {
        os: facts.os,
        interactive,
        obsidian_installed,
        obsidian_config: obsidian_config_path(facts),
        code: on_path("code"),
        terminal_editor,
    }
}

const FLATPAK_ID: &str = "md.obsidian.Obsidian";

/// Where Obsidian keeps `obsidian.json` on this OS.
pub fn obsidian_config_path(facts: &EnvFacts) -> Option<PathBuf> {
    let home = facts.home();
    match facts.os {
        Os::Mac => home.map(|h| h.join("Library/Application Support/obsidian/obsidian.json")),
        Os::Linux => {
            let config = facts
                .var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join(".config")))?;
            let native = config.join("obsidian/obsidian.json");
            let flatpak = home.map(|h| {
                h.join(".var/app")
                    .join(FLATPAK_ID)
                    .join("config/obsidian/obsidian.json")
            });
            match flatpak {
                Some(f) if !native.exists() && f.exists() => Some(f),
                _ => Some(native),
            }
        }
        Os::Windows => facts
            .var("APPDATA")
            .map(|a| Path::new(a).join("obsidian").join("obsidian.json")),
        Os::Other => None,
    }
}

/// The first executable called `name` in `path_var`'s directories.
pub fn find_on_path(name: &str, path_var: Option<&str>, os: Os) -> Option<PathBuf> {
    let separator = if os == Os::Windows { ';' } else { ':' };
    let names: Vec<String> = if os == Os::Windows {
        ["", ".exe", ".cmd", ".bat"]
            .iter()
            .map(|ext| format!("{name}{ext}"))
            .collect()
    } else {
        vec![name.to_string()]
    };
    path_var?
        .split(separator)
        .filter(|dir| !dir.is_empty())
        .flat_map(|dir| names.iter().map(move |n| Path::new(dir).join(n)))
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

/// Whether Obsidian knows a folder as a vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Registration {
    /// This Obsidian vault is the folder or one of its ancestors.
    Registered {
        /// The vault's folder, as `obsidian.json` names it.
        vault: PathBuf,
    },
    /// `obsidian.json` was read and no vault holds the folder.
    NotRegistered,
    /// `obsidian.json` could not be read; treated as not registered.
    Unknown {
        /// Why.
        reason: String,
    },
}

/// The vault folders `obsidian.json` lists. Its shape is
/// `{"vaults": {"<id>": {"path": "...", ...}}}`.
///
/// # Errors
///
/// When the text is not JSON or not that shape.
pub fn parse_obsidian_vaults(json: &str) -> Result<Vec<PathBuf>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let vaults = value
        .get("vaults")
        .and_then(serde_json::Value::as_object)
        .ok_or("no `vaults` object")?;
    vaults
        .values()
        .map(|entry| {
            entry
                .get("path")
                .and_then(serde_json::Value::as_str)
                .map(PathBuf::from)
                .ok_or_else(|| "a vault entry has no `path` string".to_string())
        })
        .collect()
}

/// The innermost vault that is `target` or an ancestor of it, comparing
/// canonical paths. A vault path that does not exist is compared as written.
pub fn owning_vault(vaults: &[PathBuf], target: &Path) -> Option<PathBuf> {
    let target = canonical(target);
    vaults
        .iter()
        .filter(|vault| target.starts_with(canonical(vault)))
        .max_by_key(|vault| canonical(vault).components().count())
        .cloned()
}

/// Read `config` (Obsidian's `obsidian.json`) and say whether `target` is
/// inside a registered vault. Never fails: a problem is
/// [`Registration::Unknown`].
pub fn registration(config: Option<&Path>, target: &Path) -> Registration {
    let Some(config) = config else {
        return Registration::Unknown {
            reason: "no known location for obsidian.json on this system".to_string(),
        };
    };
    let text = match std::fs::read_to_string(config) {
        Ok(text) => text,
        Err(err) => {
            return Registration::Unknown {
                reason: format!("cannot read {}: {err}", config.display()),
            };
        }
    };
    match parse_obsidian_vaults(&text) {
        Ok(vaults) => match owning_vault(&vaults, target) {
            Some(vault) => Registration::Registered { vault },
            None => Registration::NotRegistered,
        },
        Err(err) => Registration::Unknown {
            reason: format!("unexpected content in {}: {err}", config.display()),
        },
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// `text` percent-encoded the way JavaScript's `encodeURIComponent` does it,
/// minus its `!*'()`: everything but `A-Z a-z 0-9 - _ . ~` is encoded.
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `obsidian://open?path=<target>`, the link that opens a folder or note in
/// the vault holding it.
pub fn obsidian_open_uri(target: &Path) -> String {
    format!(
        "obsidian://open?path={}",
        percent_encode(&target.to_string_lossy())
    )
}

/// Obsidian's vault picker, where a person adds a folder as a vault.
pub const OBSIDIAN_PICKER_URI: &str = "obsidian://choose-vault";

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
    /// means the person gave no usable answer.
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
        let facts = EnvFacts::from_process();
        let mut probe = probe(&facts, is_interactive());
        // An Obsidian.app outside /Applications and ~/Applications.
        if facts.os == Os::Mac && !probe.obsidian_installed {
            let found = ProcessRunner.run(
                &[
                    "mdfind".to_string(),
                    "kMDItemCFBundleIdentifier == 'md.obsidian'".to_string(),
                ],
                Path::new("/"),
            );
            probe.obsidian_installed = found.ok() && !found.stdout.trim().is_empty();
        }
        probe
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
        let _ = writeln!(err, "{question}");
        for (index, option) in options.iter().enumerate() {
            let _ = writeln!(err, "  {}) {option}", index + 1);
        }
        let _ = write!(err, "Choose [{}]: ", default + 1);
        let _ = err.flush();
        let mut answer = String::new();
        std::io::stdin().lock().read_line(&mut answer).ok()?;
        let answer = answer.trim();
        if answer.is_empty() {
            return Some(default);
        }
        let number: usize = answer.parse().ok()?;
        (1..=options.len()).contains(&number).then(|| number - 1)
    }
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

    let mut argv_override: Option<Vec<String>> = None;
    if editor == Editor::Obsidian {
        match registration(probe.obsidian_config.as_deref(), &target) {
            Registration::Registered { vault: owner } => {
                // Said only when Obsidian's vault is not this vault itself.
                if canonical(&owner) != vault {
                    notes.push(format!(
                        "{} opens inside the Obsidian vault {}",
                        target.display(),
                        owner.display()
                    ));
                }
            }
            other => {
                flush_notes(&mut notes, flags.quiet, ctx.err);
                if let Registration::Unknown { reason } = &other {
                    let _ = writeln!(ctx.err, "{reason}; treating the folder as not registered.");
                }
                for line in recommendation(&vault, ctx) {
                    let _ = writeln!(ctx.err, "{line}");
                }
                let choice = if ask {
                    host.choose(
                        "What now?",
                        &[
                            "Open the vault picker now".to_string(),
                            "Use another editor this time".to_string(),
                            "Cancel".to_string(),
                        ],
                        0,
                        ctx.err,
                    )
                } else {
                    Some(1)
                };
                match choice {
                    Some(0) => {
                        let mut argv = probe.os.opener().unwrap_or_default();
                        argv.push(OBSIDIAN_PICKER_URI.to_string());
                        argv_override = Some(argv);
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

    let launch = match argv_override {
        Some(argv) => Launch {
            argv,
            terminal: false,
        },
        None => plan_launch(editor, &probe, &vault, &target),
    };
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

/// `path`, absolute or relative to `cwd`, else relative to the vault; it must
/// exist and sit inside the vault. Returned canonical.
fn resolve_target(path: Option<&str>, cwd: &Path, vault: &Path) -> Result<PathBuf, String> {
    let Some(path) = path else {
        return Ok(vault.to_path_buf());
    };
    let from_cwd = resolve_against_cwd(path, cwd);
    let found = if from_cwd.exists() {
        from_cwd
    } else if Path::new(path).is_relative() && vault.join(path).exists() {
        vault.join(path)
    } else {
        return Err(format!("not found: {path}"));
    };
    let found = canonical(&found);
    if found.starts_with(vault) {
        Ok(found)
    } else {
        Err(format!(
            "{} is not inside the vault {}",
            found.display(),
            vault.display()
        ))
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
                unavailable_reason(editor, probe)
            ),
        ));
    }
    match machine_author_editor() {
        Ok(Some(editor)) if probe.has(editor) => return Ok(editor),
        Ok(Some(editor)) => notes.push(format!(
            "the saved editor `{}` is not available here: {}",
            editor.as_str(),
            unavailable_reason(editor, probe)
        )),
        Ok(None) => {}
        Err(message) => notes.push(format!("ignoring the saved editor: {message}")),
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
    match save_machine_author_editor(editor) {
        Ok(path) => notes.push(format!(
            "saved author.editor: {} in {}",
            editor.as_str(),
            path.display()
        )),
        Err(message) => notes.push(format!(
            "could not save the choice to {}: {message}",
            machine_config_path().display()
        )),
    }
    Ok(editor)
}

fn unavailable_reason(editor: Editor, probe: &Probe) -> &'static str {
    match editor {
        Editor::Obsidian if probe.os.opener().is_none() => "no system opener on this platform",
        Editor::Obsidian => "Obsidian is not installed",
        Editor::Code => "`code` is not on PATH",
        Editor::Editor => "neither $VISUAL nor $EDITOR is set",
        Editor::System => "no system opener on this platform",
    }
}

/// The process that opens `target` in `editor`.
pub fn plan_launch(editor: Editor, probe: &Probe, vault: &Path, target: &Path) -> Launch {
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let mut terminal = false;
    let argv = match editor {
        Editor::Obsidian => {
            let mut argv = probe.os.opener().unwrap_or_default();
            argv.push(obsidian_open_uri(target));
            argv
        }
        Editor::Code => {
            let mut argv = vec!["code".to_string(), path(vault)];
            if target.is_file() {
                argv.push("--goto".to_string());
                argv.push(path(target));
            }
            argv
        }
        Editor::Editor => {
            terminal = true;
            let command = probe
                .terminal_editor
                .as_ref()
                .map(|(_, value)| value.as_str())
                .unwrap_or_default();
            let mut argv: Vec<String> = command.split_whitespace().map(str::to_string).collect();
            argv.push(path(target));
            argv
        }
        Editor::System => {
            let mut argv = probe.os.opener().unwrap_or_default();
            argv.push(path(target));
            argv
        }
    };
    Launch { argv, terminal }
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
        let mut vaults = parse_obsidian_vaults(json).unwrap();
        vaults.sort();
        assert_eq!(vaults, vec![PathBuf::from("/a"), PathBuf::from("/b/c")]);
    }

    #[test]
    fn parse_refuses_the_wrong_shape() {
        assert!(parse_obsidian_vaults("not json").is_err());
        assert!(parse_obsidian_vaults(r#"{"vaults":[]}"#).is_err());
        assert!(parse_obsidian_vaults(r#"{"vaults":{"x":{"ts":1}}}"#).is_err());
        assert!(parse_obsidian_vaults("{}").is_err());
    }

    #[test]
    fn owning_vault_matches_whole_components_and_prefers_the_innermost() {
        let vaults = vec![PathBuf::from("/notes"), PathBuf::from("/notes/inner")];
        assert_eq!(
            owning_vault(&vaults, Path::new("/notes/inner/a.md")),
            Some(PathBuf::from("/notes/inner"))
        );
        assert_eq!(
            owning_vault(&vaults, Path::new("/notes")),
            Some(PathBuf::from("/notes"))
        );
        assert_eq!(owning_vault(&vaults, Path::new("/notes-other")), None);
    }

    #[test]
    fn find_on_path_needs_an_executable_file() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("code");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let path_var = dir.path().to_string_lossy().into_owned();
        #[cfg(unix)]
        {
            assert_eq!(find_on_path("code", Some(&path_var), Os::Linux), None);
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(
            find_on_path("code", Some(&path_var), Os::Linux),
            Some(bin.clone())
        );
        assert_eq!(find_on_path("code", None, Os::Linux), None);
    }

    #[test]
    fn probe_reads_a_fake_home() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let apps = dir.path().join("apps");
        std::fs::create_dir_all(home.join(".config/obsidian")).unwrap();
        std::fs::create_dir_all(apps.join("Obsidian.app")).unwrap();
        let facts = |os| EnvFacts {
            os,
            vars: [
                ("HOME".to_string(), home.to_string_lossy().into_owned()),
                ("EDITOR".to_string(), "vim".to_string()),
                ("VISUAL".to_string(), " ".to_string()),
            ]
            .into_iter()
            .collect(),
            app_dirs: vec![apps.clone()],
        };

        let mac = probe(&facts(Os::Mac), false);
        assert!(mac.obsidian_installed);
        assert_eq!(
            mac.obsidian_config,
            Some(home.join("Library/Application Support/obsidian/obsidian.json"))
        );
        assert!(!mac.code);
        assert_eq!(
            mac.terminal_editor,
            Some(("EDITOR".to_string(), "vim".to_string()))
        );
        assert_eq!(
            mac.available(),
            vec![Editor::Obsidian, Editor::Editor, Editor::System]
        );

        let linux = probe(&facts(Os::Linux), false);
        assert!(!linux.obsidian_installed);
        assert_eq!(
            linux.obsidian_config,
            Some(home.join(".config/obsidian/obsidian.json"))
        );
    }

    #[test]
    fn plan_launch_builds_each_command() {
        let probe = Probe {
            os: Os::Mac,
            interactive: false,
            obsidian_installed: true,
            obsidian_config: None,
            code: true,
            terminal_editor: Some(("VISUAL".to_string(), "nvim -p".to_string())),
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
            vec!["nvim".to_string(), "-p".to_string(), s(&note)]
        );

        let launch = plan_launch(Editor::Obsidian, &probe, vault, &note);
        assert_eq!(launch.argv[0], "open");
        assert!(launch.argv[1].starts_with("obsidian://open?path=%2F"));
        assert!(launch.argv[1].ends_with("a%20b.md"));
    }
}
