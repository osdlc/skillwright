//! Every `#[non_exhaustive]` type, used from outside the crate the documented
//! way: a request from `new` with the required inputs, then fields set on the
//! value; a settings type from `Default`; an output read by field; an enum matched
//! with a `_` arm.

use std::path::Path;

use tradecraft_core::model::LoadRequest;
use tradecraft_core::refusal;
use tradecraft_core::{
    BuildSettings, CallContext, CompileSource, CoreError, DiscoveryWarning, ExportMode, ExportUnit,
    MemVfs, RootFile, SharedDefaults, SkillIndex, UnitStatus, VaultRequest, Vfs, compile_note,
    discover_components, read_layout,
};

/// `build_vault` over the one marketplace a request covers: its report.
fn export(
    vfs: &dyn tradecraft_core::Vfs,
    request: &tradecraft_core::VaultRequest,
) -> Result<tradecraft_core::ExportReport, tradecraft_core::CoreError> {
    let mut report = tradecraft_core::build_vault(vfs, request)?;
    assert!(report.refused.is_empty(), "{:?}", report.refused);
    Ok(report.marketplaces.remove(0).report)
}

const PLUGIN: &str = "/vault/marketplaces/house/plugins/tools";

/// A vault of marketplace `house`, plugin `tools` and the skill `review`.
fn vault() -> MemVfs {
    let vfs = MemVfs::new();
    vfs.insert("/vault/skillwright.md", "---\n---\n").unwrap();
    vfs.insert("/vault/marketplaces/house/house.md", "---\n---\n")
        .unwrap();
    vfs.insert(format!("{PLUGIN}/tools.md"), "---\n---\n")
        .unwrap();
    vfs.insert(
        format!("{PLUGIN}/skills/review/review.md"),
        "---\ndescription: Review.\n---\nReview.\n",
    )
    .unwrap();
    vfs
}

#[test]
fn an_export_request_takes_its_vault_root() {
    let vfs = vault();
    let mut request = VaultRequest::new("/vault");
    assert_eq!(request.mode, ExportMode::Build);
    request.harnesses.insert("claude".to_owned());
    let report = export(&vfs, &request).unwrap();
    assert!(report.changed);
    assert!(vfs.exists(Path::new(
        "/vault/dist/house/.claude-plugin/marketplace.json"
    )));
}

#[test]
fn a_load_request_takes_its_inputs() {
    let mut load = LoadRequest::new("/vault");
    load.marketplace = Some("house".to_owned());
    assert!(load.harnesses.is_empty());
}

#[test]
fn a_compile_source_takes_its_inputs() {
    let vfs = vault();
    let layout = read_layout(&vfs, Path::new("/vault")).unwrap();
    let skills = SkillIndex::default();
    let calls = CallContext {
        skills: &skills,
        hand_written: Default::default(),
        warnings: Default::default(),
    };
    let entry = Path::new("/vault/marketplaces/house/plugins/tools/skills/review/review.md");
    let source = CompileSource::new(&layout, entry, "review", &calls);
    let compiled = compile_note(&vfs, &source, "Review.\n").unwrap();
    assert_eq!(compiled.body, "Review.\n");
    assert!(compiled.recorded.calls.is_empty());
}

#[test]
fn the_settings_types_build_from_default() {
    let mut fields = SharedDefaults::default();
    fields.license = Some("MIT".to_owned());
    let mut build = BuildSettings::default();
    build.harnesses = Some(vec!["claude".to_owned()]);
    let mut root = RootFile::default();
    root.defaults = fields;
    root.build = build;
    assert_eq!(root.defaults.license.as_deref(), Some("MIT"));
    assert_eq!(root.harness_set().len(), 1);
}

#[test]
fn the_output_types_read_by_field_and_their_enums_match_with_a_wildcard() {
    let vfs = vault();
    let scan = discover_components(&vfs, Path::new("/vault")).unwrap();
    let component = scan.components.iter().find(|c| c.name == "review").unwrap();
    assert_eq!(component.plugin.as_deref(), Some("tools"));
    let warning = DiscoveryWarning::new("review.md", "missing-version", "detail");
    assert_eq!(warning.reason, "missing-version");

    let request = VaultRequest::new("/vault");
    let report = export(&vfs, &request).unwrap();
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
fn each_unit_status_as_str_is_the_serialized_word() {
    fn wire<T: serde::Serialize>(value: T) -> String {
        serde_json::to_value(value)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
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
    let err = CoreError::refused(refusal::LAYOUT, "no");
    assert_eq!(err.variant_name(), "Refused");
    let refused = match &err {
        CoreError::Refused { token, message } => (*token, message.as_str()),
        _ => (refusal::UNSAFE_PATH, ""),
    };
    assert_eq!(refused, (refusal::LAYOUT, "no"));
}

#[test]
fn a_skipped_note_reads_by_field() {
    let vfs = vault();
    vfs.insert(
        format!("{PLUGIN}/skills/review/review.md"),
        "---\ndescription: [\n---\n",
    )
    .unwrap();
    let scan = discover_components(&vfs, Path::new("/vault")).unwrap();
    assert!(scan.components.iter().all(|c| c.name != "review"));
    assert_eq!(
        scan.skipped[0].path,
        "marketplaces/house/plugins/tools/skills/review/review.md"
    );
    assert!(!scan.skipped[0].reason.is_empty());
}
