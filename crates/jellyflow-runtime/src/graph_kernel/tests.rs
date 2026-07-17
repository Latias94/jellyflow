use crate::io::GraphFile;
use jellyflow_core::core::{
    Binding, BindingId, CanvasPoint, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node,
    NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

use super::{ApplyError, GraphHistory, GraphOp, GraphTransaction, normalize_transaction};

fn node(kind: &str, ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}

fn port(node: NodeId, key: &str, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn frozen_graph_value(graph: &Graph) -> serde_json::Value {
    serde_json::to_value(GraphFile::from_graph(graph.clone())).expect("graph file serializes")
}

#[test]
fn invalid_transaction_apply_is_atomic() {
    let graph_id = GraphId::from_u128(1);
    let existing_node_id = NodeId::from_u128(2);
    let existing_port_id = PortId::from_u128(3);
    let mut graph = GraphBuilder::new(graph_id)
        .with_node(
            existing_node_id,
            node("test.existing", vec![existing_port_id]),
        )
        .with_port(
            existing_port_id,
            port(existing_node_id, "out", PortDirection::Out),
        )
        .build()
        .expect("fixture graph is valid");
    let before = frozen_graph_value(&graph);

    let invalid_node_id = NodeId::from_u128(4);
    let missing_port_id = PortId::from_u128(5);
    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodeHidden {
            id: existing_node_id,
            from: false,
            to: true,
        },
        GraphOp::AddNode {
            id: invalid_node_id,
            node: node("test.invalid", vec![missing_port_id]),
        },
    ]);

    let error = tx
        .apply_to(&mut graph)
        .expect_err("invalid transaction result must be rejected");

    assert!(matches!(error, ApplyError::InvalidTransactionResult { .. }));
    assert_eq!(frozen_graph_value(&graph), before);
    assert!(!graph.node(&existing_node_id).expect("existing node").hidden);
    assert!(graph.node(&invalid_node_id).is_none());
}

#[test]
fn normalization_coalesces_setters_and_removes_noops_without_reordering() {
    let moved_node_id = NodeId::from_u128(10);
    let added_node_id = NodeId::from_u128(11);
    let start = CanvasPoint { x: 0.0, y: 0.0 };
    let middle = CanvasPoint { x: 10.0, y: 20.0 };
    let end = CanvasPoint { x: 30.0, y: 40.0 };
    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodePos {
            id: moved_node_id,
            from: start,
            to: middle,
        },
        GraphOp::SetNodePos {
            id: moved_node_id,
            from: middle,
            to: end,
        },
        GraphOp::SetNodeHidden {
            id: moved_node_id,
            from: false,
            to: false,
        },
        GraphOp::AddNode {
            id: added_node_id,
            node: node("test.added", Vec::new()),
        },
        GraphOp::SetNodeCollapsed {
            id: moved_node_id,
            from: false,
            to: true,
        },
    ])
    .with_label("Move and update nodes");

    let normalized = normalize_transaction(tx);

    assert_eq!(normalized.label(), Some("Move and update nodes"));
    assert!(matches!(
        normalized.ops(),
        [
            GraphOp::SetNodePos {
                id: position_id,
                from,
                to,
            },
            GraphOp::AddNode { id: added_id, .. },
            GraphOp::SetNodeCollapsed {
                id: collapsed_id,
                from: false,
                to: true,
            },
        ] if *position_id == moved_node_id
            && *from == start
            && *to == end
            && *added_id == added_node_id
            && *collapsed_id == moved_node_id
    ));
}

#[test]
fn history_undo_redo_restores_stable_ids_and_port_order() {
    let graph_id = GraphId::from_u128(20);
    let node_id = NodeId::from_u128(21);
    let first_port_id = PortId::from_u128(22);
    let second_port_id = PortId::from_u128(23);
    let mut graph = GraphBuilder::new(graph_id)
        .build()
        .expect("empty fixture graph is valid");
    let before = frozen_graph_value(&graph);

    let tx = GraphTransaction::from_ops([
        GraphOp::AddNode {
            id: node_id,
            node: node("test.shader", vec![first_port_id, second_port_id]),
        },
        GraphOp::AddPort {
            id: first_port_id,
            port: port(node_id, "a", PortDirection::In),
        },
        GraphOp::AddPort {
            id: second_port_id,
            port: port(node_id, "b", PortDirection::In),
        },
    ])
    .with_label("Add shader node");
    tx.apply_to(&mut graph).expect("apply forward transaction");
    let after = frozen_graph_value(&graph);

    let mut history = GraphHistory::default();
    history.record(tx);
    let undo_tx = history.prepare_undo().expect("undo is available");
    undo_tx.apply_to(&mut graph).expect("undo succeeds");
    history.commit_undo();

    assert_eq!(frozen_graph_value(&graph), before);

    let redo_tx = history.prepare_redo().expect("redo is available");
    redo_tx.apply_to(&mut graph).expect("redo succeeds");
    history.commit_redo();

    assert_eq!(frozen_graph_value(&graph), after);
    assert_eq!(
        graph.node(&node_id).expect("restored node").ports,
        vec![first_port_id, second_port_id]
    );
    assert!(graph.port(&first_port_id).is_some());
    assert!(graph.port(&second_port_id).is_some());
}

#[test]
fn transaction_footprint_includes_cascaded_entities_and_edge_endpoints() {
    let producer_id = NodeId::from_u128(30);
    let consumer_id = NodeId::from_u128(31);
    let output_id = PortId::from_u128(32);
    let input_id = PortId::from_u128(33);
    let edge_id = EdgeId::from_u128(34);
    let binding_id = BindingId::from_u128(35);
    let producer = node("test.producer", vec![output_id]);
    let output = port(producer_id, "out", PortDirection::Out);
    let edge = Edge::new(EdgeKind::Data, output_id, input_id);
    let binding = Binding::node_to_source(
        producer_id,
        "source.document",
        serde_json::json!({ "selection": 7 }),
    );
    let tx = GraphTransaction::from_ops([GraphOp::RemoveNode {
        id: producer_id,
        node: producer,
        ports: vec![(output_id, output)],
        edges: vec![(edge_id, edge)],
        bindings: vec![(binding_id, binding)],
    }]);

    let footprint = tx.footprint();

    assert_eq!(footprint.nodes, [producer_id].into_iter().collect());
    assert_eq!(footprint.ports, [output_id, input_id].into_iter().collect());
    assert_eq!(footprint.edges, [edge_id].into_iter().collect());
    assert_eq!(footprint.bindings, [binding_id].into_iter().collect());
    assert!(!footprint.nodes.contains(&consumer_id));
    assert!(!footprint.graph);
}
