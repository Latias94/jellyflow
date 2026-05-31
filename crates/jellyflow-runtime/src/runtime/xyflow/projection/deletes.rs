use crate::runtime::xyflow::callbacks::DeleteChange;
use jellyflow_core::core::{Edge, EdgeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

pub(super) fn delete_changes_from_transaction(tx: &GraphTransaction) -> DeleteChange {
    let mut accumulator = DeleteChangeAccumulator::default();
    for op in tx.ops() {
        accumulator.push_op(op);
    }
    accumulator.finish()
}

#[derive(Default)]
struct DeleteChangeAccumulator {
    change: DeleteChange,
}

impl DeleteChangeAccumulator {
    fn push_op(&mut self, op: &GraphOp) {
        match op {
            GraphOp::RemoveNode { id, edges, .. } => {
                self.change.nodes.push(*id);
                self.push_deleted_edge_ids(edges);
            }
            GraphOp::RemoveEdge { id, .. } => self.change.edges.push(*id),
            GraphOp::RemoveGroup { id, .. } => self.change.groups.push(*id),
            GraphOp::RemoveStickyNote { id, .. } => self.change.sticky_notes.push(*id),
            GraphOp::RemovePort { edges, .. } => {
                self.push_deleted_edge_ids(edges);
            }
            _ => {}
        }
    }

    fn push_deleted_edge_ids(&mut self, edges: &[(EdgeId, Edge)]) {
        self.change
            .edges
            .extend(edges.iter().map(|(id, _edge)| *id));
    }

    fn finish(mut self) -> DeleteChange {
        sort_dedup(&mut self.change.nodes);
        sort_dedup(&mut self.change.edges);
        sort_dedup(&mut self.change.groups);
        sort_dedup(&mut self.change.sticky_notes);

        self.change
    }
}

fn sort_dedup<T: Ord>(items: &mut Vec<T>) {
    items.sort_unstable();
    items.dedup();
}
