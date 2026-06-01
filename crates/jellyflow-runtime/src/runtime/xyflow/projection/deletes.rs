use crate::runtime::xyflow::callbacks::DeleteChange;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use super::removed_edges::visit_removed_edges;

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
            GraphOp::RemoveNode { id, .. } => {
                self.change.push_node(*id);
                self.push_deleted_edges(op);
            }
            GraphOp::RemoveEdge { .. } | GraphOp::RemovePort { .. } => {
                self.push_deleted_edges(op);
            }
            GraphOp::RemoveGroup { id, .. } => self.change.push_group(*id),
            GraphOp::RemoveStickyNote { id, .. } => self.change.push_sticky_note(*id),
            _ => {}
        }
    }

    fn push_deleted_edges(&mut self, op: &GraphOp) {
        visit_removed_edges(op, |id, _edge| {
            self.change.push_edge(id);
        });
    }

    fn finish(mut self) -> DeleteChange {
        self.change.sort_dedup();
        self.change
    }
}
