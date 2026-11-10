//! What more than one integration binary here needs.

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
