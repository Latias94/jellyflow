use crate::core::{Binding, BindingId};
use crate::ops::{GraphOp, GraphTransaction};

use super::GraphMutationPlanner;
use crate::ops::mutation::GraphMutationError;

impl GraphMutationPlanner<'_> {
    pub fn add_binding_op(
        &self,
        id: BindingId,
        binding: Binding,
    ) -> Result<GraphOp, GraphMutationError> {
        if self.graph.bindings.contains_key(&id) {
            return Err(GraphMutationError::BindingAlreadyExists(id));
        }
        Ok(GraphOp::AddBinding { id, binding })
    }

    pub fn add_binding_tx(
        &self,
        id: BindingId,
        binding: Binding,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction::new()
            .with_label(label)
            .with_ops([self.add_binding_op(id, binding)?]))
    }

    pub fn remove_binding_op(&self, id: BindingId) -> Result<GraphOp, GraphMutationError> {
        let binding = self
            .graph
            .bindings
            .get(&id)
            .cloned()
            .ok_or(GraphMutationError::MissingBinding(id))?;
        Ok(GraphOp::RemoveBinding { id, binding })
    }

    pub fn remove_binding_tx(
        &self,
        id: BindingId,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction::new()
            .with_label(label)
            .with_ops([self.remove_binding_op(id)?]))
    }
}
