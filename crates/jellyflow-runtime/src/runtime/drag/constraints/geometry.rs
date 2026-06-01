use crate::runtime::geometry::CanvasBounds;
use jellyflow_core::core::{CanvasPoint, CanvasRect};

use super::super::candidates::DragCandidate;

pub(super) fn candidate_bounds(candidates: &[DragCandidate]) -> Option<CanvasBounds> {
    candidates
        .iter()
        .filter_map(|candidate| candidate_bounds_at(*candidate, candidate.from))
        .reduce(CanvasBounds::union)
}

pub(super) fn candidate_bounds_at(
    candidate: DragCandidate,
    position: CanvasPoint,
) -> Option<CanvasBounds> {
    let origin = CanvasPoint {
        x: position.x - candidate.origin.0 * candidate.size.width,
        y: position.y - candidate.origin.1 * candidate.size.height,
    };
    CanvasBounds::from_rect(CanvasRect {
        origin,
        size: candidate.size,
    })
}

pub(super) fn normalized_rect(rect: CanvasRect) -> Option<CanvasRect> {
    CanvasBounds::from_rect(rect).map(CanvasBounds::to_rect)
}
