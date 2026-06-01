use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::runtime::policy::resolve_node_interaction_policy;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, Graph, Node, NodeId};

use super::constraints::{normalized_size, resolved_extent_rect};

#[derive(Debug, Clone, Copy)]
pub(super) struct DragCandidate {
    pub(super) node: NodeId,
    pub(super) from: CanvasPoint,
    pub(super) size: CanvasSize,
    pub(super) extent: Option<CanvasRect>,
    pub(super) node_extent_override: bool,
}

pub(super) fn drag_candidates(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    primary: NodeId,
) -> Vec<DragCandidate> {
    let mut nodes = view_state.selected_nodes.clone();
    nodes.push(primary);
    nodes.sort();
    nodes.dedup();

    nodes
        .into_iter()
        .filter_map(|node| {
            let graph_node = graph.nodes.get(&node)?;
            if !node_is_draggable(graph_node, view_state, interaction) {
                return None;
            }
            let policy = resolve_node_interaction_policy(graph_node, interaction);
            Some(DragCandidate {
                node,
                from: graph_node.pos,
                size: normalized_size(graph_node.size),
                extent: resolved_extent_rect(
                    graph,
                    graph_node,
                    policy.extent,
                    policy.expand_parent,
                ),
                node_extent_override: graph_node.extent.is_some(),
            })
        })
        .collect()
}

fn node_is_draggable(
    node: &Node,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
) -> bool {
    if node.hidden || !node.pos.is_finite() {
        return false;
    }
    if node
        .parent
        .is_some_and(|parent| view_state.selected_groups.contains(&parent))
    {
        return false;
    }
    resolve_node_interaction_policy(node, interaction).draggable
}
