use crate::io::NodeGraphInteractionState;
use jellyflow_core::core::{Node, NodeExtent};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphNodeInteractionPolicy {
    pub selectable: bool,
    pub draggable: bool,
    pub connectable: bool,
    pub deletable: bool,
    pub extent: Option<NodeExtent>,
    pub expand_parent: bool,
}

impl NodeGraphNodeInteractionPolicy {
    pub fn can_delete(self) -> bool {
        self.deletable
    }
}

pub fn resolve_node_interaction_policy(
    node: &Node,
    state: &NodeGraphInteractionState,
) -> NodeGraphNodeInteractionPolicy {
    NodeGraphNodeInteractionPolicy {
        selectable: node.selectable.unwrap_or(state.elements_selectable),
        draggable: node.draggable.unwrap_or(state.nodes_draggable),
        connectable: node.connectable.unwrap_or(state.nodes_connectable),
        deletable: node.deletable.unwrap_or(state.nodes_deletable),
        extent: node
            .extent
            .or_else(|| state.node_extent.map(|rect| NodeExtent::Rect { rect })),
        expand_parent: node.expand_parent.unwrap_or(false),
    }
}
