//! Every `#[non_exhaustive]` type, used from outside the crate the documented
//! way: a request from `new` with the required inputs, then fields set on the
//! value; a config from `Default`; an output read by field; an enum matched
//! with a `_` arm.

use std::path::{Path, PathBuf};

use tradecraft_core::model::{ComponentRequest, LoadRequest};
use tradecraft_core::{
    AdoptMode, AdoptRequest, AdoptStatus, AssetSpec, AuthoringOp, BuildSettings, CompileSource,
    CoreError, DiscoveryWarning, EntityKind, ExportMode, ExportRequest, ExportUnit, FieldEdit,
    FieldValue, HookFileSpec, LinkIndex, MarketplaceConfig, MarketplaceFields, MemVfs, Namespace,
    NewNote, Placement, PluginOverrides, PublishMode, PublishOutcome, PublishRequest, PublishState,
    PublishStatus, ReasonToken, RootFile, StaleKind, StatusRequest, UnitStatus, VaultScope, Vfs,
    discover_components, export_skills, plan_create_note, publish_skill,
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

    let mut note = NewNote::new("lint");
    note.body = "\n# Lint\n";
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

#[test]
fn the_settings_types_build_from_default() {
    let mut placement = Placement::default();
    placement.kind = Some("skill".to_owned());
    let mut fields = MarketplaceFields::default();
    fields.name = Some("house".to_owned());
    let mut build = BuildSettings::default();
    build.out = Some("dist".to_owned());
    let mut root = RootFile::default();
    root.marketplace = fields;
    root.build = build;
    let config = root.to_config("house");
    assert_eq!(config.name.as_deref(), Some("house"));
}

#[test]
fn a_new_note_needs_only_a_name_and_takes_reexported_field_values() {
    let vfs = MemVfs::new();
    vfs.create_dir_all(Path::new("/vault/skills")).unwrap();
    vfs.write_file(
        Path::new("/vault/skillwright.md"),
        b"---\nname: house\n---\n",
    )
    .unwrap();
    let edits = [FieldEdit::set(
        "description",
        FieldValue::Str("Lint.".to_owned()),
    )];
    let mut note = NewNote::new("lint");
    note.fields = &edits;
    let op = plan_create_note(&vfs, Path::new("/vault"), &Namespace::bare(), &note).unwrap();
    let path = match op {
        AuthoringOp::Create { path, .. } => path,
        _ => panic!("expected a create"),
    };
    assert_eq!(path, "skills/lint.md");
}

#[test]
fn a_compile_source_takes_a_reexported_link_index() {
    let index = LinkIndex::build(&["skills/a.md".to_owned()]);
    let mut source = CompileSource::new(Path::new("/vault"), Path::new("/vault/skills/a.md"), "a");
    source.links = Some(&index);
    assert!(source.links.is_some());
}

#[test]
fn the_output_types_read_by_field_and_their_enums_match_with_a_wildcard() {
    let vfs = MemVfs::new();
    vfs.create_dir_all(Path::new("/vault/skills")).unwrap();
    vfs.write_file(
        Path::new("/vault/skills/review.md"),
        b"---\ndescription: Review.\n---\nReview.\n",
    )
    .unwrap();

    let scan = discover_components(&vfs, Path::new("/vault"), None, &Namespace::bare()).unwrap();
    let component = &scan.components[0];
    assert_eq!(component.name, "review");
    let warning = DiscoveryWarning::new("skills/review.md", "missing-version", "detail");
    assert_eq!(warning.reason, "missing-version");

    let request = PublishRequest::new("/vault/skills/review.md", "/project", "claude-code");
    let report = publish_skill(&vfs, &request, &Namespace::bare()).unwrap();
    for file in &report.files {
        let word = match file.status {
            PublishStatus::Written => "written",
            _ => file.status.as_str(),
        };
        assert_eq!(word, "written");
        let outcome = match file.outcome {
            PublishOutcome::FirstWrite => "first-write",
            _ => file.outcome.as_str(),
        };
        assert!(!outcome.is_empty());
    }

    let mut config = MarketplaceConfig::default();
    config.name = Some("house".to_owned());
    let request = ExportRequest::new("/vault", "/market", config);
    let report = export_skills(&vfs, &request, &Namespace::bare()).unwrap();
    assert_eq!(report.mode, ExportMode::Build);
    for unit in &report.units {
        let status = match unit {
            ExportUnit::Plugin { status, .. } | ExportUnit::Manifest { status, .. } => *status,
            _ => continue,
        };
        let word = match status {
            UnitStatus::Written => "written",
            _ => status.as_str(),
        };
        assert_eq!(word, "written");
    }
}

#[test]
fn each_as_str_is_the_serialized_word() {
    fn wire<T: serde::Serialize>(value: T) -> String {
        serde_json::to_value(value)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }
    for status in [
        PublishStatus::Written,
        PublishStatus::Unchanged,
        PublishStatus::Refused,
    ] {
        assert_eq!(status.as_str(), wire(status));
    }
    for outcome in [
        PublishOutcome::FirstWrite,
        PublishOutcome::Rewrite,
        PublishOutcome::Claimed,
        PublishOutcome::Overwritten,
        PublishOutcome::Unchanged,
        PublishOutcome::Conflict,
        PublishOutcome::Foreign,
        PublishOutcome::Blocked,
        PublishOutcome::Manifest,
    ] {
        assert_eq!(outcome.as_str(), wire(outcome));
    }
    for kind in [StaleKind::Plugin, StaleKind::File, StaleKind::EmptyDir] {
        assert_eq!(kind.as_str(), wire(kind));
    }
    for status in [
        UnitStatus::Ok,
        UnitStatus::Drift,
        UnitStatus::Written,
        UnitStatus::Unchanged,
    ] {
        assert_eq!(status.as_str(), wire(status));
    }
    for status in [
        AdoptStatus::Adopted,
        AdoptStatus::Updated,
        AdoptStatus::Unchanged,
        AdoptStatus::Refused,
    ] {
        assert_eq!(status.as_str(), wire(status));
    }
    let state = match PublishState::Current {
        PublishState::Current => "current",
        _ => "other",
    };
    assert_eq!(state, PublishState::Current.as_str());
}

#[test]
fn a_core_error_names_its_variant_and_matches_with_a_wildcard() {
    let err = CoreError::Refused("no".to_owned());
    assert_eq!(err.variant_name(), "Refused");
    let refused = match &err {
        CoreError::Refused(why) => why.as_str(),
        _ => "",
    };
    assert_eq!(refused, "no");
}
