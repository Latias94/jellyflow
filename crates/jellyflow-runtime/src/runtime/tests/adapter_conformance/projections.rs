use super::super::fixtures::make_graph;
use super::super::harness::{HarnessEvent, InteractionHarness};
use super::support::{assert_conformance_trace, insert_input_port};

use crate::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceScenario, ConformanceTraceConfig,
    ConformanceTraceEvent,
};
use crate::runtime::xyflow::callbacks::{
    ConnectionChange, EdgeConnection, connection_changes_from_transaction,
    delete_changes_from_transaction,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{Edge, EdgeId, EdgeKind};
use jellyflow_core::ops::{EdgeEndpoints, GraphOp, GraphTransaction};

#[test]
fn adapter_conformance_connect_dispatches_patch_and_xyflow_projection() {
    let (graph, _a, _b, out_port, in_port, _eid) = make_graph();
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
    let connection = EdgeConnection::new(edge_id, out_port, in_port, EdgeKind::Data);
    let scenario = ConformanceScenario::new("connect dispatches patch and projection", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::dispatch_transaction(tx)])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(Some("adapter connect"), ["add_edge"]),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some("adapter connect".to_owned()),
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
