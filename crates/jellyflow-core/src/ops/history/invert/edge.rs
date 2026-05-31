use crate::ops::GraphOp;

pub(super) fn invert_edge_op(op: &GraphOp) -> Vec<GraphOp> {
    match op {
        GraphOp::AddEdge { id, edge } => vec![GraphOp::RemoveEdge {
            id: *id,
            edge: edge.clone(),
        }],
        GraphOp::RemoveEdge { id, edge } => vec![GraphOp::AddEdge {
            id: *id,
            edge: edge.clone(),
        }],
        GraphOp::SetEdgeKind { id, from, to } => vec![GraphOp::SetEdgeKind {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetEdgeSelectable { id, from, to } => vec![GraphOp::SetEdgeSelectable {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetEdgeDeletable { id, from, to } => vec![GraphOp::SetEdgeDeletable {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetEdgeReconnectable { id, from, to } => {
            vec![GraphOp::SetEdgeReconnectable {
                id: *id,
                from: *to,
                to: *from,
            }]
        }
        GraphOp::SetEdgeEndpoints { id, from, to } => vec![GraphOp::SetEdgeEndpoints {
            id: *id,
            from: *to,
            to: *from,
        }],
        _ => unreachable!("edge invert handler received non-edge operation"),
    }
}
