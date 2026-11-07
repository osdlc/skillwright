//! Every `#[non_exhaustive]` request and config type, built from outside the
//! crate the documented way: `new` with the required inputs, then fields set
//! on the value; a config from `Default`.

use std::path::{Path, PathBuf};

use tradecraft_core::model::{ComponentRequest, LoadRequest};
use tradecraft_core::status::StatusRequest;
use tradecraft_core::{
    AdoptMode, AdoptRequest, AssetSpec, CompileSource, EntityKind, ExportMode, ExportRequest,
    HookFileSpec, MarketplaceConfig, MemVfs, Namespace, NewNote, PluginOverrides, PublishMode,
    PublishRequest, ReasonToken, VaultScope, Vfs, export_skills, publish_skill,
};

#[test]
fn a_publish_request_takes_its_three_inputs_and_starts_empty() {
    let vfs = MemVfs::new();
    vfs.create_dir_all(Path::new("/vault/skills")).unwrap();
    vfs.write_file(
        Path::new("/vault/skills/review.md"),
        b"---\ndescription: Review.\n---\nReview.\n",
    )
    .unwrap();
    let mut request = PublishRequest::new("/vault/skills/review.md", "/project", "claude-code");
    assert_eq!(request.mode, PublishMode::Write);
    assert!(request.config.is_none() && request.overwrite.is_empty() && !request.user_folders);
    request.skills_root = Some(PathBuf::from("/vault"));
    let report = publish_skill(&vfs, &request, &Namespace::default()).unwrap();
    assert_eq!(report.files.len(), 2);
}

#[test]
fn a_status_request_takes_its_three_inputs() {
    let mut request = StatusRequest::new("/vault/skills/review.md", "/project", "claude-code");
    request.user_folders = true;
    assert_eq!(request.target, "claude-code");
    assert!(request.skills_root.is_none() && request.config.is_none());
}

#[test]
fn an_export_request_builds_from_a_default_config() {
    let vfs = MemVfs::new();
    vfs.create_dir_all(Path::new("/vault/skills")).unwrap();
    vfs.write_file(
        Path::new("/vault/skills/review.md"),
        b"---\ndescription: Review.\n---\nReview.\n",
    )
    .unwrap();
    let mut config = MarketplaceConfig::default();
    config.name = Some("house".to_owned());
    let mut overrides = PluginOverrides::default();
    overrides.version = Some("1.0.0".to_owned());
    config.plugins.insert("house".to_owned(), overrides);
    let mut request = ExportRequest::new("/vault", "/market", config);
    assert_eq!(request.mode, ExportMode::Build);
    request.harnesses.insert("claude".to_owned());
    request
        .assets
        .push(AssetSpec::new("/vault/bin/tool", "bin/tool"));
    request.assets.clear();
    let report = export_skills(&vfs, &request, &Namespace::bare()).unwrap();
    assert!(report.changed);
    assert!(vfs.exists(Path::new("/market/.claude-plugin/marketplace.json")));
}

#[test]
fn the_small_specs_default_to_the_marketplaces_own_plugin() {
    let mut asset = AssetSpec::new("/vault/bin/tool", "bin/tool");
    assert!(asset.plugin.is_none());
    asset.plugin = Some("tools".to_owned());
    let hooks = HookFileSpec::new("/vault/hooks.json", "hooks.json");
    assert!(hooks.plugin.is_none());
}

#[test]
fn a_load_request_and_an_adopt_request_take_their_inputs() {
    let mut load = LoadRequest::new("/vault", MarketplaceConfig::default());
    load.marketplace = Some("house".to_owned());
    assert!(load.harnesses.is_empty());

    let mut adopt = AdoptRequest::new("/src/skill", "/vault", "2026-01-01T00:00:00Z");
    assert_eq!(adopt.mode, AdoptMode::Write);
    adopt.mode = AdoptMode::DryRun;
    adopt.plugin = Some("tools".to_owned());
    assert!(adopt.also_scan.is_empty() && adopt.targets.is_none());
}

#[test]
fn the_borrowed_requests_take_their_inputs() {
    let vault = Path::new("/vault");
    let mut scope = VaultScope::new(vault);
    scope.marketplace = Some("house");
    assert!(scope.skills_root.is_none());

    let mut component = ComponentRequest::new(
        Path::new("/vault/skills/a.md"),
        vault,
        EntityKind::Skill,
        "claude",
        "claude-code",
    );
    component.bundle_root = Some(Path::new("/vault/skills/a"));
    assert_eq!(component.kind, EntityKind::Skill);

    let mut source = CompileSource::new(vault, Path::new("/vault/skills/a.md"), "a");
    assert!(source.calls.is_none() && source.links.is_none());
    source.bundle_root = Some(Path::new("/vault/skills/a"));
    assert_eq!(source.skill_name, "a");

    let mut note = NewNote::new("lint", &[], "\n# Lint\n");
    note.bundled = true;
    assert!(note.placement.is_none());
}

#[test]
fn a_match_on_a_reason_token_needs_a_wildcard_arm() {
    let label = match ReasonToken::Conflict {
        ReasonToken::Conflict => "conflict",
        _ => "other",
    };
    assert_eq!(label, "conflict");
}
