use super::super::fixtures::make_graph;
use super::super::harness::{HarnessCallbackEvent, HarnessEvent, InteractionHarness};
use super::support::{assert_conformance_trace, insert_input_port};

use crate::io::NodeGraphViewState;
use crate::rules::EdgeEndpoint;
use crate::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceScenario, ConformanceTraceEvent,
    ConformanceViewChange,
};
use crate::runtime::connection::{
    CONNECT_EDGE_TRANSACTION_LABEL, ConnectEdgeRequest, RECONNECT_EDGE_TRANSACTION_LABEL,
    ReconnectEdgeRequest,
};
use crate::runtime::delete::DELETE_SELECTION_TRANSACTION_LABEL;
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{EdgeId, EdgeKind};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::EdgeEndpoints;

#[test]
fn adapter_conformance_connect_dispatches_patch_and_xyflow_projection() {
    let (mut graph, _a, b, out_port, _in_port, _eid) = make_graph();
    let next_in = insert_input_port(&mut graph, b, "in2");
    let edge_id = EdgeId::from_u128(301);

    let connection = EdgeConnection::new(edge_id, out_port, next_in, EdgeKind::Data);
    let scenario = ConformanceScenario::new("connect dispatches patch and projection", graph)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_connect_edge(
            ConnectEdgeRequest::new(out_port, next_in, NodeGraphConnectionMode::Strict)
                .with_edge_id(edge_id),
        )])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(Some(CONNECT_EDGE_TRANSACTION_LABEL), ["add_edge"]),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(CONNECT_EDGE_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 0,
                edges: 1,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::EdgesChange { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectionChange(
                ConnectionChange::Connected(connection),
            )),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Connect(connection)),
        ]);

    assert_conformance_trace(&scenario);
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
    let _callback_token = harness.install_callback_trace();

    let outcome = harness
        .store_mut()
        .apply_reconnect_edge(ReconnectEdgeRequest::new(
            edge_id,
            EdgeEndpoint::To,
            next_in,
            NodeGraphConnectionMode::Strict,
        ))
        .expect("dispatch reconnect")
        .expect("reconnect should commit");
    let changes = NodeGraphChanges::from_patch(&outcome.patch);

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
    harness.assert_events(&[
        HarnessEvent::graph_commit(
            Some(RECONNECT_EDGE_TRANSACTION_LABEL),
            ["set_edge_endpoints"],
        ),
        HarnessEvent::callback(HarnessCallbackEvent::GraphCommit {
            label: Some(RECONNECT_EDGE_TRANSACTION_LABEL.to_owned()),
        }),
        HarnessEvent::callback(HarnessCallbackEvent::NodeEdgeChanges { nodes: 0, edges: 1 }),
        HarnessEvent::callback(HarnessCallbackEvent::EdgesChange { count: 1 }),
        HarnessEvent::callback(HarnessCallbackEvent::ConnectionChange(
            ConnectionChange::Reconnected {
                edge: edge_id,
                from,
                to,
            },
        )),
        HarnessEvent::callback(HarnessCallbackEvent::Reconnect {
            edge: edge_id,
            from,
            to,
        }),
    ]);
}

#[test]
fn adapter_conformance_delete_node_cascades_edges_and_projects_delete_payload() {
    let (graph, node_id, _b, out_port, in_port, edge_id) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![node_id], Vec::new(), Vec::new());
    let mut harness =
        InteractionHarness::with_view_state("delete node cascades edges", graph, view_state);
    let disconnected = EdgeConnection::new(edge_id, out_port, in_port, EdgeKind::Data);
    let _callback_token = harness.install_callback_trace();

    let outcome = harness
        .store_mut()
        .apply_delete_selection()
        .expect("dispatch delete node")
        .expect("delete should commit");
    let changes = NodeGraphChanges::from_patch(&outcome.patch);

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
    harness.assert_events(&[
        HarnessEvent::graph_commit(Some(DELETE_SELECTION_TRANSACTION_LABEL), ["remove_node"]),
        HarnessEvent::callback(HarnessCallbackEvent::GraphCommit {
            label: Some(DELETE_SELECTION_TRANSACTION_LABEL.to_owned()),
        }),
        HarnessEvent::callback(HarnessCallbackEvent::NodeEdgeChanges { nodes: 1, edges: 1 }),
        HarnessEvent::callback(HarnessCallbackEvent::NodesChange { count: 1 }),
        HarnessEvent::callback(HarnessCallbackEvent::EdgesChange { count: 1 }),
        HarnessEvent::callback(HarnessCallbackEvent::ConnectionChange(
            ConnectionChange::Disconnected(disconnected),
        )),
        HarnessEvent::callback(HarnessCallbackEvent::Disconnect(disconnected)),
        HarnessEvent::callback(HarnessCallbackEvent::NodesDelete { count: 1 }),
        HarnessEvent::callback(HarnessCallbackEvent::EdgesDelete { count: 1 }),
        HarnessEvent::callback(HarnessCallbackEvent::Delete {
            nodes: 1,
            edges: 1,
            groups: 0,
            sticky_notes: 0,
        }),
        HarnessEvent::selection(Vec::new(), Vec::new(), Vec::new()),
        HarnessEvent::callback(HarnessCallbackEvent::ViewChange {
            changes: vec![ConformanceViewChange::Selection {
                nodes: Vec::new(),
                edges: Vec::new(),
                groups: Vec::new(),
            }],
        }),
        HarnessEvent::callback(HarnessCallbackEvent::SelectionChange {
            nodes: Vec::new(),
            edges: Vec::new(),
            groups: Vec::new(),
        }),
    ]);
}
