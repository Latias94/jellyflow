use jellyflow_core::core::Node;

use super::GraphRuntimePolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedNodePolicy {
    pub(crate) connectable: bool,
    pub(crate) deletable: bool,
}

impl ResolvedNodePolicy {
    pub(crate) fn can_delete(self) -> bool {
        self.deletable
    }
}

pub(crate) fn resolve_node_policy(node: &Node, policy: &GraphRuntimePolicy) -> ResolvedNodePolicy {
    ResolvedNodePolicy {
        connectable: node
            .connectable
            .unwrap_or(policy.node_connections_enabled()),
        deletable: node.deletable.unwrap_or(policy.node_deletion_enabled()),
    }
}
