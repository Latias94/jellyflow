use jellyflow_core::core::{Node, Port};

use super::{GraphRuntimePolicy, resolve_node_policy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedPortPolicy {
    pub(crate) connectable: bool,
    pub(crate) connectable_start: bool,
    pub(crate) connectable_end: bool,
}

impl ResolvedPortPolicy {
    pub(crate) fn can_start_connection(self) -> bool {
        self.connectable_start
    }

    pub(crate) fn can_accept_connection(self) -> bool {
        self.connectable_end
    }
}

pub(crate) fn resolve_port_policy(
    node: &Node,
    port: &Port,
    policy: &GraphRuntimePolicy,
) -> ResolvedPortPolicy {
    let node_policy = resolve_node_policy(node, policy);
    let connectable = node_policy.connectable && port.connectable.unwrap_or(true);
    ResolvedPortPolicy {
        connectable,
        connectable_start: connectable && port.connectable_start.unwrap_or(true),
        connectable_end: connectable && port.connectable_end.unwrap_or(true),
    }
}
