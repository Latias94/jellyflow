use jellyflow_core::core::{CanvasPoint, CanvasRect};
use serde::{Deserialize, Serialize};

/// Side of a node or handle where an edge endpoint attaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandlePosition {
    Top,
    Right,
    Bottom,
    Left,
}

/// Renderer-neutral handle bounds relative to the owning node's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
