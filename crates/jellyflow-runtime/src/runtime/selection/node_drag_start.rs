use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::runtime::drag::{
    PointerGestureClaim, PointerGestureClaimInput, resolve_pointer_gesture_claim,
};
use crate::runtime::policy::resolve_node_interaction_policy;
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::CanvasPoint;
use jellyflow_core::core::{EdgeId, Graph, GroupId, NodeId};

use super::types::SelectionModifier;

/// Input for resolving the selection side-effect of starting a node drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeDragStartSelectionInput {
    pub node: NodeId,
    pub modifier: SelectionModifier,
}

impl NodeDragStartSelectionInput {
    pub fn new(node: NodeId, multi_selection_active: bool) -> Self {
        Self {
            node,
            modifier: if multi_selection_active {
                SelectionModifier::Additive
            } else {
                SelectionModifier::Replace
            },
        }
    }

    pub fn with_modifier(node: NodeId, modifier: SelectionModifier) -> Self {
        Self { node, modifier }
    }
}

/// Selection mutation implied by starting a node drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeDragStartSelectionAction {
    /// Keep the current selection unchanged.
    Unchanged,
    /// Clear node, edge, and group selection.
    Clear,
    /// Select only the dragged node and clear edge/group selection.
    SelectOnly(NodeId),
    /// Add the dragged node to the existing node selection.
    Add(NodeId),
    /// Remove the dragged node from the existing node selection.
    Remove(NodeId),
}

/// Combined decision for a node pointer-down that may update selection and enable node dragging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodePointerDownDecision {
    pub selection: NodeDragStartSelectionAction,
    pub drag_claim: PointerGestureClaim,
}

impl NodePointerDownDecision {
    pub fn new(selection: NodeDragStartSelectionAction, drag_claim: PointerGestureClaim) -> Self {
        Self {
            selection,
            drag_claim,
        }
    }
}

/// Input for resolving the first node pointer-down decision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodePointerDownInput {
    pub selection: NodeDragStartSelectionInput,
    pub screen_delta: CanvasPoint,
}

impl NodePointerDownInput {
    pub fn new(selection: NodeDragStartSelectionInput, screen_delta: CanvasPoint) -> Self {
        Self {
            selection,
            screen_delta,
        }
    }
}

impl NodeDragStartSelectionAction {
    pub fn is_unchanged(self) -> bool {
        self == Self::Unchanged
    }

    pub fn apply_to_view_state(self, view_state: &mut NodeGraphViewState) {
        match self {
            Self::Unchanged => {}
            Self::Clear => view_state.set_selection(Vec::new(), Vec::new(), Vec::new()),
            Self::SelectOnly(node) => view_state.set_selection(vec![node], Vec::new(), Vec::new()),
            Self::Add(node) => {
                if !view_state.selected_nodes.contains(&node) {
                    view_state.selected_nodes.push(node);
                    view_state.selected_nodes.sort();
                    view_state.selected_nodes.dedup();
                }
            }
            Self::Remove(node) => {
                view_state
                    .selected_nodes
                    .retain(|selected| *selected != node);
            }
        }
    }

    fn selection_after(
        self,
        view_state: &NodeGraphViewState,
    ) -> Option<(Vec<NodeId>, Vec<EdgeId>, Vec<GroupId>)> {
        if self.is_unchanged() {
            return None;
        }

        let mut next = view_state.clone();
        self.apply_to_view_state(&mut next);
        Some((
            next.selected_nodes,
            next.selected_edges,
            next.selected_groups,
        ))
    }
}

/// Resolves XyFlow-compatible selection behavior for a node-drag start.
///
/// This mirrors the `selectNodesOnDrag` branch in XyFlow: selectable nodes select on drag start by
/// default, multi-selection toggles selected nodes, and disabled `selectNodesOnDrag` clears an
/// existing selection only when dragging an unselected node outside multi-selection mode.
pub fn resolve_node_drag_start_selection(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    input: NodeDragStartSelectionInput,
) -> NodeDragStartSelectionAction {
    let Some(node) = graph.nodes.get(&input.node) else {
        return NodeDragStartSelectionAction::Unchanged;
    };
    if node.hidden {
        return NodeDragStartSelectionAction::Unchanged;
    }

    let selected = view_state.selected_nodes.contains(&input.node);
    let selectable = resolve_node_interaction_policy(node, interaction).selectable;
    let selection = interaction.selection_interaction();

    if (!selection.select_nodes_on_drag || !selectable) && !input.modifier.additive() {
        return if selected {
            NodeDragStartSelectionAction::Unchanged
        } else {
            NodeDragStartSelectionAction::Clear
        };
    }

    if !selectable || !selection.select_nodes_on_drag {
        return NodeDragStartSelectionAction::Unchanged;
    }

    if !selected {
        if input.modifier.additive() {
            NodeDragStartSelectionAction::Add(input.node)
        } else {
            NodeDragStartSelectionAction::SelectOnly(input.node)
        }
    } else if input.modifier.additive() {
        NodeDragStartSelectionAction::Remove(input.node)
    } else {
        NodeDragStartSelectionAction::Unchanged
    }
}

/// Resolves the first headless decision for a node pointer-down.
///
/// This keeps the existing XyFlow-compatible selection side effect while also exposing whether the
/// pointer state should proceed toward node dragging or is still unclaimed.
pub fn resolve_node_pointer_down(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    input: NodePointerDownInput,
) -> NodePointerDownDecision {
    let selection =
        resolve_node_drag_start_selection(graph, view_state, interaction, input.selection);
    let drag_claim = resolve_pointer_gesture_claim(PointerGestureClaimInput::new(
        input.screen_delta,
        input.selection.modifier.additive(),
        false,
        false,
        interaction.node_drag_interaction().node_drag_threshold,
        interaction.node_drag_interaction().node_drag_threshold,
    ));

    NodePointerDownDecision::new(selection, drag_claim)
}

impl NodeGraphStore {
    /// Applies XyFlow-compatible selection behavior for a node-drag start.
    pub fn apply_node_drag_start_selection(
        &mut self,
        input: NodeDragStartSelectionInput,
    ) -> NodeDragStartSelectionAction {
        let interaction = self.resolved_interaction_state();
        let action =
            resolve_node_drag_start_selection(self.graph(), self.view_state(), &interaction, input);
        if let Some((nodes, edges, groups)) = action.selection_after(self.view_state()) {
            self.set_selection(nodes, edges, groups);
        }
        action
    }

    /// Resolves the node pointer-down decision against current graph, selection, and interaction.
    pub fn resolve_node_pointer_down(
        &self,
        input: NodePointerDownInput,
    ) -> NodePointerDownDecision {
        let interaction = self.resolved_interaction_state();
        resolve_node_pointer_down(self.graph(), self.view_state(), &interaction, input)
    }
}
