use std::path::Path;

use jellyflow_core::{CanvasPoint, CanvasSize, Graph, GraphId, Node, NodeId, NodeKindKey};
use jellyflow_runtime::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceFixtureDirectory,
    ConformanceFixtureDirectoryApprovalReport, ConformanceFixtureDirectoryReport,
    ConformanceRunReport, ConformanceScenario, ConformanceSuite, ConformanceSuiteReport,
    ConformanceTraceConfig, ConformanceTraceEvent, ConformanceViewChange,
};
use jellyflow_runtime::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use jellyflow_runtime::runtime::events::{
    NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent, ViewportMove, ViewportMoveEnd,
    ViewportMoveEndOutcome, ViewportMoveKind, ViewportMoveStart,
};
use jellyflow_runtime::runtime::viewport::ViewportPanRequest;

pub fn adapter_smoke_suite() -> ConformanceSuite {
    ConformanceSuite::new("headless adapter template")
        .with_scenarios([node_drag_scenario(), viewport_pan_scenario()])
}

pub fn check_builtin_suite() -> ConformanceSuiteReport {
    adapter_smoke_suite().run()
}

pub fn check_fixture_directory(
    fixture_dir: impl AsRef<Path>,
) -> Result<ConformanceFixtureDirectoryReport, String> {
    let directory = ConformanceFixtureDirectory::load_json(fixture_dir.as_ref())
        .map_err(|err| err.to_string())?;
    Ok(directory.run())
}

pub fn approve_fixture_directory(
    fixture_dir: impl AsRef<Path>,
) -> Result<ConformanceFixtureDirectoryApprovalReport, String> {
    let directory = ConformanceFixtureDirectory::load_json(fixture_dir.as_ref())
        .map_err(|err| err.to_string())?;
    directory
        .approve_actual_traces_to_json()
        .map_err(|err| err.to_string())
}

pub fn run_node_drag_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(&node_drag_scenario())
        .map_err(|err| err.to_string())
}

fn node_drag_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(2);
    let graph = graph_with_node(node_id);
    let start = NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 12.0, y: 16.0 },
    };
    let target = CanvasPoint { x: 96.0, y: 128.0 };
    let update = NodeDragUpdate {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
    };
    let start_event = NodeGraphGestureEvent::NodeDragStart(start.clone());
    let update_event = NodeGraphGestureEvent::NodeDragUpdate(update.clone());

    ConformanceScenario::new("template node drag", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::emit_gesture(start_event.clone()),
            ConformanceAction::apply_node_drag(node_id, target),
            ConformanceAction::emit_gesture(update_event.clone()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDragStart(start)),
            ConformanceTraceEvent::graph_commit(
                Some(NODE_DRAG_TRANSACTION_LABEL),
                ["set_node_pos"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
            ConformanceTraceEvent::gesture(update_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDrag(update)),
        ])
}

fn viewport_pan_scenario() -> ConformanceScenario {
    let graph = Graph::new(GraphId::from_u128(10));
    let start = ViewportMoveStart {
        kind: ViewportMoveKind::PanDrag,
        pan: CanvasPoint::default(),
        zoom: 1.0,
    };
    let start_event = NodeGraphGestureEvent::ViewportMoveStart(start);
    let pan = CanvasPoint { x: 40.0, y: -10.0 };
    let update = ViewportMove {
        kind: ViewportMoveKind::PanDrag,
        pan,
        zoom: 1.0,
    };
    let update_event = NodeGraphGestureEvent::ViewportMove(update);
    let end = ViewportMoveEnd {
        kind: ViewportMoveKind::PanDrag,
        pan,
        zoom: 1.0,
        outcome: ViewportMoveEndOutcome::Ended,
    };
    let end_event = NodeGraphGestureEvent::ViewportMoveEnd(end);

    ConformanceScenario::new("template viewport pan", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::emit_gesture(start_event.clone()),
            ConformanceAction::apply_viewport_pan(ViewportPanRequest::new(pan)),
            ConformanceAction::emit_gesture(update_event.clone()),
            ConformanceAction::emit_gesture(end_event.clone()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMoveStart(start)),
            ConformanceTraceEvent::viewport(pan, 1.0),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport { pan, zoom: 1.0 }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan,
                zoom: 1.0,
            }),
            ConformanceTraceEvent::gesture(update_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMove(update)),
            ConformanceTraceEvent::gesture(end_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMoveEnd(end)),
        ])
}

fn graph_with_node(node_id: NodeId) -> Graph {
    let mut graph = Graph::new(GraphId::from_u128(1));
    graph.nodes.insert(
        node_id,
        Node {
            kind: NodeKindKey::new("template.node"),
            kind_version: 1,
            pos: CanvasPoint { x: 10.0, y: 20.0 },
            origin: None,
            selectable: None,
            focusable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 160.0,
                height: 80.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::Value::Null,
        },
    );
    graph
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn built_in_headless_suite_matches() {
        let report = check_builtin_suite();

        assert!(report.is_match(), "{report}");
        assert_eq!(report.scenario_count(), 2);
    }

    #[test]
    fn node_drag_smoke_runs_as_single_scenario() {
        let report = run_node_drag_smoke().expect("node drag scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn saved_suite_can_be_checked_as_fixture_directory() {
        let root = temp_fixture_dir("roundtrip");
        std::fs::create_dir_all(&root).expect("create fixture directory");
        adapter_smoke_suite()
            .save_json(root.join("suite.json"))
            .expect("save fixture suite");

        let report = check_fixture_directory(&root).expect("check fixture directory");
        let _ = std::fs::remove_dir_all(&root);

        assert!(report.is_match(), "{report}");
        assert_eq!(report.file_count(), 1);
        assert_eq!(report.scenario_count(), 2);
    }

    fn temp_fixture_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "jellyflow-headless-adapter-template-{name}-{nanos}"
        ))
    }
}
