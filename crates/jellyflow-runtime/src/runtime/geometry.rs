use crate::node_origin::normalize_node_origin;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

/// Side of a node or handle where an edge endpoint attaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandlePosition {
    Top,
    Right,
    Bottom,
    Left,
}

/// Renderer-neutral handle bounds relative to the owning node's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandleBounds {
    pub rect: CanvasRect,
    pub position: HandlePosition,
}

/// Input for resolving one edge endpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeEndpointInput {
    pub node_rect: CanvasRect,
    pub handle: Option<HandleBounds>,
    pub fallback_position: HandlePosition,
}

/// Resolved edge endpoint in canvas space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeEndpointPosition {
    pub point: CanvasPoint,
    pub position: HandlePosition,
}

/// Resolved source and target endpoint geometry in canvas space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePosition {
    pub source: EdgeEndpointPosition,
    pub target: EdgeEndpointPosition,
}

/// Renderer-neutral path command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    MoveTo(CanvasPoint),
    LineTo(CanvasPoint),
    CubicTo {
        control1: CanvasPoint,
        control2: CanvasPoint,
        to: CanvasPoint,
    },
}

/// Label placement derived from an edge path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePathLabel {
    pub point: CanvasPoint,
    pub offset_x: f32,
    pub offset_y: f32,
}

/// Renderer-neutral edge path.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgePath {
    pub commands: Vec<PathCommand>,
    pub label: EdgePathLabel,
}

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

/// Smoothstep-like edge options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothStepEdgeOptions {
    pub offset: f32,
    pub step_position: f32,
}

impl Default for SmoothStepEdgeOptions {
    fn default() -> Self {
        Self {
            offset: 20.0,
            step_position: 0.5,
        }
    }
}

/// Options for numeric edge hit testing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeHitTestOptions {
    pub interaction_width: f32,
    pub curve_samples: usize,
}

impl EdgeHitTestOptions {
    pub fn new(interaction_width: f32, curve_samples: usize) -> Self {
        let width = if interaction_width.is_finite() && interaction_width > 0.0 {
            interaction_width
        } else {
            1.0
        };
        let samples = curve_samples.max(1);
        Self {
            interaction_width: width,
            curve_samples: samples,
        }
    }

    fn radius(self) -> f32 {
        self.interaction_width * 0.5
    }
}

impl Default for EdgeHitTestOptions {
    fn default() -> Self {
        Self {
            interaction_width: 12.0,
            curve_samples: 24,
        }
    }
}

pub fn edge_position(source: EdgeEndpointInput, target: EdgeEndpointInput) -> Option<EdgePosition> {
    Some(EdgePosition {
        source: handle_anchor_position(source.node_rect, source.handle, source.fallback_position)?,
        target: handle_anchor_position(target.node_rect, target.handle, target.fallback_position)?,
    })
}

pub fn handle_anchor_position(
    node_rect: CanvasRect,
    handle: Option<HandleBounds>,
    fallback_position: HandlePosition,
) -> Option<EdgeEndpointPosition> {
    let (rect, position) = absolute_handle_rect(node_rect, handle, fallback_position)?;
    let point = match position {
        HandlePosition::Top => CanvasPoint {
            x: rect.origin.x + rect.size.width * 0.5,
            y: rect.origin.y,
        },
        HandlePosition::Right => CanvasPoint {
            x: rect.origin.x + rect.size.width,
            y: rect.origin.y + rect.size.height * 0.5,
        },
        HandlePosition::Bottom => CanvasPoint {
            x: rect.origin.x + rect.size.width * 0.5,
            y: rect.origin.y + rect.size.height,
        },
        HandlePosition::Left => CanvasPoint {
            x: rect.origin.x,
            y: rect.origin.y + rect.size.height * 0.5,
        },
    };

    point
        .is_finite()
        .then_some(EdgeEndpointPosition { point, position })
}

pub fn straight_edge_path(
    source: EdgeEndpointPosition,
    target: EdgeEndpointPosition,
) -> Option<EdgePath> {
    let label = edge_center_label(source.point, target.point)?;
    Some(EdgePath {
        commands: vec![
            PathCommand::MoveTo(source.point),
            PathCommand::LineTo(target.point),
        ],
        label,
    })
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

pub fn smoothstep_edge_path(
    source: EdgeEndpointPosition,
    target: EdgeEndpointPosition,
    options: SmoothStepEdgeOptions,
) -> Option<EdgePath> {
    if !source.point.is_finite() || !target.point.is_finite() {
        return None;
    }

    let options = normalized_smoothstep_options(options);
    let source_dir = handle_direction(source.position);
    let target_dir = handle_direction(target.position);
    let source_gap = translate_point(source.point, source_dir, options.offset)?;
    let target_gap = translate_point(target.point, target_dir, options.offset)?;

    let mut points = Vec::with_capacity(6);
    push_distinct_point(&mut points, source.point);
    push_distinct_point(&mut points, source_gap);

    let horizontal = matches!(
        source.position,
        HandlePosition::Left | HandlePosition::Right
    );
    let label_point = if horizontal {
        let center_x = source_gap.x + (target_gap.x - source_gap.x) * options.step_position;
        push_distinct_point(
            &mut points,
            CanvasPoint {
                x: center_x,
                y: source_gap.y,
            },
        );
        push_distinct_point(
            &mut points,
            CanvasPoint {
                x: center_x,
                y: target_gap.y,
            },
        );
        CanvasPoint {
            x: center_x,
            y: 0.5 * (source_gap.y + target_gap.y),
        }
    } else {
        let center_y = source_gap.y + (target_gap.y - source_gap.y) * options.step_position;
        push_distinct_point(
            &mut points,
            CanvasPoint {
                x: source_gap.x,
                y: center_y,
            },
        );
        push_distinct_point(
            &mut points,
            CanvasPoint {
                x: target_gap.x,
                y: center_y,
            },
        );
        CanvasPoint {
            x: 0.5 * (source_gap.x + target_gap.x),
            y: center_y,
        }
    };

    push_distinct_point(&mut points, target_gap);
    push_distinct_point(&mut points, target.point);

    let mut commands = Vec::with_capacity(points.len());
    let first = *points.first()?;
    commands.push(PathCommand::MoveTo(first));
    for point in points.into_iter().skip(1) {
        commands.push(PathCommand::LineTo(point));
    }

    Some(EdgePath {
        commands,
        label: EdgePathLabel {
            point: label_point,
            offset_x: (label_point.x - source.point.x).abs(),
            offset_y: (label_point.y - source.point.y).abs(),
        },
    })
}

pub fn edge_path_contains_point(
    path: &EdgePath,
    point: CanvasPoint,
    options: EdgeHitTestOptions,
) -> bool {
    edge_path_distance(path, point, options).is_some_and(|distance| distance <= options.radius())
}

pub fn edge_path_distance(
    path: &EdgePath,
    point: CanvasPoint,
    options: EdgeHitTestOptions,
) -> Option<f32> {
    if !point.is_finite() {
        return None;
    }

    let mut current: Option<CanvasPoint> = None;
    let mut min_distance = f32::INFINITY;
    let samples = options.curve_samples.max(1);

    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(to) => {
                if !to.is_finite() {
                    return None;
                }
                current = Some(to);
            }
            PathCommand::LineTo(to) => {
                let from = current?;
                if !to.is_finite() {
                    return None;
                }
                min_distance = min_distance.min(distance_to_segment(point, from, to)?);
                current = Some(to);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let from = current?;
                if !control1.is_finite() || !control2.is_finite() || !to.is_finite() {
                    return None;
                }
                let mut prev = from;
                for step in 1..=samples {
                    let t = step as f32 / samples as f32;
                    let next = cubic_point(from, control1, control2, to, t);
                    min_distance = min_distance.min(distance_to_segment(point, prev, next)?);
                    prev = next;
                }
                current = Some(to);
            }
        }
    }

    min_distance.is_finite().then_some(min_distance)
}

pub fn handle_center_position(
    node_rect: CanvasRect,
    handle: Option<HandleBounds>,
    fallback_position: HandlePosition,
) -> Option<CanvasPoint> {
    let (rect, _position) = absolute_handle_rect(node_rect, handle, fallback_position)?;
    Some(CanvasPoint {
        x: rect.origin.x + rect.size.width * 0.5,
        y: rect.origin.y + rect.size.height * 0.5,
    })
}

fn absolute_handle_rect(
    node_rect: CanvasRect,
    handle: Option<HandleBounds>,
    fallback_position: HandlePosition,
) -> Option<(CanvasRect, HandlePosition)> {
    if !node_rect.is_positive_finite() {
        return None;
    }

    let Some(handle) = handle else {
        return Some((node_rect, fallback_position));
    };

    if !handle.rect.is_positive_finite() {
        return None;
    }

    let rect = CanvasRect {
        origin: CanvasPoint {
            x: node_rect.origin.x + handle.rect.origin.x,
            y: node_rect.origin.y + handle.rect.origin.y,
        },
        size: handle.rect.size,
    };

    rect.is_positive_finite().then_some((rect, handle.position))
}

fn edge_center_label(source: CanvasPoint, target: CanvasPoint) -> Option<EdgePathLabel> {
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

fn bezier_label(
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

fn normalized_smoothstep_options(options: SmoothStepEdgeOptions) -> SmoothStepEdgeOptions {
    SmoothStepEdgeOptions {
        offset: if options.offset.is_finite() && options.offset >= 0.0 {
            options.offset
        } else {
            SmoothStepEdgeOptions::default().offset
        },
        step_position: if options.step_position.is_finite() {
            options.step_position.clamp(0.0, 1.0)
        } else {
            SmoothStepEdgeOptions::default().step_position
        },
    }
}

fn handle_direction(position: HandlePosition) -> (f32, f32) {
    match position {
        HandlePosition::Top => (0.0, -1.0),
        HandlePosition::Right => (1.0, 0.0),
        HandlePosition::Bottom => (0.0, 1.0),
        HandlePosition::Left => (-1.0, 0.0),
    }
}

fn translate_point(
    point: CanvasPoint,
    direction: (f32, f32),
    distance: f32,
) -> Option<CanvasPoint> {
    let out = CanvasPoint {
        x: point.x + direction.0 * distance,
        y: point.y + direction.1 * distance,
    };
    out.is_finite().then_some(out)
}

fn push_distinct_point(points: &mut Vec<CanvasPoint>, point: CanvasPoint) {
    if points.last().is_some_and(|last| *last == point) {
        return;
    }
    points.push(point);
}

fn distance_to_segment(point: CanvasPoint, a: CanvasPoint, b: CanvasPoint) -> Option<f32> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if !len_sq.is_finite() {
        return None;
    }
    if len_sq <= f32::EPSILON {
        return distance(point, a);
    }

    let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    distance(
        point,
        CanvasPoint {
            x: a.x + t * dx,
            y: a.y + t * dy,
        },
    )
}

fn distance(a: CanvasPoint, b: CanvasPoint) -> Option<f32> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let distance = (dx * dx + dy * dy).sqrt();
    distance.is_finite().then_some(distance)
}

fn cubic_point(
    start: CanvasPoint,
    control1: CanvasPoint,
    control2: CanvasPoint,
    end: CanvasPoint,
    t: f32,
) -> CanvasPoint {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let t2 = t * t;
    CanvasPoint {
        x: start.x * mt2 * mt
            + control1.x * 3.0 * mt2 * t
            + control2.x * 3.0 * mt * t2
            + end.x * t2 * t,
        y: start.y * mt2 * mt
            + control1.y * 3.0 * mt2 * t
            + control2.y * 3.0 * mt * t2
            + end.y * t2 * t,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CanvasBounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl CanvasBounds {
    pub(crate) fn empty() -> Self {
        Self {
            min_x: f32::INFINITY,
            min_y: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            max_y: f32::NEG_INFINITY,
        }
    }

    pub(crate) fn from_rect(rect: CanvasRect) -> Option<Self> {
        if !rect.is_finite() {
            return None;
        }

        let width = rect.size.width.max(0.0);
        let height = rect.size.height.max(0.0);
        Some(Self {
            min_x: rect.origin.x,
            min_y: rect.origin.y,
            max_x: rect.origin.x + width,
            max_y: rect.origin.y + height,
        })
    }

    pub(crate) fn from_top_left_rect(pos: CanvasPoint, size: CanvasSize) -> Option<Self> {
        if !pos.is_finite() || !size.is_positive_finite() {
            return None;
        }

        Some(Self {
            min_x: pos.x,
            min_y: pos.y,
            max_x: pos.x + size.width,
            max_y: pos.y + size.height,
        })
    }

    pub(crate) fn from_node(
        pos: CanvasPoint,
        size: CanvasSize,
        node_origin: (f32, f32),
    ) -> Option<Self> {
        let top_left = top_left_from_node_origin(pos, size, node_origin)?;
        Self::from_top_left_rect(top_left, size)
    }

    pub(crate) fn top_left(self) -> CanvasPoint {
        CanvasPoint {
            x: self.min_x,
            y: self.min_y,
        }
    }

    pub(crate) fn center(self) -> CanvasPoint {
        CanvasPoint {
            x: 0.5 * (self.min_x + self.max_x),
            y: 0.5 * (self.min_y + self.max_y),
        }
    }

    pub(crate) fn to_rect(self) -> CanvasRect {
        CanvasRect {
            origin: self.top_left(),
            size: CanvasSize {
                width: (self.max_x - self.min_x).max(0.0),
                height: (self.max_y - self.min_y).max(0.0),
            },
        }
    }

    pub(crate) fn include(&mut self, other: Self) {
        self.min_x = self.min_x.min(other.min_x);
        self.min_y = self.min_y.min(other.min_y);
        self.max_x = self.max_x.max(other.max_x);
        self.max_y = self.max_y.max(other.max_y);
    }

    pub(crate) fn union(mut self, other: Self) -> Self {
        self.include(other);
        self
    }

    pub(crate) fn intersects(self, other: Self) -> bool {
        self.min_x < other.max_x
            && self.max_x > other.min_x
            && self.min_y < other.max_y
            && self.max_y > other.min_y
    }

    pub(crate) fn contains(self, other: Self) -> bool {
        other.min_x >= self.min_x
            && other.min_y >= self.min_y
            && other.max_x <= self.max_x
            && other.max_y <= self.max_y
    }

    pub(crate) fn is_valid(self) -> bool {
        self.min_x.is_finite()
            && self.min_y.is_finite()
            && self.max_x.is_finite()
            && self.max_y.is_finite()
            && self.min_x <= self.max_x
            && self.min_y <= self.max_y
    }
}

pub(crate) fn top_left_from_node_origin(
    pos: CanvasPoint,
    size: CanvasSize,
    node_origin: (f32, f32),
) -> Option<CanvasPoint> {
    if !pos.is_finite() || !size.is_positive_finite() {
        return None;
    }

    let (origin_x, origin_y) = normalize_node_origin(node_origin);
    Some(CanvasPoint {
        x: pos.x - origin_x * size.width,
        y: pos.y - origin_y * size.height,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ViewportFitFrame {
    width_px: f32,
    height_px: f32,
    margin_x: f32,
    margin_y: f32,
}

impl ViewportFitFrame {
    pub(crate) fn from_viewport_and_padding(
        width_px: f32,
        height_px: f32,
        padding: f32,
        margin_px_fallback: f32,
    ) -> Self {
        let (margin_x, margin_y) = if padding > 0.0 {
            (width_px * padding, height_px * padding)
        } else {
            (margin_px_fallback, margin_px_fallback)
        };

        Self {
            width_px,
            height_px,
            margin_x,
            margin_y,
        }
    }

    pub(crate) fn available_width(self) -> f32 {
        self.width_px - 2.0 * self.margin_x
    }

    pub(crate) fn available_height(self) -> f32 {
        self.height_px - 2.0 * self.margin_y
    }

    pub(crate) fn pan_for_center(self, center: CanvasPoint, zoom: f32) -> CanvasPoint {
        CanvasPoint {
            x: 0.5 * self.width_px / zoom - center.x,
            y: 0.5 * self.height_px / zoom - center.y,
        }
    }

    pub(crate) fn fit_rect(
        self,
        target_canvas: CanvasRect,
        min_zoom: f32,
        max_zoom: f32,
    ) -> Option<(CanvasPoint, f32)> {
        if !target_canvas.is_positive_finite() {
            return None;
        }

        let zoom_x = self.available_width() / target_canvas.size.width;
        let zoom_y = self.available_height() / target_canvas.size.height;
        if !zoom_x.is_finite() || !zoom_y.is_finite() {
            return None;
        }

        let zoom = zoom_x.min(zoom_y).clamp(min_zoom, max_zoom);
        if !zoom.is_finite() || zoom <= 0.0 {
            return None;
        }

        let bounds = CanvasBounds::from_rect(target_canvas)?;
        let pan = self.pan_for_center(bounds.center(), zoom);
        if !pan.is_finite() {
            return None;
        }

        Some((pan, zoom))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BezierEdgeOptions, CanvasBounds, EdgeEndpointInput, EdgeEndpointPosition,
        EdgeHitTestOptions, HandleBounds, HandlePosition, PathCommand, SmoothStepEdgeOptions,
        ViewportFitFrame, bezier_edge_path, edge_path_contains_point, edge_path_distance,
        edge_position, handle_anchor_position, handle_center_position, smoothstep_edge_path,
        straight_edge_path, top_left_from_node_origin,
    };
    use crate::io::NodeGraphInteractionState;
    use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

    #[test]
    fn edge_position_uses_handle_rect_side_anchors() {
        let source_rect = CanvasRect {
            origin: CanvasPoint { x: 100.0, y: 50.0 },
            size: CanvasSize {
                width: 200.0,
                height: 100.0,
            },
        };
        let target_rect = CanvasRect {
            origin: CanvasPoint { x: 400.0, y: 80.0 },
            size: CanvasSize {
                width: 120.0,
                height: 80.0,
            },
        };

        let position = edge_position(
            EdgeEndpointInput {
                node_rect: source_rect,
                handle: Some(HandleBounds {
                    rect: CanvasRect {
                        origin: CanvasPoint { x: 190.0, y: 40.0 },
                        size: CanvasSize {
                            width: 10.0,
                            height: 20.0,
                        },
                    },
                    position: HandlePosition::Right,
                }),
                fallback_position: HandlePosition::Bottom,
            },
            EdgeEndpointInput {
                node_rect: target_rect,
                handle: Some(HandleBounds {
                    rect: CanvasRect {
                        origin: CanvasPoint { x: 0.0, y: 30.0 },
                        size: CanvasSize {
                            width: 12.0,
                            height: 20.0,
                        },
                    },
                    position: HandlePosition::Left,
                }),
                fallback_position: HandlePosition::Top,
            },
        )
        .expect("edge position");

        assert_eq!(position.source.position, HandlePosition::Right);
        assert!((position.source.point.x - 300.0).abs() <= 1.0e-6);
        assert!((position.source.point.y - 100.0).abs() <= 1.0e-6);
        assert_eq!(position.target.position, HandlePosition::Left);
        assert!((position.target.point.x - 400.0).abs() <= 1.0e-6);
        assert!((position.target.point.y - 120.0).abs() <= 1.0e-6);
    }

    #[test]
    fn edge_position_falls_back_to_node_side_without_handle_bounds() {
        let node_rect = CanvasRect {
            origin: CanvasPoint { x: 10.0, y: 20.0 },
            size: CanvasSize {
                width: 80.0,
                height: 40.0,
            },
        };

        let source =
            handle_anchor_position(node_rect, None, HandlePosition::Bottom).expect("source anchor");
        assert_eq!(source.position, HandlePosition::Bottom);
        assert!((source.point.x - 50.0).abs() <= 1.0e-6);
        assert!((source.point.y - 60.0).abs() <= 1.0e-6);

        let target =
            handle_anchor_position(node_rect, None, HandlePosition::Top).expect("target anchor");
        assert_eq!(target.position, HandlePosition::Top);
        assert!((target.point.x - 50.0).abs() <= 1.0e-6);
        assert!((target.point.y - 20.0).abs() <= 1.0e-6);
    }

    #[test]
    fn edge_position_can_return_handle_center_for_hit_testing() {
        let node_rect = CanvasRect {
            origin: CanvasPoint { x: 10.0, y: 20.0 },
            size: CanvasSize {
                width: 80.0,
                height: 40.0,
            },
        };
        let center = handle_center_position(
            node_rect,
            Some(HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 70.0, y: 10.0 },
                    size: CanvasSize {
                        width: 10.0,
                        height: 20.0,
                    },
                },
                position: HandlePosition::Right,
            }),
            HandlePosition::Left,
        )
        .expect("center");

        assert!((center.x - 85.0).abs() <= 1.0e-6);
        assert!((center.y - 40.0).abs() <= 1.0e-6);
    }

    #[test]
    fn edge_position_rejects_invalid_node_or_handle_rects() {
        let node_rect = CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 80.0,
                height: 40.0,
            },
        };

        assert!(
            handle_anchor_position(
                CanvasRect {
                    origin: CanvasPoint {
                        x: f32::INFINITY,
                        y: 0.0,
                    },
                    ..node_rect
                },
                None,
                HandlePosition::Left,
            )
            .is_none()
        );
        assert!(
            handle_anchor_position(
                node_rect,
                Some(HandleBounds {
                    rect: CanvasRect {
                        origin: CanvasPoint { x: 0.0, y: 0.0 },
                        size: CanvasSize {
                            width: 0.0,
                            height: 10.0,
                        },
                    },
                    position: HandlePosition::Left,
                }),
                HandlePosition::Left,
            )
            .is_none()
        );
    }

    #[test]
    fn edge_path_straight_commands_and_label_are_renderer_neutral() {
        let source = EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 20.0 },
            position: HandlePosition::Right,
        };
        let target = EdgeEndpointPosition {
            point: CanvasPoint { x: 150.0, y: 100.0 },
            position: HandlePosition::Left,
        };

        let path = straight_edge_path(source, target).expect("path");
        assert_eq!(
            path.commands,
            vec![
                PathCommand::MoveTo(source.point),
                PathCommand::LineTo(target.point)
            ]
        );
        assert!((path.label.point.x - 75.0).abs() <= 1.0e-6);
        assert!((path.label.point.y - 60.0).abs() <= 1.0e-6);
        assert!((path.label.offset_x - 75.0).abs() <= 1.0e-6);
        assert!((path.label.offset_y - 40.0).abs() <= 1.0e-6);
    }

    #[test]
    fn edge_path_bezier_uses_side_curvature_controls() {
        let source = EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 0.0 },
            position: HandlePosition::Right,
        };
        let target = EdgeEndpointPosition {
            point: CanvasPoint { x: 100.0, y: 0.0 },
            position: HandlePosition::Left,
        };

        let path = bezier_edge_path(source, target, BezierEdgeOptions::default()).expect("path");
        assert_eq!(
            path.commands,
            vec![
                PathCommand::MoveTo(source.point),
                PathCommand::CubicTo {
                    control1: CanvasPoint { x: 50.0, y: 0.0 },
                    control2: CanvasPoint { x: 50.0, y: 0.0 },
                    to: target.point,
                },
            ]
        );
        assert!((path.label.point.x - 50.0).abs() <= 1.0e-6);
        assert!((path.label.point.y - 0.0).abs() <= 1.0e-6);
    }

    #[test]
    fn edge_path_smoothstep_like_routes_orthogonal_points() {
        let source = EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 0.0 },
            position: HandlePosition::Right,
        };
        let target = EdgeEndpointPosition {
            point: CanvasPoint { x: 100.0, y: 40.0 },
            position: HandlePosition::Left,
        };

        let path =
            smoothstep_edge_path(source, target, SmoothStepEdgeOptions::default()).expect("path");
        assert_eq!(
            path.commands,
            vec![
                PathCommand::MoveTo(CanvasPoint { x: 0.0, y: 0.0 }),
                PathCommand::LineTo(CanvasPoint { x: 20.0, y: 0.0 }),
                PathCommand::LineTo(CanvasPoint { x: 50.0, y: 0.0 }),
                PathCommand::LineTo(CanvasPoint { x: 50.0, y: 40.0 }),
                PathCommand::LineTo(CanvasPoint { x: 80.0, y: 40.0 }),
                PathCommand::LineTo(CanvasPoint { x: 100.0, y: 40.0 }),
            ]
        );
        assert!((path.label.point.x - 50.0).abs() <= 1.0e-6);
        assert!((path.label.point.y - 20.0).abs() <= 1.0e-6);
    }

    #[test]
    fn hit_test_uses_edge_path_distance_for_straight_and_bezier_paths() {
        let source = EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 0.0 },
            position: HandlePosition::Right,
        };
        let target = EdgeEndpointPosition {
            point: CanvasPoint { x: 100.0, y: 0.0 },
            position: HandlePosition::Left,
        };
        let straight = straight_edge_path(source, target).expect("straight path");
        let options = EdgeHitTestOptions::new(12.0, 24);

        assert!(edge_path_contains_point(
            &straight,
            CanvasPoint { x: 50.0, y: 5.0 },
            options
        ));
        assert!(!edge_path_contains_point(
            &straight,
            CanvasPoint { x: 50.0, y: 7.0 },
            options
        ));

        let bezier =
            bezier_edge_path(source, target, BezierEdgeOptions::default()).expect("bezier path");
        let distance = edge_path_distance(&bezier, CanvasPoint { x: 50.0, y: 0.0 }, options)
            .expect("distance");
        assert!(distance <= 1.0e-3);
    }

    #[test]
    fn hit_test_options_are_derived_from_interaction_state() {
        let mut state = NodeGraphInteractionState::default();
        state.edge_interaction_width = 18.0;
        state.bezier_hit_test_steps = 32;

        let options = state.edge_hit_test_options();
        assert!((options.interaction_width - 18.0).abs() <= 1.0e-6);
        assert_eq!(options.curve_samples, 32);
    }

    #[test]
    fn node_origin_projection_returns_top_left_and_rejects_invalid_geometry() {
        let top_left = top_left_from_node_origin(
            CanvasPoint { x: 20.0, y: 10.0 },
            CanvasSize {
                width: 10.0,
                height: 6.0,
            },
            (0.5, 0.5),
        )
        .expect("top left");

        assert!((top_left.x - 15.0).abs() <= 1.0e-6);
        assert!((top_left.y - 7.0).abs() <= 1.0e-6);

        assert!(
            top_left_from_node_origin(
                CanvasPoint {
                    x: f32::INFINITY,
                    y: 0.0
                },
                CanvasSize {
                    width: 10.0,
                    height: 6.0,
                },
                (0.0, 0.0),
            )
            .is_none()
        );
    }

    #[test]
    fn canvas_bounds_unions_intersects_and_contains_rects() {
        let a = CanvasBounds::from_top_left_rect(
            CanvasPoint { x: 0.0, y: 0.0 },
            CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        )
        .expect("bounds");
        let b = CanvasBounds::from_top_left_rect(
            CanvasPoint { x: 9.0, y: 9.0 },
            CanvasSize {
                width: 5.0,
                height: 5.0,
            },
        )
        .expect("bounds");
        let query = CanvasBounds::from_rect(CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        })
        .expect("query");

        assert!(query.contains(a));
        assert!(query.intersects(b));
        assert!(!query.contains(b));

        let union = a.union(b).to_rect();
        assert!((union.origin.x - 0.0).abs() <= 1.0e-6);
        assert!((union.origin.y - 0.0).abs() <= 1.0e-6);
        assert!((union.size.width - 14.0).abs() <= 1.0e-6);
        assert!((union.size.height - 14.0).abs() <= 1.0e-6);
    }

    #[test]
    fn viewport_fit_frame_targets_canvas_rect() {
        let frame = ViewportFitFrame::from_viewport_and_padding(800.0, 600.0, 0.0, 24.0);
        let (pan, zoom) = frame
            .fit_rect(
                CanvasRect {
                    origin: CanvasPoint { x: 100.0, y: 50.0 },
                    size: CanvasSize {
                        width: 400.0,
                        height: 200.0,
                    },
                },
                0.1,
                4.0,
            )
            .expect("target");

        assert!((zoom - 1.88).abs() <= 1.0e-6);
        assert!((pan.x - (-87.23404)).abs() <= 1.0e-4);
        assert!((pan.y - 9.574471).abs() <= 1.0e-4);
    }
}
