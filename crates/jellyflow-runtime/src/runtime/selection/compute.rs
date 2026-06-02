use crate::io::{NodeGraphInteractionState, NodeGraphSelectionMode, NodeGraphViewState};
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::policy::resolve_node_interaction_policy;
use crate::runtime::utils::{GetNodesInsideOptions, NodeInclusion, get_nodes_inside};
use jellyflow_core::core::{CanvasRect, Graph};

use super::additive::apply_additive_selection;
use super::edges::selection_box_edges;
use super::types::{
    SelectionBoxDecision, SelectionBoxInput, SelectionBoxOptions, SelectionBoxResult,
};

/// Resolves the ordered selection state for a canvas-space marquee selection gesture.
pub fn resolve_selection_box(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    input: SelectionBoxInput,
) -> SelectionBoxDecision {
    SelectionBoxDecision::new(compute_selection_box(
        graph,
        lookups,
        view_state,
        interaction,
        input.rect,
        input.options,
    ))
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
