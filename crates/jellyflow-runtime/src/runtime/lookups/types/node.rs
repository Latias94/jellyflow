use jellyflow_core::core::{CanvasPoint, CanvasSize, GroupId, Node, NodeKindKey, PortId};

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
    pub(crate) fn from_node(node: &Node) -> Self {
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
