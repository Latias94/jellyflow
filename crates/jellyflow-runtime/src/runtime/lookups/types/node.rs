use jellyflow_core::core::{
    CanvasPoint, CanvasSize, GroupId, Node, NodeKindKey, NodeOrigin, PortId,
};

use crate::runtime::measurement::{MeasuredHandle, NodeMeasurement};

#[derive(Debug, Clone, PartialEq)]
pub struct NodeLookupEntry {
    pub kind: NodeKindKey,
    pub kind_version: u32,
    pub pos: CanvasPoint,
    pub origin: Option<NodeOrigin>,
    pub parent: Option<GroupId>,
    pub size: Option<CanvasSize>,
    pub hidden: bool,
    pub collapsed: bool,
    pub ports: Vec<PortId>,
    pub measured_size: Option<CanvasSize>,
    pub measured_handles: Vec<MeasuredHandle>,
}

impl NodeLookupEntry {
    pub(crate) fn from_node(node: &Node) -> Self {
        Self {
            kind: node.kind.clone(),
            kind_version: node.kind_version,
            pos: node.pos,
            origin: node.origin,
            parent: node.parent,
            size: node.size,
            hidden: node.hidden,
            collapsed: node.collapsed,
            ports: node.ports.clone(),
            measured_size: None,
            measured_handles: Vec::new(),
        }
    }

    pub(crate) fn is_visible_with_hidden_policy(&self, include_hidden: bool) -> bool {
        include_hidden || !self.hidden
    }

    pub(crate) fn resolved_size(&self, fallback_size: Option<CanvasSize>) -> Option<CanvasSize> {
        self.size.or(self.measured_size).or(fallback_size)
    }

    pub(crate) fn apply_measurement(&mut self, measurement: &NodeMeasurement) -> bool {
        let changed =
            self.measured_size != measurement.size || self.measured_handles != measurement.handles;
        self.measured_size = measurement.size;
        self.measured_handles = measurement.handles.clone();
        changed
    }

    pub(crate) fn clear_measurement(&mut self) -> bool {
        let changed = self.measured_size.is_some() || !self.measured_handles.is_empty();
        self.measured_size = None;
        self.measured_handles.clear();
        changed
    }

    pub(crate) fn measurement(
        &self,
        node: jellyflow_core::core::NodeId,
    ) -> Option<NodeMeasurement> {
        if self.measured_size.is_none() && self.measured_handles.is_empty() {
            return None;
        }
        Some(NodeMeasurement {
            node,
            size: self.measured_size,
            handles: self.measured_handles.clone(),
        })
    }
}
