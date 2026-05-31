use serde::{Deserialize, Serialize};

use crate::core::ids::PortId;

/// Edge kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Typed data flow.
    Data,
    /// Exec/control flow.
    Exec,
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
    /// Whether the edge can be selected (XyFlow `edge.selectable`).
    ///
    /// When omitted, the global `NodeGraphInteractionState.edges_selectable` decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selectable: Option<bool>,

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
}

/// Per-edge reconnect enablement (XyFlow `edge.reconnectable`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EdgeReconnectable {
    Bool(bool),
    Endpoint(EdgeReconnectableEndpoint),
}

/// Which endpoint is reconnectable (`'source' | 'target'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeReconnectableEndpoint {
    Source,
    Target,
}
