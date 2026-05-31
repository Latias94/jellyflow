use jellyflow_core::core::{
    CanvasPoint, CanvasSize, EdgeId, EdgeKind, EdgeReconnectable, GroupId, Node, NodeId,
    NodeKindKey, PortDirection, PortId,
};
use jellyflow_core::ops::EdgeEndpoints;

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
    pub(super) fn from_edge_lookup(edge: EdgeId, entry: EdgeLookupEntry) -> Self {
        Self {
            edge,
            source_node: entry.from_node,
            source_port: entry.from,
            target_node: entry.to_node,
            target_port: entry.to,
            kind: entry.kind,
        }
    }

    pub(super) fn lookup_keys(self) -> [ConnectionLookupKey; 6] {
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

#[derive(Debug, Clone, PartialEq)]
pub struct NodeLookupEntry {
    pub kind: NodeKindKey,
    pub kind_version: u32,
    pub pos: CanvasPoint,
    pub parent: Option<GroupId>,
    pub size: Option<CanvasSize>,
    pub hidden: bool,
    pub collapsed: bool,
    pub ports: Vec<PortId>,
}

impl NodeLookupEntry {
    pub(super) fn from_node(node: &Node) -> Self {
        Self {
            kind: node.kind.clone(),
            kind_version: node.kind_version,
            pos: node.pos,
            parent: node.parent,
            size: node.size,
            hidden: node.hidden,
            collapsed: node.collapsed,
            ports: node.ports.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeLookupEntry {
    pub kind: EdgeKind,
    pub from: PortId,
    pub to: PortId,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub reconnectable: Option<EdgeReconnectable>,
}

impl EdgeLookupEntry {
    pub(super) fn with_parts(
        kind: EdgeKind,
        endpoints: EdgeEndpoints,
        from_node: NodeId,
        to_node: NodeId,
        reconnectable: Option<EdgeReconnectable>,
    ) -> Self {
        Self {
            kind,
            from: endpoints.from,
            to: endpoints.to,
            from_node,
            to_node,
            reconnectable,
        }
    }
}
