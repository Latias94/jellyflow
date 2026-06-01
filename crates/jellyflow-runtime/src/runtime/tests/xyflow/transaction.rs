use super::super::fixtures::make_graph;

use crate::runtime::xyflow::changes::{
    ChangesToTransactionError, EdgeChange, NodeChange, NodeGraphChanges,
};
use jellyflow_core::core::{CanvasPoint, EdgeId, NodeId, PortId};
use jellyflow_core::ops::GraphOp;

#[test]
fn changes_to_transaction_is_reversible_and_applicable() {
    let (g0, a, _b, out_port, in_port, eid) = make_graph();

    let changes = NodeGraphChanges::from_parts(
        vec![NodeChange::Position {
            id: a,
            position: CanvasPoint { x: 42.0, y: 7.0 },
        }],
        vec![
            EdgeChange::Endpoints {
                id: eid,
                from: out_port,
                to: in_port,
            },
            EdgeChange::Hidden {
                id: eid,
                hidden: true,
            },
        ],
    );

    let tx = changes.to_transaction(&g0).expect("tx");
    let mut g1 = g0.clone();
    tx.apply_to(&mut g1).expect("apply");

    assert_eq!(
        g1.nodes.get(&a).unwrap().pos,
        CanvasPoint { x: 42.0, y: 7.0 }
    );
    assert_eq!(g1.edges.get(&eid).unwrap().from, out_port);
    assert_eq!(g1.edges.get(&eid).unwrap().to, in_port);
    assert!(g1.edges.get(&eid).unwrap().hidden);
}

#[test]
fn changes_to_transaction_remove_node_captures_ports_and_edges() {
    let (g0, a, b, out_port, in_port, eid) = make_graph();

    let changes = NodeGraphChanges::from_parts(vec![NodeChange::Remove { id: a }], Vec::new());

    let tx = changes.to_transaction(&g0).expect("tx");
    assert_eq!(tx.ops().len(), 1);
    match &tx.ops()[0] {
        GraphOp::RemoveNode {
            id, ports, edges, ..
        } => {
            assert_eq!(*id, a);
            assert_eq!(
                ports.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                vec![out_port]
            );
            assert_eq!(
                edges.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                vec![eid]
            );
        }
        other => panic!("expected remove node op, got {other:?}"),
    }

    let mut g1 = g0.clone();
    tx.apply_to(&mut g1).expect("apply");
    assert!(!g1.nodes.contains_key(&a));
    assert!(g1.nodes.contains_key(&b));
    assert!(!g1.ports.contains_key(&out_port));
    assert!(g1.ports.contains_key(&in_port));
    assert!(!g1.edges.contains_key(&eid));
}

#[test]
fn changes_to_transaction_reports_missing_node() {
    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let missing = NodeId::new();
    let changes = NodeGraphChanges::from_parts(
        vec![NodeChange::Position {
            id: missing,
            position: CanvasPoint { x: 10.0, y: 20.0 },
        }],
        Vec::new(),
    );

    let err = changes.to_transaction(&g0).expect_err("missing node");
    assert!(matches!(err, ChangesToTransactionError::MissingNode(id) if id == missing));
}

#[test]
fn changes_to_transaction_reports_missing_edge() {
    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let missing = EdgeId::new();
    let changes = NodeGraphChanges::from_parts(
        Vec::new(),
        vec![EdgeChange::Endpoints {
            id: missing,
            from: PortId::new(),
            to: PortId::new(),
        }],
    );

    let err = changes.to_transaction(&g0).expect_err("missing edge");
    assert!(matches!(err, ChangesToTransactionError::MissingEdge(id) if id == missing));
}

#[test]
fn changes_to_transaction_accepts_empty_changes() {
    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let changes = NodeGraphChanges::from_parts(Vec::new(), Vec::new());

    let tx = changes.to_transaction(&g0).expect("tx");
    assert!(tx.is_empty());
    assert!(tx.label().is_none());
}

#[test]
fn node_graph_changes_facade_consumes_parts() {
    let node = NodeId::new();
    let edge = EdgeId::new();
    let changes = NodeGraphChanges::from_parts(
        vec![NodeChange::Hidden {
            id: node,
            hidden: true,
        }],
        vec![EdgeChange::Remove { id: edge }],
    );

    let (nodes, edges) = changes.into_parts();

    assert!(matches!(nodes.as_slice(), [NodeChange::Hidden { id, hidden: true }] if *id == node));
    assert!(matches!(edges.as_slice(), [EdgeChange::Remove { id }] if *id == edge));
}
