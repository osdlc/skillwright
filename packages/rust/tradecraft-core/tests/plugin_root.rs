//! Plugin-root paths (spec section 4 rules 15 to 26, D9), built through the
//! core's public API. Every vault has marketplace `acme` and plugin `tools`.

use std::path::Path;

use tradecraft_core::{CoreError, ExportMode, MemVfs, Vfs, export_skills, load_vault};

const SKILL: &str = "---\ndescription: Use it.\n---\n\n";

const PLUGIN: &str = "marketplaces/acme/plugins/tools";

const AUTHOR: &str = "skills/author/author.md";
const TOOL: &str = "scripts/tool";

/// Each harness's pack of `tools` and `author`'s folder in it.
const PACKS: [(&str, &str, &str); 4] = [
    ("claude", "dist/acme/.claude-plugin/packs/tools", "author"),
    ("codex", "dist/acme/.agents/packs/tools", "author"),
    (
        "cursor",
        "dist/acme/.cursor-plugin/packs/tools",
        "tools-author",
    ),
    ("pi", "dist/acme/.pi/packs/tools", "tools-author"),
];

/// A vault at `/v` with plugin `tools`, plus `files` under the plugin folder.
fn vault(files: &[(&str, &str)]) -> MemVfs {
    let vfs = MemVfs::new();
    put(&vfs, "skillwright.md", "---\n---\n");
    put(&vfs, "marketplaces/acme/acme.md", "---\n---\n");
    put(
        &vfs,
        &format!("{PLUGIN}/tools.md"),
        "---\nversion: 1.0.0\n---\n",
    );
    for (path, text) in files {
        put(&vfs, &format!("{PLUGIN}/{path}"), text);
    }
    vfs
}

/// [`vault`] with `author` holding `body` and the shared `scripts/tool`.
fn with_author(body: &str, files: &[(&str, &str)]) -> MemVfs {
    let author = skill(body);
    let mut all = vec![(AUTHOR, author.as_str()), (TOOL, "#!/bin/sh\n")];
    all.extend_from_slice(files);
    vault(&all)
}

fn skill(body: &str) -> String {
    format!("{SKILL}{body}\n")
}

fn put(vfs: &MemVfs, rel: &str, text: &str) {
    let path = Path::new("/v").join(rel);
    vfs.create_dir_all(path.parent().unwrap()).unwrap();
    vfs.write_file(&path, text.as_bytes()).unwrap();
}

/// Builds `harnesses`, or every harness when empty.
fn build_for(vfs: &dyn Vfs, harnesses: &[&str]) -> Result<(), CoreError> {
    let loaded = load_vault(vfs, Path::new("/v"), None)?;
    let mut request = loaded.export_request(&loaded.marketplaces[0], ExportMode::Build);
    if !harnesses.is_empty() {
        request.harnesses = harnesses.iter().map(|h| (*h).to_owned()).collect();
    }
    export_skills(vfs, &request).map(|_| ())
}

fn build(vfs: &dyn Vfs) {
    build_for(vfs, &[]).unwrap();
}

fn refusal_for(vfs: &dyn Vfs, harnesses: &[&str]) -> String {
    match build_for(vfs, harnesses) {
        Ok(()) => panic!("the build succeeded"),
        Err(err) => err.to_string(),
    }
}

fn read(vfs: &dyn Vfs, rel: &str) -> String {
    String::from_utf8(vfs.read_file(&Path::new("/v").join(rel)).unwrap()).unwrap()
}

/// `author`'s file `file` in each harness's pack, by harness.
fn in_author(vfs: &dyn Vfs, file: &str) -> Vec<(&'static str, String)> {
    PACKS
        .iter()
        .map(|(harness, pack, folder)| {
            (
                *harness,
                read(vfs, &format!("{pack}/skills/{folder}/{file}")),
            )
        })
        .collect()
}

/// Asserts each harness's text holds `claude` on Claude Code and `other` on
/// the other three.
fn holds(texts: &[(&str, String)], claude: &str, other: &str) {
    for (harness, text) in texts {
        let want = if *harness == "claude" { claude } else { other };
        assert!(text.contains(want), "{harness}: no `{want}` in {text}");
    }
}

// --- The table --------------------------------------------------------------

#[test]
fn a_path_in_a_skills_prose_code_span_and_fenced_block_is_rewritten() {
    let vfs = with_author(
        "Run ${PLUGIN_ROOT}/scripts/tool now, or `${PLUGIN_ROOT}/scripts/tool -x`.\n\n\
         ```sh\n${PLUGIN_ROOT}/scripts/tool --all\n```",
        &[],
    );
    build(&vfs);
    let texts = in_author(&vfs, "SKILL.md");
    holds(
        &texts,
        "Run ${CLAUDE_PLUGIN_ROOT}/scripts/tool now, or `${CLAUDE_PLUGIN_ROOT}/scripts/tool -x`.",
        "Run ../../scripts/tool now, or `../../scripts/tool -x`.",
    );
    holds(
        &texts,
        "```sh\n${CLAUDE_PLUGIN_ROOT}/scripts/tool --all\n```",
        "```sh\n../../scripts/tool --all\n```",
    );
}

#[test]
fn a_path_in_a_skills_reference_is_written_from_the_reference() {
    let vfs = with_author(
        "See [[guide]].",
        &[(
            "skills/author/references/guide.md",
            "Run ${PLUGIN_ROOT}/scripts/tool now.\n",
        )],
    );
    build(&vfs);
    holds(
        &in_author(&vfs, "references/guide.md"),
        "${CLAUDE_PLUGIN_ROOT}/scripts/tool now",
        "../../../scripts/tool now",
    );
}

#[test]
fn a_path_in_the_shared_references_is_written_from_the_pack_root() {
    let vfs = with_author(
        "See [[style]].",
        &[(
            "references/style.md",
            "Run ${PLUGIN_ROOT}/scripts/tool now.\n",
        )],
    );
    build(&vfs);
    for (harness, pack, _) in PACKS {
        let text = read(&vfs, &format!("{pack}/references/style.md"));
        let want = if harness == "claude" {
            "${CLAUDE_PLUGIN_ROOT}/scripts/tool now"
        } else {
            "../scripts/tool now"
        };
        assert!(text.contains(want), "{harness}: {text}");
    }
}

#[test]
fn a_full_stop_after_a_path_is_kept_as_punctuation() {
    let vfs = with_author("Run ${PLUGIN_ROOT}/scripts/tool.", &[]);
    build(&vfs);
    holds(
        &in_author(&vfs, "SKILL.md"),
        "Run ${CLAUDE_PLUGIN_ROOT}/scripts/tool.\n",
        "Run ../../scripts/tool.\n",
    );
}

#[test]
fn a_path_to_a_command_is_written_for_claude_code_and_fails_elsewhere() {
    let vfs = with_author(
        "See ${PLUGIN_ROOT}/commands/deploy.md.",
        &[("commands/deploy/deploy.md", &skill("Deploy."))],
    );
    build_for(&vfs, &["claude"]).unwrap();
    let claude = read(&vfs, &format!("{}/skills/author/SKILL.md", PACKS[0].1));
    assert!(
        claude.contains("See ${CLAUDE_PLUGIN_ROOT}/commands/deploy.md."),
        "{claude}"
    );
    for harness in ["codex", "cursor", "pi"] {
        let err = refusal_for(&vfs, &[harness]);
        assert!(err.contains("plugin-root-not-shipped"), "{harness}: {err}");
        assert!(err.contains(AUTHOR), "{harness}: {err}");
        assert!(
            err.contains("${PLUGIN_ROOT}/commands/deploy.md"),
            "{harness}: {err}"
        );
    }
}

#[test]
fn an_unbraced_variable_ships_as_written() {
    let vfs = with_author("Run `$PLUGIN_ROOT/x` and $PLUGIN_ROOT/x.", &[]);
    build(&vfs);
    holds(
        &in_author(&vfs, "SKILL.md"),
        "Run `$PLUGIN_ROOT/x` and $PLUGIN_ROOT/x.",
        "Run `$PLUGIN_ROOT/x` and $PLUGIN_ROOT/x.",
    );
}

// --- Also -------------------------------------------------------------------

#[test]
fn a_path_into_another_skill_follows_its_folder() {
    let vfs = with_author(
        "Run ${PLUGIN_ROOT}/skills/setup/scripts/run.sh.",
        &[
            ("skills/setup/setup.md", &skill("Set up.")),
            ("skills/setup/scripts/run.sh", "echo\n"),
        ],
    );
    build(&vfs);
    let texts = in_author(&vfs, "SKILL.md");
    let want = [
        "${CLAUDE_PLUGIN_ROOT}/skills/setup/scripts/run.sh.",
        "../setup/scripts/run.sh.",
        "../tools-setup/scripts/run.sh.",
        "../tools-setup/scripts/run.sh.",
    ];
    for ((harness, text), want) in texts.iter().zip(want) {
        assert!(text.contains(&format!("Run {want}")), "{harness}: {text}");
    }
}

#[test]
fn an_extension_is_part_of_the_path() {
    let vfs = with_author("Run `${PLUGIN_ROOT}/scripts/tool.js --check`.", &[]);
    let err = refusal_for(&vfs, &[]);
    assert!(err.contains("plugin-root-not-shipped"), "{err}");
    assert!(err.contains("scripts/tool.js"), "{err}");
}

#[test]
fn a_path_naming_nothing_shipped_fails_naming_the_file_and_the_reference() {
    let vfs = with_author("Run ${PLUGIN_ROOT}/scripts/missing now.", &[]);
    let err = refusal_for(&vfs, &[]);
    assert!(err.contains("plugin-root-not-shipped"), "{err}");
    assert!(err.contains(AUTHOR), "{err}");
    assert!(err.contains("`${PLUGIN_ROOT}/scripts/missing`"), "{err}");
}

#[test]
fn a_lone_plugin_root_in_markdown_is_an_error() {
    let vfs = with_author("Run from ${PLUGIN_ROOT} now.", &[]);
    let err = refusal_for(&vfs, &[]);
    assert!(err.contains("plugin-root-no-path"), "{err}");
    assert!(err.contains(AUTHOR), "{err}");
}

#[test]
fn claude_code_s_variable_in_a_note_or_a_code_span_is_an_error() {
    for body in [
        "Run ${CLAUDE_PLUGIN_ROOT}/scripts/tool.",
        "Run `${CLAUDE_PLUGIN_ROOT}/scripts/tool`.",
    ] {
        let vfs = with_author(body, &[]);
        let err = refusal_for(&vfs, &[]);
        assert!(err.contains("claude-plugin-root"), "{err}");
        assert!(err.contains(AUTHOR), "{err}");
        assert!(err.contains("write `${PLUGIN_ROOT}`"), "{err}");
    }
}

#[test]
fn claude_code_s_variable_in_docs_or_a_plugin_note_s_body_is_not_checked() {
    let vfs = with_author(
        "Use it.",
        &[
            (
                "docs/notes.md",
                "Run ${CLAUDE_PLUGIN_ROOT}/x and ${PLUGIN_ROOT}.\n",
            ),
            (
                "tools.md",
                "---\nversion: 1.0.0\n---\n\nRun ${CLAUDE_PLUGIN_ROOT}/x and ${PLUGIN_ROOT}.\n",
            ),
        ],
    );
    build(&vfs);
}

#[test]
fn a_plain_relative_path_ships_as_written_and_is_not_checked() {
    let vfs = with_author("Run scripts/x.py now.", &[]);
    build(&vfs);
    holds(
        &in_author(&vfs, "SKILL.md"),
        "Run scripts/x.py now.",
        "Run scripts/x.py now.",
    );
}

#[test]
fn an_embedded_path_is_written_from_the_embedding_file() {
    let vfs = with_author(
        "![[snippet]]",
        &[(
            "references/snippet.md",
            "Run ${PLUGIN_ROOT}/scripts/tool now.\n",
        )],
    );
    build(&vfs);
    let texts = in_author(&vfs, "SKILL.md");
    holds(
        &texts,
        "Run ${CLAUDE_PLUGIN_ROOT}/scripts/tool now.",
        "Run ../../scripts/tool now.",
    );
    // The shared copy is written from where it lands.
    let codex = read(&vfs, &format!("{}/references/snippet.md", PACKS[1].1));
    assert!(codex.contains("Run ../scripts/tool now."), "{codex}");
}

#[test]
fn a_script_holding_the_variable_ships_byte_for_byte() {
    let script = "#!/bin/sh\n${PLUGIN_ROOT}/scripts/x ${PLUGIN_ROOT} ${CLAUDE_PLUGIN_ROOT}\n";
    let vfs = with_author("Use it.", &[("scripts/run", script)]);
    build(&vfs);
    for (_, pack, _) in PACKS {
        assert_eq!(read(&vfs, &format!("{pack}/scripts/run")), script);
    }
}

#[test]
fn an_inlined_file_holding_every_form_ships_unchanged() {
    let script = "${PLUGIN_ROOT}/scripts/tool\ncd ${PLUGIN_ROOT}\n${CLAUDE_PLUGIN_ROOT}/x\n";
    let vfs = with_author("![[run.sh]]", &[("skills/author/scripts/run.sh", script)]);
    build(&vfs);
    let block = format!("```bash\n{script}```");
    holds(&in_author(&vfs, "SKILL.md"), &block, &block);
}

fn hook(command: &str) -> String {
    format!(r#"{{"hooks": {{"SessionStart": [{{"hooks": [{{"command": "{command}"}}]}}]}}}}"#)
}

#[test]
fn a_hook_path_naming_nothing_shipped_fails_the_build() {
    let vfs = with_author(
        "Use it.",
        &[(
            "hooks/hooks.json",
            &hook("${PLUGIN_ROOT}/scripts/missing.sh"),
        )],
    );
    let err = refusal_for(&vfs, &[]);
    assert!(err.contains("plugin-root-not-shipped"), "{err}");
    assert!(err.contains("hooks/hooks.json"), "{err}");
    assert!(err.contains("${PLUGIN_ROOT}/scripts/missing.sh"), "{err}");
}

#[test]
fn a_lone_plugin_root_in_a_hook_is_allowed() {
    let vfs = with_author(
        "Use it.",
        &[(
            "hooks/hooks.json",
            &hook("cd ${PLUGIN_ROOT} && ${PLUGIN_ROOT}/scripts/tool"),
        )],
    );
    build(&vfs);
    let hooks = read(&vfs, &format!("{}/hooks/hooks.json", PACKS[0].1));
    assert!(
        hooks.contains("cd ${CLAUDE_PLUGIN_ROOT} && ${CLAUDE_PLUGIN_ROOT}/scripts/tool"),
        "{hooks}"
    );
}
