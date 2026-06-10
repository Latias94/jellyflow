use crate::runtime::xyflow::callbacks::DeleteChange;
use jellyflow_core::ops::GraphOp;

use super::removed_edges::visit_removed_edges;

#[derive(Default)]
pub(super) struct DeleteChangeAccumulator {
    change: DeleteChange,
}

impl DeleteChangeAccumulator {
    pub(super) fn push_op(&mut self, op: &GraphOp) {
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

    pub(super) fn finish(mut self) -> DeleteChange {
        self.change.sort_dedup();
        self.change
    }
}
