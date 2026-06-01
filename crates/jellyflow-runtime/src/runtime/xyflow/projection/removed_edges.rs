use jellyflow_core::core::{Edge, EdgeId};
use jellyflow_core::ops::GraphOp;

pub(super) fn visit_removed_edges(op: &GraphOp, mut visit: impl FnMut(EdgeId, &Edge)) -> bool {
    match op {
        GraphOp::RemoveNode { edges, .. } | GraphOp::RemovePort { edges, .. } => {
            for (id, edge) in edges {
                visit(*id, edge);
            }
            true
        }
        GraphOp::RemoveEdge { id, edge } => {
            visit(*id, edge);
            true
        }
        _ => false,
    }
}
