use crate::io::NodeGraphInteractionState;
use crate::rules::{DeletePlan, Diagnostic};
use jellyflow_core::core::{EdgeId, Graph, NodeId};
use jellyflow_core::ops::GraphOp;

use super::builder::DeleteOpBuilder;
use super::diagnostics::rejected;
use super::selection::DeleteSelection;
use super::validation::DeletePolicyValidator;

pub(super) struct DeletePlanner<'a> {
    graph: &'a Graph,
    state: &'a NodeGraphInteractionState,
}

impl<'a> DeletePlanner<'a> {
    pub(super) fn new(graph: &'a Graph, state: &'a NodeGraphInteractionState) -> Self {
        Self { graph, state }
    }

    pub(super) fn plan(
        self,
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
    ) -> DeletePlan {
        let selection = DeleteSelection::from_requested(self.graph, nodes, edges);
        if selection.is_empty() {
            return DeletePlan::accept();
        }

        let diagnostics = DeletePolicyValidator::new(self.graph, self.state).validate(&selection);
        if !diagnostics.is_empty() {
            return rejected(diagnostics);
        }

        match self.build_ops(&selection) {
            Ok(ops) => DeletePlan::from_ops(ops),
            Err(diagnostic) => rejected(vec![diagnostic]),
        }
    }

    fn build_ops(&self, selection: &DeleteSelection) -> Result<Vec<GraphOp>, Diagnostic> {
        let mut builder = DeleteOpBuilder::new(self.graph);

        for node_id in selection.nodes() {
            builder.remove_node(*node_id)?;
        }

        for edge_id in selection.edges() {
            if selection.edge_is_cascaded(edge_id) || !builder.has_edge(edge_id) {
                continue;
            }

            builder.remove_edge(*edge_id)?;
        }

        Ok(builder.into_ops())
    }
}
