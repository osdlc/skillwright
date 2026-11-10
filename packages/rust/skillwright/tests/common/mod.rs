//! What more than one integration binary here needs.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use skillwright::cli::run_with;
use skillwright::commands::harness::cli_runner::Runner;
use skillwright::ctx::CommandContext;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Point the machine file at `path` (or back at its default), run `f`, then
/// restore what was there.
#[allow(dead_code)]
pub fn with_machine_env<T>(path: Option<&Path>, f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved = skillwright::config::replace_machine_config_override(path.map(Path::to_path_buf));
    let result = f();
    skillwright::config::replace_machine_config_override(saved);
    result
}

#[allow(dead_code)]
static GIT_LOCK: Mutex<()> = Mutex::new(());

/// Run one `--from-git` case at a time.
///
/// In a cargo test binary `tiered-dirs` resolves the checkout cache under its
/// own per-process scratch root, so nothing here reads or writes a real
/// `$HOME`. The cases share that cache, so they run one at a time.
#[allow(dead_code)]
pub fn with_git_cache<T>(f: impl FnOnce() -> T) -> T {
    let _guard = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

/// What one in-process invocation returned.
#[allow(dead_code)]
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// One invocation in `cwd`.
#[allow(dead_code)]
pub fn run(argv: &[&str], cwd: &Path) -> Run {
    invoke(argv, cwd, None, None)
}

/// One invocation in `cwd`, with `runner` in place of the harness CLIs.
#[allow(dead_code)]
pub fn run_with_runner(argv: &[&str], cwd: &Path, runner: &dyn Runner) -> Run {
    invoke(argv, cwd, Some(runner), None)
}

/// [`run_with_runner`], with `home` standing in for the user's home
/// directory.
#[allow(dead_code)]
pub fn run_home(argv: &[&str], cwd: &Path, home: &Path, runner: &dyn Runner) -> Run {
    invoke(argv, cwd, Some(runner), Some(home))
}

fn invoke(argv: &[&str], cwd: &Path, runner: Option<&dyn Runner>, home: Option<&Path>) -> Run {
    let owned: Vec<String> = argv.iter().map(|s| (*s).to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = {
        let mut ctx = CommandContext::new(cwd, &mut out, &mut err);
        if let Some(runner) = runner {
            ctx = ctx.with_runner(runner);
        }
        if let Some(home) = home {
            ctx = ctx.with_home(home);
        }
        run_with(&owned, &mut ctx)
    };
    Run {
        code,
        stdout: String::from_utf8(out).unwrap(),
        stderr: String::from_utf8(err).unwrap(),
    }
}

/// `vault` as a CLI argument.
#[allow(dead_code)]
pub fn vault_str(vault: &Path) -> String {
    vault.display().to_string()
}

/// Runs `git` in `repo` with no system or global config and a fixed
/// identity, so a test reads the same on any machine. Its stdout.
#[allow(dead_code)]
pub fn git(repo: &Path, args: &[&str]) -> String {
    let mut cmd = typed_git::git();
    cmd.dir(repo)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@t.invalid")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@t.invalid");
    cmd.run().unwrap()
}

/// A vault named `acme` that builds to `market` inside it, with plugins
/// `widget` (0.2.1) and `gadget` (1.0.0) under `marketplaces/acme/`.
/// `extra_config` goes under `build:`.
#[allow(dead_code)]
pub fn project(extra_config: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("skillwright.md"),
        format!("---\nname: acme\nbuild:\n  out: market\n{extra_config}---\n"),
    )
    .unwrap();
    for (plugin, version) in [("widget", "0.2.1"), ("gadget", "1.0.0")] {
        let base = root.join("marketplaces/acme/plugins").join(plugin);
        fs::create_dir_all(base.join("skills")).unwrap();
        fs::write(
            base.join(format!("{plugin}.md")),
            format!(
                "---\nname: {plugin}\ndescription: The {plugin} plugin.\nversion: {version}\n---\n"
            ),
        )
        .unwrap();
        write_skill(root, plugin, "First draft.");
    }
    dir
}

/// Write `<plugin>`'s one skill, `<plugin>-skill`, with `body`.
#[allow(dead_code)]
pub fn write_skill(root: &Path, plugin: &str, body: &str) {
    fs::write(
        root.join("marketplaces/acme/plugins")
            .join(plugin)
            .join("skills")
            .join(format!("{plugin}-skill.md")),
        format!("---\nname: {plugin}-skill\ndescription: Use {plugin}.\n---\n\n{body}\n"),
    )
    .unwrap();
}

/// The one note under `market/` that is not output: a skill that
/// `build.patterns` places at `market/extra/`.
#[allow(dead_code)]
pub const EXTRA_NOTE: &str = "market/extra/extra.md";

/// A built [`project`] whose Pi packs sit in a plain folder inside `out`
/// (`market/pi-packs`), which only `build.out` marks as output. It also holds
/// [`EXTRA_NOTE`], a real note under `market/` that no harness owns, so a test
/// can tell a hidden output path from one that was never scanned. Built fresh
/// for each test.
#[allow(dead_code)]
pub fn built_with_moved_packs() -> tempfile::TempDir {
    let dir = project("  harnesses: [claude, pi]\n  patterns:\n    skills: market/extra/*\n");
    let root = dir.path();
    let config = fs::read_to_string(root.join("skillwright.md")).unwrap();
    fs::write(
        root.join("skillwright.md"),
        config.replace(
            "name: acme\n",
            "name: acme\nversion: 1.0.0\npi: {packs: pi-packs}\n",
        ),
    )
    .unwrap();
    fs::create_dir_all(root.join("market/extra")).unwrap();
    fs::write(
        root.join(EXTRA_NOTE),
        "---\ndescription: Use extra.\n---\n\nExtra.\n",
    )
    .unwrap();
    let built = run(&["build", "--yes"], root);
    assert_eq!(built.code, 0, "{}", built.stderr);
    assert!(root.join("market/pi-packs/widget").is_dir());
    dir
}

/// Whether `said` names any path a build of [`built_with_moved_packs`] owns.
#[allow(dead_code)]
pub fn names_owned_output(said: &str) -> bool {
    said.contains("market/pi-packs") || said.contains("market/.claude-plugin")
}
