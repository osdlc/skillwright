//! Skill names on Cursor and Pi (spec section 8 rule 6), for the `tools`
//! plugin's `review` skill, built through the core's public API.

use std::collections::BTreeSet;
use std::path::Path;

use tradecraft_core::{CoreError, ExportMode, MemVfs, Vfs, export_skills, load_vault};

const SKILL: &str = "---\ndescription: Use it.\n---\n\nBody.\n";

/// The packs of marketplace `acme`, by harness.
const PACKS: [(&str, &str); 4] = [
    ("claude", "dist/acme/.claude-plugin/packs"),
    ("codex", "dist/acme/.agents/packs"),
    ("cursor", "dist/acme/.cursor-plugin/packs"),
    ("pi", "dist/acme/.pi/packs"),
];

fn put(vfs: &MemVfs, rel: &str, text: &str) {
    let path = Path::new("/v").join(rel);
    vfs.create_dir_all(path.parent().unwrap()).unwrap();
    vfs.write_file(&path, text.as_bytes()).unwrap();
}

/// A vault at `/v` with marketplace `acme` (its note's frontmatter body is
/// `market`) and no plugins.
fn vault(market: &str) -> MemVfs {
    let vfs = MemVfs::new();
    put(&vfs, "skillwright.md", "---\n---\n");
    put(
        &vfs,
        "marketplaces/acme/acme.md",
        &format!("---\n{market}---\n"),
    );
    vfs
}

/// Plugin `plugin` with frontmatter body `note` and a skill per name.
fn plugin(vfs: &MemVfs, plugin: &str, note: &str, skills: &[&str]) {
    let dir = format!("marketplaces/acme/plugins/{plugin}");
    put(
        vfs,
        &format!("{dir}/{plugin}.md"),
        &format!("---\nversion: 1.0.0\n{note}---\n"),
    );
    for skill in skills {
        put(vfs, &format!("{dir}/skills/{skill}/{skill}.md"), SKILL);
    }
}

/// The `tools` plugin with its `review` skill, plugin frontmatter `note`.
fn tools(market: &str, note: &str) -> MemVfs {
    let vfs = vault(market);
    plugin(&vfs, "tools", note, &["review"]);
    vfs
}

/// Builds every harness, or only `harnesses` when given.
fn build_for(vfs: &dyn Vfs, harnesses: &[&str]) -> Result<(), CoreError> {
    let loaded = load_vault(vfs, Path::new("/v"), None)?;
    let mut request = loaded.export_request(&loaded.marketplaces[0], ExportMode::Build);
    if !harnesses.is_empty() {
        request.harnesses = harnesses
            .iter()
            .map(|h| (*h).to_owned())
            .collect::<BTreeSet<_>>();
    }
    export_skills(vfs, &request).map(|_| ())
}

fn build(vfs: &dyn Vfs) -> Result<(), CoreError> {
    build_for(vfs, &[])
}

fn pack(harness: &str) -> &'static str {
    PACKS.iter().find(|(h, _)| *h == harness).unwrap().1
}

fn read(vfs: &dyn Vfs, rel: &str) -> String {
    String::from_utf8(vfs.read_file(&Path::new("/v").join(rel)).unwrap())
        .unwrap_or_else(|_| panic!("{rel} is not UTF-8"))
}

/// Asserts the skill of `plugin` lands at `skills/<folder>/SKILL.md` on
/// `harness`, with `name: <folder>`.
fn assert_skill(vfs: &dyn Vfs, harness: &str, plugin: &str, folder: &str) {
    let rel = format!("{}/{plugin}/skills/{folder}/SKILL.md", pack(harness));
    assert!(
        vfs.exists(&Path::new("/v").join(&rel)),
        "{harness}: no {rel}"
    );
    let text = read(vfs, &rel);
    assert!(
        text.contains(&format!("\nname: {folder}\n")),
        "{harness}: {text}"
    );
}

#[test]
fn by_default_cursor_and_pi_prefix_the_plugin() {
    let vfs = tools("", "");
    build(&vfs).unwrap();
    assert_skill(&vfs, "cursor", "tools", "tools-review");
    assert_skill(&vfs, "pi", "tools", "tools-review");
    assert_skill(&vfs, "claude", "tools", "review");
    assert_skill(&vfs, "codex", "tools", "review");
}

#[test]
fn bare_on_the_plugin_keeps_the_plain_name() {
    let vfs = tools("", "cursor:\n  naming: bare\npi:\n  naming: bare\n");
    build(&vfs).unwrap();
    for (harness, _) in PACKS {
        assert_skill(&vfs, harness, "tools", "review");
    }
}

#[test]
fn bare_on_the_marketplace_reaches_every_plugin_on_pi() {
    let vfs = tools("pi:\n  naming: bare\n", "");
    plugin(&vfs, "ops", "", &["lint"]);
    build(&vfs).unwrap();
    assert_skill(&vfs, "pi", "tools", "review");
    assert_skill(&vfs, "pi", "ops", "lint");
    assert_skill(&vfs, "cursor", "tools", "tools-review");
    assert_skill(&vfs, "cursor", "ops", "ops-lint");
    assert_skill(&vfs, "claude", "tools", "review");
    assert_skill(&vfs, "codex", "tools", "review");
}

#[test]
fn a_plugins_prefixed_wins_over_the_marketplaces_bare() {
    let vfs = tools("pi:\n  naming: bare\n", "pi:\n  naming: prefixed\n");
    build(&vfs).unwrap();
    assert_skill(&vfs, "pi", "tools", "tools-review");
    assert_skill(&vfs, "claude", "tools", "review");
    assert_skill(&vfs, "codex", "tools", "review");
}

#[test]
fn another_naming_value_is_refused() {
    for harness in ["cursor", "pi"] {
        let vfs = tools("", &format!("{harness}:\n  naming: plain\n"));
        let err = build(&vfs).unwrap_err().to_string();
        assert!(err.contains("`bare` or `prefixed`"), "{harness}: {err}");
        assert!(err.contains("plain"), "{harness}: {err}");
    }
}

#[test]
fn naming_on_a_skill_is_refused() {
    for harness in ["cursor", "pi"] {
        let vfs = tools("", "");
        put(
            &vfs,
            "marketplaces/acme/plugins/tools/skills/review/review.md",
            &format!("---\ndescription: Use it.\n{harness}:\n  naming: bare\n---\n\nBody.\n"),
        );
        let err = build(&vfs).unwrap_err().to_string();
        assert!(err.contains("review.md"), "{harness}: {err}");
        assert!(err.contains("naming"), "{harness}: {err}");
        assert!(err.contains("on the plugin"), "{harness}: {err}");
    }
}

#[test]
fn a_prefixed_name_over_64_characters_is_refused() {
    let long_plugin = "p".repeat(32);
    let long_skill = "s".repeat(32);
    let vfs = vault("");
    plugin(&vfs, &long_plugin, "", &[&long_skill]);
    let err = build(&vfs).unwrap_err().to_string();
    assert!(
        err.contains(&format!("{long_plugin}-{long_skill}")),
        "{err}"
    );
    assert!(err.contains("64"), "{err}");
    build_for(&vfs, &["claude", "codex"]).unwrap();
}

#[test]
fn bare_skills_of_one_name_in_two_plugins_fail_that_harness() {
    let vfs = tools("pi:\n  naming: bare\n", "");
    plugin(&vfs, "ops", "", &["review"]);
    let err = build_for(&vfs, &["pi"]).unwrap_err().to_string();
    assert!(err.contains("tools") && err.contains("ops"), "{err}");
    build_for(&vfs, &["cursor"]).unwrap();
    build_for(&vfs, &["claude", "codex"]).unwrap();
}

#[test]
fn prefixed_names_that_meet_fail_cursor_and_pi() {
    let vfs = vault("");
    plugin(&vfs, "a-b", "", &["c"]);
    plugin(&vfs, "a", "", &["b-c"]);
    for harness in ["cursor", "pi"] {
        let err = build_for(&vfs, &[harness]).unwrap_err().to_string();
        assert!(err.contains("a-b-c"), "{harness}: {err}");
        assert!(
            err.contains("`a`") && err.contains("`a-b`"),
            "{harness}: {err}"
        );
    }
    build_for(&vfs, &["claude", "codex"]).unwrap();
}

#[test]
fn bare_in_the_root_file_reaches_every_plugin() {
    let vfs = tools("", "");
    put(&vfs, "skillwright.md", "---\npi: {naming: bare}\n---\n");
    build(&vfs).unwrap();
    assert_skill(&vfs, "pi", "tools", "review");
    assert_skill(&vfs, "cursor", "tools", "tools-review");
}
