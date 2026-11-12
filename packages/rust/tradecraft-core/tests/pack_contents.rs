//! What a pack holds (spec section 9), one test per row of the table, built
//! through the core's public API.

use std::path::{Path, PathBuf};

use tradecraft_core::{BuildScope, MemVfs, NativeVfs, VaultRequest, Vfs, load_vault};

/// `build_vault` over the one marketplace a request covers: its report.
fn export(
    vfs: &dyn tradecraft_core::Vfs,
    request: &tradecraft_core::VaultRequest,
) -> Result<tradecraft_core::ExportReport, tradecraft_core::CoreError> {
    let mut report = tradecraft_core::build_vault(vfs, request)?;
    assert!(report.refused.is_empty(), "{:?}", report.refused);
    Ok(report.marketplaces.remove(0).report)
}

const NOTE: &str = "---\nversion: 1.0.0\n---\n";
const SKILL: &str = "---\ndescription: Use it.\n---\n\nBody.\n";

/// The four packs of plugin `tools` in marketplace `acme`, under `dist/`.
const PACKS: [&str; 4] = [
    "dist/acme/.claude-plugin/packs/tools",
    "dist/acme/.agents/packs/tools",
    "dist/acme/.cursor-plugin/packs/tools",
    "dist/acme/.pi/packs/tools",
];

/// The Claude Code pack of plugin `tools`.
const CLAUDE: &str = "dist/acme/.claude-plugin/packs/tools";

/// A vault at `/v` with marketplace `acme` and plugin `tools`, plus `files`
/// under the plugin folder.
fn vault(files: &[(&str, &str)]) -> MemVfs {
    let vfs = MemVfs::new();
    let plugin = "marketplaces/acme/plugins/tools";
    put(&vfs, "skillwright.md", "---\n---\n");
    put(&vfs, "marketplaces/acme/acme.md", "---\n---\n");
    put(&vfs, &format!("{plugin}/tools.md"), NOTE);
    for (path, text) in files {
        put(&vfs, &format!("{plugin}/{path}"), text);
    }
    vfs
}

fn put(vfs: &MemVfs, rel: &str, text: &str) {
    let path = Path::new("/v").join(rel);
    vfs.create_dir_all(path.parent().unwrap()).unwrap();
    vfs.write_file(&path, text.as_bytes()).unwrap();
}

/// Builds the vault at `root`.
fn build(vfs: &dyn Vfs, root: &Path) -> Result<(), tradecraft_core::CoreError> {
    let loaded = load_vault(vfs, root, &BuildScope::Vault)?;
    let request = VaultRequest::for_marketplace(&loaded.root, &loaded.marketplaces[0].name);
    export(vfs, &request).map(|_| ())
}

fn read(vfs: &dyn Vfs, rel: &str) -> String {
    String::from_utf8(vfs.read_file(&Path::new("/v").join(rel)).unwrap()).unwrap()
}

fn exists(vfs: &dyn Vfs, rel: &str) -> bool {
    vfs.exists(&Path::new("/v").join(rel))
}

/// Every file under `dir`, relative to `/v`.
fn files_under(vfs: &dyn Vfs, dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = vfs.read_dir(dir) else {
        return out;
    };
    for entry in entries {
        let path = dir.join(&entry.name);
        if entry.is_dir {
            out.extend(files_under(vfs, &path));
        } else {
            out.push(
                path.strip_prefix("/v")
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    out
}

#[test]
fn a_plugins_assets_logo_lands_at_pack_assets_logo() {
    let vfs = vault(&[
        ("skills/review/review.md", SKILL),
        ("assets/logo.png", "png"),
        ("references/guide.md", "A guide.\n"),
        ("scripts/deploy.sh", "#!/bin/sh\n"),
    ]);
    build(&vfs, Path::new("/v")).unwrap();
    for pack in PACKS {
        assert_eq!(read(&vfs, &format!("{pack}/assets/logo.png")), "png");
        assert!(!exists(&vfs, &format!("{pack}/logo.png")), "{pack}");
        assert_eq!(
            read(&vfs, &format!("{pack}/references/guide.md")),
            "A guide.\n"
        );
        assert_eq!(
            read(&vfs, &format!("{pack}/scripts/deploy.sh")),
            "#!/bin/sh\n"
        );
    }
}

#[test]
fn a_skills_script_keeps_its_exec_bit() {
    let vfs = vault(&[
        ("skills/review/review.md", SKILL),
        ("skills/review/scripts/run.sh", "#!/bin/sh\n"),
        ("skills/review/scripts/data.txt", "data\n"),
    ]);
    let script = Path::new("/v/marketplaces/acme/plugins/tools/skills/review/scripts/run.sh");
    vfs.set_executable(script, true).unwrap();
    build(&vfs, Path::new("/v")).unwrap();
    let shipped = Path::new("/v").join(CLAUDE).join("skills/review/scripts");
    assert!(vfs.is_executable(&shipped.join("run.sh")));
    assert!(!vfs.is_executable(&shipped.join("data.txt")));
}

#[test]
fn markdown_in_references_is_compiled_and_the_changelog_is_copied_byte_for_byte() {
    let changelog = "# Changelog\n\n## 1.0.0\n\nSee [[guide]] and /[[review]].\n";
    let vfs = vault(&[
        ("CHANGELOG.md", changelog),
        ("skills/review/review.md", SKILL),
        ("skills/review/references/guide.md", "See [[notes]].\n"),
        ("skills/review/references/notes.md", "Notes.\n"),
    ]);
    build(&vfs, Path::new("/v")).unwrap();
    let guide = read(&vfs, &format!("{CLAUDE}/skills/review/references/guide.md"));
    assert!(!guide.contains("[[notes]]"), "{guide}");
    assert!(guide.contains("notes.md"), "{guide}");
    for pack in PACKS {
        assert_eq!(read(&vfs, &format!("{pack}/CHANGELOG.md")), changelog);
    }
}

#[test]
fn docs_inside_references_ships_and_docs_in_a_skill_folder_does_not() {
    let vfs = vault(&[
        ("skills/review/review.md", SKILL),
        ("skills/review/references/docs/x.md", "Shipped.\n"),
        ("skills/review/docs/x.md", "For authors.\n"),
        ("docs/plan.md", "For authors.\n"),
    ]);
    build(&vfs, Path::new("/v")).unwrap();
    // The skill folder's name depends on the harness's naming; the folder
    // is found by its file.
    for pack in PACKS {
        let all = files_under(&vfs, &Path::new("/v").join(pack));
        let shipped: Vec<&String> = all
            .iter()
            .filter(|f| f.ends_with("review/references/docs/x.md"))
            .collect();
        assert_eq!(shipped.len(), 1, "{all:?}");
        assert_eq!(read(&vfs, shipped[0]), "Shipped.\n");
        assert!(
            !all.iter()
                .any(|f| f.ends_with("review/docs/x.md") || f.ends_with("plan.md")),
            "{all:?}"
        );
    }
}

#[test]
fn an_agent_ships_nowhere() {
    let vfs = vault(&[
        ("skills/review/review.md", SKILL),
        ("agents/triage/triage.md", SKILL),
    ]);
    build(&vfs, Path::new("/v")).unwrap();
    let all = files_under(&vfs, Path::new("/v/dist"));
    assert!(!all.iter().any(|f| f.contains("triage")), "{all:?}");
}

#[test]
fn the_pi_pack_always_holds_package_json() {
    let vfs = vault(&[("skills/review/review.md", SKILL)]);
    build(&vfs, Path::new("/v")).unwrap();
    assert!(exists(&vfs, "dist/acme/.pi/packs/tools/package.json"));
}

#[test]
fn src_inside_scripts_ships() {
    let vfs = vault(&[
        ("skills/review/review.md", SKILL),
        ("scripts/src/main.js", "main();\n"),
        ("src/cli.ts", "never ships\n"),
    ]);
    build(&vfs, Path::new("/v")).unwrap();
    for pack in PACKS {
        assert_eq!(
            read(&vfs, &format!("{pack}/scripts/src/main.js")),
            "main();\n"
        );
        assert!(!exists(&vfs, &format!("{pack}/src/cli.ts")), "{pack}");
    }
}

#[test]
fn markdown_in_scripts_ships_byte_for_byte_with_no_error() {
    let text = "Run `${CLAUDE_PLUGIN_ROOT}/scripts/x.sh`. See [[nowhere]].\n";
    let vfs = vault(&[
        ("skills/review/review.md", SKILL),
        ("skills/review/scripts/usage.md", text),
        ("scripts/usage.md", text),
    ]);
    build(&vfs, Path::new("/v")).unwrap();
    assert_eq!(
        read(&vfs, &format!("{CLAUDE}/skills/review/scripts/usage.md")),
        text
    );
    assert_eq!(read(&vfs, &format!("{CLAUDE}/scripts/usage.md")), text);
}

/// A temporary vault on disk, for symlinks, removed on drop.
struct DiskVault(PathBuf);

impl DiskVault {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("tc-pack-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let plugin = root.join("marketplaces/acme/plugins/tools");
        std::fs::create_dir_all(plugin.join("skills/review/scripts")).unwrap();
        std::fs::write(root.join("skillwright.md"), "---\n---\n").unwrap();
        std::fs::write(root.join("marketplaces/acme/acme.md"), "---\n---\n").unwrap();
        std::fs::write(plugin.join("tools.md"), NOTE).unwrap();
        std::fs::write(plugin.join("skills/review/review.md"), SKILL).unwrap();
        Self(root)
    }

    fn plugin(&self) -> PathBuf {
        self.0.join("marketplaces/acme/plugins/tools")
    }
}

impl Drop for DiskVault {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
#[test]
fn an_in_scope_symlink_in_a_skills_scripts_ships_as_a_copy() {
    let vault = DiskVault::new("in");
    let plugin = vault.plugin();
    std::fs::create_dir_all(plugin.join("scripts")).unwrap();
    std::fs::write(plugin.join("scripts/tool.sh"), "#!/bin/sh\n").unwrap();
    std::os::unix::fs::symlink(
        plugin.join("scripts/tool.sh"),
        plugin.join("skills/review/scripts/tool.sh"),
    )
    .unwrap();
    build(&NativeVfs, &vault.0).unwrap();
    let shipped = vault.0.join(CLAUDE).join("skills/review/scripts/tool.sh");
    let meta = std::fs::symlink_metadata(&shipped).unwrap();
    assert!(!meta.file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&shipped).unwrap(), "#!/bin/sh\n");
}

#[cfg(unix)]
#[test]
fn a_symlink_that_leaves_the_marketplace_is_refused() {
    let vault = DiskVault::new("out");
    let outside = vault.0.join("outside.sh");
    std::fs::write(&outside, "#!/bin/sh\n").unwrap();
    std::os::unix::fs::symlink(
        &outside,
        vault.plugin().join("skills/review/scripts/tool.sh"),
    )
    .unwrap();
    let said = build(&NativeVfs, &vault.0).unwrap_err().to_string();
    assert!(said.contains("link-out-of-scope"), "{said}");
    assert!(said.contains("skills/review/scripts/tool.sh"), "{said}");
}
