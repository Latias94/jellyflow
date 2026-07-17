use crate::rules::{Diagnostic, DiagnosticTarget};
use crate::runtime::policy::{GraphRuntimePolicy, resolve_edge_policy, resolve_node_policy};
use jellyflow_core::core::{EdgeId, Graph, NodeId};

use super::diagnostics::delete_diagnostic;

pub(super) struct DeletePolicyValidator<'a> {
    graph: &'a Graph,
    policy: &'a GraphRuntimePolicy,
}

impl<'a> DeletePolicyValidator<'a> {
    pub(super) fn new(graph: &'a Graph, policy: &'a GraphRuntimePolicy) -> Self {
        Self { graph, policy }
    }

    pub(super) fn validate_node(&self, node_id: NodeId) -> Option<Diagnostic> {
        let Some(node) = self.graph.nodes().get(&node_id) else {
            return Some(delete_diagnostic(
                "delete.missing_node",
                DiagnosticTarget::Node { id: node_id },
                format!("missing node: {node_id:?}"),
            ));
        };

        (!resolve_node_policy(node, self.policy).can_delete()).then(|| {
            delete_diagnostic(
                "delete.node_not_deletable",
                DiagnosticTarget::Node { id: node_id },
                format!("node is not deletable: {node_id:?}"),
            )
        })
    }

    pub(super) fn validate_edge(&self, edge_id: EdgeId) -> Option<Diagnostic> {
        let Some(edge) = self.graph.edges().get(&edge_id) else {
            return Some(delete_diagnostic(
                "delete.missing_edge",
                DiagnosticTarget::Edge { id: edge_id },
                format!("missing edge: {edge_id:?}"),
            ));
        };

        (!resolve_edge_policy(edge, self.policy).can_delete()).then(|| {
            delete_diagnostic(
                "delete.edge_not_deletable",
                DiagnosticTarget::Edge { id: edge_id },
                format!("edge is not deletable: {edge_id:?}"),
            )
        })
    }
}
