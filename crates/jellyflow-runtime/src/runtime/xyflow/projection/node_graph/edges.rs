use crate::runtime::xyflow::changes::{EdgeChange, NodeGraphChanges};
use crate::runtime::xyflow::dialect::edge_update_change_from_op;
use jellyflow_core::core::{Edge, EdgeId};
use jellyflow_core::ops::GraphOp;

pub(super) fn try_push_edge_change(op: &GraphOp, out: &mut NodeGraphChanges) -> bool {
    match op {
        GraphOp::RemovePort { edges, .. } => {
            push_removed_edge_changes(edges, out);
        }
        GraphOp::AddEdge { id, edge } => {
            out.push_edge(EdgeChange::Add {
                id: *id,
                edge: edge.clone(),
            });
        }
        GraphOp::RemoveEdge { id, .. } => push_edge_remove(*id, out),
        _ => {
            let Some(change) = edge_update_change_from_op(op) else {
                return false;
            };
            out.push_edge(change);
        }
    }
    true
}

pub(super) fn push_removed_edge_changes(edges: &[(EdgeId, Edge)], out: &mut NodeGraphChanges) {
    for (id, _edge) in edges {
        push_edge_remove(*id, out);
    }
}

fn push_edge_remove(id: EdgeId, out: &mut NodeGraphChanges) {
    if has_pending_edge_remove_since_last_add(out.edges(), id) {
        return;
    }
    out.push_edge(EdgeChange::Remove { id });
}

fn has_pending_edge_remove_since_last_add(changes: &[EdgeChange], id: EdgeId) -> bool {
    let mut removed = false;
    for change in changes {
        match change {
            EdgeChange::Add { id: added, .. } if *added == id => removed = false,
            EdgeChange::Remove { id: removed_id } if *removed_id == id => removed = true,
            _ => {}
        }
    }
    removed
}
