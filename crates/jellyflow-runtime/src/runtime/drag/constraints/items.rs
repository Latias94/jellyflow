use crate::io::NodeGraphInteractionState;
use crate::runtime::geometry::CanvasBounds;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

use super::super::candidates::DragCandidate;
use super::super::types::NodeDragItem;
use super::geometry::{candidate_bounds, candidate_bounds_at, normalized_rect};

pub(in crate::runtime::drag) fn drag_items(
    interaction: &NodeGraphInteractionState,
    candidates: &[DragCandidate],
    delta: CanvasPoint,
) -> Vec<NodeDragItem> {
    let node_drag = interaction.node_drag_interaction();
    let node_origin = node_drag.node_origin.normalized();
    let node_origin = (node_origin.x, node_origin.y);
    let global_extent = node_drag.node_extent.and_then(normalized_rect);
    let group_bounds = (candidates.len() > 1)
        .then(|| candidate_bounds(candidates, node_origin))
        .flatten()
        .map(CanvasBounds::to_rect);

    candidates
        .iter()
        .filter_map(|candidate| {
            let desired = CanvasPoint {
                x: candidate.from.x + delta.x,
                y: candidate.from.y + delta.y,
            };
            let extent =
                adjusted_candidate_extent(*candidate, node_origin, global_extent, group_bounds);
            let to = extent
                .map(|extent| clamp_candidate_position(*candidate, desired, node_origin, extent))
                .unwrap_or(desired);
            to.is_finite().then_some(NodeDragItem {
                node: candidate.node,
                from: candidate.from,
                to,
            })
        })
        .collect()
}

fn adjusted_candidate_extent(
    candidate: DragCandidate,
    node_origin: (f32, f32),
    global_extent: Option<CanvasRect>,
    group_bounds: Option<CanvasRect>,
) -> Option<CanvasRect> {
    if !candidate.node_extent_override
        && let (Some(global_extent), Some(group_bounds), Some(candidate_bounds)) = (
            global_extent,
            group_bounds,
            candidate_bounds_at(candidate, candidate.from, node_origin).map(CanvasBounds::to_rect),
        )
    {
        let group_max_x = group_bounds.origin.x + group_bounds.size.width;
        let group_max_y = group_bounds.origin.y + group_bounds.size.height;
        let candidate_max_x = candidate_bounds.origin.x + candidate_bounds.size.width;
        let candidate_max_y = candidate_bounds.origin.y + candidate_bounds.size.height;
        let extent_max_x = global_extent.origin.x + global_extent.size.width;
        let extent_max_y = global_extent.origin.y + global_extent.size.height;

        let min = CanvasPoint {
            x: candidate_bounds.origin.x - group_bounds.origin.x + global_extent.origin.x,
            y: candidate_bounds.origin.y - group_bounds.origin.y + global_extent.origin.y,
        };
        let max = CanvasPoint {
            x: candidate_max_x - group_max_x + extent_max_x,
            y: candidate_max_y - group_max_y + extent_max_y,
        };

        return normalized_rect(CanvasRect {
            origin: min,
            size: CanvasSize {
                width: max.x - min.x,
                height: max.y - min.y,
            },
        });
    }

    candidate.extent
}

fn clamp_candidate_position(
    candidate: DragCandidate,
    target: CanvasPoint,
    node_origin: (f32, f32),
    extent: CanvasRect,
) -> CanvasPoint {
    let Some(bounds) =
        candidate_bounds_at(candidate, target, node_origin).map(CanvasBounds::to_rect)
    else {
        return target;
    };

    let max_x = extent.origin.x + extent.size.width - bounds.size.width;
    let max_y = extent.origin.y + extent.size.height - bounds.size.height;
    let top_left = CanvasPoint {
        x: clamp(bounds.origin.x, extent.origin.x, max_x),
        y: clamp(bounds.origin.y, extent.origin.y, max_y),
    };
    CanvasPoint {
        x: top_left.x + node_origin.0 * candidate.size.width,
        y: top_left.y + node_origin.1 * candidate.size.height,
    }
}

fn clamp(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}
