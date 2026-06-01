use jellyflow_core::interaction::NodeGraphModifierKey;

use crate::io::config::keys::NodeGraphDeleteKey;
use crate::io::config::types::{NodeGraphBoxSelectEdges, NodeGraphSelectionMode};

use super::super::NodeGraphInteractionState;

/// Selection behaviour resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphSelectionInteraction {
    pub elements_selectable: bool,
    pub edges_selectable: bool,
    pub selection_on_drag: bool,
    pub select_nodes_on_drag: bool,
    pub selection_mode: NodeGraphSelectionMode,
    pub box_select_edges: NodeGraphBoxSelectEdges,
    pub selection_key: NodeGraphModifierKey,
    pub multi_selection_key: NodeGraphModifierKey,
}

/// Delete policy and keyboard binding resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeGraphDeleteInteraction {
    pub nodes_deletable: bool,
    pub edges_deletable: bool,
    pub delete_key: NodeGraphDeleteKey,
}

impl NodeGraphInteractionState {
    pub fn selection_interaction(&self) -> NodeGraphSelectionInteraction {
        NodeGraphSelectionInteraction {
            elements_selectable: self.elements_selectable,
            edges_selectable: self.edges_selectable,
            selection_on_drag: self.selection_on_drag,
            select_nodes_on_drag: self.select_nodes_on_drag,
            selection_mode: self.selection_mode,
            box_select_edges: self.box_select_edges,
            selection_key: self.selection_key,
            multi_selection_key: self.multi_selection_key,
        }
    }

    pub fn delete_interaction(&self) -> NodeGraphDeleteInteraction {
        NodeGraphDeleteInteraction {
            nodes_deletable: self.nodes_deletable,
            edges_deletable: self.edges_deletable,
            delete_key: self.delete_key,
        }
    }
}
