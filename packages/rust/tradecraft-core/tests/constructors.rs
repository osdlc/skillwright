//! Every `#[non_exhaustive]` type, used from outside the crate the documented
//! way: a request from `new` with the required inputs, then fields set on the
//! value; a settings type from `Default`; an output read by field; an enum matched
//! with a `_` arm.

use std::path::Path;

use tradecraft_core::model::LoadRequest;
use tradecraft_core::refusal;
use tradecraft_core::{
    BuildScope, BuildSettings, CoreError, DiscoveryWarning, ExportMode, ExportUnit, MemVfs,
    RootFile, SharedDefaults, UnitStatus, VaultRequest, Vfs, discover_components,
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
    assert_eq!(request.options.mode, ExportMode::Build);
    request.options.harnesses.insert("claude".to_owned());
    let report = export(&vfs, &request).unwrap();
    assert!(report.changed);
    assert!(vfs.exists(Path::new(
        "/vault/dist/house/.claude-plugin/marketplace.json"
    )));
}

#[test]
fn a_load_request_takes_its_inputs() {
    let mut load = LoadRequest::new("/vault");
    load.scope = BuildScope::Marketplace("house".to_owned());
    assert!(load.harnesses.is_empty());
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
    let component = scan
        .components
        .iter()
        .find(|c| c.position.name() == "review")
        .unwrap();
    assert_eq!(component.position.plugin(), Some("tools"));
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
        assert_eq!(status, UnitStatus::Differs);
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
    assert!(
        scan.components
            .iter()
            .all(|c| c.position.name() != "review")
    );
    assert_eq!(
        scan.skipped[0].path,
        "marketplaces/house/plugins/tools/skills/review/review.md"
    );
    assert!(!scan.skipped[0].reason.is_empty());
}
