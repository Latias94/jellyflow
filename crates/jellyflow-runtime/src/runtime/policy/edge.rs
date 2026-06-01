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
    let connection = state.connection_interaction();
    let selection = state.selection_interaction();
    let delete = state.delete_interaction();
    let reconnectable = edge
        .reconnectable
        .unwrap_or(EdgeReconnectable::Bool(connection.edges_reconnectable));
    let (reconnect_source, reconnect_target) = reconnectable.endpoint_flags();

    NodeGraphEdgeInteractionPolicy {
        selectable: edge.selectable.unwrap_or(selection.edges_selectable),
        deletable: edge.deletable.unwrap_or(delete.edges_deletable),
        reconnect_source,
        reconnect_target,
    }
}
