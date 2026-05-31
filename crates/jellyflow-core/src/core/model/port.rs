use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::ids::{NodeId, PortKey};
use crate::types::TypeDesc;

use super::edge::EdgeKind;

/// Port direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortDirection {
    /// Input port.
    In,
    /// Output port.
    Out,
}

/// Port kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortKind {
    /// Data port.
    Data,
    /// Exec port (control flow).
    Exec,
}

impl PortKind {
    pub fn edge_kind(self) -> EdgeKind {
        match self {
            Self::Data => EdgeKind::Data,
            Self::Exec => EdgeKind::Exec,
        }
    }
}

/// Connection capacity for a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortCapacity {
    /// Single connection.
    Single,
    /// Multiple connections.
    Multi,
}

/// Port instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    /// Owning node.
    pub node: NodeId,
    /// Schema key for stable migrations.
    pub key: PortKey,
    /// Port direction.
    pub dir: PortDirection,
    /// Port kind.
    pub kind: PortKind,
    /// Capacity rule.
    pub capacity: PortCapacity,

    /// Whether this port can be used for creating/accepting connections via editor interactions.
    ///
    /// This mirrors XyFlow handle-level `isConnectable`. When omitted, the owning node's
    /// `Node.connectable` / global `NodeGraphInteractionState.nodes_connectable` decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connectable: Option<bool>,

    /// Dictates whether a connection can start from this port.
    ///
    /// This mirrors XyFlow handle-level `isConnectableStart`. When omitted, the port is treated as
    /// start-connectable (subject to `connectable`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connectable_start: Option<bool>,

    /// Dictates whether a connection can end on this port.
    ///
    /// This mirrors XyFlow handle-level `isConnectableEnd`. When omitted, the port is treated as
    /// end-connectable (subject to `connectable`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connectable_end: Option<bool>,

    /// Optional type descriptor.
    ///
    /// Profiles may choose to infer or override this via concretization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<TypeDesc>,

    /// Opaque port payload (domain-owned).
    #[serde(default)]
    pub data: Value,
}
