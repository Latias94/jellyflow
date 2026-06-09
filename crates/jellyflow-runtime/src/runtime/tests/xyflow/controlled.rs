use super::super::fixtures::{make_graph, make_store};

use crate::runtime::xyflow::{ControlledGraph, EdgeChange, NodeChange};
use jellyflow_core::core::{CanvasPoint, EdgeId, EdgeReconnectable, NodeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn controlled_graph_applies_projected_store_patch() {
    let (g0, a, _b, _out_port, _in_port, eid) = make_graph();
    let mut store = make_store(g0.clone());
    let mut controlled = ControlledGraph::new(g0);

    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodePos {
            id: a,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 123.0, y: 456.0 },
        },
        GraphOp::SetEdgeReconnectable {
            id: eid,
            from: None,
            to: Some(EdgeReconnectable::Bool(false)),
        },
    ]);
    let outcome = store.dispatch_transaction(&tx).expect("dispatch");
    let report = controlled.apply_patch_changes(&outcome.patch);

    assert_eq!(report.applied(), 2);
    assert_eq!(report.ignored(), 0);
    let store_json = serde_json::to_value(store.graph()).expect("store json");
    let controlled_json = serde_json::to_value(controlled.graph()).expect("controlled json");
    assert_eq!(store_json, controlled_json);
}

#[test]
fn controlled_graph_preserves_xyflow_ignore_missing_contract() {
    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let missing_node = NodeId::new();
    let missing_edge = EdgeId::new();
    let mut controlled = ControlledGraph::new(g0.clone());

    let report = controlled.apply_node_changes(&[
        NodeChange::Position {
            id: missing_node,
            position: CanvasPoint { x: 1.0, y: 2.0 },
        },
        NodeChange::Remove { id: missing_node },
    ]);
    let edge_report = controlled.apply_edge_changes(&[EdgeChange::Remove { id: missing_edge }]);

    assert_eq!(report.applied(), 0);
    assert_eq!(report.ignored(), 2);
    assert_eq!(edge_report.applied(), 0);
    assert_eq!(edge_report.ignored(), 1);
    assert_eq!(
        serde_json::to_value(controlled.graph()).expect("controlled json"),
        serde_json::to_value(&g0).expect("original json"),
    );
}
