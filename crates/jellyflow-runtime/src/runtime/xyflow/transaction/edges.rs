use crate::runtime::xyflow::changes::{ChangesToTransactionError, EdgeChange};
use crate::runtime::xyflow::dialect::{edge_update_id, edge_update_op};
use jellyflow_core::core::EdgeId;
use jellyflow_core::ops::{GraphMutationPlanner, GraphOp};

use super::ChangesTransactionPlanner;

impl<'a> ChangesTransactionPlanner<'a> {
    pub(super) fn push_edge_change(
        &mut self,
        change: &EdgeChange,
    ) -> Result<(), ChangesToTransactionError> {
        match change {
            EdgeChange::Add { id, edge } => {
                self.push_op(GraphOp::AddEdge {
                    id: *id,
                    edge: edge.clone(),
                });
            }
            EdgeChange::Remove { id } => {
                self.push_remove_edge_change(*id)?;
            }
            _ => self.push_edge_update_change(change)?,
        }
        Ok(())
    }

    fn push_edge_update_change(
        &mut self,
        change: &EdgeChange,
    ) -> Result<(), ChangesToTransactionError> {
        let id = edge_update_id(change).expect("edge update change should have an id");
        let op = {
            let edge = self.existing_edge(id)?;
            edge_update_op(change, edge).expect("edge update change should produce an op")
        };
        self.push_op(op);
        Ok(())
    }

    fn push_remove_edge_change(&mut self, id: EdgeId) -> Result<(), ChangesToTransactionError> {
        let op = GraphMutationPlanner::new(self.graph)
            .remove_edge_op(id)
            .map_err(|_| ChangesToTransactionError::MissingEdge(id))?;
        self.push_op(op);
        Ok(())
    }
}
