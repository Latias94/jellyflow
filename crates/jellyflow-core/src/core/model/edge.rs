use serde::{Deserialize, Serialize};

use crate::core::ids::PortId;

use super::port::PortKind;

fn is_false(v: &bool) -> bool {
    !*v
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
