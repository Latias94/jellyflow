//! Renderer-neutral ordering helpers.
//!
//! Adapters still own painting, widgets, and GPU/UI details. This module only resolves the stable
//! node order they should use when interpreting Jellyflow view-state.

use std::collections::HashSet;

use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{Graph, NodeId};

/// Options for resolving a node render order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRenderOrderOptions {
    /// Include hidden nodes in the result.
    pub include_hidden: bool,
    /// Raise selected nodes to the front of the returned order.
    pub elevate_nodes_on_select: bool,
}

impl NodeRenderOrderOptions {
    pub fn from_interaction(interaction: &NodeGraphInteractionState) -> Self {
        Self {
            include_hidden: false,
            elevate_nodes_on_select: interaction.rendering_interaction().elevate_nodes_on_select,
        }
    }
}

impl Default for NodeRenderOrderOptions {
    fn default() -> Self {
        Self {
            include_hidden: false,
            elevate_nodes_on_select: true,
        }
    }
}

/// Resolves the stable node order an adapter should paint.
///
/// The base order is:
/// 1. valid IDs from `view_state.draw_order`,
/// 2. remaining graph nodes in deterministic ID order.
///
/// When `elevate_nodes_on_select` is enabled, selected nodes are moved to the end while preserving
/// their relative order. Adapters can treat later IDs as visually above earlier IDs, mirroring the
/// default XyFlow `elevateNodesOnSelect` feel without introducing a renderer dependency.
pub fn resolve_node_render_order(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    options: NodeRenderOrderOptions,
) -> Vec<NodeId> {
    let mut seen: HashSet<NodeId> = HashSet::new();
    let mut base: Vec<NodeId> = Vec::with_capacity(graph.nodes.len());

    for id in &view_state.draw_order {
        if seen.insert(*id) && node_is_renderable(graph, *id, options.include_hidden) {
            base.push(*id);
        }
    }

    for (id, node) in &graph.nodes {
        if seen.insert(*id) && (options.include_hidden || !node.hidden) {
            base.push(*id);
        }
    }

    if !options.elevate_nodes_on_select || view_state.selected_nodes.is_empty() {
        return base;
    }

    let selected: HashSet<NodeId> = view_state.selected_nodes.iter().copied().collect();
    let mut normal: Vec<NodeId> = Vec::with_capacity(base.len());
    let mut elevated: Vec<NodeId> = Vec::new();

    for id in base {
        if selected.contains(&id) {
            elevated.push(id);
        } else {
            normal.push(id);
        }
    }

    normal.extend(elevated);
    normal
}

fn node_is_renderable(graph: &Graph, id: NodeId, include_hidden: bool) -> bool {
    graph
        .nodes
        .get(&id)
        .is_some_and(|node| include_hidden || !node.hidden)
}

impl NodeGraphStore {
    /// Resolves the current node render order using the store's view-state and editor config.
    pub fn node_render_order(&self) -> Vec<NodeId> {
        let interaction = self.resolved_interaction_state();
        resolve_node_render_order(
            self.graph(),
            self.view_state(),
            NodeRenderOrderOptions::from_interaction(&interaction),
        )
    }
}
