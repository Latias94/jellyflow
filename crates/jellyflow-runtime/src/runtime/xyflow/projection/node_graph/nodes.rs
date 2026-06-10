use crate::runtime::xyflow::changes::{NodeChange, NodeGraphChanges};
use crate::runtime::xyflow::dialect::node_update_change_from_op;
use jellyflow_core::ops::GraphOp;

use super::edges::push_removed_edge_changes;

pub(super) fn try_push_node_change(op: &GraphOp, out: &mut NodeGraphChanges) -> bool {
    match op {
        GraphOp::AddNode { id, node } => {
            out.push_node(NodeChange::Add {
                id: *id,
                node: node.clone(),
            });
        }
        GraphOp::RemoveNode { id, edges, .. } => {
            out.push_node(NodeChange::Remove { id: *id });
            push_removed_edge_changes(edges, out);
        }
        GraphOp::RemoveGroup { detached, .. } => {
            for (node_id, _previous_parent) in detached {
                out.push_node(NodeChange::Parent {
                    id: *node_id,
                    parent: None,
                });
            }
        }
        _ => {
            let Some(change) = node_update_change_from_op(op) else {
                return false;
            };
            out.push_node(change);
        }
    }
    true
}
