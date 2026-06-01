use jellyflow_core::core::CanvasPoint;

use super::types::EdgePathLabel;

pub(super) fn edge_center_label(source: CanvasPoint, target: CanvasPoint) -> Option<EdgePathLabel> {
    if !source.is_finite() || !target.is_finite() {
        return None;
    }

    let offset_x = (target.x - source.x).abs() * 0.5;
    let offset_y = (target.y - source.y).abs() * 0.5;
    let point = CanvasPoint {
        x: if target.x < source.x {
            target.x + offset_x
        } else {
            target.x - offset_x
        },
        y: if target.y < source.y {
            target.y + offset_y
        } else {
            target.y - offset_y
        },
    };

    point.is_finite().then_some(EdgePathLabel {
        point,
        offset_x,
        offset_y,
    })
}

pub(super) fn bezier_label(
    source: CanvasPoint,
    control1: CanvasPoint,
    control2: CanvasPoint,
    target: CanvasPoint,
) -> Option<EdgePathLabel> {
    let point = CanvasPoint {
        x: source.x * 0.125 + control1.x * 0.375 + control2.x * 0.375 + target.x * 0.125,
        y: source.y * 0.125 + control1.y * 0.375 + control2.y * 0.375 + target.y * 0.125,
    };

    point.is_finite().then_some(EdgePathLabel {
        point,
        offset_x: (point.x - source.x).abs(),
        offset_y: (point.y - source.y).abs(),
    })
}
