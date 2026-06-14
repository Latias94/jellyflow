use std::collections::BTreeSet;

use crate::io::NodeGraphInteractionState;
use crate::rules::{Diagnostic, DiagnosticTarget};
use jellyflow_core::core::{EdgeId, Graph, NodeId};

use super::diagnostics::delete_diagnostic;
use super::selection::DeleteSelection;

pub(super) struct DeletePolicyValidator<'a> {
    graph: &'a Graph,
    state: &'a NodeGraphInteractionState,
}

impl<'a> DeletePolicyValidator<'a> {
    pub(super) fn new(graph: &'a Graph, state: &'a NodeGraphInteractionState) -> Self {
        Self { graph, state }
    }

    pub(super) fn validate(&self, selection: &DeleteSelection) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        self.validate_nodes(selection.nodes(), &mut diagnostics);
        self.validate_direct_edges(
            selection.edges(),
            selection.cascaded_edges(),
            &mut diagnostics,
        );
        diagnostics
    }

    fn validate_nodes(&self, nodes: &BTreeSet<NodeId>, diagnostics: &mut Vec<Diagnostic>) {
        for node_id in nodes {
            let Some(node) = self.graph.nodes().get(node_id) else {
                diagnostics.push(delete_diagnostic(
                    "delete.missing_node",
                    DiagnosticTarget::Node { id: *node_id },
                    format!("missing node: {node_id:?}"),
                ));
                continue;
            };

            if !self.state.node_interaction_policy(node).can_delete() {
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

            let Some(edge) = self.graph.edges().get(edge_id) else {
                diagnostics.push(delete_diagnostic(
                    "delete.missing_edge",
                    DiagnosticTarget::Edge { id: *edge_id },
                    format!("missing edge: {edge_id:?}"),
                ));
                continue;
            };

            if !self.state.edge_interaction_policy(edge).can_delete() {
                diagnostics.push(delete_diagnostic(
                    "delete.edge_not_deletable",
                    DiagnosticTarget::Edge { id: *edge_id },
                    format!("edge is not deletable: {edge_id:?}"),
                ));
            }
        }
    }
}
