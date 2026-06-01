use super::super::super::fixtures::make_graph;

use crate::runtime::xyflow::callbacks::{ConnectionChange, connection_changes_from_transaction};
use jellyflow_core::core::{Edge, EdgeKind};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn connection_changes_from_transaction_maps_edge_ops() {
    let (_g0, _a, _b, out_port, in_port, eid) = make_graph();

    let tx = GraphTransaction::from_ops([
        GraphOp::AddEdge {
            id: eid,
            edge: Edge {
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
        },
        GraphOp::SetEdgeEndpoints {
            id: eid,
            from: jellyflow_core::ops::EdgeEndpoints {
                from: out_port,
                to: in_port,
            },
            to: jellyflow_core::ops::EdgeEndpoints {
                from: out_port,
                to: in_port,
            },
        },
        GraphOp::RemoveEdge {
            id: eid,
            edge: Edge {
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
        },
    ]);

    let changes = connection_changes_from_transaction(&tx);
    assert_eq!(changes.len(), 3);
    assert!(matches!(changes[0], ConnectionChange::Connected(_)));
    assert!(matches!(changes[1], ConnectionChange::Reconnected { .. }));
    assert!(matches!(changes[2], ConnectionChange::Disconnected(_)));
}
