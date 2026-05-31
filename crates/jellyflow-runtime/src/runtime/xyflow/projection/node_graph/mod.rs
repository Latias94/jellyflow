mod edges;
mod nodes;

use crate::runtime::xyflow::changes::NodeGraphChanges;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

pub(super) fn node_graph_changes_from_transaction(tx: &GraphTransaction) -> NodeGraphChanges {
    let mut out = NodeGraphChanges::default();
    for op in tx.ops() {
        push_node_graph_change(op, &mut out);
    }
    out
}

fn push_node_graph_change(op: &GraphOp, out: &mut NodeGraphChanges) {
    if nodes::try_push_node_change(op, out) {
        return;
    }
    edges::try_push_edge_change(op, out);
}
