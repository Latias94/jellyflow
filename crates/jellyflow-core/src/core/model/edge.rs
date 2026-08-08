use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::ids::PortId;

use super::{geometry::CanvasPoint, port::PortKind};

fn is_false(v: &bool) -> bool {
    !*v
}

fn edge_view_descriptor_is_default(value: &EdgeViewDescriptor) -> bool {
    value.is_default()
}

/// Edge kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Typed data flow.
    Data,
    /// Exec/control flow.
    Exec,
}

impl EdgeKind {
    pub fn port_kind(self) -> PortKind {
        match self {
            Self::Data => PortKind::Data,
            Self::Exec => PortKind::Exec,
        }
    }
}

/// Complete, renderer-neutral specification for resolving an edge route.
///
/// Every parameter that affects route geometry lives in the selected variant. This prevents a
/// route kind from being combined with parameters that it cannot interpret.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EdgeRouteSpec {
    /// Direct line between resolved endpoints.
    Straight,
    /// Cubic bezier route using endpoint sides as curvature hints.
    Bezier { curvature: f32 },
    /// Orthogonal/polyline route with right-angle legs.
    Orthogonal {
        offset: f32,
        step_position: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        center: Option<CanvasPoint>,
    },
    /// Orthogonal route with rounded corners.
    SmoothStep {
        offset: f32,
        step_position: f32,
        border_radius: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        center: Option<CanvasPoint>,
    },
}

impl EdgeRouteSpec {
    pub const DEFAULT_BEZIER_CURVATURE: f32 = 0.25;
    pub const DEFAULT_ORTHOGONAL_OFFSET: f32 = 20.0;
    pub const DEFAULT_STEP_POSITION: f32 = 0.5;
    pub const DEFAULT_SMOOTH_STEP_BORDER_RADIUS: f32 = 5.0;
    pub const MAX_CURVATURE: f32 = 10.0;
    pub const MAX_OFFSET: f32 = 100_000.0;
    pub const MAX_BORDER_RADIUS: f32 = 100_000.0;

    pub const fn straight() -> Self {
        Self::Straight
    }

    pub fn bezier(curvature: f32) -> Result<Self, EdgeRouteSpecValidationError> {
        let route = Self::Bezier { curvature };
        route.validate()?;
        Ok(route)
    }

    pub fn orthogonal(
        offset: f32,
        step_position: f32,
        center: Option<CanvasPoint>,
    ) -> Result<Self, EdgeRouteSpecValidationError> {
        let route = Self::Orthogonal {
            offset,
            step_position,
            center,
        };
        route.validate()?;
        Ok(route)
    }

    pub fn smooth_step(
        offset: f32,
        step_position: f32,
        border_radius: f32,
        center: Option<CanvasPoint>,
    ) -> Result<Self, EdgeRouteSpecValidationError> {
        let route = Self::SmoothStep {
            offset,
            step_position,
            border_radius,
            center,
        };
        route.validate()?;
        Ok(route)
    }

    pub const fn default_orthogonal() -> Self {
        Self::Orthogonal {
            offset: Self::DEFAULT_ORTHOGONAL_OFFSET,
            step_position: Self::DEFAULT_STEP_POSITION,
            center: None,
        }
    }

    pub const fn default_smooth_step() -> Self {
        Self::SmoothStep {
            offset: Self::DEFAULT_ORTHOGONAL_OFFSET,
            step_position: Self::DEFAULT_STEP_POSITION,
            border_radius: Self::DEFAULT_SMOOTH_STEP_BORDER_RADIUS,
            center: None,
        }
    }

    pub fn validate(self) -> Result<(), EdgeRouteSpecValidationError> {
        match self {
            Self::Straight => Ok(()),
            Self::Bezier { curvature } => {
                validate_bounded_non_negative(curvature, Self::MAX_CURVATURE)
                    .map_err(|()| EdgeRouteSpecValidationError::BezierCurvature { curvature })
            }
            Self::Orthogonal {
                offset,
                step_position,
                center,
            } => {
                validate_bounded_non_negative(offset, Self::MAX_OFFSET)
                    .map_err(|()| EdgeRouteSpecValidationError::OrthogonalOffset { offset })?;
                validate_step_position(step_position).map_err(|()| {
                    EdgeRouteSpecValidationError::OrthogonalStepPosition { step_position }
                })?;
                validate_center(center)
                    .map_err(|center| EdgeRouteSpecValidationError::OrthogonalCenter { center })
            }
            Self::SmoothStep {
                offset,
                step_position,
                border_radius,
                center,
            } => {
                validate_bounded_non_negative(offset, Self::MAX_OFFSET)
                    .map_err(|()| EdgeRouteSpecValidationError::SmoothStepOffset { offset })?;
                validate_step_position(step_position).map_err(|()| {
                    EdgeRouteSpecValidationError::SmoothStepStepPosition { step_position }
                })?;
                validate_bounded_non_negative(border_radius, Self::MAX_BORDER_RADIUS).map_err(
                    |()| EdgeRouteSpecValidationError::SmoothStepBorderRadius { border_radius },
                )?;
                validate_center(center)
                    .map_err(|center| EdgeRouteSpecValidationError::SmoothStepCenter { center })
            }
        }
    }
}

impl Default for EdgeRouteSpec {
    fn default() -> Self {
        Self::Bezier {
            curvature: Self::DEFAULT_BEZIER_CURVATURE,
        }
    }
}

/// Why an edge route specification cannot be installed in a graph.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum EdgeRouteSpecValidationError {
    #[error("bezier curvature must be finite and within the supported range: {curvature}")]
    BezierCurvature { curvature: f32 },
    #[error("orthogonal offset must be finite and within the supported range: {offset}")]
    OrthogonalOffset { offset: f32 },
    #[error("orthogonal step position must be finite and within 0..=1: {step_position}")]
    OrthogonalStepPosition { step_position: f32 },
    #[error("orthogonal center must be finite: {center:?}")]
    OrthogonalCenter { center: CanvasPoint },
    #[error("smooth-step offset must be finite and within the supported range: {offset}")]
    SmoothStepOffset { offset: f32 },
    #[error("smooth-step step position must be finite and within 0..=1: {step_position}")]
    SmoothStepStepPosition { step_position: f32 },
    #[error(
        "smooth-step border radius must be finite and within the supported range: {border_radius}"
    )]
    SmoothStepBorderRadius { border_radius: f32 },
    #[error("smooth-step center must be finite: {center:?}")]
    SmoothStepCenter { center: CanvasPoint },
}

impl EdgeRouteSpecValidationError {
    pub fn field(self) -> &'static str {
        match self {
            Self::BezierCurvature { .. } => "curvature",
            Self::OrthogonalOffset { .. } | Self::SmoothStepOffset { .. } => "offset",
            Self::OrthogonalStepPosition { .. } | Self::SmoothStepStepPosition { .. } => {
                "step_position"
            }
            Self::OrthogonalCenter { .. } | Self::SmoothStepCenter { .. } => "center",
            Self::SmoothStepBorderRadius { .. } => "border_radius",
        }
    }

    pub fn is_non_finite(self) -> bool {
        match self {
            Self::BezierCurvature { curvature } => !curvature.is_finite(),
            Self::OrthogonalOffset { offset } | Self::SmoothStepOffset { offset } => {
                !offset.is_finite()
            }
            Self::OrthogonalStepPosition { step_position }
            | Self::SmoothStepStepPosition { step_position } => !step_position.is_finite(),
            Self::OrthogonalCenter { center } | Self::SmoothStepCenter { center } => {
                !center.is_finite()
            }
            Self::SmoothStepBorderRadius { border_radius } => !border_radius.is_finite(),
        }
    }
}

fn validate_bounded_non_negative(value: f32, maximum: f32) -> Result<(), ()> {
    (value.is_finite() && (0.0..=maximum).contains(&value))
        .then_some(())
        .ok_or(())
}

fn validate_step_position(value: f32) -> Result<(), ()> {
    (value.is_finite() && (0.0..=1.0).contains(&value))
        .then_some(())
        .ok_or(())
}

fn validate_center(center: Option<CanvasPoint>) -> Result<(), CanvasPoint> {
    match center {
        Some(center) if !center.is_finite() => Err(center),
        Some(_) | None => Ok(()),
    }
}

/// Edge between two ports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Edge kind.
    pub kind: EdgeKind,
    /// Source port.
    pub from: PortId,
    /// Target port.
    pub to: PortId,
    /// Whether the edge is hidden (XyFlow `edge.hidden`).
    ///
    /// Hidden edges are excluded from derived selection and rendering surfaces.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,

    /// Optional adapter-facing selection override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selectable: Option<bool>,

    /// Optional adapter-facing edge hit-test width in logical pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_width: Option<f32>,

    /// Whether graph commands may delete this edge. Runtime policy supplies the fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletable: Option<bool>,

    /// Whether the edge can be reconnected via editor interactions (XyFlow `edge.reconnectable`).
    ///
    /// In XyFlow this field is a `boolean | 'source' | 'target'`. `true` enables reconnecting both
    /// endpoints, `'source'` only enables reconnecting the source endpoint and `'target'` only
    /// enables reconnecting the target endpoint.
    ///
    /// When omitted, runtime policy supplies the fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconnectable: Option<EdgeReconnectable>,

    /// Opaque edge payload (domain-owned).
    ///
    /// This is for product facts that belong to the relationship itself: branch conditions,
    /// cardinality, error-path facts, and adapter-readable labels that are part of graph meaning.
    #[serde(default)]
    pub data: Value,

    /// Renderer-neutral presentation metadata for this edge.
    ///
    /// Adapter-ephemeral state such as hover, selection, focused editor widgets, or open toolbars
    /// must remain outside the graph.
    #[serde(default, skip_serializing_if = "edge_view_descriptor_is_default")]
    pub view: EdgeViewDescriptor,
}

impl Edge {
    /// Creates an edge with renderer-neutral defaults.
    pub fn new(kind: EdgeKind, from: PortId, to: PortId) -> Self {
        Self {
            kind,
            from,
            to,
            hidden: false,
            selectable: None,
            interaction_width: None,
            deletable: None,
            reconnectable: None,
            data: Value::Null,
            view: EdgeViewDescriptor::default(),
        }
    }
}

/// Renderer-neutral presentation metadata for an edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EdgeViewDescriptor {
    /// Adapter-facing renderer key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer_key: Option<String>,
    /// Adapter-facing label text when the label is a view concern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Label placement hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_anchor: Option<EdgeLabelAnchor>,
    /// Source marker key, interpreted by adapters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_marker_key: Option<String>,
    /// Target marker key, interpreted by adapters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_marker_key: Option<String>,
    /// Style token, interpreted by adapters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_token: Option<String>,
    /// Complete route specification. When omitted, the runtime resolves [`EdgeRouteSpec::default`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<EdgeRouteSpec>,
}

impl EdgeViewDescriptor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_renderer_key(mut self, renderer_key: impl Into<String>) -> Self {
        self.renderer_key = Some(renderer_key.into());
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_label_anchor(mut self, label_anchor: EdgeLabelAnchor) -> Self {
        self.label_anchor = Some(label_anchor);
        self
    }

    pub fn with_source_marker_key(mut self, source_marker_key: impl Into<String>) -> Self {
        self.source_marker_key = Some(source_marker_key.into());
        self
    }

    pub fn with_target_marker_key(mut self, target_marker_key: impl Into<String>) -> Self {
        self.target_marker_key = Some(target_marker_key.into());
        self
    }

    pub fn with_style_token(mut self, style_token: impl Into<String>) -> Self {
        self.style_token = Some(style_token.into());
        self
    }

    pub fn with_route(mut self, route: EdgeRouteSpec) -> Self {
        self.route = Some(route);
        self
    }

    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Label anchor hint for adapter-owned edge labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeLabelAnchor {
    Source,
    Center,
    Target,
}

/// Per-edge reconnect enablement (XyFlow `edge.reconnectable`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EdgeReconnectable {
    Bool(bool),
    Endpoint(EdgeReconnectableEndpoint),
}

impl EdgeReconnectable {
    pub fn allows_source(self) -> bool {
        matches!(
            self,
            Self::Bool(true) | Self::Endpoint(EdgeReconnectableEndpoint::Source)
        )
    }

    pub fn allows_target(self) -> bool {
        matches!(
            self,
            Self::Bool(true) | Self::Endpoint(EdgeReconnectableEndpoint::Target)
        )
    }

    pub fn endpoint_flags(self) -> (bool, bool) {
        (self.allows_source(), self.allows_target())
    }
}

/// Which endpoint is reconnectable (`'source' | 'target'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeReconnectableEndpoint {
    Source,
    Target,
}
