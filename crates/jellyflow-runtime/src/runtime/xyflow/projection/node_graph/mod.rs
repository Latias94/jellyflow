mod edges;
mod nodes;

use crate::runtime::xyflow::changes::NodeGraphChanges;
use jellyflow_core::ops::GraphOp;

#[derive(Debug, Default)]
pub(super) struct NodeGraphChangeAccumulator {
    out: NodeGraphChanges,
}

impl NodeGraphChangeAccumulator {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn push_op(&mut self, op: &GraphOp) {
        push_node_graph_change(op, &mut self.out);
    }

    pub(super) fn finish(self) -> NodeGraphChanges {
        self.out
    }
}

fn push_node_graph_change(op: &GraphOp, out: &mut NodeGraphChanges) {
    if nodes::try_push_node_change(op, out) {
        return;
    }
    edges::try_push_edge_change(op, out);
}
