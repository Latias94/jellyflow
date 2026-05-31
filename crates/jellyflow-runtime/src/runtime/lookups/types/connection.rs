use jellyflow_core::core::{EdgeId, EdgeKind, NodeId, PortDirection, PortId};

use super::edge::EdgeLookupEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionSide {
    Source,
    Target,
}

impl ConnectionSide {
    pub fn from_port_dir(dir: PortDirection) -> Self {
        match dir {
            PortDirection::In => Self::Target,
            PortDirection::Out => Self::Source,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionLookupKey {
    Node(NodeId),
    NodeSide {
        node: NodeId,
        side: ConnectionSide,
    },
    NodeSidePort {
        node: NodeId,
        side: ConnectionSide,
        port: PortId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandleConnection {
    pub edge: EdgeId,
    pub source_node: NodeId,
    pub source_port: PortId,
    pub target_node: NodeId,
    pub target_port: PortId,
    pub kind: EdgeKind,
}

impl HandleConnection {
    pub(crate) fn from_edge_lookup(edge: EdgeId, entry: EdgeLookupEntry) -> Self {
        Self {
            edge,
            source_node: entry.from_node,
            source_port: entry.from,
            target_node: entry.to_node,
            target_port: entry.to,
            kind: entry.kind,
        }
    }

    pub(crate) fn lookup_keys(self) -> [ConnectionLookupKey; 6] {
        [
            ConnectionLookupKey::Node(self.source_node),
            ConnectionLookupKey::NodeSide {
                node: self.source_node,
                side: ConnectionSide::Source,
            },
            ConnectionLookupKey::NodeSidePort {
                node: self.source_node,
                side: ConnectionSide::Source,
                port: self.source_port,
            },
            ConnectionLookupKey::Node(self.target_node),
            ConnectionLookupKey::NodeSide {
                node: self.target_node,
                side: ConnectionSide::Target,
            },
            ConnectionLookupKey::NodeSidePort {
                node: self.target_node,
                side: ConnectionSide::Target,
                port: self.target_port,
            },
        ]
    }
}
