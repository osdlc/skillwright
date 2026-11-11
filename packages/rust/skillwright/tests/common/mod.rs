//! What more than one integration binary here needs.

use std::path::Path;

use skillwright::cli::run_with;
use skillwright::ctx::CommandContext;

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
    let owned: Vec<String> = argv.iter().map(|s| (*s).to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = {
        let mut ctx = CommandContext::new(cwd, &mut out, &mut err);
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
