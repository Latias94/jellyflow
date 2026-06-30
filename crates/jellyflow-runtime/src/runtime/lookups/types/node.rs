use jellyflow_core::core::{
    CanvasPoint, CanvasSize, GroupId, Node, NodeKindKey, NodeOrigin, PortId, PortKey,
};

use crate::runtime::measurement::{
    MeasuredHandle, MeasuredSurfaceAnchor, MeasuredSurfaceSlot, NodeInternalsInvalidationReason,
    NodeMeasurement, NodeMeasurementStatus,
};
use crate::schema::kit::NodeKitContentDensity;

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
    pub measurement_revision: Option<u64>,
    pub measurement_density: Option<NodeKitContentDensity>,
    pub measurement_dirty: Option<NodeInternalsInvalidationReason>,
    pub measured_size: Option<CanvasSize>,
    pub measured_handles: Vec<MeasuredHandle>,
    pub measured_slots: Vec<MeasuredSurfaceSlot>,
    pub measured_anchors: Vec<MeasuredSurfaceAnchor>,
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
            measurement_revision: None,
            measurement_density: None,
            measurement_dirty: None,
            measured_size: None,
            measured_handles: Vec::new(),
            measured_slots: Vec::new(),
            measured_anchors: Vec::new(),
        }
    }

    pub(crate) fn is_visible_with_hidden_policy(&self, include_hidden: bool) -> bool {
        include_hidden || !self.hidden
    }

    pub(crate) fn resolved_size(&self, fallback_size: Option<CanvasSize>) -> Option<CanvasSize> {
        self.size.or(self.measured_size).or(fallback_size)
    }

    pub(crate) fn apply_measurement(&mut self, measurement: &NodeMeasurement) -> bool {
        if self.measurement_dirty.is_some()
            && self
                .measurement_revision
                .is_some_and(|revision| measurement.revision <= revision)
        {
            return false;
        }

        let changed = self.measurement_revision != Some(measurement.revision)
            || self.measurement_density != measurement.density
            || self.measurement_dirty.is_some()
            || self.measured_size != measurement.size
            || self.measured_handles != measurement.handles
            || self.measured_slots != measurement.slots
            || self.measured_anchors != measurement.anchors;
        self.measurement_revision = Some(measurement.revision);
        self.measurement_density = measurement.density;
        self.measurement_dirty = None;
        self.measured_size = measurement.size;
        self.measured_handles = measurement.handles.clone();
        self.measured_slots = measurement.slots.clone();
        self.measured_anchors = measurement.anchors.clone();
        changed
    }

    pub(crate) fn clear_measurement(&mut self) -> bool {
        let changed = self.measurement_revision.is_some()
            || self.measurement_density.is_some()
            || self.measurement_dirty.is_some()
            || self.measured_size.is_some()
            || !self.measured_handles.is_empty()
            || !self.measured_slots.is_empty()
            || !self.measured_anchors.is_empty();
        self.measurement_revision = None;
        self.measurement_density = None;
        self.measurement_dirty = None;
        self.measured_size = None;
        self.measured_handles.clear();
        self.measured_slots.clear();
        self.measured_anchors.clear();
        changed
    }

    pub(crate) fn mark_measurement_dirty(
        &mut self,
        reason: NodeInternalsInvalidationReason,
    ) -> bool {
        if self.measurement_dirty == Some(reason) {
            return false;
        }
        self.measurement_dirty = Some(reason);
        true
    }

    pub(crate) fn mark_measurement_dirty_if_present(
        &mut self,
        reason: NodeInternalsInvalidationReason,
    ) -> bool {
        if !self.has_measurement() {
            return false;
        }
        self.mark_measurement_dirty(reason)
    }

    pub(crate) fn measurement_status(&self) -> NodeMeasurementStatus {
        if let Some(reason) = self.measurement_dirty {
            return NodeMeasurementStatus::Dirty {
                revision: self.measurement_revision.unwrap_or_default(),
                reason,
            };
        }
        self.measurement_revision
            .map(|revision| NodeMeasurementStatus::Fresh { revision })
            .unwrap_or(NodeMeasurementStatus::Missing)
    }

    pub(crate) fn has_measurement(&self) -> bool {
        self.measurement_revision.is_some()
            || self.measurement_density.is_some()
            || self.measured_size.is_some()
            || !self.measured_handles.is_empty()
            || !self.measured_slots.is_empty()
            || !self.measured_anchors.is_empty()
    }

    pub(crate) fn retain_measurements_for_ports(
        &mut self,
        port_ids: &[PortId],
        port_keys: &[PortKey],
    ) {
        self.measured_handles
            .retain(|measured| port_ids.contains(&measured.handle.port));
        self.measured_anchors.retain(|anchor| {
            anchor.port.is_none_or(|port| port_ids.contains(&port))
                && anchor
                    .port_key
                    .as_ref()
                    .is_none_or(|port_key| port_keys.contains(port_key))
        });
    }

    pub(crate) fn measurement(
        &self,
        node: jellyflow_core::core::NodeId,
    ) -> Option<NodeMeasurement> {
        if !self.has_measurement() {
            return None;
        }
        Some(NodeMeasurement {
            node,
            revision: self.measurement_revision.unwrap_or_default(),
            density: self.measurement_density,
            size: self.measured_size,
            handles: self.measured_handles.clone(),
            slots: self.measured_slots.clone(),
            anchors: self.measured_anchors.clone(),
        })
    }
}
