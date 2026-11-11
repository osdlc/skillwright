//! A plugin's `hooks/hooks.json`, built into the packs whose harness carries
//! hooks, against fixtures under `tests/fixtures/hooks-file/` (spec section 4
//! rule 23).
//!
//! `TRADECRAFT_BLESS=1 cargo test -p tradecraft-core --test hooks_file`
//! rewrites the expected files from the current output.

use std::collections::BTreeSet;
use std::path::Path;

use tradecraft_core::{ExportMode, ExportReport, ExportRequest, MemVfs, Vfs, export_skills};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hooks-file");

const PLUGIN: &str = "/vault/marketplaces/house/plugins/core";

const HOOKS_REL: &str = "marketplaces/house/plugins/core/hooks/hooks.json";

const PACKS: [(&str, &str); 4] = [
    ("claude", "/vault/dist/house/.claude-plugin/packs/core"),
    ("codex", "/vault/dist/house/.agents/packs/core"),
    ("cursor", "/vault/dist/house/.cursor-plugin/packs/core"),
    ("pi", "/vault/dist/house/.pi/packs/core"),
];

fn write(vfs: &dyn Vfs, path: &str, content: &str) {
    let path = Path::new(path);
    vfs.create_dir_all(path.parent().unwrap()).unwrap();
    vfs.write_file(path, content.as_bytes()).unwrap();
}

fn text_at(vfs: &dyn Vfs, path: &str) -> String {
    String::from_utf8(vfs.read_file(Path::new(path)).unwrap()).unwrap()
}

/// A vault of marketplace `house` and plugin `core` with a `review` skill,
/// `scripts/tool`, and `hooks` as the plugin's `hooks/hooks.json`.
fn vault(hooks: &str) -> MemVfs {
    let vfs = without_hooks();
    write(&vfs, &format!("{PLUGIN}/hooks/hooks.json"), hooks);
    vfs
}

/// [`vault`] with no hooks file.
fn without_hooks() -> MemVfs {
    let vfs = MemVfs::new();
    write(&vfs, "/vault/skillwright.md", "---\n---\n");
    write(&vfs, "/vault/marketplaces/house/house.md", "---\n---\n");
    write(&vfs, &format!("{PLUGIN}/core.md"), "---\n---\n");
    write(
        &vfs,
        &format!("{PLUGIN}/skills/review/review.md"),
        "---\ndescription: Use review.\n---\n\n# review\n",
    );
    write(&vfs, &format!("{PLUGIN}/scripts/tool"), "#!/bin/sh\n");
    vfs
}

fn request(mode: ExportMode) -> ExportRequest {
    let mut request = ExportRequest::new("/vault");
    request.mode = mode;
    request
}

fn canonical() -> String {
    std::fs::read_to_string(format!("{FIXTURES}/hooks.json")).unwrap()
}

fn built() -> MemVfs {
    let vfs = vault(&canonical());
    export_skills(&vfs, &request(ExportMode::Build)).unwrap();
    vfs
}

/// One `Stop` hook running `command`.
fn one_hook(command: &str) -> String {
    format!(r#"{{"hooks": {{"Stop": [{{"hooks": [{{"command": "{command}"}}]}}]}}}}"#)
}

/// The rendered text against `expected/<name>`, or a rewrite of it when
/// blessing.
fn assert_fixture(name: &str, actual: &str) {
    let path = format!("{FIXTURES}/expected/{name}");
    if std::env::var_os("TRADECRAFT_BLESS").is_some() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap();
    assert_eq!(actual, expected, "{name} differs from its fixture");
}

/// The `cursor-hooks-left-out` warnings of a build.
fn cursor_warnings(report: &ExportReport) -> Vec<String> {
    report
        .warnings
        .iter()
        .filter(|w| w.reason == "cursor-hooks-left-out")
        .map(|w| w.path.clone())
        .collect()
}

#[test]
fn claude_and_codex_packs_match_their_fixtures() {
    let vfs = built();
    for (harness, pack) in &PACKS[..2] {
        let hooks = text_at(&vfs, &format!("{pack}/hooks/hooks.json"));
        assert_fixture(&format!("{harness}-hooks.json"), &hooks);
    }
}

/// The translation table of the plan's commit 4: one hook, per harness.
#[test]
fn each_harness_writes_its_own_plugin_root_or_no_hooks() {
    let vfs = vault(&one_hook("${PLUGIN_ROOT}/scripts/hook.sh"));
    write(&vfs, &format!("{PLUGIN}/scripts/hook.sh"), "#!/bin/sh\n");
    let report = export_skills(&vfs, &request(ExportMode::Build)).unwrap();
    let command = |pack: &str| -> String {
        let hooks: serde_json::Value =
            serde_json::from_str(&text_at(&vfs, &format!("{pack}/hooks/hooks.json"))).unwrap();
        hooks["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(command(PACKS[0].1), "${CLAUDE_PLUGIN_ROOT}/scripts/hook.sh");
    assert_eq!(command(PACKS[1].1), "${PLUGIN_ROOT}/scripts/hook.sh");

    let cursor = PACKS[2].1;
    assert!(!vfs.exists(Path::new(&format!("{cursor}/hooks/hooks.json"))));
    let manifest = text_at(&vfs, &format!("{cursor}/.cursor-plugin/plugin.json"));
    assert!(!manifest.contains("\"hooks\""), "{manifest}");
    assert_eq!(cursor_warnings(&report), [HOOKS_REL]);
    let warning = report
        .warnings
        .iter()
        .find(|w| w.reason == "cursor-hooks-left-out")
        .unwrap();
    assert!(warning.detail.contains("hooks/hooks.json"), "{warning:?}");

    let pi = PACKS[3].1;
    assert!(!vfs.exists(Path::new(&format!("{pi}/hooks/hooks.json"))));
}

#[test]
fn each_pack_names_its_own_plugin_root_and_fills_the_placeholders() {
    let vfs = built();
    for ((harness, pack), var) in PACKS[..2].iter().zip(["CLAUDE_PLUGIN_ROOT", "PLUGIN_ROOT"]) {
        let hooks: serde_json::Value =
            serde_json::from_str(&text_at(&vfs, &format!("{pack}/hooks/hooks.json"))).unwrap();
        assert_eq!(
            hooks["hooks"]["Stop"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap(),
            format!("${{{var}}}/scripts/tool session stop --harness {harness} --event Stop"),
            "{harness}"
        );
    }
}

/// A plugin with no skill, command or other source still ships its hooks.
#[test]
fn a_plugin_with_only_a_hooks_file_ships_its_hooks() {
    let vfs = MemVfs::new();
    write(&vfs, "/vault/skillwright.md", "---\n---\n");
    write(&vfs, "/vault/marketplaces/house/house.md", "---\n---\n");
    write(&vfs, &format!("{PLUGIN}/core.md"), "---\n---\n");
    write(
        &vfs,
        &format!("{PLUGIN}/hooks/hooks.json"),
        &one_hook("${PLUGIN_ROOT}"),
    );
    export_skills(&vfs, &request(ExportMode::Build)).unwrap();
    let hooks = text_at(&vfs, &format!("{}/hooks/hooks.json", PACKS[0].1));
    assert!(hooks.contains("${CLAUDE_PLUGIN_ROOT}"), "{hooks}");
}

#[test]
fn claude_plugin_root_in_a_hooks_file_is_refused() {
    let vfs = vault(&one_hook("${CLAUDE_PLUGIN_ROOT}/scripts/tool"));
    let err = export_skills(&vfs, &request(ExportMode::Build))
        .unwrap_err()
        .to_string();
    assert!(err.contains(HOOKS_REL), "{err}");
    assert!(err.contains("write `${PLUGIN_ROOT}`"), "{err}");
    assert!(!vfs.exists(Path::new("/vault/dist/house/.claude-plugin")));
}

#[test]
fn the_cursor_warning_needs_cursor_in_the_build() {
    let vfs = vault(&canonical());
    let mut request = request(ExportMode::Build);
    request.harnesses = BTreeSet::from(["claude".to_owned(), "codex".to_owned()]);
    let report = export_skills(&vfs, &request).unwrap();
    assert!(cursor_warnings(&report).is_empty(), "{:?}", report.warnings);
}

#[test]
fn the_cursor_warning_needs_cursor_in_the_plugins_targets() {
    let vfs = vault(&canonical());
    write(
        &vfs,
        &format!("{PLUGIN}/core.md"),
        "---\ntargets: [claude, codex]\n---\n",
    );
    let report = export_skills(&vfs, &request(ExportMode::Build)).unwrap();
    assert!(cursor_warnings(&report).is_empty(), "{:?}", report.warnings);
}

#[test]
fn the_cursor_warning_needs_a_hooks_file() {
    let vfs = without_hooks();
    let report = export_skills(&vfs, &request(ExportMode::Build)).unwrap();
    assert!(cursor_warnings(&report).is_empty(), "{:?}", report.warnings);
}

/// Once per plugin, however many skills and hooks it has.
#[test]
fn the_cursor_warning_fires_once_per_plugin() {
    let vfs = vault(&canonical());
    write(
        &vfs,
        &format!("{PLUGIN}/skills/lint/lint.md"),
        "---\ndescription: Use lint.\n---\n\n# lint\n",
    );
    let other = "/vault/marketplaces/house/plugins/extra";
    write(&vfs, &format!("{other}/extra.md"), "---\n---\n");
    write(&vfs, &format!("{other}/hooks/hooks.json"), &canonical());
    write(&vfs, &format!("{other}/scripts/tool"), "#!/bin/sh\n");
    let report = export_skills(&vfs, &request(ExportMode::Build)).unwrap();
    assert_eq!(
        cursor_warnings(&report),
        [
            HOOKS_REL,
            "marketplaces/house/plugins/extra/hooks/hooks.json"
        ]
    );
}

#[test]
fn an_unknown_placeholder_fails_the_build() {
    let vfs = vault(&one_hook("x {{foo}}"));
    let err = export_skills(&vfs, &request(ExportMode::Build))
        .unwrap_err()
        .to_string();
    assert!(err.contains(HOOKS_REL), "{err}");
    assert!(err.contains("hooks.Stop[0].hooks[0]"), "{err}");
    assert!(err.contains("{{foo}}"), "{err}");
    assert!(!vfs.exists(Path::new("/vault/dist/house/.claude-plugin")));
}

/// No pack ships a gap report: spec section 9 "What a pack holds" lists
/// none. Cursor ships no hooks file and Pi no `hooks/` folder at all.
#[test]
fn no_pack_ships_a_gap_report_and_pi_has_no_hooks_folder() {
    let vfs = built();
    for (harness, pack) in &PACKS {
        assert!(
            !vfs.exists(Path::new(&format!("{pack}/hooks/gap-report.json"))),
            "{harness}"
        );
    }
    let cursor = PACKS[2].1;
    assert!(!vfs.exists(Path::new(&format!("{cursor}/hooks/hooks.json"))));
    let pi = PACKS[3].1;
    assert!(!vfs.exists(Path::new(&format!("{pi}/hooks"))));
}

/// An event a harness does not carry is left out of its file, and the build
/// goes on: Claude Code carries `Notification`, Codex does not.
#[test]
fn an_uncarried_event_is_left_out() {
    let vfs = built();
    let claude = text_at(&vfs, &format!("{}/hooks/hooks.json", PACKS[0].1));
    let codex = text_at(&vfs, &format!("{}/hooks/hooks.json", PACKS[1].1));
    assert!(claude.contains("session notify"), "{claude}");
    assert!(!codex.contains("session notify"), "{codex}");
}

/// A hook on an event only Codex documents gives no other pack a hooks file.
#[test]
fn an_event_only_codex_carries_gives_only_codex_a_hooks_file() {
    let vfs = vault(r#"{"hooks": {"Interrupt": [{"hooks": [{"command": "x"}]}]}}"#);
    export_skills(&vfs, &request(ExportMode::Build)).unwrap();
    for (harness, pack) in &PACKS {
        let path = format!("{pack}/hooks/hooks.json");
        assert_eq!(
            vfs.exists(Path::new(&path)),
            *harness == "codex",
            "{harness}"
        );
    }
}

#[test]
fn a_hand_edited_hooks_file_is_drift() {
    let vfs = built();
    let path = format!("{}/hooks/hooks.json", PACKS[1].1);
    let edited = text_at(&vfs, &path).replace("session stop", "session halt");
    write(&vfs, &path, &edited);
    let report = export_skills(&vfs, &request(ExportMode::Check)).unwrap();
    assert!(report.changed, "the edit is reported as drift");
}

#[test]
fn an_unknown_event_fails_the_build() {
    let vfs = vault(r#"{"hooks": {"OnBoot": [{"hooks": [{"command": "x"}]}]}}"#);
    let err = export_skills(&vfs, &request(ExportMode::Build))
        .unwrap_err()
        .to_string();
    assert!(err.contains(HOOKS_REL), "{err}");
    assert!(err.contains("OnBoot"), "{err}");
    assert!(!vfs.exists(Path::new("/vault/dist/house/.claude-plugin")));
}

/// A hook a harness that carries hooks cannot carry is left out of its pack
/// with one warning per hook, naming the harness and the event.
#[test]
fn each_hook_a_harness_cannot_carry_is_a_warning() {
    let vfs = vault(
        r#"{"hooks": {
            "Stop": [{"hooks": [{"command": "stop"}]}],
            "Notification": [{"hooks": [{"command": "a"}, {"command": "b"}]}]
        }}"#,
    );
    let mut request = request(ExportMode::Build);
    request.harnesses = BTreeSet::from(["claude".to_owned(), "codex".to_owned()]);
    let report = export_skills(&vfs, &request).unwrap();
    let left_out: Vec<_> = report
        .warnings
        .iter()
        .filter(|w| w.reason == "hook-left-out")
        .collect();
    assert_eq!(left_out.len(), 2, "{:?}", report.warnings);
    for warning in &left_out {
        assert_eq!(warning.path, HOOKS_REL);
        assert!(warning.detail.contains("Codex"), "{warning:?}");
        assert!(warning.detail.contains("Notification"), "{warning:?}");
    }
    let codex = text_at(&vfs, &format!("{}/hooks/hooks.json", PACKS[1].1));
    assert!(!codex.contains("Notification"), "{codex}");
}
