use crate::runtime::xyflow::changes::EdgeChange;
use jellyflow_core::core::{Edge, EdgeId};
use jellyflow_core::ops::GraphOp;

pub(super) fn push_edge_change(op: &GraphOp, out: &mut Vec<EdgeChange>) {
    match op {
        GraphOp::RemovePort { edges, .. } => {
            push_removed_edge_changes(edges, out);
        }
        GraphOp::AddEdge { id, edge } => out.push(EdgeChange::Add {
            id: *id,
            edge: edge.clone(),
        }),
        GraphOp::RemoveEdge { id, .. } => out.push(EdgeChange::Remove { id: *id }),
        GraphOp::SetEdgeKind { id, to, .. } => out.push(EdgeChange::Kind { id: *id, kind: *to }),
        GraphOp::SetEdgeSelectable { id, to, .. } => out.push(EdgeChange::Selectable {
            id: *id,
            selectable: *to,
        }),
        GraphOp::SetEdgeDeletable { id, to, .. } => out.push(EdgeChange::Deletable {
            id: *id,
            deletable: *to,
        }),
        GraphOp::SetEdgeReconnectable { id, to, .. } => out.push(EdgeChange::Reconnectable {
            id: *id,
            reconnectable: *to,
        }),
        GraphOp::SetEdgeEndpoints { id, to, .. } => out.push(EdgeChange::Endpoints {
            id: *id,
            from: to.from,
            to: to.to,
        }),
        _ => unreachable!("edge projection called with non-edge graph operation"),
    }
}

pub(super) fn push_removed_edge_changes(edges: &[(EdgeId, Edge)], out: &mut Vec<EdgeChange>) {
    for (id, _edge) in edges {
        out.push(EdgeChange::Remove { id: *id });
    }
}
