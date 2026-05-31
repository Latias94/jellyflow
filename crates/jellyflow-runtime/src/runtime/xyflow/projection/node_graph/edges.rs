use crate::runtime::xyflow::changes::{EdgeChange, NodeGraphChanges};
use jellyflow_core::core::{Edge, EdgeId};
use jellyflow_core::ops::GraphOp;

pub(super) fn push_edge_change(op: &GraphOp, out: &mut NodeGraphChanges) {
    match op {
        GraphOp::RemovePort { edges, .. } => {
            push_removed_edge_changes(edges, out);
        }
        GraphOp::AddEdge { id, edge } => out.push_edge(EdgeChange::Add {
            id: *id,
            edge: edge.clone(),
        }),
        GraphOp::RemoveEdge { id, .. } => out.push_edge(EdgeChange::Remove { id: *id }),
        GraphOp::SetEdgeKind { id, to, .. } => {
            out.push_edge(EdgeChange::Kind { id: *id, kind: *to })
        }
        GraphOp::SetEdgeSelectable { id, to, .. } => out.push_edge(EdgeChange::Selectable {
            id: *id,
            selectable: *to,
        }),
        GraphOp::SetEdgeDeletable { id, to, .. } => out.push_edge(EdgeChange::Deletable {
            id: *id,
            deletable: *to,
        }),
        GraphOp::SetEdgeReconnectable { id, to, .. } => out.push_edge(EdgeChange::Reconnectable {
            id: *id,
            reconnectable: *to,
        }),
        GraphOp::SetEdgeEndpoints { id, to, .. } => out.push_edge(EdgeChange::Endpoints {
            id: *id,
            from: to.from,
            to: to.to,
        }),
        _ => unreachable!("edge projection called with non-edge graph operation"),
    }
}

pub(super) fn push_removed_edge_changes(edges: &[(EdgeId, Edge)], out: &mut NodeGraphChanges) {
    for (id, _edge) in edges {
        out.push_edge(EdgeChange::Remove { id: *id });
    }
}
