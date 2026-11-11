//! Every `#[non_exhaustive]` type, used from outside the crate the documented
//! way: a request from `new` with the required inputs, then fields set on the
//! value; a config from `Default`; an output read by field; an enum matched
//! with a `_` arm.

use std::path::Path;

use tradecraft_core::model::{ComponentRequest, LoadRequest};
use tradecraft_core::{
    AssetSpec, BuildSettings, CompileSource, CoreError, DiscoveryWarning, EntityKind, ExportMode,
    ExportRequest, ExportUnit, FieldEdit, FieldValue, HookFileSpec, LinkIndex, MarketplaceConfig,
    MarketplaceFields, MemVfs, Namespace, PluginOverrides, ReasonToken, RootFile, StaleKind,
    UnitStatus, VaultScope, Vfs, apply_field_edits, discover_components, export_skills,
};

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
    overrides.identity.version = Some("1.0.0".to_owned());
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
fn a_load_request_takes_its_inputs() {
    let mut load = LoadRequest::new("/vault", MarketplaceConfig::default());
    load.marketplace = Some("house".to_owned());
    assert!(load.harnesses.is_empty());
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
fn a_field_edit_takes_a_reexported_field_value() {
    let edits = [FieldEdit::set(
        "description",
        FieldValue::Str("Lint.".to_owned()),
    )];
    let note = apply_field_edits("---\nname: lint\n---\nBody.\n", &edits).unwrap();
    assert_eq!(note, "---\nname: lint\ndescription: Lint.\n---\nBody.\n");
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

#[test]
fn a_core_error_names_its_fallback_reason_token() {
    assert_eq!(
        CoreError::Projection("x".to_owned()).reason_token(),
        ReasonToken::NoRenderer
    );
    assert_eq!(
        CoreError::Conflict("x".to_owned()).reason_token(),
        ReasonToken::Conflict
    );
    assert_eq!(
        CoreError::Refused("x".to_owned()).reason_token(),
        ReasonToken::Io
    );
}

#[test]
fn a_skipped_note_reads_by_field() {
    let vfs = MemVfs::new();
    vfs.create_dir_all(Path::new("/vault/skills")).unwrap();
    vfs.write_file(
        Path::new("/vault/skills/skillwright.md"),
        b"---\ndescription: X.\n---\n",
    )
    .unwrap();
    let scan = discover_components(&vfs, Path::new("/vault"), None, &Namespace::bare()).unwrap();
    assert!(scan.components.is_empty());
    assert_eq!(scan.skipped[0].path, "skills/skillwright.md");
    assert!(
        scan.skipped[0].reason.contains("reserved-name"),
        "{}",
        scan.skipped[0].reason
    );
}
