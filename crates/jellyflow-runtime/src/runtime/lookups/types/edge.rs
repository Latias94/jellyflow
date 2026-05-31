use jellyflow_core::core::{EdgeKind, EdgeReconnectable, NodeId, PortId};
use jellyflow_core::ops::EdgeEndpoints;

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
    pub(crate) fn with_parts(
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
