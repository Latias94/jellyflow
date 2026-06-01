use super::fixtures::make_graph;
use super::harness::{HarnessCallbackEvent, HarnessEvent, InteractionHarness};

use crate::rules::plan_connect;
use crate::runtime::drag::{NODE_DRAG_TRANSACTION_LABEL, NodeDragRequest};
use crate::runtime::events::{
    ConnectDragKind, ConnectEnd, ConnectEndOutcome, ConnectStart, NodeDragEnd, NodeDragEndOutcome,
    NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent,
};
use crate::runtime::geometry::{
    BezierEdgeOptions, EdgeEndpointInput, EdgeHitTestOptions, HandleBounds, HandlePosition,
    bezier_edge_path, edge_path_contains_point, edge_position,
};
use crate::runtime::xyflow::callbacks::{
    ConnectionChange, EdgeConnection, connection_changes_from_transaction,
    delete_changes_from_transaction,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, NodeId, Port, PortCapacity,
    PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::{EdgeEndpoints, GraphOp, GraphTransaction};

#[test]
fn adapter_conformance_connect_dispatches_patch_and_xyflow_projection() {
    let (graph, _a, _b, out_port, in_port, _eid) = make_graph();
    let mut harness = InteractionHarness::new("connect dispatches patch and projection", graph);
    let edge_id = EdgeId::new();
    let edge = Edge {
        kind: EdgeKind::Data,
        from: out_port,
        to: in_port,
        selectable: None,
        deletable: None,
        reconnectable: None,
    };

    let tx = GraphTransaction::from_ops([GraphOp::AddEdge { id: edge_id, edge }])
        .with_label("adapter connect");
    let outcome = harness.dispatch_transaction(&tx).expect("dispatch connect");
    let changes = NodeGraphChanges::from_patch(&outcome.patch);
    let connection_changes = connection_changes_from_transaction(outcome.committed());

    assert_eq!(outcome.committed().label(), Some("adapter connect"));
    assert!(harness.store().graph().edges.contains_key(&edge_id));
    assert!(
        matches!(changes.edges(), [EdgeChange::Add { id, .. }] if *id == edge_id),
        "connect should project to one edge add",
    );
    assert!(
        matches!(connection_changes.as_slice(), [ConnectionChange::Connected(conn)]
            if conn.edge == edge_id && conn.from == out_port && conn.to == in_port),
        "connect should project to one connection event",
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some("adapter connect"),
        &["add_edge"],
    )]);
}

#[test]
fn adapter_conformance_reconnect_preserves_edge_id_and_projects_endpoint_change() {
    let (mut graph, _a, b, out_port, in_port, edge_id) = make_graph();
    let next_in = insert_input_port(&mut graph, b, "in2");
    let mut harness = InteractionHarness::new("reconnect preserves edge id", graph);
    let from = EdgeEndpoints {
        from: out_port,
        to: in_port,
    };
    let to = EdgeEndpoints {
        from: out_port,
        to: next_in,
    };

    let tx = GraphTransaction::from_ops([GraphOp::SetEdgeEndpoints {
        id: edge_id,
        from,
        to,
    }])
    .with_label("adapter reconnect");
    let outcome = harness
        .dispatch_transaction(&tx)
        .expect("dispatch reconnect");
    let changes = NodeGraphChanges::from_patch(&outcome.patch);
    let connection_changes = connection_changes_from_transaction(outcome.committed());

    let edge = harness
        .store()
        .graph()
        .edges
        .get(&edge_id)
        .expect("edge remains");
    assert_eq!(edge.from, out_port);
    assert_eq!(edge.to, next_in);
    assert!(
        matches!(changes.edges(), [EdgeChange::Endpoints { id, from, to }]
            if *id == edge_id && *from == out_port && *to == next_in),
        "reconnect should project to one endpoint change",
    );
    assert!(
        matches!(connection_changes.as_slice(), [ConnectionChange::Reconnected { edge, from: old, to: new }]
            if *edge == edge_id && *old == from && *new == to),
        "reconnect should preserve edge id and expose old/new endpoints",
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some("adapter reconnect"),
        &["set_edge_endpoints"],
    )]);
}

#[test]
fn adapter_conformance_delete_node_cascades_edges_and_projects_delete_payload() {
    let (graph, node_id, _b, out_port, _in_port, edge_id) = make_graph();
    let node = graph.nodes.get(&node_id).expect("node").clone();
    let port = graph.ports.get(&out_port).expect("port").clone();
    let edge = graph.edges.get(&edge_id).expect("edge").clone();
    let mut harness = InteractionHarness::new("delete node cascades edges", graph);

    let tx = GraphTransaction::from_ops([GraphOp::RemoveNode {
        id: node_id,
        node,
        ports: vec![(out_port, port)],
        edges: vec![(edge_id, edge)],
    }])
    .with_label("adapter delete node");
    let outcome = harness
        .dispatch_transaction(&tx)
        .expect("dispatch delete node");
    let changes = NodeGraphChanges::from_patch(&outcome.patch);
    let deleted = delete_changes_from_transaction(outcome.committed());

    assert!(!harness.store().graph().nodes.contains_key(&node_id));
    assert!(!harness.store().graph().edges.contains_key(&edge_id));
    assert!(
        matches!(changes.nodes(), [NodeChange::Remove { id }] if *id == node_id),
        "delete should project to one node remove",
    );
    assert!(
        matches!(changes.edges(), [EdgeChange::Remove { id }] if *id == edge_id),
        "delete should project cascaded edge removal",
    );
    assert_eq!(deleted.nodes(), &[node_id]);
    assert_eq!(deleted.edges(), &[edge_id]);
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some("adapter delete node"),
        &["remove_node"],
    )]);
}

#[test]
fn adapter_conformance_viewport_and_selection_emit_ordered_view_changes() {
    let (graph, node_id, _b, _out_port, _in_port, edge_id) = make_graph();
    let mut harness = InteractionHarness::new("viewport and selection ordering", graph);

    harness.set_viewport(CanvasPoint { x: 10.0, y: 20.0 }, 1.25);
    harness.set_selection(vec![node_id], vec![edge_id], Vec::new());

    harness.assert_events(&[
        HarnessEvent::viewport(CanvasPoint { x: 10.0, y: 20.0 }, 1.25),
        HarnessEvent::selection(vec![node_id], vec![edge_id], Vec::new()),
    ]);
}

#[test]
fn adapter_conformance_harness_records_connect_gesture_lifecycle() {
    let (graph, _a, _b, out_port, in_port, _eid) = make_graph();
    let mut harness = InteractionHarness::new("connect gesture lifecycle", graph);
    let kind = ConnectDragKind::New {
        from: out_port,
        bundle: vec![out_port],
    };
    let start = NodeGraphGestureEvent::ConnectStart(ConnectStart {
        kind: kind.clone(),
        mode: NodeGraphConnectionMode::Strict,
    });
    let end = NodeGraphGestureEvent::ConnectEnd(ConnectEnd {
        kind,
        mode: NodeGraphConnectionMode::Strict,
        target: Some(in_port),
        outcome: ConnectEndOutcome::Committed,
    });

    harness.emit_gesture(start.clone());
    harness.emit_gesture(end.clone());

    harness.assert_events(&[HarnessEvent::gesture(start), HarnessEvent::gesture(end)]);
}

#[test]
fn adapter_conformance_harness_records_connect_gesture_transaction_and_callbacks() {
    let (mut graph, _a, b, out_port, _in_port, _eid) = make_graph();
    let next_in = insert_input_port(&mut graph, b, "in2");
    let mut harness = InteractionHarness::new("connect gesture transaction callbacks", graph);
    let _callbacks = harness.install_callback_trace();
    let kind = ConnectDragKind::New {
        from: out_port,
        bundle: vec![out_port],
    };
    let start = ConnectStart {
        kind: kind.clone(),
        mode: NodeGraphConnectionMode::Strict,
    };
    let start_event = NodeGraphGestureEvent::ConnectStart(start.clone());

    harness.emit_gesture(start_event.clone());

    let plan = plan_connect(harness.store().graph(), out_port, next_in);
    assert!(plan.is_accept(), "connect gesture fixture should accept");
    let tx = GraphTransaction::from_ops(plan.into_ops()).with_label("connect gesture commit");
    let (edge_id, edge) = match tx.ops() {
        [GraphOp::AddEdge { id, edge }] => (*id, edge.clone()),
        other => panic!("expected single add-edge op, got {other:#?}"),
    };
    let connection = EdgeConnection::new(edge_id, out_port, next_in, EdgeKind::Data);

    let _outcome = harness
        .dispatch_transaction(&tx)
        .expect("dispatch connect gesture transaction");

    let end = ConnectEnd {
        kind,
        mode: NodeGraphConnectionMode::Strict,
        target: Some(next_in),
        outcome: ConnectEndOutcome::Committed,
    };
    let end_event = NodeGraphGestureEvent::ConnectEnd(end.clone());
    harness.emit_gesture(end_event.clone());

    assert_eq!(edge.from, out_port);
    assert_eq!(edge.to, next_in);
    harness.assert_events(&[
        HarnessEvent::gesture(start_event),
        HarnessEvent::callback(HarnessCallbackEvent::ConnectStart(start)),
        HarnessEvent::graph_commit(Some("connect gesture commit"), &["add_edge"]),
        HarnessEvent::callback(HarnessCallbackEvent::GraphCommit {
            label: Some("connect gesture commit".to_owned()),
        }),
        HarnessEvent::callback(HarnessCallbackEvent::NodeEdgeChanges { nodes: 0, edges: 1 }),
        HarnessEvent::callback(HarnessCallbackEvent::EdgesChange { count: 1 }),
        HarnessEvent::callback(HarnessCallbackEvent::ConnectionChange(
            ConnectionChange::Connected(connection),
        )),
        HarnessEvent::callback(HarnessCallbackEvent::Connect(connection)),
        HarnessEvent::gesture(end_event),
        HarnessEvent::callback(HarnessCallbackEvent::ConnectEnd(end)),
    ]);
}

#[test]
fn adapter_conformance_harness_records_node_drag_gesture_transaction_and_callbacks() {
    let (graph, node_id, _b, _out_port, _in_port, _eid) = make_graph();
    let mut harness = InteractionHarness::new("node drag gesture transaction callbacks", graph);
    let _callbacks = harness.install_callback_trace();

    let start = NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
    };
    let start_event = NodeGraphGestureEvent::NodeDragStart(start.clone());
    harness.emit_gesture(start_event.clone());

    let target = CanvasPoint { x: 32.0, y: 16.0 };
    let outcome = harness
        .store_mut()
        .apply_node_drag(NodeDragRequest {
            node: node_id,
            to: target,
        })
        .expect("dispatch node drag")
        .expect("node drag commits");
    let changes = NodeGraphChanges::from_patch(&outcome.patch);
    assert!(
        matches!(changes.nodes(), [NodeChange::Position { id, position }]
            if *id == node_id && *position == target),
        "node drag should project to one position change",
    );

    let update = NodeDragUpdate {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
    };
    let update_event = NodeGraphGestureEvent::NodeDragUpdate(update.clone());
    harness.emit_gesture(update_event.clone());

    let end = NodeDragEnd {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
        outcome: NodeDragEndOutcome::Committed,
    };
    let end_event = NodeGraphGestureEvent::NodeDragEnd(end.clone());
    harness.emit_gesture(end_event.clone());

    harness.assert_events(&[
        HarnessEvent::gesture(start_event),
        HarnessEvent::callback(HarnessCallbackEvent::NodeDragStart(start)),
        HarnessEvent::graph_commit(Some(NODE_DRAG_TRANSACTION_LABEL), &["set_node_pos"]),
        HarnessEvent::callback(HarnessCallbackEvent::GraphCommit {
            label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
        }),
        HarnessEvent::callback(HarnessCallbackEvent::NodeEdgeChanges { nodes: 1, edges: 0 }),
        HarnessEvent::callback(HarnessCallbackEvent::NodesChange { count: 1 }),
        HarnessEvent::gesture(update_event),
        HarnessEvent::callback(HarnessCallbackEvent::NodeDrag(update)),
        HarnessEvent::gesture(end_event),
        HarnessEvent::callback(HarnessCallbackEvent::NodeDragEnd(end)),
    ]);
}

#[test]
fn adapter_conformance_geometry_hit_test_is_renderer_neutral() {
    let source_node = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 120.0,
            height: 80.0,
        },
    };
    let target_node = CanvasRect {
        origin: CanvasPoint { x: 240.0, y: 40.0 },
        size: CanvasSize {
            width: 120.0,
            height: 80.0,
        },
    };

    let endpoints = edge_position(
        EdgeEndpointInput {
            node_rect: source_node,
            handle: Some(HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 112.0, y: 32.0 },
                    size: CanvasSize {
                        width: 8.0,
                        height: 16.0,
                    },
                },
                position: HandlePosition::Right,
            }),
            fallback_position: HandlePosition::Right,
        },
        EdgeEndpointInput {
            node_rect: target_node,
            handle: Some(HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 0.0, y: 32.0 },
                    size: CanvasSize {
                        width: 8.0,
                        height: 16.0,
                    },
                },
                position: HandlePosition::Left,
            }),
            fallback_position: HandlePosition::Left,
        },
    )
    .expect("edge endpoints");
    let path = bezier_edge_path(
        endpoints.source,
        endpoints.target,
        BezierEdgeOptions::default(),
    )
    .expect("bezier path");

    assert!(edge_path_contains_point(
        &path,
        path.label.point,
        EdgeHitTestOptions::default(),
    ));
}

fn insert_input_port(graph: &mut jellyflow_core::core::Graph, node: NodeId, key: &str) -> PortId {
    let port_id = PortId::new();
    graph
        .nodes
        .get_mut(&node)
        .expect("node exists")
        .ports
        .push(port_id);
    graph.ports.insert(
        port_id,
        Port {
            node,
            key: PortKey::new(key),
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
    port_id
}
