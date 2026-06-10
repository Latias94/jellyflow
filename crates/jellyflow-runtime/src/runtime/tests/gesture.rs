use super::fixtures::make_graph;
use super::harness::{HarnessCallbackEvent, HarnessEvent, InteractionHarness};

use crate::runtime::conformance::{ConformanceCallbackEvent, ConformanceTraceEvent};
use crate::runtime::connection::ConnectionHandleRef;
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{
    NodeDragEnd, NodeDragEndOutcome, NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent,
    ViewportMove, ViewportMoveEnd, ViewportMoveEndOutcome, ViewportMoveKind, ViewportMoveStart,
};
use crate::runtime::gesture::{
    NodeDragSession, PointerSessionClaim, PointerSessionClaimInput, PointerSessionTarget,
    ViewportDragPanSession,
};
use crate::runtime::viewport::{
    ViewportDragPanInput, ViewportGestureContext, ViewportPointerButton,
};
use jellyflow_core::core::{CanvasPoint, PortDirection};

#[test]
fn pointer_session_claim_resolves_common_adapter_targets() {
    let (graph, node_id, _b, out_port, _in_port, _eid) = make_graph();
    let mut blocked_graph = graph.clone();
    blocked_graph
        .ports
        .get_mut(&out_port)
        .expect("source port exists")
        .connectable_start = Some(false);
    let store = super::fixtures::make_store(graph);
    let blocked_store = super::fixtures::make_store(blocked_graph);
    let delta = CanvasPoint { x: 3.0, y: 4.0 };

    assert_eq!(
        store.resolve_pointer_session_claim(PointerSessionClaimInput::new(
            PointerSessionTarget::Node(node_id),
            delta,
            ViewportGestureContext {
                connection_in_progress: true,
                ..ViewportGestureContext::idle()
            },
        )),
        PointerSessionClaim::Connection
    );

    assert_eq!(
        store.resolve_pointer_session_claim(PointerSessionClaimInput::new(
            PointerSessionTarget::Node(node_id),
            delta,
            ViewportGestureContext {
                selection_key_pressed: true,
                ..ViewportGestureContext::idle()
            },
        )),
        PointerSessionClaim::Selection
    );

    assert_eq!(
        store.resolve_pointer_session_claim(PointerSessionClaimInput::new(
            PointerSessionTarget::Node(node_id),
            delta,
            ViewportGestureContext::idle(),
        )),
        PointerSessionClaim::NodeDrag
    );

    assert_eq!(
        store.resolve_pointer_session_claim(PointerSessionClaimInput::new(
            PointerSessionTarget::ConnectionHandle(ConnectionHandleRef::new(
                node_id,
                out_port,
                PortDirection::Out,
            )),
            delta,
            ViewportGestureContext::idle(),
        )),
        PointerSessionClaim::Connection
    );

    assert_eq!(
        blocked_store.resolve_pointer_session_claim(PointerSessionClaimInput::new(
            PointerSessionTarget::ConnectionHandle(ConnectionHandleRef::new(
                node_id,
                out_port,
                PortDirection::Out,
            )),
            delta,
            ViewportGestureContext::idle(),
        )),
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
        PointerSessionClaim::None
    );

    assert_eq!(
        store.resolve_pointer_session_claim(PointerSessionClaimInput::new(
            PointerSessionTarget::Pane {
                button: ViewportPointerButton::Left,
            },
            delta,
            ViewportGestureContext::idle(),
        )),
        PointerSessionClaim::ViewportPan
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
