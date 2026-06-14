use crate::core::{Edge, EdgeId};
use crate::ops::{GraphOp, GraphTransaction};

use super::GraphMutationPlanner;
use crate::ops::mutation::GraphMutationError;

impl GraphMutationPlanner<'_> {
    pub fn add_edge_tx(
        &self,
        id: EdgeId,
        edge: Edge,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction::new()
            .with_label(label)
            .with_ops([self.add_edge_op(id, edge)?]))
    }

    pub fn add_edge_op(&self, id: EdgeId, edge: Edge) -> Result<GraphOp, GraphMutationError> {
        if self.graph.edges().contains_key(&id) {
            return Err(GraphMutationError::EdgeAlreadyExists(id));
        }
        if !self.graph.ports().contains_key(&edge.from) {
            return Err(GraphMutationError::MissingPort(edge.from));
        }
        if !self.graph.ports().contains_key(&edge.to) {
            return Err(GraphMutationError::MissingPort(edge.to));
        }
        Ok(GraphOp::AddEdge { id, edge })
    }

    pub fn remove_edge_op(&self, id: EdgeId) -> Result<GraphOp, GraphMutationError> {
        let edge = self
            .graph
            .edges
            .get(&id)
            .cloned()
            .ok_or(GraphMutationError::MissingEdge(id))?;
        Ok(GraphOp::RemoveEdge {
            id,
            edge,
            bindings: bindings_for_edge(self.graph, id),
        })
    }

    pub fn remove_edge_tx(
        &self,
        id: EdgeId,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction::new()
            .with_label(label)
            .with_ops([self.remove_edge_op(id)?]))
    }
}
use crate::ops::mutation::collect::bindings_for_edge;
