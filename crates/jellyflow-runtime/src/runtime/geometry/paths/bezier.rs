use jellyflow_core::core::CanvasPoint;

use super::super::endpoints::{EdgeEndpointPosition, HandlePosition};
use super::label::bezier_label;
use super::types::{EdgePath, PathCommand};

/// Bezier edge options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BezierEdgeOptions {
    pub curvature: f32,
}

impl Default for BezierEdgeOptions {
    fn default() -> Self {
        Self { curvature: 0.25 }
    }
}

pub fn bezier_edge_path(
    source: EdgeEndpointPosition,
    target: EdgeEndpointPosition,
    options: BezierEdgeOptions,
) -> Option<EdgePath> {
    if !source.point.is_finite() || !target.point.is_finite() {
        return None;
    }

    let curvature = if options.curvature.is_finite() && options.curvature >= 0.0 {
        options.curvature
    } else {
        BezierEdgeOptions::default().curvature
    };
    let control1 = control_with_curvature(source.position, source.point, target.point, curvature);
    let control2 = control_with_curvature(target.position, target.point, source.point, curvature);
    let label = bezier_label(source.point, control1, control2, target.point)?;

    Some(EdgePath {
        commands: vec![
            PathCommand::MoveTo(source.point),
            PathCommand::CubicTo {
                control1,
                control2,
                to: target.point,
            },
        ],
        label,
    })
}

fn control_with_curvature(
    position: HandlePosition,
    from: CanvasPoint,
    to: CanvasPoint,
    curvature: f32,
) -> CanvasPoint {
    match position {
        HandlePosition::Left => CanvasPoint {
            x: from.x - control_offset(from.x - to.x, curvature),
            y: from.y,
        },
        HandlePosition::Right => CanvasPoint {
            x: from.x + control_offset(to.x - from.x, curvature),
            y: from.y,
        },
        HandlePosition::Top => CanvasPoint {
            x: from.x,
            y: from.y - control_offset(from.y - to.y, curvature),
        },
        HandlePosition::Bottom => CanvasPoint {
            x: from.x,
            y: from.y + control_offset(to.y - from.y, curvature),
        },
    }
}

fn control_offset(distance: f32, curvature: f32) -> f32 {
    if distance >= 0.0 {
        0.5 * distance
    } else {
        curvature * 25.0 * (-distance).sqrt()
    }
}
