use crate::runtime::xyflow::changes::NodeChange;
use crate::runtime::xyflow::dialect::{node_update_id, node_update_op};
use jellyflow_core::core::NodeId;
use jellyflow_core::ops::{GraphMutationPlanner, GraphOp, GraphTransaction};

use super::ApplyChangesPlanner;

impl<'a> ApplyChangesPlanner<'a> {
    pub(super) fn apply_nodes(&mut self, changes: &[NodeChange]) {
        for change in changes {
            self.apply_node_change(change);
        }
    }

    fn apply_node_change(&mut self, change: &NodeChange) {
        let Some(op) = self.node_change_op(change) else {
            self.mark_ignored();
            return;
        };
        if GraphTransaction::from_ops([op])
            .apply_to(self.graph)
            .is_ok()
        {
            self.mark_applied();
        } else {
            self.mark_ignored();
        }
    }

    fn node_change_op(&self, change: &NodeChange) -> Option<GraphOp> {
        Some(match change {
            NodeChange::Add { id, node } => {
                if self.graph.nodes().contains_key(id) {
                    return None;
                }
                GraphOp::AddNode {
                    id: *id,
                    node: node.clone(),
                }
            }
            NodeChange::Remove { id } => self.remove_node_change_op(*id)?,
            _ => self.node_update_change_op(change)?,
        })
    }

    fn node_update_change_op(&self, change: &NodeChange) -> Option<GraphOp> {
        let id = node_update_id(change)?;
        let node = self.graph.nodes().get(&id)?;
        node_update_op(change, node)
    }

    fn remove_node_change_op(&self, id: NodeId) -> Option<GraphOp> {
        GraphMutationPlanner::new(self.graph)
            .remove_node_op(id)
            .ok()
    }
}
