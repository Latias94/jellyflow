use jellyflow_core::core::{Edge, EdgeReconnectable};

use super::GraphRuntimePolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedEdgePolicy {
    pub(crate) deletable: bool,
    pub(crate) reconnect_source: bool,
    pub(crate) reconnect_target: bool,
}

impl ResolvedEdgePolicy {
    pub(crate) fn can_delete(self) -> bool {
        self.deletable
    }

    #[cfg(test)]
    pub(crate) fn reconnectable(self) -> bool {
        self.reconnect_source || self.reconnect_target
    }

    pub(crate) fn can_reconnect_source(self) -> bool {
        self.reconnect_source
    }

    pub(crate) fn can_reconnect_target(self) -> bool {
        self.reconnect_target
    }
}

pub(crate) fn resolve_edge_policy(edge: &Edge, policy: &GraphRuntimePolicy) -> ResolvedEdgePolicy {
    let reconnectable = edge
        .reconnectable
        .unwrap_or(EdgeReconnectable::Bool(policy.edge_reconnection_enabled()));
    let (reconnect_source, reconnect_target) = reconnectable.endpoint_flags();

    ResolvedEdgePolicy {
        deletable: edge.deletable.unwrap_or(policy.edge_deletion_enabled()),
        reconnect_source,
        reconnect_target,
    }
}
