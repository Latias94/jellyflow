use super::fixtures::{GraphFixtureUpdateExt, default_editor_config, make_graph};
use super::harness::{HarnessCallbackEvent, HarnessEvent, InteractionHarness};

use crate::runtime::conformance::{ConformanceCallbackEvent, ConformanceTraceEvent};
use crate::runtime::connection::{
    CONNECT_EDGE_TRANSACTION_LABEL, ConnectEdgeRequest, ConnectionHandleRef,
};
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{
    ConnectDragKind, ConnectEnd, ConnectEndOutcome, ConnectStart, NodeDragEnd, NodeDragEndOutcome,
    NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent, ViewportMove, ViewportMoveEnd,
    ViewportMoveEndOutcome, ViewportMoveKind, ViewportMoveStart,
};
use crate::runtime::gesture::{
    ConnectEdgeSession, NodeDragSession, PointerSessionClaim, PointerSessionClaimInput,
    PointerSessionClaimOutcome, PointerSessionClaimRejection, PointerSessionTarget,
    ViewportDragPanSession,
};
use crate::runtime::store::NodeGraphStore;
use crate::runtime::viewport::{
    ViewportDragPanInput, ViewportGestureContext, ViewportPointerButton,
};
use crate::runtime::xyflow::callbacks::ConnectionChange;
use jellyflow_core::core::{
    CanvasPoint, EdgeId, EdgeKind, Graph, NodeId, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn pointer_session_claim_resolves_common_adapter_targets() {
    let (graph, node_id, _b, out_port, _in_port, _eid) = make_graph();
    let mut blocked_graph = graph.clone();
    blocked_graph
        .update_port(&out_port, |port| port.connectable_start = Some(false))
        .expect("source port exists");
    let store = super::fixtures::make_store(graph);
    let blocked_store = super::fixtures::make_store(blocked_graph);
    let delta = CanvasPoint { x: 3.0, y: 4.0 };

    assert_eq!(
        pointer_session_claim(
            &store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::Node(node_id),
                delta,
                ViewportGestureContext {
                    connection_in_progress: true,
                    ..ViewportGestureContext::idle()
                },
            )
        ),
        PointerSessionClaim::Connection
    );

    assert_eq!(
        pointer_session_claim(
            &store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::Node(node_id),
                delta,
                ViewportGestureContext {
                    selection_key_pressed: true,
                    ..ViewportGestureContext::idle()
                },
            )
        ),
        PointerSessionClaim::Selection
    );

    assert_eq!(
        pointer_session_claim(
            &store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::Node(node_id),
                delta,
                ViewportGestureContext::idle(),
            )
        ),
        PointerSessionClaim::NodeDrag
    );

    assert_eq!(
        pointer_session_claim(
            &store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::ConnectionHandle(ConnectionHandleRef::new(
                    node_id,
                    out_port,
                    PortDirection::Out,
                )),
                delta,
                ViewportGestureContext::idle(),
            )
        ),
        PointerSessionClaim::Connection
    );

    assert_eq!(
        pointer_session_claim(
            &blocked_store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::ConnectionHandle(ConnectionHandleRef::new(
                    node_id,
                    out_port,
                    PortDirection::Out,
                )),
                delta,
                ViewportGestureContext::idle(),
            )
        ),
        PointerSessionClaim::None
    );
    assert_eq!(
        blocked_store
            .resolve_pointer_session_claim(PointerSessionClaimInput::new(
                PointerSessionTarget::ConnectionHandle(ConnectionHandleRef::new(
                    node_id,
                    out_port,
                    PortDirection::Out,
                )),
                delta,
                ViewportGestureContext::idle(),
            ))
            .rejection,
        Some(PointerSessionClaimRejection::TargetPolicyBlocked)
    );

    assert_eq!(
        pointer_session_claim(
            &store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::ConnectionHandle(ConnectionHandleRef::new(
                    node_id,
                    out_port,
                    PortDirection::Out,
                )),
                CanvasPoint::default(),
                ViewportGestureContext::idle(),
            )
        ),
        PointerSessionClaim::None
    );
    assert_eq!(
        store.resolve_pointer_session_claim(PointerSessionClaimInput::new(
            PointerSessionTarget::ConnectionHandle(ConnectionHandleRef::new(
                node_id,
                out_port,
                PortDirection::Out,
            )),
            CanvasPoint::default(),
            ViewportGestureContext::idle(),
        )),
        PointerSessionClaimOutcome::rejected(
            PointerSessionClaimRejection::ActivationThresholdNotMet,
        )
    );

    assert_eq!(
        pointer_session_claim(
            &store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::Pane {
                    button: ViewportPointerButton::Left,
                },
                delta,
                ViewportGestureContext::idle(),
            )
        ),
        PointerSessionClaim::ViewportPan
    );
}

#[test]
fn pointer_session_claim_prioritizes_selection_and_connection_before_viewport_pan() {
    let (graph, _node_id, _b, _out_port, _in_port, _eid) = make_graph();
    let store = super::fixtures::make_store(graph);
    let delta = CanvasPoint { x: 3.0, y: 4.0 };
    let pane = PointerSessionTarget::Pane {
        button: ViewportPointerButton::Left,
    };

    assert_eq!(
        pointer_session_claim(
            &store,
            PointerSessionClaimInput::new(
                pane,
                delta,
                ViewportGestureContext {
                    selection_key_pressed: true,
                    ..ViewportGestureContext::idle()
                },
            )
        ),
        PointerSessionClaim::Selection
    );

    assert_eq!(
        pointer_session_claim(
            &store,
            PointerSessionClaimInput::new(
                pane,
                delta,
                ViewportGestureContext {
                    connection_in_progress: true,
                    ..ViewportGestureContext::idle()
                },
            )
        ),
        PointerSessionClaim::Connection
    );
}

#[test]
fn pointer_session_claim_requires_node_drag_threshold_and_policy() {
    let (mut graph, node_id, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = super::fixtures::make_store(graph.clone());
    let mut editor_config = default_editor_config();
    editor_config.interaction.node_drag_threshold = 5.0;
    store.replace_editor_config(editor_config);

    assert_eq!(
        pointer_session_claim(
            &store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::Node(node_id),
                CanvasPoint { x: 3.0, y: 4.0 },
                ViewportGestureContext::idle(),
            )
        ),
        PointerSessionClaim::None
    );
    assert_eq!(
        store.resolve_pointer_session_claim(PointerSessionClaimInput::new(
            PointerSessionTarget::Node(node_id),
            CanvasPoint { x: 3.0, y: 4.0 },
            ViewportGestureContext::idle(),
        )),
        PointerSessionClaimOutcome::rejected(
            PointerSessionClaimRejection::ActivationThresholdNotMet,
        )
    );

    assert_eq!(
        pointer_session_claim(
            &store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::Node(node_id),
                CanvasPoint { x: 5.1, y: 0.0 },
                ViewportGestureContext::idle(),
            )
        ),
        PointerSessionClaim::NodeDrag
    );

    graph
        .update_node(&node_id, |node| node.draggable = Some(false))
        .expect("node exists");
    let blocked_store = super::fixtures::make_store(graph.clone());
    assert_eq!(
        pointer_session_claim(
            &blocked_store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::Node(node_id),
                CanvasPoint { x: 5.1, y: 0.0 },
                ViewportGestureContext::idle(),
            )
        ),
        PointerSessionClaim::None
    );
    assert_eq!(
        blocked_store
            .resolve_pointer_session_claim(PointerSessionClaimInput::new(
                PointerSessionTarget::Node(node_id),
                CanvasPoint { x: 5.1, y: 0.0 },
                ViewportGestureContext::idle(),
            ))
            .rejection,
        Some(PointerSessionClaimRejection::TargetPolicyBlocked)
    );

    graph
        .update_node(&node_id, |node| node.draggable = Some(true))
        .expect("node exists");
    graph
        .update_node(&node_id, |node| node.hidden = true)
        .expect("node exists");
    let hidden_store = super::fixtures::make_store(graph);
    assert_eq!(
        pointer_session_claim(
            &hidden_store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::Node(node_id),
                CanvasPoint { x: 5.1, y: 0.0 },
                ViewportGestureContext::idle(),
            )
        ),
        PointerSessionClaim::None
    );
    assert_eq!(
        hidden_store
            .resolve_pointer_session_claim(PointerSessionClaimInput::new(
                PointerSessionTarget::Node(node_id),
                CanvasPoint { x: 5.1, y: 0.0 },
                ViewportGestureContext::idle(),
            ))
            .rejection,
        Some(PointerSessionClaimRejection::TargetUnavailable)
    );

    assert_eq!(
        pointer_session_claim(
            &store,
            PointerSessionClaimInput::new(
                PointerSessionTarget::Node(NodeId::from_u128(999)),
                CanvasPoint { x: 5.1, y: 0.0 },
                ViewportGestureContext::idle(),
            )
        ),
        PointerSessionClaim::None
    );
    assert_eq!(
        store
            .resolve_pointer_session_claim(PointerSessionClaimInput::new(
                PointerSessionTarget::Node(NodeId::from_u128(999)),
                CanvasPoint { x: 5.1, y: 0.0 },
                ViewportGestureContext::idle(),
            ))
            .rejection,
        Some(PointerSessionClaimRejection::TargetUnavailable)
    );
}

#[test]
fn node_drag_session_emits_lifecycle_around_store_commit() {
    let (graph, node_id, _b, _out_port, _in_port, _eid) = make_graph();
    let mut harness = InteractionHarness::new("node drag session", graph);
    let _callback_token = harness.install_callback_trace();
    let start_pointer = CanvasPoint { x: 1.0, y: 2.0 };
    let target = CanvasPoint { x: 32.0, y: 16.0 };

    let outcome = harness
        .store_mut()
        .apply_node_drag_session(NodeDragSession::new(node_id, start_pointer, target))
        .expect("node drag session");
    assert!(outcome.committed_update().is_some());

    let start = NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: start_pointer,
    };
    let update = NodeDragUpdate {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
    };
    let end = NodeDragEnd {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
        outcome: NodeDragEndOutcome::Committed,
    };

    harness.assert_events(&[
        HarnessEvent::gesture(NodeGraphGestureEvent::NodeDragStart(start.clone())),
        HarnessEvent::callback(HarnessCallbackEvent::NodeDragStart(start)),
        HarnessEvent::graph_commit(Some(NODE_DRAG_TRANSACTION_LABEL), ["set_node_pos"]),
        HarnessEvent::callback(HarnessCallbackEvent::GraphCommit {
            label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
        }),
        HarnessEvent::callback(HarnessCallbackEvent::NodeEdgeChanges { nodes: 1, edges: 0 }),
        HarnessEvent::callback(HarnessCallbackEvent::NodesChange { count: 1 }),
        HarnessEvent::gesture(NodeGraphGestureEvent::NodeDragUpdate(update.clone())),
        HarnessEvent::callback(HarnessCallbackEvent::NodeDrag(update)),
        HarnessEvent::gesture(NodeGraphGestureEvent::NodeDragEnd(end.clone())),
        HarnessEvent::callback(HarnessCallbackEvent::NodeDragEnd(end)),
    ]);
}

#[test]
fn connect_edge_session_emits_lifecycle_around_store_commit() {
    let (mut graph, _node_id, target_node, out_port, _in_port, _eid) = make_graph();
    let next_in = insert_input_port(&mut graph, target_node, "in2");
    let edge_id = EdgeId::from_u128(400);
    let mut harness = InteractionHarness::new("connect edge session", graph);
    let _callback_token = harness.install_callback_trace();
    let kind = ConnectDragKind::New {
        from: out_port,
        bundle: vec![out_port],
    };
    let start = ConnectStart {
        kind: kind.clone(),
        mode: NodeGraphConnectionMode::Strict,
    };
    let request = ConnectEdgeRequest::new(out_port, next_in, NodeGraphConnectionMode::Strict)
        .with_edge_id(edge_id);

    let outcome = harness
        .store_mut()
        .apply_connect_edge_session(ConnectEdgeSession::new(start.clone(), request))
        .expect("connect edge session");
    assert!(outcome.committed_update().is_some());
    assert_eq!(
        outcome.lifecycle().state,
        crate::runtime::connection::ConnectionLifecycleState::Committed
    );
    assert_eq!(outcome.lifecycle().target, Some(next_in));
    assert!(outcome.lifecycle().did_commit());

    let end = ConnectEnd {
        kind,
        mode: NodeGraphConnectionMode::Strict,
        target: Some(next_in),
        outcome: ConnectEndOutcome::Committed,
    };
    let connection = crate::runtime::xyflow::callbacks::EdgeConnection::new(
        edge_id,
        out_port,
        next_in,
        EdgeKind::Data,
    );

    harness.assert_events(&[
        HarnessEvent::gesture(NodeGraphGestureEvent::ConnectStart(start.clone())),
        HarnessEvent::callback(HarnessCallbackEvent::ConnectStart(start)),
        HarnessEvent::graph_commit(Some(CONNECT_EDGE_TRANSACTION_LABEL), ["add_edge"]),
        HarnessEvent::callback(HarnessCallbackEvent::GraphCommit {
            label: Some(CONNECT_EDGE_TRANSACTION_LABEL.to_owned()),
        }),
        HarnessEvent::callback(HarnessCallbackEvent::NodeEdgeChanges { nodes: 0, edges: 1 }),
        HarnessEvent::callback(HarnessCallbackEvent::EdgesChange { count: 1 }),
        HarnessEvent::callback(HarnessCallbackEvent::ConnectionChange(
            ConnectionChange::Connected(connection),
        )),
        HarnessEvent::callback(HarnessCallbackEvent::Connect(connection)),
        HarnessEvent::gesture(NodeGraphGestureEvent::ConnectEnd(end.clone())),
        HarnessEvent::callback(HarnessCallbackEvent::ConnectEnd(end)),
    ]);
}

fn insert_input_port(graph: &mut Graph, node: NodeId, key: &str) -> PortId {
    let port_id = PortId::new();
    let from = graph.nodes().get(&node).expect("node exists").ports.clone();
    let mut to = from.clone();
    to.push(port_id);

    GraphTransaction::from_ops([
        GraphOp::AddPort {
            id: port_id,
            port: Port {
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
        },
        GraphOp::SetNodePorts { id: node, from, to },
    ])
    .apply_to(graph)
    .expect("insert input port");

    port_id
}

#[test]
fn viewport_drag_pan_session_emits_lifecycle_around_view_change() {
    let (graph, _node_id, _b, _out_port, _in_port, _eid) = make_graph();
    let mut harness = InteractionHarness::new("viewport drag pan session", graph);
    let _callback_token = harness.install_callback_trace();
    let pan = CanvasPoint { x: 40.0, y: -10.0 };

    let outcome = harness
        .store_mut()
        .apply_viewport_drag_pan_session(ViewportDragPanSession::new(
            ViewportGestureContext::idle(),
            ViewportDragPanInput::new(ViewportPointerButton::Left, pan),
        ))
        .expect("viewport drag-pan session");
    assert_eq!(outcome.transform.pan, pan);
    assert_eq!(outcome.transform.zoom, 1.0);

    let start = ViewportMoveStart {
        kind: ViewportMoveKind::PanDrag,
        pan: CanvasPoint::default(),
        zoom: 1.0,
    };
    let update = ViewportMove {
        kind: ViewportMoveKind::PanDrag,
        pan,
        zoom: 1.0,
    };
    let end = ViewportMoveEnd {
        kind: ViewportMoveKind::PanDrag,
        pan,
        zoom: 1.0,
        outcome: ViewportMoveEndOutcome::Ended,
    };

    harness.assert_events(&[
        ConformanceTraceEvent::gesture(NodeGraphGestureEvent::ViewportMoveStart(start)),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMoveStart(start)),
        ConformanceTraceEvent::viewport(pan, 1.0),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
            changes: vec![
                crate::runtime::conformance::ConformanceViewChange::Viewport { pan, zoom: 1.0 },
            ],
        }),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
            pan,
            zoom: 1.0,
        }),
        ConformanceTraceEvent::gesture(NodeGraphGestureEvent::ViewportMove(update)),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMove(update)),
        ConformanceTraceEvent::gesture(NodeGraphGestureEvent::ViewportMoveEnd(end)),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMoveEnd(end)),
    ]);
}

fn pointer_session_claim(
    store: &NodeGraphStore,
    input: PointerSessionClaimInput,
) -> PointerSessionClaim {
    store.resolve_pointer_session_claim(input).claim
}
