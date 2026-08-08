mod diagnostics;
mod validation;

use crate::runtime::policy::GraphRuntimePolicy;
use jellyflow_core::core::{EdgeId, Graph, NodeId};

use super::Diagnostic;
use validation::DeletePolicyValidator;

pub(crate) fn validate_delete_edge_policy(
    graph: &Graph,
    edge: EdgeId,
    policy: &GraphRuntimePolicy,
) -> Option<Diagnostic> {
    DeletePolicyValidator::new(graph, policy).validate_edge(edge)
}

pub(crate) fn validate_delete_node_policy(
    graph: &Graph,
    node: NodeId,
    policy: &GraphRuntimePolicy,
) -> Option<Diagnostic> {
    DeletePolicyValidator::new(graph, policy).validate_node(node)
}
