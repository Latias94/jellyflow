use std::time::{SystemTime, UNIX_EPOCH};

use jellyflow_headless_adapter_template::{adapter_smoke_suite, check_fixture_directory};

#[test]
fn template_adapter_conformance_suite_matches() {
    let report = adapter_smoke_suite().run();

    assert!(report.is_match(), "{report}");
    assert_eq!(report.scenario_count(), 5);
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
    assert_eq!(report.scenario_count(), 5);
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
