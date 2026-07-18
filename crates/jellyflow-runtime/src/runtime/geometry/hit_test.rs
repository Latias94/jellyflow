use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

use super::paths::{EdgePath, PathCommand};

pub const DEFAULT_EDGE_INTERACTION_WIDTH: f32 = 18.0;
pub const DEFAULT_EDGE_FLATTEN_ERROR: f32 = 0.75;
pub const DEFAULT_EDGE_FLATTEN_MAX_DEPTH: u8 = 12;
pub const DEFAULT_EDGE_FLATTEN_MAX_SEGMENTS: usize = 256;

/// Options for numeric edge hit testing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeHitTestOptions {
    pub interaction_width: f32,
    pub flatten: EdgeFlattenOptions,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeFlattenOptions {
    pub max_error: f32,
    pub max_depth: u8,
    pub max_segments: usize,
}

impl EdgeHitTestOptions {
    pub fn new(interaction_width: f32, max_error: f32) -> Self {
        let width = if interaction_width.is_finite() && interaction_width > 0.0 {
            interaction_width
        } else {
            DEFAULT_EDGE_INTERACTION_WIDTH
        };
        Self {
            interaction_width: width,
            flatten: EdgeFlattenOptions::new(max_error),
        }
    }

    pub fn with_limits(mut self, max_depth: u8, max_segments: usize) -> Self {
        self.flatten = self.flatten.with_limits(max_depth, max_segments);
        self
    }

    /// Converts screen-pixel interaction and flatten tolerances to canvas space once.
    pub fn for_zoom(mut self, zoom: f32) -> Option<Self> {
        if !zoom.is_finite() || zoom <= 0.0 {
            return None;
        }
        self.interaction_width /= zoom;
        self.flatten = self.flatten.for_zoom(zoom)?;
        Some(self)
    }

    fn radius(self) -> f32 {
        self.interaction_width * 0.5
    }
}

impl EdgeFlattenOptions {
    pub fn new(max_error: f32) -> Self {
        Self {
            max_error: if max_error.is_finite() && max_error > 0.0 {
                max_error
            } else {
                DEFAULT_EDGE_FLATTEN_ERROR
            },
            max_depth: DEFAULT_EDGE_FLATTEN_MAX_DEPTH,
            max_segments: DEFAULT_EDGE_FLATTEN_MAX_SEGMENTS,
        }
    }

    pub fn with_limits(mut self, max_depth: u8, max_segments: usize) -> Self {
        self.max_depth = max_depth;
        self.max_segments = max_segments.max(1);
        self
    }

    pub fn for_zoom(mut self, zoom: f32) -> Option<Self> {
        if !zoom.is_finite() || zoom <= 0.0 {
            return None;
        }
        self.max_error /= zoom;
        Some(self)
    }
}

impl Default for EdgeFlattenOptions {
    fn default() -> Self {
        Self {
            max_error: DEFAULT_EDGE_FLATTEN_ERROR,
            max_depth: DEFAULT_EDGE_FLATTEN_MAX_DEPTH,
            max_segments: DEFAULT_EDGE_FLATTEN_MAX_SEGMENTS,
        }
    }
}

impl Default for EdgeHitTestOptions {
    fn default() -> Self {
        Self {
            interaction_width: DEFAULT_EDGE_INTERACTION_WIDTH,
            flatten: EdgeFlattenOptions::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePathSegment {
    from: CanvasPoint,
    to: CanvasPoint,
}

impl EdgePathSegment {
    pub fn from(self) -> CanvasPoint {
        self.from
    }

    pub fn to(self) -> CanvasPoint {
        self.to
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlattenedEdgePath {
    segments: Vec<EdgePathSegment>,
    conservative_bounds: Option<CanvasRect>,
}

impl FlattenedEdgePath {
    pub fn segments(&self) -> &[EdgePathSegment] {
        &self.segments
    }

    pub fn conservative_bounds(&self) -> Option<CanvasRect> {
        self.conservative_bounds
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EdgePathFlattenError {
    #[error("edge path contains invalid geometry")]
    InvalidGeometry,
    #[error("edge path flattening exceeded the maximum subdivision depth {max_depth}")]
    DepthLimitExceeded { max_depth: u8 },
    #[error("edge path flattening exceeded the maximum segment count {max_segments}")]
    SegmentLimitExceeded { max_segments: usize },
}

pub fn edge_path_contains_point(
    path: &EdgePath,
    point: CanvasPoint,
    options: EdgeHitTestOptions,
) -> Result<bool, EdgePathFlattenError> {
    Ok(edge_path_distance(path, point, options)?
        .is_some_and(|distance| distance <= options.radius()))
}

pub fn edge_path_distance(
    path: &EdgePath,
    point: CanvasPoint,
    options: EdgeHitTestOptions,
) -> Result<Option<f32>, EdgePathFlattenError> {
    if !point.is_finite() {
        return Ok(None);
    }

    let flattened = flatten_edge_path(path, options.flatten)?;
    let mut min_distance = f32::INFINITY;
    for segment in flattened.segments {
        min_distance = min_distance.min(distance_to_segment(point, segment.from, segment.to)?);
    }
    Ok(min_distance.is_finite().then_some(min_distance))
}

pub fn flatten_edge_path(
    path: &EdgePath,
    options: EdgeFlattenOptions,
) -> Result<FlattenedEdgePath, EdgePathFlattenError> {
    let mut current = None;
    let mut segments = Vec::new();
    let mut bounds = PathBounds::default();

    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(to) => {
                bounds.include(to)?;
                current = Some(to);
            }
            PathCommand::LineTo(to) => {
                let from = current.ok_or(EdgePathFlattenError::InvalidGeometry)?;
                bounds.include(to)?;
                push_segment(&mut segments, from, to, options.max_segments)?;
                current = Some(to);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let from = current.ok_or(EdgePathFlattenError::InvalidGeometry)?;
                bounds.include(control1)?;
                bounds.include(control2)?;
                bounds.include(to)?;
                flatten_cubic(
                    &mut segments,
                    Cubic {
                        start: from,
                        control1,
                        control2,
                        end: to,
                    },
                    0,
                    options,
                )?;
                current = Some(to);
            }
        }
    }

    Ok(FlattenedEdgePath {
        segments,
        conservative_bounds: bounds.finish()?,
    })
}

pub(crate) fn edge_path_conservative_bounds(
    path: &EdgePath,
) -> Result<Option<CanvasRect>, EdgePathFlattenError> {
    let mut bounds = PathBounds::default();
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(point) | PathCommand::LineTo(point) => bounds.include(point)?,
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                bounds.include(control1)?;
                bounds.include(control2)?;
                bounds.include(to)?;
            }
        }
    }
    bounds.finish()
}

fn push_segment(
    segments: &mut Vec<EdgePathSegment>,
    from: CanvasPoint,
    to: CanvasPoint,
    max_segments: usize,
) -> Result<(), EdgePathFlattenError> {
    if segments.len() >= max_segments {
        return Err(EdgePathFlattenError::SegmentLimitExceeded { max_segments });
    }
    segments.push(EdgePathSegment { from, to });
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Cubic {
    start: CanvasPoint,
    control1: CanvasPoint,
    control2: CanvasPoint,
    end: CanvasPoint,
}

fn flatten_cubic(
    segments: &mut Vec<EdgePathSegment>,
    curve: Cubic,
    depth: u8,
    options: EdgeFlattenOptions,
) -> Result<(), EdgePathFlattenError> {
    if cubic_flatness(curve)? <= options.max_error {
        return push_segment(segments, curve.start, curve.end, options.max_segments);
    }
    if depth >= options.max_depth {
        return Err(EdgePathFlattenError::DepthLimitExceeded {
            max_depth: options.max_depth,
        });
    }
    let (left, right) = split_cubic(curve)?;
    flatten_cubic(segments, left, depth + 1, options)?;
    flatten_cubic(segments, right, depth + 1, options)
}

fn cubic_flatness(curve: Cubic) -> Result<f32, EdgePathFlattenError> {
    let first = distance_to_infinite_line(curve.control1, curve.start, curve.end)?;
    let second = distance_to_infinite_line(curve.control2, curve.start, curve.end)?;
    Ok(first.max(second))
}

fn split_cubic(curve: Cubic) -> Result<(Cubic, Cubic), EdgePathFlattenError> {
    let a = midpoint(curve.start, curve.control1)?;
    let b = midpoint(curve.control1, curve.control2)?;
    let c = midpoint(curve.control2, curve.end)?;
    let d = midpoint(a, b)?;
    let e = midpoint(b, c)?;
    let center = midpoint(d, e)?;
    Ok((
        Cubic {
            start: curve.start,
            control1: a,
            control2: d,
            end: center,
        },
        Cubic {
            start: center,
            control1: e,
            control2: c,
            end: curve.end,
        },
    ))
}

fn midpoint(a: CanvasPoint, b: CanvasPoint) -> Result<CanvasPoint, EdgePathFlattenError> {
    let midpoint = CanvasPoint {
        x: a.x * 0.5 + b.x * 0.5,
        y: a.y * 0.5 + b.y * 0.5,
    };
    midpoint
        .is_finite()
        .then_some(midpoint)
        .ok_or(EdgePathFlattenError::InvalidGeometry)
}

fn distance_to_infinite_line(
    point: CanvasPoint,
    a: CanvasPoint,
    b: CanvasPoint,
) -> Result<f32, EdgePathFlattenError> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = (dx * dx + dy * dy).sqrt();
    if !length.is_finite() {
        return Err(EdgePathFlattenError::InvalidGeometry);
    }
    if length <= f32::EPSILON {
        return distance(point, a);
    }
    let cross = ((point.x - a.x) * dy - (point.y - a.y) * dx).abs();
    let distance = cross / length;
    distance
        .is_finite()
        .then_some(distance)
        .ok_or(EdgePathFlattenError::InvalidGeometry)
}

fn distance_to_segment(
    point: CanvasPoint,
    a: CanvasPoint,
    b: CanvasPoint,
) -> Result<f32, EdgePathFlattenError> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if !len_sq.is_finite() {
        return Err(EdgePathFlattenError::InvalidGeometry);
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

fn distance(a: CanvasPoint, b: CanvasPoint) -> Result<f32, EdgePathFlattenError> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let distance = (dx * dx + dy * dy).sqrt();
    distance
        .is_finite()
        .then_some(distance)
        .ok_or(EdgePathFlattenError::InvalidGeometry)
}

#[derive(Debug, Default)]
struct PathBounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    initialized: bool,
}

impl PathBounds {
    fn include(&mut self, point: CanvasPoint) -> Result<(), EdgePathFlattenError> {
        if !point.is_finite() {
            return Err(EdgePathFlattenError::InvalidGeometry);
        }
        if !self.initialized {
            self.min_x = point.x;
            self.min_y = point.y;
            self.max_x = point.x;
            self.max_y = point.y;
            self.initialized = true;
            return Ok(());
        }
        self.min_x = self.min_x.min(point.x);
        self.min_y = self.min_y.min(point.y);
        self.max_x = self.max_x.max(point.x);
        self.max_y = self.max_y.max(point.y);
        Ok(())
    }

    fn finish(self) -> Result<Option<CanvasRect>, EdgePathFlattenError> {
        if !self.initialized {
            return Ok(None);
        }
        let rect = CanvasRect {
            origin: CanvasPoint {
                x: self.min_x,
                y: self.min_y,
            },
            size: CanvasSize {
                width: self.max_x - self.min_x,
                height: self.max_y - self.min_y,
            },
        };
        rect.is_finite()
            .then_some(Some(rect))
            .ok_or(EdgePathFlattenError::InvalidGeometry)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EdgeFlattenOptions, EdgeHitTestOptions, EdgePathFlattenError, edge_path_contains_point,
        edge_path_distance, flatten_edge_path,
    };
    use crate::runtime::geometry::{
        BezierEdgeOptions, EdgeEndpointPosition, EdgePath, HandlePosition, PathCommand,
        bezier_edge_path, straight_edge_path,
    };
    use jellyflow_core::core::CanvasPoint;

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
        let options = EdgeHitTestOptions::new(12.0, 0.75);

        assert!(
            edge_path_contains_point(&straight, CanvasPoint { x: 50.0, y: 5.0 }, options).unwrap()
        );
        assert!(
            !edge_path_contains_point(&straight, CanvasPoint { x: 50.0, y: 7.0 }, options).unwrap()
        );

        let bezier =
            bezier_edge_path(source, target, BezierEdgeOptions::default()).expect("bezier path");
        let distance = edge_path_distance(&bezier, CanvasPoint { x: 50.0, y: 0.0 }, options)
            .expect("flatten")
            .expect("distance");
        assert!(distance <= 1.0e-3);
    }

    #[test]
    fn screen_space_interaction_width_is_zoom_invariant() {
        let source = EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 0.0 },
            position: HandlePosition::Right,
        };
        let target = EdgeEndpointPosition {
            point: CanvasPoint { x: 100.0, y: 0.0 },
            position: HandlePosition::Left,
        };
        let path = straight_edge_path(source, target).expect("path");

        for zoom in [0.5, 1.0, 2.0] {
            let options = EdgeHitTestOptions::default()
                .for_zoom(zoom)
                .expect("valid zoom");
            assert!(
                edge_path_contains_point(
                    &path,
                    CanvasPoint {
                        x: 50.0,
                        y: 8.0 / zoom,
                    },
                    options,
                )
                .unwrap()
            );
            assert!(
                !edge_path_contains_point(
                    &path,
                    CanvasPoint {
                        x: 50.0,
                        y: 10.0 / zoom,
                    },
                    options,
                )
                .unwrap()
            );
        }
    }

    #[test]
    fn adaptive_flattening_reports_depth_and_segment_limits() {
        let curved = EdgePath {
            commands: vec![
                PathCommand::MoveTo(CanvasPoint { x: 0.0, y: 0.0 }),
                PathCommand::CubicTo {
                    control1: CanvasPoint { x: 0.0, y: 1_000.0 },
                    control2: CanvasPoint {
                        x: 1_000.0,
                        y: 1_000.0,
                    },
                    to: CanvasPoint { x: 1_000.0, y: 0.0 },
                },
            ],
            label: crate::runtime::geometry::EdgePathLabel {
                point: CanvasPoint { x: 500.0, y: 750.0 },
                offset_x: 500.0,
                offset_y: 750.0,
            },
        };
        let depth_limited = EdgeFlattenOptions::new(0.000_001).with_limits(1, 256);
        assert_eq!(
            flatten_edge_path(&curved, depth_limited),
            Err(EdgePathFlattenError::DepthLimitExceeded { max_depth: 1 })
        );

        let two_lines = EdgePath {
            commands: vec![
                PathCommand::MoveTo(CanvasPoint { x: 0.0, y: 0.0 }),
                PathCommand::LineTo(CanvasPoint { x: 10.0, y: 0.0 }),
                PathCommand::LineTo(CanvasPoint { x: 20.0, y: 0.0 }),
            ],
            label: crate::runtime::geometry::EdgePathLabel {
                point: CanvasPoint { x: 10.0, y: 0.0 },
                offset_x: 10.0,
                offset_y: 0.0,
            },
        };
        let segment_limited = EdgeFlattenOptions::default().with_limits(12, 1);
        assert_eq!(
            flatten_edge_path(&two_lines, segment_limited),
            Err(EdgePathFlattenError::SegmentLimitExceeded { max_segments: 1 })
        );
    }
}
