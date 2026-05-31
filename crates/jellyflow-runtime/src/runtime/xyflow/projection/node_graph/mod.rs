mod edges;
mod nodes;
mod target;

use crate::runtime::xyflow::changes::NodeGraphChanges;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use self::target::NodeGraphProjectionTarget;

pub(super) fn node_graph_changes_from_transaction(tx: &GraphTransaction) -> NodeGraphChanges {
    let mut out = NodeGraphChanges::default();
    for op in tx.ops() {
        push_node_graph_change(op, &mut out);
    }
    out
}

fn push_node_graph_change(op: &GraphOp, out: &mut NodeGraphChanges) {
    match NodeGraphProjectionTarget::for_op(op) {
        NodeGraphProjectionTarget::Node => nodes::push_node_change(op, out),
        NodeGraphProjectionTarget::Edge => edges::push_edge_change(op, &mut out.edges),
        NodeGraphProjectionTarget::Ignore => {}
    }
}
