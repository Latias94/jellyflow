use crate::runtime::xyflow::changes::{ChangesToTransactionError, NodeChange};
use crate::runtime::xyflow::dialect::{node_update_id, node_update_op};
use jellyflow_core::core::NodeId;
use jellyflow_core::ops::{GraphMutationPlanner, GraphOp};

use super::ChangesTransactionPlanner;

impl<'a> ChangesTransactionPlanner<'a> {
    pub(super) fn push_node_change(
        &mut self,
        change: &NodeChange,
    ) -> Result<(), ChangesToTransactionError> {
        match change {
            NodeChange::Add { id, node } => {
                self.push_op(GraphOp::AddNode {
                    id: *id,
                    node: node.clone(),
                });
            }
            NodeChange::Remove { id } => {
                self.push_remove_node_change(*id)?;
            }
            _ => self.push_node_update_change(change)?,
        }
        Ok(())
    }

    fn push_node_update_change(
        &mut self,
        change: &NodeChange,
    ) -> Result<(), ChangesToTransactionError> {
        let id = node_update_id(change).expect("node update change should have an id");
        let op = {
            let node = self.existing_node(id)?;
            node_update_op(change, node).expect("node update change should produce an op")
        };
        self.push_op(op);
        Ok(())
    }

    fn push_remove_node_change(&mut self, id: NodeId) -> Result<(), ChangesToTransactionError> {
        let op = GraphMutationPlanner::new(self.graph)
            .remove_node_op(id)
            .map_err(|_| ChangesToTransactionError::MissingNode(id))?;
        self.push_op(op);
        Ok(())
    }
}
