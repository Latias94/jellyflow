use std::collections::BTreeSet;

use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{Edge, EdgeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

pub(super) fn connection_changes_from_transaction(tx: &GraphTransaction) -> Vec<ConnectionChange> {
    let mut out = Vec::new();
    out.reserve(tx.ops.len().min(8));

    let mut removed_edges: BTreeSet<EdgeId> = BTreeSet::new();
    for op in &tx.ops {
        match op {
            GraphOp::AddEdge { id, edge } => {
                out.push(ConnectionChange::Connected(edge_connection(*id, edge)))
            }
            GraphOp::RemoveNode { edges, .. } => {
                push_disconnected_edges(edges, &mut removed_edges, &mut out);
            }
            GraphOp::RemovePort { edges, .. } => {
                push_disconnected_edges(edges, &mut removed_edges, &mut out);
            }
            GraphOp::RemoveEdge { id, edge } => {
                let _ = removed_edges.insert(*id);
                out.push(ConnectionChange::Disconnected(edge_connection(*id, edge)))
            }
            GraphOp::SetEdgeEndpoints { id, from, to } => out.push(ConnectionChange::Reconnected {
                edge: *id,
                from: *from,
                to: *to,
            }),
            _ => {}
        }
    }

    out
}

fn push_disconnected_edges(
    edges: &[(EdgeId, Edge)],
    removed_edges: &mut BTreeSet<EdgeId>,
    out: &mut Vec<ConnectionChange>,
) {
    for (id, edge) in edges {
        if !removed_edges.insert(*id) {
            continue;
        }
        out.push(ConnectionChange::Disconnected(edge_connection(*id, edge)))
    }
}

fn edge_connection(id: EdgeId, edge: &Edge) -> EdgeConnection {
    EdgeConnection {
        edge: id,
        from: edge.from,
        to: edge.to,
        kind: edge.kind,
    }
}
