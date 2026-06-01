use serde::{Deserialize, Serialize};

use jellyflow_core::core::{Edge, EdgeId, EdgeKind, PortId};
use jellyflow_core::ops::EdgeEndpoints;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeConnection {
    pub edge: EdgeId,
    pub from: PortId,
    pub to: PortId,
    pub kind: EdgeKind,
}

impl EdgeConnection {
    pub fn new(edge: EdgeId, from: PortId, to: PortId, kind: EdgeKind) -> Self {
        Self {
            edge,
            from,
            to,
            kind,
        }
    }

    pub(in crate::runtime::xyflow) fn from_edge(edge_id: EdgeId, edge: &Edge) -> Self {
        Self::new(edge_id, edge.from, edge.to, edge.kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConnectionChange {
    Connected(EdgeConnection),
    Disconnected(EdgeConnection),
    Reconnected {
        edge: EdgeId,
        from: EdgeEndpoints,
        to: EdgeEndpoints,
    },
}
