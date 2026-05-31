use std::collections::BTreeSet;

use crate::io::NodeGraphInteractionState;
use crate::rules::{DeletePlan, Diagnostic, DiagnosticTarget};
use jellyflow_core::core::{EdgeId, Graph, NodeId};
use jellyflow_core::ops::{GraphMutationPlanner, GraphOp, GraphTransaction};

use super::diagnostics::{delete_diagnostic, planning_diagnostic, rejected};
use super::selection::DeleteSelection;

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

        let mut diagnostics = Vec::new();
        self.validate_nodes(selection.nodes(), &mut diagnostics);
        self.validate_direct_edges(
            selection.edges(),
            selection.cascaded_edges(),
            &mut diagnostics,
        );
        if !diagnostics.is_empty() {
            return rejected(diagnostics);
        }

        match self.build_ops(&selection) {
            Ok(ops) => DeletePlan::from_ops(ops),
            Err(diagnostic) => rejected(vec![diagnostic]),
        }
    }

    fn validate_nodes(&self, nodes: &BTreeSet<NodeId>, diagnostics: &mut Vec<Diagnostic>) {
        for node_id in nodes {
            let Some(node) = self.graph.nodes.get(node_id) else {
                diagnostics.push(delete_diagnostic(
                    "delete.missing_node",
                    DiagnosticTarget::Node { id: *node_id },
                    format!("missing node: {node_id:?}"),
                ));
                continue;
            };

            if !self.state.node_interaction_policy(node).deletable {
                diagnostics.push(delete_diagnostic(
                    "delete.node_not_deletable",
                    DiagnosticTarget::Node { id: *node_id },
                    format!("node is not deletable: {node_id:?}"),
                ));
            }
        }
    }

    fn validate_direct_edges(
        &self,
        edges: &BTreeSet<EdgeId>,
        cascaded_edges: &BTreeSet<EdgeId>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for edge_id in edges {
            if cascaded_edges.contains(edge_id) {
                continue;
            }

            let Some(edge) = self.graph.edges.get(edge_id) else {
                diagnostics.push(delete_diagnostic(
                    "delete.missing_edge",
                    DiagnosticTarget::Edge { id: *edge_id },
                    format!("missing edge: {edge_id:?}"),
                ));
                continue;
            };

            if !self.state.edge_interaction_policy(edge).deletable {
                diagnostics.push(delete_diagnostic(
                    "delete.edge_not_deletable",
                    DiagnosticTarget::Edge { id: *edge_id },
                    format!("edge is not deletable: {edge_id:?}"),
                ));
            }
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

struct DeleteOpBuilder {
    scratch: Graph,
    ops: Vec<GraphOp>,
}

impl DeleteOpBuilder {
    fn new(graph: &Graph) -> Self {
        Self {
            scratch: graph.clone(),
            ops: Vec::new(),
        }
    }

    fn has_edge(&self, edge_id: &EdgeId) -> bool {
        self.scratch.edges.contains_key(edge_id)
    }

    fn remove_node(&mut self, node_id: NodeId) -> Result<(), Diagnostic> {
        let op = GraphMutationPlanner::new(&self.scratch)
            .remove_node_op(node_id)
            .map_err(|error| {
                planning_diagnostic(format!("failed to plan node deletion: {error}"))
            })?;
        self.push_applied(op)
    }

    fn remove_edge(&mut self, edge_id: EdgeId) -> Result<(), Diagnostic> {
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

    fn into_ops(self) -> Vec<GraphOp> {
        self.ops
    }
}

fn apply_planned_op(graph: &mut Graph, op: &GraphOp) -> Result<(), Diagnostic> {
    let tx = GraphTransaction::from_ops([op.clone()]);
    tx.apply_to(graph)
        .map_err(|error| planning_diagnostic(format!("failed to apply planned deletion: {error}")))
}
