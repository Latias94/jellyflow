//! Transaction planner for XyFlow-style node/edge changes.

mod edges;
mod nodes;

use crate::runtime::xyflow::changes::{ChangesToTransactionError, NodeGraphChanges};
use jellyflow_core::core::{Edge, EdgeId, Graph, Node, NodeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

pub(super) fn changes_to_transaction(
    changes: &NodeGraphChanges,
    graph: &Graph,
) -> Result<GraphTransaction, ChangesToTransactionError> {
    ChangesTransactionPlanner::new(graph).finish(changes)
}

struct ChangesTransactionPlanner<'a> {
    graph: &'a Graph,
    tx: GraphTransaction,
}

impl<'a> ChangesTransactionPlanner<'a> {
    fn new(graph: &'a Graph) -> Self {
        Self {
            graph,
            tx: GraphTransaction::new(),
        }
    }

    fn finish(
        mut self,
        changes: &NodeGraphChanges,
    ) -> Result<GraphTransaction, ChangesToTransactionError> {
        for change in changes.nodes() {
            self.push_node_change(change)?;
        }
        for change in changes.edges() {
            self.push_edge_change(change)?;
        }

        Ok(self.tx)
    }

    fn existing_node(&self, id: NodeId) -> Result<&'a Node, ChangesToTransactionError> {
        self.graph
            .nodes
            .get(&id)
            .ok_or(ChangesToTransactionError::MissingNode(id))
    }

    fn existing_edge(&self, id: EdgeId) -> Result<&'a Edge, ChangesToTransactionError> {
        self.graph
            .edges
            .get(&id)
            .ok_or(ChangesToTransactionError::MissingEdge(id))
    }

    fn push_op(&mut self, op: GraphOp) {
        self.tx.push(op);
    }
}
