use crate::runtime::conformance::{
    ConformanceFixtureDirectory, ConformanceFixtureFileError, ConformanceScenario, ConformanceSuite,
};
use jellyflow_core::core::{Graph, GraphId};

use super::support::{temp_dir, temp_path};

#[test]
fn conformance_file_suite_load_save_roundtrips_and_runs() {
    let path = temp_path("suite-roundtrip");
    let suite =
        ConformanceSuite::new("file-backed suite").with_scenarios([ConformanceScenario::new(
            "empty fixture",
            Graph::new(GraphId::new()),
        )]);

    suite.save_json(&path).expect("save suite");
    let loaded = ConformanceSuite::load_json(&path).expect("load suite");
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        serde_json::to_value(&loaded).expect("loaded suite json"),
        serde_json::to_value(&suite).expect("suite json"),
    );
    assert!(loaded.run().is_match());
}

#[test]
fn conformance_file_suite_load_if_exists_returns_none_for_missing_files() {
    let path = temp_path("suite-missing");

    let loaded = ConformanceSuite::load_json_if_exists(&path).expect("optional load");

    assert!(loaded.is_none());
}

#[test]
fn conformance_file_suite_load_reports_parse_errors_with_path_context() {
    let path = temp_path("suite-parse-error");
    std::fs::write(&path, b"{not json").expect("write invalid fixture");

    let err = ConformanceSuite::load_json(&path).expect_err("parse error");
    let _ = std::fs::remove_file(&path);

    assert!(matches!(err, ConformanceFixtureFileError::Parse { .. }));
    assert!(err.to_string().contains("suite-parse-error"));
}

#[test]
fn conformance_fixture_directory_discovers_json_suites_recursively_in_sorted_order() {
    let root = temp_dir("fixture-directory");
    let nested = root.join("nested");
    std::fs::create_dir_all(&nested).expect("create nested fixture dir");

    ConformanceSuite::new("suite b")
        .with_scenarios([ConformanceScenario::new(
            "empty b",
            Graph::new(GraphId::new()),
        )])
        .save_json(root.join("b.json"))
        .expect("save b suite");
    ConformanceSuite::new("suite a")
        .with_scenarios([ConformanceScenario::new(
            "empty a",
            Graph::new(GraphId::new()),
        )])
        .save_json(root.join("a.json"))
        .expect("save a suite");
    ConformanceSuite::new("suite c")
        .with_scenarios([ConformanceScenario::new(
            "empty c",
            Graph::new(GraphId::new()),
        )])
        .save_json(nested.join("c.json"))
        .expect("save c suite");
    std::fs::write(root.join("ignore.txt"), b"not a suite").expect("write ignored file");

    let directory = ConformanceFixtureDirectory::load_json(root.clone()).expect("load directory");
    let names = directory
        .files
        .iter()
        .map(|file| file.suite.name.as_str())
        .collect::<Vec<_>>();
    let relative_paths = directory
        .files
        .iter()
        .map(|file| {
            file.path
                .strip_prefix(&root)
                .expect("relative path")
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
        })
        .collect::<Vec<_>>();
    let report = directory.run();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(names, ["suite a", "suite b", "suite c"]);
    assert_eq!(relative_paths, ["a.json", "b.json", "nested/c.json"]);
    assert_eq!(report.file_count(), 3);
    assert_eq!(report.scenario_count(), 3);
    assert!(report.is_match(), "{report}");
}

#[test]
fn conformance_fixture_directory_load_if_exists_returns_none_for_missing_directories() {
    let root = temp_dir("fixture-directory-missing");

    let directory = ConformanceFixtureDirectory::load_json_if_exists(&root)
        .expect("optional fixture directory");

    assert!(directory.is_none());
}

#[test]
fn conformance_fixture_directory_reports_invalid_json_path_context() {
    let root = temp_dir("fixture-directory-invalid");
    std::fs::create_dir_all(&root).expect("create fixture dir");
    std::fs::write(root.join("bad.json"), b"{not json").expect("write invalid fixture");

    let err = ConformanceFixtureDirectory::load_json(&root).expect_err("invalid fixture");
    let _ = std::fs::remove_dir_all(&root);

    assert!(matches!(err, ConformanceFixtureFileError::Parse { .. }));
    assert!(err.to_string().contains("bad.json"));
}
