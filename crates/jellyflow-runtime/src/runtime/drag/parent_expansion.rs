use std::collections::BTreeMap;

use crate::runtime::geometry::CanvasBounds;
use jellyflow_core::core::{CanvasRect, Graph, GroupId, NodeId};
use jellyflow_core::ops::GraphOp;

use super::candidates::DragCandidate;
use super::constraints::candidate_bounds_at;
use super::types::NodeDragItem;

pub(super) fn parent_expansion_ops(
    graph: &Graph,
    candidates: &[DragCandidate],
    items: &[NodeDragItem],
) -> Vec<GraphOp> {
    let item_targets = items
        .iter()
        .map(|item| (item.node, item.to))
        .collect::<BTreeMap<NodeId, _>>();
    let mut expanded_by_parent = BTreeMap::<GroupId, CanvasRect>::new();

    for candidate in candidates {
        if !candidate.expand_parent {
            continue;
        }
        let Some(parent) = candidate.parent else {
            continue;
        };
        let Some(target) = item_targets.get(&candidate.node).copied() else {
            continue;
        };
        let Some(parent_rect) = graph.groups.get(&parent).map(|group| group.rect) else {
            continue;
        };
        let Some(child_bounds) = candidate_bounds_at(*candidate, target) else {
            continue;
        };
        let current_rect = expanded_by_parent
            .get(&parent)
            .copied()
            .unwrap_or(parent_rect);
        let Some(expanded_rect) = expand_rect(current_rect, child_bounds.to_rect()) else {
            continue;
        };

        expanded_by_parent.insert(parent, expanded_rect);
    }

    expanded_by_parent
        .into_iter()
        .filter_map(|(id, to)| {
            let from = graph.groups.get(&id)?.rect;
            (from != to).then_some(GraphOp::SetGroupRect { id, from, to })
        })
        .collect()
}

fn expand_rect(parent: CanvasRect, child: CanvasRect) -> Option<CanvasRect> {
    let parent_bounds = CanvasBounds::from_rect(parent)?;
    let child_bounds = CanvasBounds::from_rect(child)?;
    Some(parent_bounds.union(child_bounds).to_rect())
}
