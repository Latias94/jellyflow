use jellyflow_core::core::{CanvasPoint, EdgeRouteSpec};

use super::super::endpoints::{EdgeEndpointPosition, HandlePosition};
use super::types::{EdgePath, EdgePathLabel, PathCommand};

/// Options for a direction-aware orthogonal edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrthogonalEdgeOptions {
    pub offset: f32,
    pub step_position: f32,
    pub center: Option<CanvasPoint>,
}

impl Default for OrthogonalEdgeOptions {
    fn default() -> Self {
        Self {
            offset: EdgeRouteSpec::DEFAULT_ORTHOGONAL_OFFSET,
            step_position: EdgeRouteSpec::DEFAULT_STEP_POSITION,
            center: None,
        }
    }
}

pub fn orthogonal_edge_path(
    source: EdgeEndpointPosition,
    target: EdgeEndpointPosition,
    options: OrthogonalEdgeOptions,
) -> Option<EdgePath> {
    let route = plan_orthogonal_route(source, target, options)?;
    let mut commands = Vec::with_capacity(route.points.len());
    commands.push(PathCommand::MoveTo(*route.points.first()?));
    commands.extend(
        route
            .points
            .iter()
            .skip(1)
            .copied()
            .map(PathCommand::LineTo),
    );
    Some(EdgePath {
        commands,
        label: route.label,
    })
}

pub(super) struct OrthogonalRoute {
    pub(super) points: Vec<CanvasPoint>,
    pub(super) label: EdgePathLabel,
}

pub(super) fn plan_orthogonal_route(
    source: EdgeEndpointPosition,
    target: EdgeEndpointPosition,
    options: OrthogonalEdgeOptions,
) -> Option<OrthogonalRoute> {
    if !source.point.is_finite() || !target.point.is_finite() {
        return None;
    }

    let options = normalized_options(options);
    let source_direction = handle_direction(source.position);
    let target_direction = handle_direction(target.position);
    let source_gap = translate(source.point, source_direction, options.offset)?;
    let target_gap = translate(target.point, target_direction, options.offset)?;
    let axis = Axis::for_handle(source.position);
    let route_direction = if coordinate(source_gap, axis) < coordinate(target_gap, axis) {
        1.0
    } else {
        -1.0
    };

    let mut source_gap_offset = CanvasPoint { x: 0.0, y: 0.0 };
    let mut target_gap_offset = CanvasPoint { x: 0.0, y: 0.0 };
    let mut intermediate = Vec::with_capacity(2);
    let label_point;

    if component(source_direction, axis) * component(target_direction, axis) == -1.0 {
        let center_axis = options.center.map_or_else(
            || {
                coordinate(source_gap, axis)
                    + (coordinate(target_gap, axis) - coordinate(source_gap, axis))
                        * options.step_position
            },
            |center| coordinate(center, axis),
        );
        let center_cross = options.center.map_or_else(
            || {
                0.5 * (coordinate(source_gap, axis.opposite())
                    + coordinate(target_gap, axis.opposite()))
            },
            |center| coordinate(center, axis.opposite()),
        );
        label_point = point_from_axes(axis, center_axis, center_cross);

        let split_along_source_axis = [
            point_from_axes(axis, center_axis, coordinate(source_gap, axis.opposite())),
            point_from_axes(axis, center_axis, coordinate(target_gap, axis.opposite())),
        ];
        let split_across_source_axis = [
            point_from_axes(axis, coordinate(source_gap, axis), center_cross),
            point_from_axes(axis, coordinate(target_gap, axis), center_cross),
        ];
        let chosen = if component(source_direction, axis) == route_direction {
            split_along_source_axis
        } else {
            split_across_source_axis
        };
        intermediate.extend(chosen);
    } else {
        let source_axis_target_cross = point_from_axes(
            axis,
            coordinate(source_gap, axis),
            coordinate(target_gap, axis.opposite()),
        );
        let target_axis_source_cross = point_from_axes(
            axis,
            coordinate(target_gap, axis),
            coordinate(source_gap, axis.opposite()),
        );
        let mut bend = match axis {
            Axis::X if component(source_direction, axis) == route_direction => {
                target_axis_source_cross
            }
            Axis::X => source_axis_target_cross,
            Axis::Y if component(source_direction, axis) == route_direction => {
                source_axis_target_cross
            }
            Axis::Y => target_axis_source_cross,
        };

        if source.position == target.position {
            let distance = (coordinate(source.point, axis) - coordinate(target.point, axis)).abs();
            if distance <= options.offset {
                let gap = (options.offset - 1.0)
                    .min(options.offset - distance)
                    .max(0.0);
                if component(source_direction, axis) == route_direction {
                    set_coordinate(
                        &mut source_gap_offset,
                        axis,
                        if coordinate(source_gap, axis) > coordinate(source.point, axis) {
                            -gap
                        } else {
                            gap
                        },
                    );
                } else {
                    set_coordinate(
                        &mut target_gap_offset,
                        axis,
                        if coordinate(target_gap, axis) > coordinate(target.point, axis) {
                            -gap
                        } else {
                            gap
                        },
                    );
                }
            }
        } else if Axis::for_handle(target.position) != axis {
            let cross_axis = axis.opposite();
            let same_direction =
                component(source_direction, axis) == component(target_direction, cross_axis);
            let source_cross = coordinate(source_gap, cross_axis);
            let target_cross = coordinate(target_gap, cross_axis);
            let flip = (component(source_direction, axis) == 1.0
                && ((!same_direction && source_cross > target_cross)
                    || (same_direction && source_cross < target_cross)))
                || (component(source_direction, axis) != 1.0
                    && ((!same_direction && source_cross < target_cross)
                        || (same_direction && source_cross > target_cross)));
            if flip {
                bend = match axis {
                    Axis::X => source_axis_target_cross,
                    Axis::Y => target_axis_source_cross,
                };
            }
        }
        let adjusted_source = add(source_gap, source_gap_offset)?;
        let adjusted_target = add(target_gap, target_gap_offset)?;
        bend = if bend == source_axis_target_cross {
            point_from_axes(
                axis,
                coordinate(adjusted_source, axis),
                coordinate(adjusted_target, axis.opposite()),
            )
        } else {
            point_from_axes(
                axis,
                coordinate(adjusted_target, axis),
                coordinate(adjusted_source, axis.opposite()),
            )
        };
        intermediate.push(bend);

        let axis_distance = (coordinate(adjusted_source, axis) - coordinate(bend, axis))
            .abs()
            .max((coordinate(adjusted_target, axis) - coordinate(bend, axis)).abs());
        let cross_distance = (coordinate(adjusted_source, axis.opposite())
            - coordinate(bend, axis.opposite()))
        .abs()
        .max(
            (coordinate(adjusted_target, axis.opposite()) - coordinate(bend, axis.opposite()))
                .abs(),
        );
        label_point = if axis_distance >= cross_distance {
            point_from_axes(
                axis,
                0.5 * (coordinate(adjusted_source, axis) + coordinate(adjusted_target, axis)),
                coordinate(bend, axis.opposite()),
            )
        } else {
            point_from_axes(
                axis,
                coordinate(bend, axis),
                0.5 * (coordinate(adjusted_source, axis.opposite())
                    + coordinate(adjusted_target, axis.opposite())),
            )
        };
    }

    let adjusted_source = add(source_gap, source_gap_offset)?;
    let adjusted_target = add(target_gap, target_gap_offset)?;
    let mut points = Vec::with_capacity(intermediate.len() + 4);
    push_distinct(&mut points, source.point);
    if intermediate
        .first()
        .is_some_and(|first| *first != adjusted_source)
    {
        push_distinct(&mut points, adjusted_source);
    }
    for point in intermediate {
        push_distinct(&mut points, point);
    }
    if points.last().is_some_and(|last| *last != adjusted_target) {
        push_distinct(&mut points, adjusted_target);
    }
    push_distinct(&mut points, target.point);

    Some(OrthogonalRoute {
        points,
        label: EdgePathLabel {
            point: label_point,
            offset_x: (label_point.x - source.point.x).abs(),
            offset_y: (label_point.y - source.point.y).abs(),
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

impl Axis {
    fn for_handle(position: HandlePosition) -> Self {
        match position {
            HandlePosition::Left | HandlePosition::Right => Self::X,
            HandlePosition::Top | HandlePosition::Bottom => Self::Y,
        }
    }

    fn opposite(self) -> Self {
        match self {
            Self::X => Self::Y,
            Self::Y => Self::X,
        }
    }
}

fn normalized_options(options: OrthogonalEdgeOptions) -> OrthogonalEdgeOptions {
    OrthogonalEdgeOptions {
        offset: if options.offset.is_finite() && options.offset >= 0.0 {
            options.offset
        } else {
            OrthogonalEdgeOptions::default().offset
        },
        step_position: if options.step_position.is_finite() {
            options.step_position.clamp(0.0, 1.0)
        } else {
            OrthogonalEdgeOptions::default().step_position
        },
        center: options.center.filter(|center| center.is_finite()),
    }
}

fn handle_direction(position: HandlePosition) -> CanvasPoint {
    match position {
        HandlePosition::Top => CanvasPoint { x: 0.0, y: -1.0 },
        HandlePosition::Right => CanvasPoint { x: 1.0, y: 0.0 },
        HandlePosition::Bottom => CanvasPoint { x: 0.0, y: 1.0 },
        HandlePosition::Left => CanvasPoint { x: -1.0, y: 0.0 },
    }
}

fn coordinate(point: CanvasPoint, axis: Axis) -> f32 {
    match axis {
        Axis::X => point.x,
        Axis::Y => point.y,
    }
}

fn set_coordinate(point: &mut CanvasPoint, axis: Axis, value: f32) {
    match axis {
        Axis::X => point.x = value,
        Axis::Y => point.y = value,
    }
}

fn component(direction: CanvasPoint, axis: Axis) -> f32 {
    coordinate(direction, axis)
}

fn point_from_axes(axis: Axis, along: f32, across: f32) -> CanvasPoint {
    match axis {
        Axis::X => CanvasPoint {
            x: along,
            y: across,
        },
        Axis::Y => CanvasPoint {
            x: across,
            y: along,
        },
    }
}

fn translate(point: CanvasPoint, direction: CanvasPoint, distance: f32) -> Option<CanvasPoint> {
    let translated = CanvasPoint {
        x: point.x + direction.x * distance,
        y: point.y + direction.y * distance,
    };
    translated.is_finite().then_some(translated)
}

fn add(a: CanvasPoint, b: CanvasPoint) -> Option<CanvasPoint> {
    let sum = CanvasPoint {
        x: a.x + b.x,
        y: a.y + b.y,
    };
    sum.is_finite().then_some(sum)
}

fn push_distinct(points: &mut Vec<CanvasPoint>, point: CanvasPoint) {
    if points.last().is_none_or(|last| *last != point) {
        points.push(point);
    }
}
