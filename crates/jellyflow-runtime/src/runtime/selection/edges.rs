use std::collections::HashSet;

use crate::io::{NodeGraphBoxSelectEdges, NodeGraphInteractionState};
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::policy::resolve_edge_interaction_policy;
use jellyflow_core::core::{EdgeId, Graph, NodeId};

pub(super) fn selection_box_edges(
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
        let Some(edge) = graph.edges().get(edge_id) else {
            continue;
        };
        if edge.hidden {
            continue;
        }
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
