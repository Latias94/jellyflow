use crate::io::NodeGraphInteractionState;
use jellyflow_core::core::{Edge, EdgeReconnectable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeGraphEdgeInteractionPolicy {
    pub selectable: bool,
    pub deletable: bool,
    pub reconnect_source: bool,
    pub reconnect_target: bool,
}

impl NodeGraphEdgeInteractionPolicy {
    pub fn can_delete(self) -> bool {
        self.deletable
    }

    pub fn reconnectable(self) -> bool {
        self.reconnect_source || self.reconnect_target
    }

    pub fn can_reconnect_source(self) -> bool {
        self.reconnect_source
    }

    pub fn can_reconnect_target(self) -> bool {
        self.reconnect_target
    }
}

pub fn resolve_edge_interaction_policy(
    edge: &Edge,
    state: &NodeGraphInteractionState,
) -> NodeGraphEdgeInteractionPolicy {
    let reconnectable = edge
        .reconnectable
        .unwrap_or(EdgeReconnectable::Bool(state.edges_reconnectable));
    let (reconnect_source, reconnect_target) = reconnectable.endpoint_flags();

    NodeGraphEdgeInteractionPolicy {
        selectable: edge.selectable.unwrap_or(state.edges_selectable),
        deletable: edge.deletable.unwrap_or(state.edges_deletable),
        reconnect_source,
        reconnect_target,
    }
}
