use crate::rules::Diagnostic;
use jellyflow_core::core::{EdgeId, Graph, NodeId};
use jellyflow_core::ops::{GraphMutationPlanner, GraphOp, GraphTransaction};

use super::diagnostics::planning_diagnostic;

pub(super) struct DeleteOpBuilder {
    scratch: Graph,
    ops: Vec<GraphOp>,
}

impl DeleteOpBuilder {
    pub(super) fn new(graph: &Graph) -> Self {
        Self {
            scratch: graph.clone(),
            ops: Vec::new(),
        }
    }

    pub(super) fn has_edge(&self, edge_id: &EdgeId) -> bool {
        self.scratch.edges().contains_key(edge_id)
    }

    pub(super) fn remove_node(&mut self, node_id: NodeId) -> Result<(), Diagnostic> {
        let op = GraphMutationPlanner::new(&self.scratch)
            .remove_node_op(node_id)
            .map_err(|error| {
                planning_diagnostic(format!("failed to plan node deletion: {error}"))
            })?;
        self.push_applied(op)
    }

    pub(super) fn remove_edge(&mut self, edge_id: EdgeId) -> Result<(), Diagnostic> {
        let op = GraphMutationPlanner::new(&self.scratch)
            .remove_edge_op(edge_id)
            .map_err(|error| {
                planning_diagnostic(format!("failed to plan edge deletion: {error}"))
            })?;
        self.push_applied(op)
    }

    fn push_applied(&mut self, op: GraphOp) -> Result<(), Diagnostic> {
        apply_planned_op(&mut self.scratch, &op)?;
        self.ops.push(op);
        Ok(())
    }

    pub(super) fn into_ops(self) -> Vec<GraphOp> {
        self.ops
    }
}

fn apply_planned_op(graph: &mut Graph, op: &GraphOp) -> Result<(), Diagnostic> {
    let tx = GraphTransaction::from_ops([op.clone()]);
    tx.apply_to(graph)
        .map_err(|error| planning_diagnostic(format!("failed to apply planned deletion: {error}")))
}
