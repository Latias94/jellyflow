use crate::io::NodeGraphInteractionState;
use jellyflow_core::core::{CanvasPoint, CanvasSize};

use super::super::candidates::DragCandidate;

pub(in crate::runtime::drag) fn snapped_delta(
    interaction: &NodeGraphInteractionState,
    candidates: &[DragCandidate],
    delta: CanvasPoint,
) -> CanvasPoint {
    let node_drag = interaction.node_drag_interaction();
    if !node_drag.snap_to_grid || !node_drag.snap_grid.is_positive_finite() {
        return delta;
    }

    let Some(reference) = candidates.first() else {
        return delta;
    };
    let reference_target = CanvasPoint {
        x: reference.from.x + delta.x,
        y: reference.from.y + delta.y,
    };
    let snapped = snap_point(reference_target, node_drag.snap_grid);

    CanvasPoint {
        x: snapped.x - reference.from.x,
        y: snapped.y - reference.from.y,
    }
}

fn snap_point(point: CanvasPoint, grid: CanvasSize) -> CanvasPoint {
    CanvasPoint {
        x: grid.width * js_round(point.x / grid.width),
        y: grid.height * js_round(point.y / grid.height),
    }
}

fn js_round(value: f32) -> f32 {
    (value + 0.5).floor()
}
