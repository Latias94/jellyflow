use jellyflow_core::core::{CanvasPoint, EdgeRouteSpec};

use super::super::endpoints::EdgeEndpointPosition;
use super::orthogonal::{OrthogonalEdgeOptions, plan_orthogonal_route};
use super::types::{EdgePath, PathCommand};

/// Smoothstep-like edge options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothStepEdgeOptions {
    pub offset: f32,
    pub step_position: f32,
    pub border_radius: f32,
    pub center: Option<CanvasPoint>,
}

impl Default for SmoothStepEdgeOptions {
    fn default() -> Self {
        Self {
            offset: EdgeRouteSpec::DEFAULT_ORTHOGONAL_OFFSET,
            step_position: EdgeRouteSpec::DEFAULT_STEP_POSITION,
            border_radius: EdgeRouteSpec::DEFAULT_SMOOTH_STEP_BORDER_RADIUS,
            center: None,
        }
    }
}

pub fn smoothstep_edge_path(
    source: EdgeEndpointPosition,
    target: EdgeEndpointPosition,
    options: SmoothStepEdgeOptions,
) -> Option<EdgePath> {
    let border_radius = normalized_border_radius(options.border_radius);
    let route = plan_orthogonal_route(
        source,
        target,
        OrthogonalEdgeOptions {
            offset: options.offset,
            step_position: options.step_position,
            center: options.center,
        },
    )?;
    let first = *route.points.first()?;
    let mut commands = Vec::with_capacity(route.points.len() * 2);
    commands.push(PathCommand::MoveTo(first));
    let mut current = first;
    for points in route.points.windows(3) {
        let [before, corner, after] = [points[0], points[1], points[2]];
        let Some((bend_start, bend_end)) = rounded_corner(before, corner, after, border_radius)
        else {
            push_line(&mut commands, &mut current, corner);
            continue;
        };
        push_line(&mut commands, &mut current, bend_start);
        let control1 = interpolate(bend_start, corner, 2.0 / 3.0);
        let control2 = interpolate(bend_end, corner, 2.0 / 3.0);
        commands.push(PathCommand::CubicTo {
            control1,
            control2,
            to: bend_end,
        });
        current = bend_end;
    }
    push_line(&mut commands, &mut current, *route.points.last()?);

    Some(EdgePath {
        commands,
        label: route.label,
    })
}

fn normalized_border_radius(border_radius: f32) -> f32 {
    if border_radius.is_finite() && border_radius >= 0.0 {
        border_radius
    } else {
        SmoothStepEdgeOptions::default().border_radius
    }
}

fn rounded_corner(
    before: CanvasPoint,
    corner: CanvasPoint,
    after: CanvasPoint,
    radius: f32,
) -> Option<(CanvasPoint, CanvasPoint)> {
    if (before.x == corner.x && corner.x == after.x)
        || (before.y == corner.y && corner.y == after.y)
    {
        return None;
    }
    let incoming = distance(before, corner);
    let outgoing = distance(corner, after);
    let bend = radius.min(incoming * 0.5).min(outgoing * 0.5);
    (bend > f32::EPSILON).then(|| {
        (
            interpolate(corner, before, bend / incoming),
            interpolate(corner, after, bend / outgoing),
        )
    })
}

fn distance(a: CanvasPoint, b: CanvasPoint) -> f32 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

fn interpolate(from: CanvasPoint, to: CanvasPoint, amount: f32) -> CanvasPoint {
    CanvasPoint {
        x: from.x + (to.x - from.x) * amount,
        y: from.y + (to.y - from.y) * amount,
    }
}

fn push_line(commands: &mut Vec<PathCommand>, current: &mut CanvasPoint, point: CanvasPoint) {
    if *current != point {
        commands.push(PathCommand::LineTo(point));
        *current = point;
    }
}
