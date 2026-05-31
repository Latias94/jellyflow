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
                self.change.push_node(*id);
                self.push_deleted_edge_ids(edges);
            }
            GraphOp::RemoveEdge { id, .. } => self.change.push_edge(*id),
            GraphOp::RemoveGroup { id, .. } => self.change.push_group(*id),
            GraphOp::RemoveStickyNote { id, .. } => self.change.push_sticky_note(*id),
            GraphOp::RemovePort { edges, .. } => {
                self.push_deleted_edge_ids(edges);
            }
            _ => {}
        }
    }

    fn push_deleted_edge_ids(&mut self, edges: &[(EdgeId, Edge)]) {
        self.change
            .extend_edges(edges.iter().map(|(id, _edge)| *id));
    }

    fn finish(mut self) -> DeleteChange {
        self.change.sort_dedup();
        self.change
    }
}
