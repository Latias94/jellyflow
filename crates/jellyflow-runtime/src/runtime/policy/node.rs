use crate::io::NodeGraphInteractionState;
use jellyflow_core::core::{Node, NodeExtent};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphNodeInteractionPolicy {
    pub selectable: bool,
    pub focusable: bool,
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
    let selection = state.selection_interaction();
    let keyboard = state.keyboard_interaction();
    let node_drag = state.node_drag_interaction();
    let connection = state.connection_interaction();
    let delete = state.delete_interaction();

    NodeGraphNodeInteractionPolicy {
        selectable: node.selectable.unwrap_or(selection.elements_selectable),
        focusable: keyboard.nodes_focusable,
        draggable: node.draggable.unwrap_or(node_drag.nodes_draggable),
        connectable: node.connectable.unwrap_or(connection.nodes_connectable),
        deletable: node.deletable.unwrap_or(delete.nodes_deletable),
        extent: node
            .extent
            .or_else(|| node_drag.node_extent.map(|rect| NodeExtent::Rect { rect })),
        expand_parent: node.expand_parent.unwrap_or(false),
    }
}
