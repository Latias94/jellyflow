use crate::io::NodeGraphViewState;
use jellyflow_core::core::Graph;

use super::types::{SelectionBoxOptions, SelectionBoxResult};

pub(super) fn apply_additive_selection(
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
