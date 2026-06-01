use super::*;

#[test]
fn conformance_suite_runs_all_scenarios_and_reports_mismatches() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let matching = ConformanceScenario::new("matching viewport", graph.clone())
        .with_actions([ConformanceAction::set_viewport(
            CanvasPoint { x: 1.0, y: 2.0 },
            1.25,
        )])
        .with_expected_trace([ConformanceTraceEvent::viewport(
            CanvasPoint { x: 1.0, y: 2.0 },
            1.25,
        )]);
    let mismatched = ConformanceScenario::new("mismatched viewport", graph)
        .with_actions([ConformanceAction::set_viewport(
            CanvasPoint { x: 3.0, y: 4.0 },
            1.5,
        )])
        .with_expected_trace([ConformanceTraceEvent::viewport(
            CanvasPoint { x: 30.0, y: 40.0 },
            1.5,
        )]);
    let suite =
        ConformanceSuite::new("adapter viewport suite").with_scenarios([matching, mismatched]);

    let report = run_conformance_suite(&suite);

    assert!(!report.is_match(), "{report}");
    assert_eq!(report.scenario_reports.len(), 2);
    assert_eq!(report.errors.len(), 0);
    assert_eq!(report.failed_scenarios(), 1);
    assert!(report.to_string().contains("adapter viewport suite"));
    assert!(report.to_string().contains("mismatched viewport"));
}

#[test]
fn conformance_suite_captures_action_errors_without_aborting_later_scenarios() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let rejected = ConformanceScenario::new("rejected pan", graph.clone()).with_actions([
        ConformanceAction::apply_viewport_pan(ViewportPanRequest::new(CanvasPoint {
            x: f32::NAN,
            y: 0.0,
        })),
    ]);
    let matching = ConformanceScenario::new("later matching viewport", graph)
        .with_actions([ConformanceAction::set_viewport(
            CanvasPoint { x: 1.0, y: 2.0 },
            1.25,
        )])
        .with_expected_trace([ConformanceTraceEvent::viewport(
            CanvasPoint { x: 1.0, y: 2.0 },
            1.25,
        )]);
    let suite = ConformanceSuite::new("adapter error suite").with_scenarios([rejected, matching]);

    let report = run_conformance_suite(&suite);

    assert!(!report.is_match(), "{report}");
    assert_eq!(report.scenario_reports.len(), 1);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].scenario, "rejected pan");
    assert!(report.to_string().contains("rejected pan"));
}
