use super::*;
use crate::runtime::selection::NodePointerDownInput;
use jellyflow_core::core::{EdgeId, NodeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

fn node_pointer_down_selection_trace(node_id: NodeId) -> Vec<ConformanceTraceEvent> {
    selection_trace(vec![node_id], Vec::new())
}

fn selection_trace(nodes: Vec<NodeId>, edges: Vec<EdgeId>) -> Vec<ConformanceTraceEvent> {
    vec![
        ConformanceTraceEvent::selection(nodes.clone(), edges.clone(), Vec::new()),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
            changes: vec![ConformanceViewChange::Selection {
                nodes: nodes.clone(),
                edges: edges.clone(),
                groups: Vec::new(),
            }],
        }),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::SelectionChange {
            nodes,
            edges,
            groups: Vec::new(),
        }),
    ]
}

fn node_pointer_down_view_state(other: NodeId, edge_id: EdgeId) -> crate::io::NodeGraphViewState {
    let mut view_state = crate::io::NodeGraphViewState::default();
    view_state.set_selection(vec![other], vec![edge_id], Vec::new());
    view_state
}

#[test]
fn conformance_runner_executes_keyboard_nudge_fixture_and_matches_trace() {
    let (graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let mut view_state = crate::io::NodeGraphViewState::default();
    view_state.set_selection(vec![node_id], Vec::new(), Vec::new());
    view_state.zoom = 2.0;

    let scenario = ConformanceScenario::new("keyboard nudge runner", graph)
        .with_view_state(view_state)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_node_nudge(NodeNudgeRequest {
            direction: NodeNudgeDirection::Right,
            fast: false,
        })])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_NUDGE_TRANSACTION_LABEL),
                ["set_node_pos"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_NUDGE_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_executes_delete_selection_fixture_and_matches_trace() {
    let (graph, node_id, _b, out_port, in_port, edge_id) = make_graph();
    let mut view_state = crate::io::NodeGraphViewState::default();
    view_state.set_selection(vec![node_id], vec![edge_id], Vec::new());
    let disconnected = EdgeConnection::new(edge_id, out_port, in_port, EdgeKind::Data);

    let scenario = ConformanceScenario::new("delete selection runner", graph)
        .with_view_state(view_state)
        .with_xyflow_callbacks()
        .with_delete_selection_contract(
            ConformanceDeleteSelectionContract::new(1, 1)
                .for_key(keyboard_types::Code::Backspace)
                .with_disconnected([disconnected]),
        );

    let report = run_conformance_scenario(&scenario).expect("fixture should run");
    let expected_trace = scenario.expanded_expected_trace();

    assert!(report.is_match(), "{report}");
    assert!(scenario.actions.is_empty());
    assert_eq!(scenario.expanded_actions().len(), 1);
    assert_eq!(report.actual_trace(), expected_trace.as_slice());
}

#[test]
fn conformance_runner_expands_delete_selection_during_node_drag_behavior_contract() {
    let (graph, node_id, _b, out_port, in_port, edge_id) = make_graph();
    let mut view_state = crate::io::NodeGraphViewState::default();
    view_state.set_selection(vec![node_id], Vec::new(), Vec::new());
    let disconnected = EdgeConnection::new(edge_id, out_port, in_port, EdgeKind::Data);
    let start = NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
    };
    let end = NodeDragEnd {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
        outcome: NodeDragEndOutcome::Canceled,
    };

    let scenario = ConformanceScenario::new("delete during node drag behavior contract", graph)
        .with_view_state(view_state)
        .with_xyflow_callbacks()
        .with_delete_selection_during_node_drag_contract(
            ConformanceDeleteSelectionDuringNodeDragContract::new(
                start,
                end,
                ConformanceDeleteSelectionContract::new(1, 1)
                    .for_key(keyboard_types::Code::Backspace)
                    .with_disconnected([disconnected]),
            ),
        );

    let report = run_conformance_scenario(&scenario).expect("fixture should run");
    let expected_trace = scenario.expanded_expected_trace();

    assert!(report.is_match(), "{report}");
    assert_eq!(scenario.actions.len(), 0);
    assert_eq!(scenario.behaviors.len(), 1);
    assert_eq!(scenario.expanded_actions().len(), 3);
    assert_eq!(report.actual_trace(), expected_trace.as_slice());
}

#[test]
fn conformance_runner_asserts_connection_target_from_handle_candidates() {
    let (graph, source_node, target_node, out_port, in_port, _edge_id) = make_graph();
    let source = ConnectionHandleRef::new(source_node, out_port, PortDirection::Out);
    let blocked_near = ConnectionTargetCandidate::new(
        ConnectionTargetHandle::new(
            ConnectionHandleRef::new(target_node, in_port, PortDirection::In),
            true,
            false,
        ),
        connection_node_rect(CanvasPoint { x: 0.0, y: 0.0 }),
        connection_handle_bounds(CanvasPoint { x: 10.0, y: 10.0 }),
    );
    let valid_far = ConnectionTargetCandidate::new(
        ConnectionTargetHandle::new(
            ConnectionHandleRef::new(target_node, in_port, PortDirection::Out),
            true,
            true,
        ),
        connection_node_rect(CanvasPoint { x: 0.0, y: 0.0 }),
        connection_handle_bounds(CanvasPoint { x: 80.0, y: 80.0 }),
    );
    let candidates = [valid_far, blocked_near];
    let input = ConnectionTargetFromHandlesInput::new(
        CanvasPoint { x: 15.0, y: 15.0 },
        120.0,
        source,
        &candidates,
        NodeGraphConnectionMode::Strict,
    );
    let expected = ResolvedConnectionTarget {
        target: Some(blocked_near.target),
        connection: Some(ConnectionHandleConnection {
            source,
            target: blocked_near.target.handle,
        }),
        is_handle_valid: false,
        feedback: ConnectionHandleValidity::Invalid,
    };
    let scenario = ConformanceScenario::new("connection target candidates runner", graph)
        .with_actions([ConformanceAction::assert_connection_target_from_handles(
            input, expected,
        )])
        .with_expected_trace([]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_executes_node_drag_fixture_and_matches_trace() {
    let (graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let start = NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
    };
    let target = CanvasPoint { x: 32.0, y: 16.0 };
    let update = NodeDragUpdate {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
    };
    let start_event = NodeGraphGestureEvent::NodeDragStart(start.clone());
    let update_event = NodeGraphGestureEvent::NodeDragUpdate(update.clone());

    let scenario = ConformanceScenario::new("node drag runner", graph)
        .with_xyflow_callbacks()
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
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
    assert!(report.mismatches().is_empty());
}

#[test]
fn conformance_runner_expands_behavior_contracts_and_matches_trace() {
    let (graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let start = CanvasPoint { x: 1.0, y: 2.0 };
    let target = CanvasPoint { x: 32.0, y: 16.0 };
    let scenario = ConformanceScenario::new("node drag behavior contract runner", graph)
        .with_xyflow_callbacks()
        .with_behaviors([ConformanceBehavior::node_drag_session(
            ConformanceNodeDragSessionContract::new(node_id, start, target),
        )]);

    assert!(scenario.actions.is_empty());
    assert!(scenario.expected_trace.is_empty());
    assert_eq!(scenario.behaviors.len(), 1);
    assert_eq!(scenario.expanded_actions().len(), 1);
    assert_eq!(
        scenario.expanded_actions()[0].kind(),
        "apply_node_drag_session"
    );

    let report = run_conformance_scenario(&scenario).expect("fixture should run");
    let expected_trace = scenario.expanded_expected_trace();

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), expected_trace.as_slice());

    let encoded = serde_json::to_value(&scenario).expect("serialize behavior fixture");
    assert!(encoded.get("behaviors").is_some());
    assert!(encoded.get("actions").is_none());
    assert!(encoded.get("expected_trace").is_none());
    let decoded: ConformanceScenario =
        serde_json::from_value(encoded).expect("deserialize behavior fixture");
    assert_eq!(decoded.behaviors.len(), 1);
    assert_eq!(decoded.expanded_actions().len(), 1);
}

#[test]
fn conformance_runner_expands_node_resize_session_behavior_contract() {
    let (mut graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    graph
        .update_node(&node_id, |node| {
            node.size = Some(CanvasSize {
                width: 100.0,
                height: 60.0,
            })
        })
        .expect("node exists");
    let direction = NodeResizeDirection::BottomRight;
    let start_pointer = CanvasPoint { x: 110.0, y: 60.0 };
    let current_pointer = CanvasPoint { x: 150.0, y: 90.0 };
    let request = NodePointerResizeRequest::new(node_id, start_pointer, current_pointer, direction);
    let update = NodeResizeUpdate {
        node: node_id,
        direction,
        pointer: current_pointer,
        position: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 140.0,
            height: 90.0,
        },
    };
    let scenario = ConformanceScenario::new("node resize behavior contract runner", graph)
        .with_xyflow_callbacks()
        .with_node_resize_session_contract(ConformanceNodeResizeSessionContract::new(
            request, update,
        ));

    assert!(scenario.actions.is_empty());
    assert!(scenario.expected_trace.is_empty());
    assert_eq!(scenario.behaviors.len(), 1);
    assert_eq!(scenario.expanded_actions().len(), 1);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");
    let expected_trace = scenario.expanded_expected_trace();

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), expected_trace.as_slice());
}

#[test]
fn conformance_runner_expands_layout_facts_behavior_contract() {
    let (graph, source_node, target_node, out_port, in_port, edge_id) = make_graph();
    let source = ConnectionHandleRef::new(source_node, out_port, PortDirection::Out);
    let target = ConnectionHandleRef::new(target_node, in_port, PortDirection::In);
    let source_measurement = NodeMeasurement::new(source_node)
        .with_size(Some(CanvasSize {
            width: 100.0,
            height: 100.0,
        }))
        .with_handles([MeasuredHandle::new(
            source,
            HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 90.0, y: 40.0 },
                    size: CanvasSize {
                        width: 10.0,
                        height: 20.0,
                    },
                },
                position: HandlePosition::Right,
            },
        )]);
    let target_measurement = NodeMeasurement::new(target_node)
        .with_size(Some(CanvasSize {
            width: 100.0,
            height: 100.0,
        }))
        .with_handles([MeasuredHandle::new(
            target,
            HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 0.0, y: 40.0 },
                    size: CanvasSize {
                        width: 10.0,
                        height: 20.0,
                    },
                },
                position: HandlePosition::Left,
            },
        )]);
    let connection_target = ConnectionTargetHandle::new(target, true, true);
    let expected_target = ResolvedConnectionTarget {
        target: Some(connection_target),
        connection: Some(ConnectionHandleConnection {
            source,
            target: connection_target.handle,
        }),
        is_handle_valid: true,
        feedback: ConnectionHandleValidity::Valid,
    };
    let mut expected_visible_node_ids = vec![source_node, target_node];
    expected_visible_node_ids.sort();
    let expected = ConformanceLayoutFactsExpectation::new(expected_visible_node_ids, [edge_id])
        .with_edge_positions([ConformanceLayoutEdgePosition::new(
            edge_id,
            ConformanceEdgeEndpointPosition::new(
                CanvasPoint { x: 100.0, y: 50.0 },
                HandlePosition::Right,
            ),
            ConformanceEdgeEndpointPosition::new(
                CanvasPoint { x: 100.0, y: 50.0 },
                HandlePosition::Left,
            ),
        )])
        .with_connection_target(ConformanceLayoutFactsConnectionTargetExpectation::new(
            CanvasPoint { x: 105.0, y: 50.0 },
            source,
            expected_target,
        ));
    let scenario = ConformanceScenario::new("layout facts behavior contract runner", graph)
        .with_layout_facts_contract(ConformanceLayoutFactsContract::new(
            [source_measurement, target_measurement],
            CanvasSize {
                width: 320.0,
                height: 160.0,
            },
            expected,
        ));

    assert!(scenario.actions.is_empty());
    assert!(scenario.expected_trace.is_empty());
    assert_eq!(scenario.behaviors.len(), 1);
    assert_eq!(scenario.expanded_actions().len(), 3);
    assert!(scenario.expanded_expected_trace().is_empty());

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert!(report.actual_trace().is_empty());
}

fn connection_node_rect(origin: CanvasPoint) -> CanvasRect {
    CanvasRect {
        origin,
        size: CanvasSize {
            width: 200.0,
            height: 120.0,
        },
    }
}

fn connection_handle_bounds(origin: CanvasPoint) -> HandleBounds {
    HandleBounds {
        rect: CanvasRect {
            origin,
            size: CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        },
        position: HandlePosition::Right,
    }
}

#[test]
fn conformance_runner_executes_node_drag_parent_expansion_fixture_and_matches_trace() {
    let (mut graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let parent_id = GroupId::from_u128(200);
    fixture_insert_group(
        &mut graph,
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
    graph
        .update_node(&node_id, |node| {
            node.parent = Some(parent_id);
            node.extent = Some(NodeExtent::Parent);
            node.expand_parent = Some(true);
            node.size = Some(CanvasSize {
                width: 20.0,
                height: 20.0,
            });
        })
        .expect("node exists");

    let target = CanvasPoint { x: 95.0, y: 95.0 };
    let scenario = ConformanceScenario::new("node drag parent expansion runner", graph)
        .with_xyflow_callbacks()
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
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_executes_node_resize_fixture_and_matches_trace() {
    let (mut graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    graph
        .update_node(&node_id, |node| {
            node.size = Some(CanvasSize {
                width: 100.0,
                height: 60.0,
            })
        })
        .expect("node exists");

    let scenario = ConformanceScenario::new("node resize runner", graph)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_node_resize(
            NodeResizeRequest::new(
                node_id,
                CanvasSize {
                    width: 140.0,
                    height: 80.0,
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
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_executes_node_pointer_resize_fixture_and_matches_trace() {
    let (mut graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    graph
        .update_node(&node_id, |node| {
            node.size = Some(CanvasSize {
                width: 100.0,
                height: 60.0,
            })
        })
        .expect("node exists");

    let scenario = ConformanceScenario::new("node pointer resize runner", graph)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_node_pointer_resize(
            NodePointerResizeRequest::new(
                node_id,
                CanvasPoint { x: 110.0, y: 60.0 },
                CanvasPoint { x: 150.0, y: 90.0 },
                NodeResizeDirection::BottomRight,
            ),
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
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_executes_node_resize_parent_expansion_fixture_and_matches_trace() {
    let (mut graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let parent_id = GroupId::from_u128(200);
    fixture_insert_group(
        &mut graph,
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
    graph
        .update_node(&node_id, |node| {
            node.parent = Some(parent_id);
            node.extent = Some(NodeExtent::Parent);
            node.expand_parent = Some(true);
            node.size = Some(CanvasSize {
                width: 20.0,
                height: 20.0,
            });
        })
        .expect("node exists");

    let scenario = ConformanceScenario::new("node resize parent expansion runner", graph)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_node_pointer_resize(
            NodePointerResizeRequest::new(
                node_id,
                CanvasPoint { x: 20.0, y: 20.0 },
                CanvasPoint { x: 120.0, y: 115.0 },
                NodeResizeDirection::BottomRight,
            ),
        )])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_RESIZE_TRANSACTION_LABEL),
                ["set_node_size", "set_group_rect"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_RESIZE_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_executes_node_resize_lifecycle_fixture_and_matches_trace() {
    let (mut graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    graph
        .update_node(&node_id, |node| {
            node.size = Some(CanvasSize {
                width: 100.0,
                height: 60.0,
            })
        })
        .expect("node exists");

    let direction = NodeResizeDirection::BottomRight;
    let start_pointer = CanvasPoint { x: 110.0, y: 60.0 };
    let current_pointer = CanvasPoint { x: 150.0, y: 90.0 };
    let start = NodeResizeStart {
        node: node_id,
        direction,
        pointer: start_pointer,
    };
    let update = NodeResizeUpdate {
        node: node_id,
        direction,
        pointer: current_pointer,
        position: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 140.0,
            height: 90.0,
        },
    };
    let end = NodeResizeEnd {
        node: node_id,
        direction,
        pointer: current_pointer,
        outcome: NodeResizeEndOutcome::Committed,
    };
    let start_event = NodeGraphGestureEvent::NodeResizeStart(start.clone());
    let update_event = NodeGraphGestureEvent::NodeResizeUpdate(update.clone());
    let end_event = NodeGraphGestureEvent::NodeResizeEnd(end.clone());

    let scenario = ConformanceScenario::new("node resize lifecycle runner", graph)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_node_pointer_resize_session(
            NodePointerResizeRequest::new(node_id, start_pointer, current_pointer, direction),
        )])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeResizeStart(start)),
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
            ConformanceTraceEvent::gesture(update_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeResize(update)),
            ConformanceTraceEvent::gesture(end_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeResizeEnd(end)),
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_reports_noop_node_pointer_resize_as_action_error() {
    let (mut graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    graph
        .update_node(&node_id, |node| {
            node.size = Some(CanvasSize {
                width: 100.0,
                height: 60.0,
            })
        })
        .expect("node exists");

    let scenario =
        ConformanceScenario::new("noop node pointer resize runner", graph).with_actions([
            ConformanceAction::apply_node_pointer_resize(NodePointerResizeRequest::new(
                node_id,
                CanvasPoint { x: 110.0, y: 60.0 },
                CanvasPoint { x: 110.0, y: 60.0 },
                NodeResizeDirection::BottomRight,
            )),
        ]);

    let err = run_conformance_scenario(&scenario).expect_err("noop action should fail");

    assert!(err.to_string().contains("apply_node_pointer_resize"));
}

#[test]
fn conformance_runner_asserts_rendering_query_without_trace() {
    let (mut graph, node_id, outside, _out_port, _in_port, edge_id) = make_graph();
    graph
        .update_node(&node_id, |node| {
            node.size = Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            })
        })
        .expect("node exists");
    graph
        .update_node(&outside, |node| {
            node.pos = CanvasPoint { x: 140.0, y: 0.0 };
            node.size = Some(CanvasSize {
                width: 40.0,
                height: 40.0,
            });
        })
        .expect("node exists");
    let partial = NodeId::new();
    let mut partial_node = graph.nodes().get(&node_id).expect("node exists").clone();
    partial_node.pos = CanvasPoint { x: 95.0, y: 0.0 };
    partial_node.clear_ports();
    fixture_insert_node(&mut graph, partial, partial_node);
    let mut view_state = crate::io::NodeGraphViewState {
        draw_order: vec![outside, node_id, partial],
        edge_draw_order: vec![edge_id],
        ..crate::io::NodeGraphViewState::default()
    };
    view_state.set_selection(vec![node_id], vec![edge_id], Vec::new());
    let viewport_size = CanvasSize {
        width: 100.0,
        height: 100.0,
    };
    let expected = crate::runtime::store::NodeGraphStore::new(
        graph.clone(),
        view_state.clone(),
        crate::io::NodeGraphEditorConfig::default(),
    )
    .rendering_query(viewport_size);
    assert_eq!(expected.node_order, vec![outside, partial, node_id]);
    assert_eq!(expected.visible_node_render_order, vec![partial, node_id]);
    assert_eq!(expected.visible_edge_ids, vec![edge_id]);
    assert_eq!(expected.visible_edge_render_order, vec![edge_id]);

    let scenario = ConformanceScenario::new("rendering query runner", graph)
        .with_view_state(view_state)
        .with_rendering_query_contract(ConformanceRenderingQueryContract::new(
            viewport_size,
            expected,
        ));

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert!(report.actual_trace().is_empty());
    assert_eq!(scenario.expanded_actions().len(), 1);
    assert!(scenario.expanded_expected_trace().is_empty());
}

#[test]
fn conformance_runner_keeps_dispatch_transaction_as_low_level_graph_fixture_action() {
    let (graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let from = graph.nodes().get(&node_id).expect("node exists").pos;
    let to = CanvasPoint { x: 24.0, y: 12.0 };
    let label = "low-level fixture move";
    let transaction = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: node_id,
        from,
        to,
    }])
    .with_label(label);

    let scenario = ConformanceScenario::new("low-level graph transaction runner", graph)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::dispatch_transaction(transaction)])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(Some(label), ["set_node_pos"]),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(label.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_executes_selection_box_fixture_and_matches_trace() {
    let (graph, node_id, other, _out_port, _in_port, edge_id) = make_graph();
    let view_state = node_pointer_down_view_state(other, edge_id);
    let rect = CanvasRect {
        origin: CanvasPoint { x: -5.0, y: -5.0 },
        size: CanvasSize {
            width: 20.0,
            height: 20.0,
        },
    };

    let scenario = ConformanceScenario::new("selection box runner", graph)
        .with_view_state(view_state)
        .with_xyflow_callbacks()
        .with_selection_box_contract(ConformanceSelectionBoxContract::new(
            SelectionBoxInput::new(
                rect,
                SelectionBoxOptions {
                    fallback_size: Some(CanvasSize {
                        width: 10.0,
                        height: 10.0,
                    }),
                    ..SelectionBoxOptions::default()
                },
            ),
            [node_id],
            [edge_id],
        ));

    let report = run_conformance_scenario(&scenario).expect("fixture should run");
    let expected_trace = scenario.expanded_expected_trace();

    assert!(report.is_match(), "{report}");
    assert!(scenario.actions.is_empty());
    assert_eq!(scenario.expanded_actions().len(), 1);
    assert_eq!(report.actual_trace(), expected_trace.as_slice());
}

#[test]
fn conformance_runner_reports_compact_trace_mismatches() {
    let (graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let target = CanvasPoint { x: 32.0, y: 16.0 };
    let scenario = ConformanceScenario::new("node drag mismatch", graph)
        .with_actions([ConformanceAction::apply_node_drag(node_id, target)])
        .with_expected_trace([ConformanceTraceEvent::graph_commit(
            Some(NODE_DRAG_TRANSACTION_LABEL),
            ["wrong_op_kind"],
        )]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");
    let rendered = report.to_string();

    assert!(!report.is_match());
    assert_eq!(report.mismatches().len(), 1);
    assert_eq!(report.mismatches()[0].index, 0);
    assert!(rendered.contains("node drag mismatch"));
    assert!(rendered.contains("wrong_op_kind"));
    assert!(rendered.contains("set_node_pos"));
}

#[test]
fn conformance_runner_errors_when_mutating_action_produces_no_commit() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let scenario = ConformanceScenario::new("empty delete runner", graph)
        .with_actions([ConformanceAction::apply_delete_selection()]);

    let err = run_conformance_scenario(&scenario).expect_err("empty delete should error");

    assert_eq!(err.scenario, "empty delete runner");
    assert_eq!(err.action_index, 0);
    assert_eq!(err.action_kind, "apply_delete_selection");
    assert!(err.message.contains("produced no commit"));
}

#[test]
fn conformance_runner_executes_node_pointer_down_fixture_and_matches_selection_trace() {
    let (graph, node_id, other, _out_port, _in_port, edge_id) = make_graph();
    let view_state = node_pointer_down_view_state(other, edge_id);

    let scenario = ConformanceScenario::new("node pointer down runner", graph)
        .with_view_state(view_state)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_node_pointer_down(
            node_id,
            false,
            CanvasPoint { x: 3.0, y: 4.0 },
        )])
        .with_expected_trace(node_pointer_down_selection_trace(node_id));

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_expands_node_pointer_down_selection_behavior_contract() {
    let (graph, node_id, other, _out_port, _in_port, edge_id) = make_graph();
    let view_state = node_pointer_down_view_state(other, edge_id);

    let scenario = ConformanceScenario::new("node pointer down behavior contract runner", graph)
        .with_view_state(view_state)
        .with_xyflow_callbacks()
        .with_node_pointer_down_selection_contract(
            ConformanceNodePointerDownSelectionContract::new(
                NodePointerDownInput::new(node_id, false, CanvasPoint { x: 3.0, y: 4.0 }),
                PointerGestureClaim::NodeDrag,
                [node_id],
                std::iter::empty::<EdgeId>(),
            ),
        );

    assert!(scenario.actions.is_empty());
    assert_eq!(scenario.behaviors.len(), 1);
    assert_eq!(scenario.expanded_actions().len(), 1);
    assert_eq!(
        scenario.expanded_actions()[0].kind(),
        "apply_node_pointer_down"
    );

    let report = run_conformance_scenario(&scenario).expect("fixture should run");
    let expected_trace = scenario.expanded_expected_trace();

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), expected_trace.as_slice());

    let encoded = serde_json::to_value(&scenario).expect("serialize behavior fixture");
    assert!(encoded.get("behaviors").is_some());
    assert!(encoded.get("actions").is_none());
    assert!(encoded.get("expected_trace").is_none());
    let decoded: ConformanceScenario =
        serde_json::from_value(encoded).expect("deserialize behavior fixture");
    assert_eq!(decoded.behaviors.len(), 1);
    assert_eq!(decoded.expanded_actions().len(), 1);
}

#[test]
fn conformance_runner_expands_node_pointer_down_selection_behavior_contract_without_drag_claim() {
    let (graph, node_id, other, _out_port, _in_port, edge_id) = make_graph();
    let view_state = node_pointer_down_view_state(other, edge_id);

    let scenario = ConformanceScenario::new("node pointer down behavior contract none", graph)
        .with_view_state(view_state)
        .with_xyflow_callbacks()
        .with_node_pointer_down_selection_contract(
            ConformanceNodePointerDownSelectionContract::new(
                NodePointerDownInput::new(node_id, false, CanvasPoint::default()),
                PointerGestureClaim::None,
                [node_id],
                std::iter::empty::<EdgeId>(),
            ),
        );

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(
        report.actual_trace(),
        scenario.expanded_expected_trace().as_slice()
    );
}

#[test]
fn conformance_runner_executes_node_pointer_down_then_drag_chain() {
    let (graph, node_id, other, _out_port, _in_port, edge_id) = make_graph();
    let view_state = node_pointer_down_view_state(other, edge_id);

    let start = NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
    };
    let start_event = NodeGraphGestureEvent::NodeDragStart(start.clone());

    let target = CanvasPoint { x: 32.0, y: 16.0 };
    let update = NodeDragUpdate {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
    };
    let update_event = NodeGraphGestureEvent::NodeDragUpdate(update.clone());

    let mut expected_trace = node_pointer_down_selection_trace(node_id);
    expected_trace.extend([
        ConformanceTraceEvent::gesture(start_event.clone()),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDragStart(start.clone())),
        ConformanceTraceEvent::graph_commit(Some(NODE_DRAG_TRANSACTION_LABEL), ["set_node_pos"]),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
            label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
        }),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
            nodes: 1,
            edges: 0,
        }),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ConformanceTraceEvent::gesture(update_event.clone()),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDrag(update.clone())),
    ]);

    let scenario = ConformanceScenario::new("node pointer down drag chain", graph)
        .with_view_state(view_state)
        .with_xyflow_callbacks()
        .with_actions([
            ConformanceAction::apply_node_pointer_down(
                node_id,
                false,
                CanvasPoint { x: 3.0, y: 4.0 },
            ),
            ConformanceAction::emit_gesture(start_event.clone()),
            ConformanceAction::apply_node_drag(node_id, target),
            ConformanceAction::emit_gesture(update_event.clone()),
        ])
        .with_expected_trace(expected_trace);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}
