use jellyflow_core::core::CanvasPoint;

use super::super::hit_test::{
    EdgeFlattenOptions, EdgePathFlattenError, EdgePathSegment, flatten_edge_path,
};
use super::types::{EdgePath, PathCommand};

/// A semantic position along an edge path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgePathAnchor {
    Source,
    Center,
    Target,
    /// A normalized arc-length progress. Finite values are clamped to `0.0..=1.0`.
    Progress(f32),
}

impl EdgePathAnchor {
    fn progress(self) -> Result<f32, EdgePathSampleError> {
        let progress = match self {
            Self::Source => 0.0,
            Self::Center => 0.5,
            Self::Target => 1.0,
            Self::Progress(progress) if progress.is_finite() => progress.clamp(0.0, 1.0),
            Self::Progress(_) => return Err(EdgePathSampleError::InvalidProgress),
        };
        Ok(progress)
    }
}

/// A point and source-to-target unit tangent sampled from an edge path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePathSample {
    pub point: CanvasPoint,
    pub tangent: CanvasPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EdgePathSampleError {
    #[error("edge path sample progress must be finite")]
    InvalidProgress,
    #[error("edge path has no point to sample")]
    EmptyPath,
    #[error(transparent)]
    Flatten(#[from] EdgePathFlattenError),
}

impl EdgePath {
    /// Samples this resolved path at a semantic arc-length anchor.
    pub fn sample(&self, anchor: EdgePathAnchor) -> Result<EdgePathSample, EdgePathSampleError> {
        self.sample_with_options(anchor, EdgeFlattenOptions::default())
    }

    /// Samples this resolved path using explicit curve-flattening limits.
    pub fn sample_with_options(
        &self,
        anchor: EdgePathAnchor,
        options: EdgeFlattenOptions,
    ) -> Result<EdgePathSample, EdgePathSampleError> {
        let progress = anchor.progress()?;
        let flattened = flatten_edge_path(self, options)?;
        let first_point = first_path_point(self).ok_or(EdgePathSampleError::EmptyPath)?;
        let last_point = last_path_point(self).ok_or(EdgePathSampleError::EmptyPath)?;

        let total_length = flattened
            .segments()
            .iter()
            .map(|segment| segment_length(*segment))
            .sum::<f64>();
        if total_length <= f64::EPSILON {
            return Ok(EdgePathSample {
                point: if progress >= 1.0 {
                    last_point
                } else {
                    first_point
                },
                tangent: endpoint_tangent(self, progress).unwrap_or(DEFAULT_TANGENT),
            });
        }

        let target_length = f64::from(progress) * total_length;
        let mut traversed = 0.0;
        let mut last_non_degenerate = None;

        for segment in flattened.segments() {
            let length = segment_length(*segment);
            if length <= f64::EPSILON {
                continue;
            }
            last_non_degenerate = Some((*segment, length));
            let segment_end = traversed + length;
            if target_length < segment_end || progress <= 0.0 {
                let amount = ((target_length - traversed) / length).clamp(0.0, 1.0);
                let point = interpolate(segment.from(), segment.to(), amount);
                let tangent = if progress <= 0.0 {
                    source_tangent(self)
                } else {
                    unit_tangent(segment.from(), segment.to())
                }
                .unwrap_or(DEFAULT_TANGENT);
                return Ok(EdgePathSample { point, tangent });
            }
            traversed = segment_end;
        }

        let (last, _) = last_non_degenerate.ok_or(EdgePathSampleError::EmptyPath)?;
        Ok(EdgePathSample {
            point: last.to(),
            tangent: target_tangent(self)
                .or_else(|| unit_tangent(last.from(), last.to()))
                .unwrap_or(DEFAULT_TANGENT),
        })
    }
}

const DEFAULT_TANGENT: CanvasPoint = CanvasPoint { x: 1.0, y: 0.0 };

fn first_path_point(path: &EdgePath) -> Option<CanvasPoint> {
    path.commands.first().map(command_endpoint)
}

fn last_path_point(path: &EdgePath) -> Option<CanvasPoint> {
    path.commands.last().map(command_endpoint)
}

fn command_endpoint(command: &PathCommand) -> CanvasPoint {
    match *command {
        PathCommand::MoveTo(point) | PathCommand::LineTo(point) => point,
        PathCommand::CubicTo { to, .. } => to,
    }
}

fn endpoint_tangent(path: &EdgePath, progress: f32) -> Option<CanvasPoint> {
    if progress <= 0.5 {
        source_tangent(path).or_else(|| target_tangent(path))
    } else {
        target_tangent(path).or_else(|| source_tangent(path))
    }
}

fn source_tangent(path: &EdgePath) -> Option<CanvasPoint> {
    let mut current = None;
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(point) => current = Some(point),
            PathCommand::LineTo(to) => {
                let from = current?;
                if let Some(tangent) = unit_tangent(from, to) {
                    return Some(tangent);
                }
                current = Some(to);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let from = current?;
                if let Some(tangent) = unit_tangent(from, control1)
                    .or_else(|| unit_tangent(from, control2))
                    .or_else(|| unit_tangent(from, to))
                {
                    return Some(tangent);
                }
                current = Some(to);
            }
        }
    }
    None
}

fn target_tangent(path: &EdgePath) -> Option<CanvasPoint> {
    let mut current = None;
    let mut tangent = None;
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(point) => current = Some(point),
            PathCommand::LineTo(to) => {
                let from = current?;
                tangent = unit_tangent(from, to).or(tangent);
                current = Some(to);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let from = current?;
                tangent = unit_tangent(control2, to)
                    .or_else(|| unit_tangent(control1, to))
                    .or_else(|| unit_tangent(from, to))
                    .or(tangent);
                current = Some(to);
            }
        }
    }
    tangent
}

fn segment_length(segment: EdgePathSegment) -> f64 {
    let dx = f64::from(segment.to().x) - f64::from(segment.from().x);
    let dy = f64::from(segment.to().y) - f64::from(segment.from().y);
    dx.hypot(dy)
}

fn unit_tangent(from: CanvasPoint, to: CanvasPoint) -> Option<CanvasPoint> {
    let dx = f64::from(to.x) - f64::from(from.x);
    let dy = f64::from(to.y) - f64::from(from.y);
    let length = dx.hypot(dy);
    if !length.is_finite() || length <= f64::EPSILON {
        return None;
    }
    let tangent = CanvasPoint {
        x: (dx / length) as f32,
        y: (dy / length) as f32,
    };
    tangent.is_finite().then_some(tangent)
}

fn interpolate(from: CanvasPoint, to: CanvasPoint, amount: f64) -> CanvasPoint {
    CanvasPoint {
        x: (f64::from(from.x) + (f64::from(to.x) - f64::from(from.x)) * amount) as f32,
        y: (f64::from(from.y) + (f64::from(to.y) - f64::from(from.y)) * amount) as f32,
    }
}
