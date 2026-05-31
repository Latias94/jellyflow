use crate::runtime::xyflow::callbacks::DeleteChange;
use jellyflow_core::core::{Edge, EdgeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

pub(super) fn delete_changes_from_transaction(tx: &GraphTransaction) -> DeleteChange {
    let mut out = DeleteChange::default();

    for op in &tx.ops {
        match op {
            GraphOp::RemoveNode { id, edges, .. } => {
                out.nodes.push(*id);
                push_deleted_edge_ids(edges, &mut out.edges);
            }
            GraphOp::RemoveEdge { id, .. } => out.edges.push(*id),
            GraphOp::RemoveGroup { id, .. } => out.groups.push(*id),
            GraphOp::RemoveStickyNote { id, .. } => out.sticky_notes.push(*id),
            GraphOp::RemovePort { edges, .. } => {
                push_deleted_edge_ids(edges, &mut out.edges);
            }
            _ => {}
        }
    }

    sort_dedup(&mut out.nodes);
    sort_dedup(&mut out.edges);
    sort_dedup(&mut out.groups);
    sort_dedup(&mut out.sticky_notes);

    out
}

fn push_deleted_edge_ids(edges: &[(EdgeId, Edge)], out: &mut Vec<EdgeId>) {
    out.extend(edges.iter().map(|(id, _edge)| *id));
}

fn sort_dedup<T: Ord>(items: &mut Vec<T>) {
    items.sort_unstable();
    items.dedup();
}
