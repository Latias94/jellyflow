use crate::core::GroupId;
use crate::ops::{GraphOp, GraphTransaction};

use super::GraphMutationPlanner;
use crate::ops::mutation::GraphMutationError;
use crate::ops::mutation::collect::detached_nodes_for_group;

impl GraphMutationPlanner<'_> {
    pub fn remove_group_op(&self, id: GroupId) -> Result<GraphOp, GraphMutationError> {
        let group = self
            .graph
            .groups
            .get(&id)
            .cloned()
            .ok_or(GraphMutationError::MissingGroup(id))?;

        Ok(GraphOp::RemoveGroup {
            id,
            group,
            detached: detached_nodes_for_group(self.graph, id),
        })
    }

    pub fn remove_group_tx(
        &self,
        id: GroupId,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction::new()
            .with_label(label)
            .with_ops([self.remove_group_op(id)?]))
    }
}
