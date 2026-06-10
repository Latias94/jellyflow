use crate::runtime::auto_pan::{AutoPanActivation, AutoPanRequest};
use crate::runtime::conformance::{
    ConformanceAction, ConformanceBehavior, ConformanceFixtureDirectory,
    ConformanceFixtureFileError, ConformanceNodeDragSessionContract, ConformanceScenario,
    ConformanceSuite, ConformanceSuiteFile, ConformanceTraceConfig,
};
use jellyflow_core::core::{CanvasPoint, CanvasSize, Graph, GraphId};

use super::super::fixtures::make_graph;
use super::support::{temp_dir, temp_path};

#[test]
fn conformance_approval_updates_expected_trace_from_actual_runtime_trace() {
    let suite =
        ConformanceSuite::new("approval suite").with_scenarios([approval_viewport_scenario(
            "viewport approval",
            CanvasPoint { x: 10.0, y: 20.0 },
            1.5,
        )]);

    let approval = suite.approve_actual_traces();

    assert!(approval.is_approvable(), "{:?}", approval.report.errors);
    assert!(approval.has_changes());
    assert_eq!(approval.changed_scenarios(), 1);
    assert_eq!(approval.report.scenario_reports[0].expected_event_count, 0);
    assert_eq!(approval.report.scenario_reports[0].actual_event_count, 1);
    assert_eq!(approval.suite.scenarios[0].expected_trace.len(), 1);
    assert!(approval.suite.run().is_match());
}

#[test]
fn conformance_approval_keeps_behavior_contract_trace_out_of_expected_trace() {
    let (graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let suite = ConformanceSuite::new("behavior approval suite").with_scenarios([
        ConformanceScenario::new("behavior approval", graph)
            .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
            .with_behaviors([ConformanceBehavior::node_drag_session(
                ConformanceNodeDragSessionContract::new(
                    node_id,
                    CanvasPoint { x: 1.0, y: 2.0 },
                    CanvasPoint { x: 32.0, y: 16.0 },
                ),
            )]),
    ]);

    let approval = suite.approve_actual_traces();

    assert!(approval.is_approvable(), "{:?}", approval.report.errors);
    assert!(!approval.has_changes());
    assert_eq!(approval.changed_scenarios(), 0);
    assert!(approval.report.scenario_reports[0].expected_event_count > 0);
    assert_eq!(
        approval.report.scenario_reports[0].expected_event_count,
        approval.report.scenario_reports[0].actual_event_count
    );
    assert!(approval.suite.scenarios[0].expected_trace.is_empty());
    assert!(approval.suite.run().is_match());
}

#[test]
fn conformance_approval_file_write_back_saves_updated_expected_trace() {
    let path = temp_path("approval-file");
    let suite =
        ConformanceSuite::new("file approval suite").with_scenarios([approval_viewport_scenario(
            "file viewport approval",
            CanvasPoint { x: 5.0, y: 7.0 },
            1.25,
        )]);
    suite.save_json(&path).expect("save stale fixture");

    let file = ConformanceSuiteFile::load_json(&path).expect("load stale fixture");
    let approval = file
        .approve_actual_traces_to_json()
        .expect("approve fixture file");
    let reloaded = ConformanceSuite::load_json(&path).expect("reload approved fixture");
    let _ = std::fs::remove_file(&path);

    assert!(approval.is_approvable(), "{:?}", approval.report.errors);
    assert_eq!(approval.changed_scenarios(), 1);
    assert!(reloaded.run().is_match());
}

#[test]
fn conformance_approval_directory_write_back_saves_all_clean_fixture_files() {
    let root = temp_dir("approval-directory");
    std::fs::create_dir_all(&root).expect("create fixture dir");
    ConformanceSuite::new("first approval suite")
        .with_scenarios([approval_viewport_scenario(
            "first viewport approval",
            CanvasPoint { x: 1.0, y: 2.0 },
            1.2,
        )])
        .save_json(root.join("first.json"))
        .expect("save first stale fixture");
    ConformanceSuite::new("second approval suite")
        .with_scenarios([approval_viewport_scenario(
            "second viewport approval",
            CanvasPoint { x: 3.0, y: 4.0 },
            1.4,
        )])
        .save_json(root.join("second.json"))
        .expect("save second stale fixture");

    let directory = ConformanceFixtureDirectory::load_json(&root).expect("load fixture directory");
    let approval = directory
        .approve_actual_traces_to_json()
        .expect("approve fixture directory");
    let first = ConformanceSuite::load_json(root.join("first.json")).expect("reload first file");
    let second = ConformanceSuite::load_json(root.join("second.json")).expect("reload second file");
    let _ = std::fs::remove_dir_all(&root);

    assert!(approval.is_approvable());
    assert_eq!(approval.file_count(), 2);
    assert_eq!(approval.changed_files(), 2);
    assert_eq!(approval.changed_scenarios(), 2);
    assert!(first.run().is_match());
    assert!(second.run().is_match());
}

#[test]
fn conformance_approval_directory_write_back_refuses_execution_errors_without_partial_writes() {
    let root = temp_dir("approval-directory-errors");
    std::fs::create_dir_all(&root).expect("create fixture dir");
    ConformanceSuite::new("good approval suite")
        .with_scenarios([approval_viewport_scenario(
            "good viewport approval",
            CanvasPoint { x: 3.0, y: 4.0 },
            1.1,
        )])
        .save_json(root.join("good.json"))
        .expect("save good stale fixture");
    ConformanceSuite::new("bad approval suite")
        .with_scenarios([ConformanceScenario::new(
            "rejected auto-pan approval",
            Graph::new(GraphId::new()),
        )
        .with_actions([ConformanceAction::apply_auto_pan(AutoPanRequest::new(
            AutoPanActivation::Always,
            CanvasPoint { x: 190.0, y: 50.0 },
            CanvasSize {
                width: 200.0,
                height: 100.0,
            },
            0.0,
        ))])])
        .save_json(root.join("bad.json"))
        .expect("save bad fixture");

    let directory = ConformanceFixtureDirectory::load_json(&root).expect("load fixture directory");
    let err = directory
        .approve_actual_traces_to_json()
        .expect_err("execution errors reject directory approval");
    let good_after = ConformanceSuite::load_json(root.join("good.json")).expect("reload good file");
    let _ = std::fs::remove_dir_all(&root);

    assert!(matches!(err, ConformanceFixtureFileError::Approve { .. }));
    assert!(err.to_string().contains("bad.json"));
    assert!(!good_after.run().is_match());
}

fn approval_viewport_scenario(name: &str, pan: CanvasPoint, zoom: f32) -> ConformanceScenario {
    ConformanceScenario::new(name, Graph::new(GraphId::new()))
        .with_actions([ConformanceAction::set_viewport(pan, zoom)])
}
