//! With no destination configured, `publish` writes to each harness's user
//! skill folder. These run the binary, because the home directory comes from
//! `$HOME`.

use std::path::Path;
use std::process::{Command, Output};

const NOTE: &str = "---\ndescription: Sort new bug reports.\n---\nBody.\n";

fn skillwright(home: &Path, vault: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_skillwright"))
        .args(args)
        .current_dir(vault)
        .env("HOME", home)
        .output()
        .unwrap()
}

#[test]
fn default_publish_lands_in_each_harness_user_folder() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let vault = home.join("vault");
    let skills = vault.join("plugins/review/skills");
    std::fs::create_dir_all(&skills).unwrap();
    std::fs::write(
        vault.join("skillwright.md"),
        "---\nname: v\nbuild:\n  out: dist\n---\n",
    )
    .unwrap();
    std::fs::write(skills.join("triage.md"), NOTE).unwrap();

    let published = skillwright(&home, &vault, &["publish", "--yes"]);
    assert!(
        published.status.success(),
        "{}",
        String::from_utf8_lossy(&published.stderr)
    );

    for folder in [
        ".claude/skills",
        ".agents/skills",
        ".cursor/skills",
        ".pi/agent/skills",
    ] {
        let text = std::fs::read_to_string(home.join(folder).join("triage/SKILL.md"))
            .unwrap_or_else(|err| panic!("{folder}: {err}"));
        assert!(text.contains("Sort new bug reports."), "{text}");
    }
    assert!(!home.join(".claude/skills/.claude").exists());
    assert!(!home.join(".pi/skills").exists());

    let status = skillwright(&home, &vault, &["status", "--check"]);
    assert!(
        status.status.success(),
        "{}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
}
