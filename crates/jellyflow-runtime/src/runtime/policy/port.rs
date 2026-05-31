use crate::io::NodeGraphInteractionState;
use jellyflow_core::core::{Node, Port};

use super::resolve_node_interaction_policy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeGraphPortInteractionPolicy {
    pub connectable: bool,
    pub connectable_start: bool,
    pub connectable_end: bool,
}

impl NodeGraphPortInteractionPolicy {
    pub fn can_start_connection(self) -> bool {
        self.connectable_start
    }

    pub fn can_accept_connection(self) -> bool {
        self.connectable_end
    }
}

pub fn resolve_port_interaction_policy(
    node: &Node,
    port: &Port,
    state: &NodeGraphInteractionState,
) -> NodeGraphPortInteractionPolicy {
    let node_policy = resolve_node_interaction_policy(node, state);
    let connectable = node_policy.connectable && port.connectable.unwrap_or(true);
    NodeGraphPortInteractionPolicy {
        connectable,
        connectable_start: connectable && port.connectable_start.unwrap_or(true),
        connectable_end: connectable && port.connectable_end.unwrap_or(true),
    }
}
