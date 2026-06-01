//! Renderer-neutral selection helpers.
//!
//! These helpers turn canvas-space marquee rectangles into ordered selection state without
//! depending on a renderer, DOM measurement, or platform input events.

use std::collections::HashSet;

use crate::io::{
    NodeGraphBoxSelectEdges, NodeGraphInteractionState, NodeGraphSelectionMode, NodeGraphViewState,
};
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::policy::{resolve_edge_interaction_policy, resolve_node_interaction_policy};
use crate::runtime::store::NodeGraphStore;
use crate::runtime::utils::{GetNodesInsideOptions, NodeInclusion, get_nodes_inside};
use jellyflow_core::core::{CanvasRect, CanvasSize, EdgeId, Graph, GroupId, NodeId};

/// Options for applying a marquee selection box.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SelectionBoxOptions {
    /// Whether the box result is unioned with the current selection.
    pub additive: bool,
    /// Fallback size for nodes that do not have an explicit measured size.
    pub fallback_size: Option<CanvasSize>,
}

/// Ordered selection result produced by a marquee selection box.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionBoxResult {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeId>,
    pub groups: Vec<GroupId>,
}

impl SelectionBoxResult {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty() && self.groups.is_empty()
    }
}

impl NodeGraphStore {
    /// Applies a canvas-space marquee selection box to the store view-state.
    pub fn apply_selection_box(
        &mut self,
        rect: CanvasRect,
        options: SelectionBoxOptions,
    ) -> SelectionBoxResult {
        let interaction = self.resolved_interaction_state();
        let result = compute_selection_box(
            self.graph(),
            self.lookups(),
            self.view_state(),
            &interaction,
            rect,
            options,
        );
        self.set_selection(
            result.nodes.clone(),
            result.edges.clone(),
            result.groups.clone(),
        );
        result
    }
}

/// Computes the ordered selection state for a canvas-space marquee selection box.
pub fn compute_selection_box(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    rect: CanvasRect,
    options: SelectionBoxOptions,
) -> SelectionBoxResult {
    let selection = interaction.selection_interaction();
    if !selection.elements_selectable {
        return apply_additive_selection(SelectionBoxResult::default(), graph, view_state, options);
    }

    let node_drag = interaction.node_drag_interaction();
    let node_origin = node_drag.node_origin.normalized();
    let mut nodes = get_nodes_inside(
        lookups,
        rect,
        GetNodesInsideOptions {
            inclusion: match selection.selection_mode {
                NodeGraphSelectionMode::Full => NodeInclusion::Full,
                NodeGraphSelectionMode::Partial => NodeInclusion::Partial,
            },
            node_origin: (node_origin.x, node_origin.y),
            include_hidden: false,
            fallback_size: options.fallback_size,
        },
    );
    nodes.retain(|node| {
        graph
            .nodes
            .get(node)
            .is_some_and(|node| resolve_node_interaction_policy(node, interaction).selectable)
    });
    nodes.sort();
    nodes.dedup();

    let edges = selection_box_edges(
        graph,
        lookups,
        interaction,
        selection.box_select_edges,
        &nodes,
    );
    apply_additive_selection(
        SelectionBoxResult {
            nodes,
            edges,
            groups: Vec::new(),
        },
        graph,
        view_state,
        options,
    )
}

fn selection_box_edges(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    interaction: &NodeGraphInteractionState,
    mode: NodeGraphBoxSelectEdges,
    nodes: &[NodeId],
) -> Vec<EdgeId> {
    if mode == NodeGraphBoxSelectEdges::None || nodes.is_empty() {
        return Vec::new();
    }

    let selected: HashSet<NodeId> = nodes.iter().copied().collect();
    let mut edges: Vec<EdgeId> = Vec::new();
    for (edge_id, entry) in &lookups.edge_lookup {
        let Some(edge) = graph.edges.get(edge_id) else {
            continue;
        };
        if !resolve_edge_interaction_policy(edge, interaction).selectable {
            continue;
        }

        let source_selected = selected.contains(&entry.from_node);
        let target_selected = selected.contains(&entry.to_node);
        let keep = match mode {
            NodeGraphBoxSelectEdges::None => false,
            NodeGraphBoxSelectEdges::Connected => source_selected || target_selected,
            NodeGraphBoxSelectEdges::BothEndpoints => source_selected && target_selected,
        };
        if keep {
            edges.push(*edge_id);
        }
    }

    edges.sort();
    edges.dedup();
    edges
}

fn apply_additive_selection(
    mut result: SelectionBoxResult,
    graph: &Graph,
    view_state: &NodeGraphViewState,
    options: SelectionBoxOptions,
) -> SelectionBoxResult {
    if !options.additive {
        return result;
    }

    result.nodes.extend(
        view_state
            .selected_nodes
            .iter()
            .copied()
            .filter(|node| graph.nodes.contains_key(node)),
    );
    result.edges.extend(
        view_state
            .selected_edges
            .iter()
            .copied()
            .filter(|edge| graph.edges.contains_key(edge)),
    );
    result.groups.extend(
        view_state
            .selected_groups
            .iter()
            .copied()
            .filter(|group| graph.groups.contains_key(group)),
    );

    result.nodes.sort();
    result.nodes.dedup();
    result.edges.sort();
    result.edges.dedup();
    result.groups.sort();
    result.groups.dedup();
    result
}
