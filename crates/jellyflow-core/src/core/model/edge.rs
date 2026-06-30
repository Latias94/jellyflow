use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::ids::PortId;

use super::port::PortKind;

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

/// Route-style hint for adapter-owned edge drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeRouteKind {
    /// Direct line between resolved endpoints.
    Straight,
    /// Orthogonal/polyline route with right-angle legs.
    Orthogonal,
    /// Cubic bezier route using endpoint sides as curvature hints.
    Bezier,
    /// XyFlow-style smooth-step route.
    SmoothStep,
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

    /// Whether the edge can be selected (XyFlow `edge.selectable`).
    ///
    /// When omitted, the global `NodeGraphInteractionState.edges_selectable` decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selectable: Option<bool>,

    /// Whether the edge can receive keyboard focus (XyFlow `edge.focusable`).
    ///
    /// When omitted, the global `NodeGraphInteractionState.edges_focusable` decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focusable: Option<bool>,

    /// Optional edge hit-test interaction width in logical pixels (XyFlow `edge.interactionWidth`).
    ///
    /// When omitted, the global `NodeGraphInteractionState.edge_interaction_width` decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_width: Option<f32>,

    /// Whether the edge can be deleted via editor interactions (XyFlow `edge.deletable`).
    ///
    /// When omitted, the global `NodeGraphInteractionState.edges_deletable` decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletable: Option<bool>,

    /// Whether the edge can be reconnected via editor interactions (XyFlow `edge.reconnectable`).
    ///
    /// In XyFlow this field is a `boolean | 'source' | 'target'`. `true` enables reconnecting both
    /// endpoints, `'source'` only enables reconnecting the source endpoint and `'target'` only
    /// enables reconnecting the target endpoint.
    ///
    /// When omitted, the global `NodeGraphInteractionState.edges_reconnectable` decides.
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
            focusable: None,
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
    /// Route-style hint, interpreted by adapters and runtime geometry projections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_kind: Option<EdgeRouteKind>,
    /// Optional hit-test width hint in logical pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hit_target_width: Option<f32>,
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

    pub fn with_route_kind(mut self, route_kind: EdgeRouteKind) -> Self {
        self.route_kind = Some(route_kind);
        self
    }

    pub fn with_hit_target_width(mut self, hit_target_width: f32) -> Self {
        self.hit_target_width = Some(hit_target_width);
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
