use std::path::Path;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphId, Group, GroupId,
    Node, NodeExtent, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
    PortKind,
};
use jellyflow_runtime::io::{NodeGraphPanInertiaTuning, NodeGraphViewState};
use jellyflow_runtime::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceFixtureDirectory,
    ConformanceFixtureDirectoryApprovalReport, ConformanceFixtureDirectoryReport,
    ConformanceRunReport, ConformanceScenario, ConformanceSuite, ConformanceSuiteReport,
    ConformanceTraceConfig, ConformanceTraceEvent, ConformanceViewChange,
};
use jellyflow_runtime::runtime::delete::DELETE_SELECTION_TRANSACTION_LABEL;
use jellyflow_runtime::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use jellyflow_runtime::runtime::events::{
    NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent, ViewportMove, ViewportMoveEnd,
    ViewportMoveEndOutcome, ViewportMoveKind, ViewportMoveStart,
};
use jellyflow_runtime::runtime::resize::{
    NODE_RESIZE_TRANSACTION_LABEL, NodeResizeDirection, NodeResizeRequest,
};
use jellyflow_runtime::runtime::viewport::{
    ViewportAnimationEasing, ViewportAnimationOptions, ViewportAnimationPlan,
    ViewportAnimationRequest, ViewportDoubleClickZoomInput, ViewportPanInertiaRequest,
    ViewportPanRequest, ViewportTransform, plan_viewport_pan_inertia,
};
use jellyflow_runtime::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};

pub fn adapter_smoke_suite() -> ConformanceSuite {
    ConformanceSuite::new("headless adapter template")
        .with_scenarios([
            node_drag_scenario(),
            node_drag_parent_expansion_scenario(),
            node_resize_scenario(),
            delete_selection_scenario(),
            viewport_pan_scenario(),
            viewport_animation_scenario(),
            viewport_pan_inertia_scenario(),
        ])
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

pub fn run_node_drag_parent_expansion_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &node_drag_parent_expansion_scenario(),
    )
    .map_err(|err| err.to_string())
}

pub fn run_node_resize_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(&node_resize_scenario())
        .map_err(|err| err.to_string())
}

pub fn run_delete_selection_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &delete_selection_scenario(),
    )
    .map_err(|err| err.to_string())
}

pub fn run_viewport_animation_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &viewport_animation_scenario(),
    )
    .map_err(|err| err.to_string())
}

pub fn run_viewport_pan_inertia_smoke() -> Result<ConformanceRunReport, String> {
    jellyflow_runtime::runtime::conformance::run_conformance_scenario(
        &viewport_pan_inertia_scenario(),
    )
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

fn node_drag_parent_expansion_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(3);
    let parent_id = GroupId::from_u128(30);
    let graph = graph_with_parent_expanding_node(node_id, parent_id);
    let target = CanvasPoint { x: 95.0, y: 95.0 };

    ConformanceScenario::new("template node drag parent expansion", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_node_drag(node_id, target)])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_DRAG_TRANSACTION_LABEL),
                ["set_node_pos", "set_group_rect"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ])
}

fn node_resize_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(4);
    let graph = graph_with_node(node_id);

    ConformanceScenario::new("template node resize", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_node_resize(
            NodeResizeRequest::new(
                node_id,
                CanvasSize {
                    width: 220.0,
                    height: 120.0,
                },
            )
            .with_direction(NodeResizeDirection::BottomRight),
        )])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_RESIZE_TRANSACTION_LABEL),
                ["set_node_size"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_RESIZE_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ])
}

fn delete_selection_scenario() -> ConformanceScenario {
    let node_id = NodeId::from_u128(5);
    let sibling_id = NodeId::from_u128(6);
    let out_port = PortId::from_u128(50);
    let in_port = PortId::from_u128(60);
    let edge_id = EdgeId::from_u128(500);
    let graph = graph_with_connected_nodes(node_id, sibling_id, out_port, in_port, edge_id);
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![node_id], vec![edge_id], Vec::new());
    let disconnected = EdgeConnection::new(edge_id, out_port, in_port, EdgeKind::Data);

    ConformanceScenario::new("template delete selection", graph)
        .with_view_state(view_state)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_delete_selection_for_key(
            keyboard_types::Code::Backspace,
        )])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(DELETE_SELECTION_TRANSACTION_LABEL),
                ["remove_node"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(DELETE_SELECTION_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 1,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::EdgesChange { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectionChange(
                ConnectionChange::Disconnected(disconnected),
            )),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Disconnect(disconnected)),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesDelete { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::EdgesDelete { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Delete {
                nodes: 1,
                edges: 1,
                groups: 0,
                sticky_notes: 0,
            }),
            ConformanceTraceEvent::selection(Vec::new(), Vec::new(), Vec::new()),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Selection {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    groups: Vec::new(),
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::SelectionChange {
                nodes: Vec::new(),
                edges: Vec::new(),
                groups: Vec::new(),
            }),
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

fn viewport_animation_scenario() -> ConformanceScenario {
    let graph = Graph::new(GraphId::from_u128(11));
    let from = ViewportTransform::new(CanvasPoint { x: 0.0, y: 0.0 }, 1.0)
        .expect("valid viewport");
    let to = ViewportTransform::new(CanvasPoint { x: 80.0, y: -40.0 }, 2.0)
        .expect("valid viewport");
    let midpoint_pan = CanvasPoint { x: 40.0, y: -20.0 };
    let endpoint_pan = CanvasPoint { x: 80.0, y: -40.0 };

    let double_click_current =
        ViewportTransform::new(CanvasPoint { x: 10.0, y: 20.0 }, 2.0)
            .expect("valid viewport");
    let double_click_target =
        ViewportTransform::new(CanvasPoint { x: -10.0, y: 10.0 }, 3.0)
            .expect("valid viewport");
    let expected_plan = ViewportAnimationPlan {
        from: double_click_current,
        to: double_click_target,
        duration_seconds: 0.2,
        easing: ViewportAnimationEasing::CubicInOut,
    };

    ConformanceScenario::new("template viewport animation", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::apply_viewport_animation_frames(
                ViewportAnimationRequest::new(from, to, ViewportAnimationOptions::new(1.0)),
                [0.5, 1.0],
            ),
            ConformanceAction::assert_viewport_double_click_zoom(
                ViewportDoubleClickZoomInput::new(
                    double_click_current,
                    CanvasPoint { x: 120.0, y: 60.0 },
                    2.0,
                    0.5,
                    3.0,
                    ViewportAnimationOptions::new(0.2),
                ),
                expected_plan,
            ),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::viewport(midpoint_pan, 1.5),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport {
                    pan: midpoint_pan,
                    zoom: 1.5,
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan: midpoint_pan,
                zoom: 1.5,
            }),
            ConformanceTraceEvent::viewport(endpoint_pan, 2.0),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport {
                    pan: endpoint_pan,
                    zoom: 2.0,
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan: endpoint_pan,
                zoom: 2.0,
            }),
        ])
}

fn viewport_pan_inertia_scenario() -> ConformanceScenario {
    let graph = Graph::new(GraphId::from_u128(12));
    let tuning = NodeGraphPanInertiaTuning {
        enabled: true,
        decay_per_s: 2.0,
        min_speed: 100.0,
        max_speed: 1000.0,
    };
    let request = ViewportPanInertiaRequest::new(
        ViewportTransform::new(CanvasPoint::default(), 2.0).expect("valid viewport"),
        CanvasPoint { x: 1000.0, y: 0.0 },
        tuning.clone(),
    );
    let plan = plan_viewport_pan_inertia(request.clone()).expect("inertia plan");
    let mid = plan.frame_at(0.5).expect("mid inertia frame");
    let terminal = plan.terminal_frame().expect("terminal inertia frame");

    ConformanceScenario::new("template viewport pan inertia", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::apply_viewport_pan_inertia_frames(
                request,
                [0.5, plan.duration_seconds],
            ),
            ConformanceAction::expect_viewport_pan_inertia_rejected(
                ViewportPanInertiaRequest::new(
                    ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid viewport"),
                    CanvasPoint { x: 50.0, y: 0.0 },
                    tuning,
                ),
            ),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::viewport(mid.transform.pan, mid.transform.zoom),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport {
                    pan: mid.transform.pan,
                    zoom: mid.transform.zoom,
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan: mid.transform.pan,
                zoom: mid.transform.zoom,
            }),
            ConformanceTraceEvent::viewport(terminal.transform.pan, terminal.transform.zoom),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport {
                    pan: terminal.transform.pan,
                    zoom: terminal.transform.zoom,
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan: terminal.transform.pan,
                zoom: terminal.transform.zoom,
            }),
        ])
}

fn graph_with_node(node_id: NodeId) -> Graph {
    let mut graph = Graph::new(GraphId::from_u128(1));
    graph.nodes.insert(
        node_id,
        template_node(
            CanvasPoint { x: 10.0, y: 20.0 },
            Vec::new(),
            Some(CanvasSize {
                width: 160.0,
                height: 80.0,
            }),
        ),
    );
    graph
}

fn graph_with_connected_nodes(
    source_id: NodeId,
    target_id: NodeId,
    out_port: PortId,
    in_port: PortId,
    edge_id: EdgeId,
) -> Graph {
    let mut graph = Graph::new(GraphId::from_u128(2));
    graph.nodes.insert(
        source_id,
        template_node(
            CanvasPoint { x: 10.0, y: 20.0 },
            vec![out_port],
            Some(CanvasSize {
                width: 160.0,
                height: 80.0,
            }),
        ),
    );
    graph.nodes.insert(
        target_id,
        template_node(
            CanvasPoint { x: 260.0, y: 20.0 },
            vec![in_port],
            Some(CanvasSize {
                width: 160.0,
                height: 80.0,
            }),
        ),
    );
    graph.ports.insert(
        out_port,
        Port {
            node: source_id,
            key: PortKey::new("out"),
            dir: PortDirection::Out,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );
    graph.ports.insert(
        in_port,
        Port {
            node: target_id,
            key: PortKey::new("in"),
            dir: PortDirection::In,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );
    graph.edges.insert(
        edge_id,
        Edge {
            kind: EdgeKind::Data,
            from: out_port,
            to: in_port,
            hidden: false,
            selectable: None,
            focusable: None,
            interaction_width: None,
            deletable: None,
            reconnectable: None,
        },
    );
    graph
}

fn template_node(pos: CanvasPoint, ports: Vec<PortId>, size: Option<CanvasSize>) -> Node {
    Node {
        kind: NodeKindKey::new("template.node"),
        kind_version: 1,
        pos,
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size,
        hidden: false,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}

fn graph_with_parent_expanding_node(node_id: NodeId, parent_id: GroupId) -> Graph {
    let mut graph = graph_with_node(node_id);
    graph.groups.insert(
        parent_id,
        Group {
            title: "Parent".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );
    let node = graph.nodes.get_mut(&node_id).expect("node exists");
    node.parent = Some(parent_id);
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(true);
    node.size = Some(CanvasSize {
        width: 20.0,
        height: 20.0,
    });
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
        assert_eq!(report.scenario_count(), 7);
    }

    #[test]
    fn node_drag_smoke_runs_as_single_scenario() {
        let report = run_node_drag_smoke().expect("node drag scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn node_drag_parent_expansion_smoke_runs_as_single_scenario() {
        let report = run_node_drag_parent_expansion_smoke()
            .expect("node drag parent expansion scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn node_resize_smoke_runs_as_single_scenario() {
        let report = run_node_resize_smoke().expect("node resize scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn delete_selection_smoke_runs_as_single_scenario() {
        let report = run_delete_selection_smoke().expect("delete selection scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn viewport_animation_smoke_runs_as_single_scenario() {
        let report = run_viewport_animation_smoke().expect("viewport animation scenario runs");

        assert!(report.is_match(), "{report}");
    }

    #[test]
    fn viewport_pan_inertia_smoke_runs_as_single_scenario() {
        let report =
            run_viewport_pan_inertia_smoke().expect("viewport pan inertia scenario runs");

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
        assert_eq!(report.scenario_count(), 7);
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
