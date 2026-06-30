use std::time::{SystemTime, UNIX_EPOCH};

use jellyflow_headless_adapter_template::{
    adapter_smoke_suite, check_fixture_directory, run_authoring_controls_projection_smoke,
    template_capabilities,
};
use jellyflow_runtime::runtime::conformance::{
    ConformanceCapabilityKind, ConformanceSupportLevel,
};

#[test]
fn template_adapter_conformance_suite_matches() {
    let report = adapter_smoke_suite().run();

    assert!(report.is_match(), "{report}");
    assert_eq!(report.scenario_count(), 11);
    assert_eq!(
        report
            .capabilities
            .level(ConformanceCapabilityKind::MeasuredAnchors),
        ConformanceSupportLevel::Full
    );
    assert!(report.capability_gaps.is_empty());
    assert_eq!(
        report
            .capabilities
            .level(ConformanceCapabilityKind::ControlProjection),
        ConformanceSupportLevel::Projection
    );
    assert!(!report.capabilities.satisfies(
        ConformanceCapabilityKind::LayoutPassMeasurement,
        ConformanceSupportLevel::Full,
    ));
}

#[test]
fn template_adapter_fixture_directory_matches() {
    let root = temp_fixture_dir();
    std::fs::create_dir_all(&root).expect("create fixture directory");
    adapter_smoke_suite()
        .save_json(root.join("adapter-smoke.json"))
        .expect("save fixture suite");

    let report = check_fixture_directory(&root).expect("check fixture directory");
    let _ = std::fs::remove_dir_all(&root);

    assert!(report.is_match(), "{report}");
    assert_eq!(report.file_count(), 1);
    assert_eq!(report.scenario_count(), 11);
}

#[test]
fn template_adapter_capabilities_do_not_overclaim_authoring_support() {
    let capabilities = template_capabilities();

    assert!(capabilities.satisfies(
        ConformanceCapabilityKind::MeasuredHandles,
        ConformanceSupportLevel::Full,
    ));
    assert!(capabilities.satisfies(
        ConformanceCapabilityKind::ControlProjection,
        ConformanceSupportLevel::Projection,
    ));
    assert!(!capabilities.satisfies(
        ConformanceCapabilityKind::EditableControls,
        ConformanceSupportLevel::Projection,
    ));
    assert!(!capabilities.satisfies(
        ConformanceCapabilityKind::LayoutPassMeasurement,
        ConformanceSupportLevel::Full,
    ));
}

#[test]
fn template_adapter_can_project_authoring_controls_without_widgets() {
    run_authoring_controls_projection_smoke().expect("authoring controls projection smoke");
}

fn temp_fixture_dir() -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "jellyflow-headless-adapter-template-fixtures-{nanos}"
    ))
}
