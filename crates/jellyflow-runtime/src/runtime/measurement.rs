//! Renderer-neutral measurement facts reported by adapters.
//!
//! The graph document remains the persisted source of truth. Measurements live in runtime lookups
//! so adapters can report layout facts once and reuse shared rendering, endpoint, and connection
//! target behavior without copying geometry rules.

use serde::{Deserialize, Serialize};

use crate::runtime::connection::{
    ConnectionHandleRef, ConnectionTargetCandidate, ResolvedConnectionTarget,
};
use crate::runtime::geometry::{EdgePosition, EdgeRouteFacts, HandleBounds, HandlePosition};
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::rendering::RenderingQueryResult;
use crate::runtime::store::NodeGraphStore;
use crate::schema::NodeSurfaceSlotVisibility;
use crate::schema::kit::NodeKitContentDensity;
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, NodeId, PortDirection, PortId, PortKey,
};

fn default_slot_visibility() -> NodeSurfaceSlotVisibility {
    NodeSurfaceSlotVisibility::Visible
}

/// One measured handle attached to a node.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MeasuredHandle {
    pub handle: ConnectionHandleRef,
    pub bounds: HandleBounds,
}

impl MeasuredHandle {
    pub fn new(handle: ConnectionHandleRef, bounds: HandleBounds) -> Self {
        Self { handle, bounds }
    }
}

/// Measured rectangle for one semantic slot inside a node surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasuredSurfaceSlot {
    pub key: String,
    pub rect: CanvasRect,
    #[serde(default = "default_slot_visibility")]
    pub visibility: NodeSurfaceSlotVisibility,
}

impl MeasuredSurfaceSlot {
    pub fn new(key: impl Into<String>, rect: CanvasRect) -> Self {
        Self {
            key: key.into(),
            rect,
            visibility: NodeSurfaceSlotVisibility::Visible,
        }
    }

    pub fn with_visibility(mut self, visibility: NodeSurfaceSlotVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn is_visible(&self) -> bool {
        matches!(self.visibility, NodeSurfaceSlotVisibility::Visible)
    }
}

/// Measured rectangle for one placement anchor inside a node surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasuredSurfaceAnchor {
    pub anchor: String,
    pub rect: CanvasRect,
    pub position: HandlePosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<PortId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_key: Option<PortKey>,
    #[serde(default = "default_slot_visibility")]
    pub visibility: NodeSurfaceSlotVisibility,
}

impl MeasuredSurfaceAnchor {
    pub fn new(anchor: impl Into<String>, rect: CanvasRect, position: HandlePosition) -> Self {
        Self {
            anchor: anchor.into(),
            rect,
            position,
            port: None,
            port_key: None,
            visibility: NodeSurfaceSlotVisibility::Visible,
        }
    }

    pub fn with_port(mut self, port: PortId) -> Self {
        self.port = Some(port);
        self
    }

    pub fn with_port_key(mut self, port_key: impl Into<PortKey>) -> Self {
        self.port_key = Some(port_key.into());
        self
    }

    pub fn with_visibility(mut self, visibility: NodeSurfaceSlotVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn is_visible(&self) -> bool {
        matches!(self.visibility, NodeSurfaceSlotVisibility::Visible)
    }

    pub fn bounds(&self) -> HandleBounds {
        HandleBounds {
            rect: self.rect,
            position: self.position,
        }
    }
}

/// Renderer-neutral measurement facts for one node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeMeasurement {
    pub node: NodeId,
    #[serde(default)]
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<NodeKitContentDensity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<CanvasSize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handles: Vec<MeasuredHandle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<MeasuredSurfaceSlot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<MeasuredSurfaceAnchor>,
}

impl NodeMeasurement {
    pub fn new(node: NodeId) -> Self {
        Self {
            node,
            revision: 0,
            density: None,
            size: None,
            handles: Vec::new(),
            slots: Vec::new(),
            anchors: Vec::new(),
        }
    }

    pub fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }

    pub fn with_density(mut self, density: Option<NodeKitContentDensity>) -> Self {
        self.density = density;
        self
    }

    pub fn with_size(mut self, size: Option<CanvasSize>) -> Self {
        self.size = size;
        self
    }

    pub fn with_handles(mut self, handles: impl IntoIterator<Item = MeasuredHandle>) -> Self {
        self.handles = handles.into_iter().collect();
        self
    }

    pub fn with_slots(mut self, slots: impl IntoIterator<Item = MeasuredSurfaceSlot>) -> Self {
        self.slots = slots.into_iter().collect();
        self
    }

    pub fn with_anchors(
        mut self,
        anchors: impl IntoIterator<Item = MeasuredSurfaceAnchor>,
    ) -> Self {
        self.anchors = anchors.into_iter().collect();
        self
    }
}

/// Reason an adapter asks runtime to treat node-internal measurements as stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeInternalsInvalidationReason {
    DataChanged,
    ComponentStateChanged,
    ZoomChanged,
    SizeChanged,
    DensityChanged,
    AdapterRequest,
}

/// Adapter-facing request to mark one or more nodes for remeasurement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeInternalsInvalidation {
    pub nodes: Vec<NodeId>,
    pub reason: NodeInternalsInvalidationReason,
}

impl NodeInternalsInvalidation {
    pub fn new(
        nodes: impl IntoIterator<Item = NodeId>,
        reason: NodeInternalsInvalidationReason,
    ) -> Self {
        Self {
            nodes: nodes.into_iter().collect(),
            reason,
        }
    }

    pub fn one(node: NodeId, reason: NodeInternalsInvalidationReason) -> Self {
        Self::new([node], reason)
    }
}

/// Freshness state for the latest node-internal measurement facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum NodeMeasurementStatus {
    Missing,
    Fresh {
        revision: u64,
    },
    Dirty {
        revision: u64,
        reason: NodeInternalsInvalidationReason,
    },
}

impl NodeMeasurementStatus {
    pub fn is_fresh(self) -> bool {
        matches!(self, Self::Fresh { .. })
    }

    pub fn is_dirty(self) -> bool {
        matches!(self, Self::Dirty { .. })
    }
}

/// Why handle geometry fell back instead of using measured node internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeHandleFallbackReason {
    MissingMeasurement,
    DirtyMeasurement,
    MissingHandle,
}

/// Source used for resolving handle geometry inside a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "source")]
pub enum NodeHandleMeasurementSource {
    MeasuredHandle,
    MeasuredAnchor { anchor: String },
    Fallback { reason: NodeHandleFallbackReason },
}

/// Adapter/query-facing explanation of local handle geometry resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeHandleMeasurementResolution {
    pub handle: ConnectionHandleRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<HandleBounds>,
    pub source: NodeHandleMeasurementSource,
    pub status: NodeMeasurementStatus,
}

/// Result of applying measurement facts to runtime lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeMeasurementOutcome {
    Changed,
    Unchanged,
}

impl NodeMeasurementOutcome {
    pub fn changed(self) -> bool {
        matches!(self, Self::Changed)
    }
}

/// Adapter-facing facade for node-internal geometry lifecycle updates.
///
/// This is the headless equivalent of toolkits such as React Flow asking users to update node
/// internals after handles or child widgets move. It keeps the runtime contract explicit while
/// avoiding duplicated adapter code for "mark dirty, report real geometry, query status".
pub struct NodeInternalsController<'a> {
    store: &'a mut NodeGraphStore,
}

impl<'a> NodeInternalsController<'a> {
    pub(crate) fn new(store: &'a mut NodeGraphStore) -> Self {
        Self { store }
    }

    /// Marks one or more node internals dirty until the adapter reports fresh geometry.
    pub fn invalidate(
        &mut self,
        invalidation: NodeInternalsInvalidation,
    ) -> NodeMeasurementOutcome {
        self.store.invalidate_node_internals(invalidation)
    }

    /// Marks a single node dirty until the adapter reports fresh geometry.
    pub fn invalidate_one(
        &mut self,
        node: NodeId,
        reason: NodeInternalsInvalidationReason,
    ) -> NodeMeasurementOutcome {
        self.invalidate(NodeInternalsInvalidation::one(node, reason))
    }

    /// Reports freshly measured node internals from adapter widget/layout geometry.
    pub fn report(
        &mut self,
        measurement: NodeMeasurement,
    ) -> Result<NodeMeasurementOutcome, NodeMeasurementError> {
        self.store.report_node_measurement(measurement)
    }

    /// Returns whether the latest geometry facts are fresh, dirty, or missing.
    pub fn status(&self, node: NodeId) -> NodeMeasurementStatus {
        self.store.node_measurement_status(node)
    }

    /// Resolves local handle geometry from the latest fresh measurement facts.
    pub fn resolve_handle(&self, handle: ConnectionHandleRef) -> NodeHandleMeasurementResolution {
        self.store.resolve_node_handle_measurement(handle)
    }

    /// Reads the adapter-facing layout facts after dirty/fresh state has been applied.
    pub fn layout_facts(&self, viewport_size: CanvasSize) -> LayoutFactsQueryResult {
        self.store.layout_facts_query(viewport_size)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NodeMeasurementError {
    #[error("measurement target node does not exist: {0:?}")]
    MissingNode(NodeId),
    #[error("measurement size is not positive and finite for node {node:?}: {size:?}")]
    InvalidSize { node: NodeId, size: CanvasSize },
    #[error("measurement handle does not belong to node {node:?}: {handle:?}")]
    InvalidHandle {
        node: NodeId,
        handle: ConnectionHandleRef,
    },
    #[error("measurement handle bounds are not positive and finite for node {node:?}: {handle:?}")]
    InvalidHandleBounds {
        node: NodeId,
        handle: ConnectionHandleRef,
    },
    #[error("measurement slot rect is not positive and finite for node {node:?}: {slot}")]
    InvalidSlotRect { node: NodeId, slot: String },
    #[error("measurement anchor rect is not positive and finite for node {node:?}: {anchor}")]
    InvalidAnchorRect { node: NodeId, anchor: String },
    #[error("measurement anchor target does not belong to node {node:?}: {anchor}")]
    InvalidAnchorTarget { node: NodeId, anchor: String },
}

/// Resolved endpoint geometry for one visible edge in a layout-facts query.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutEdgePosition {
    pub edge: EdgeId,
    pub position: EdgePosition,
}

impl LayoutEdgePosition {
    pub fn new(edge: EdgeId, position: EdgePosition) -> Self {
        Self { edge, position }
    }
}

/// Resolved route, path, and interaction facts for one visible edge.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutEdgeRouteFacts {
    pub edge: EdgeId,
    pub facts: EdgeRouteFacts,
}

impl LayoutEdgeRouteFacts {
    pub fn new(edge: EdgeId, facts: EdgeRouteFacts) -> Self {
        Self { edge, facts }
    }
}

/// Measurement status for one visible node in a layout-facts query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LayoutNodeMeasurementStatus {
    pub node: NodeId,
    pub status: NodeMeasurementStatus,
}

impl LayoutNodeMeasurementStatus {
    pub fn new(node: NodeId, status: NodeMeasurementStatus) -> Self {
        Self { node, status }
    }
}

/// Store-level layout facts derived from the graph, view state, and reported measurements.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutFactsQueryResult {
    pub revision: u64,
    pub rendering: RenderingQueryResult,
    pub visible_edge_positions: Vec<LayoutEdgePosition>,
    pub visible_edge_route_facts: Vec<LayoutEdgeRouteFacts>,
    pub connection_target_candidates: Vec<ConnectionTargetCandidate>,
    pub node_measurement_statuses: Vec<LayoutNodeMeasurementStatus>,
}

impl LayoutFactsQueryResult {
    pub fn new(
        revision: u64,
        rendering: RenderingQueryResult,
        visible_edge_positions: Vec<LayoutEdgePosition>,
        connection_target_candidates: Vec<ConnectionTargetCandidate>,
    ) -> Self {
        Self {
            revision,
            rendering,
            visible_edge_positions,
            visible_edge_route_facts: Vec::new(),
            connection_target_candidates,
            node_measurement_statuses: Vec::new(),
        }
    }

    pub fn with_edge_route_facts(
        mut self,
        facts: impl IntoIterator<Item = LayoutEdgeRouteFacts>,
    ) -> Self {
        self.visible_edge_route_facts = facts.into_iter().collect();
        self
    }

    pub fn with_node_measurement_statuses(
        mut self,
        statuses: impl IntoIterator<Item = LayoutNodeMeasurementStatus>,
    ) -> Self {
        self.node_measurement_statuses = statuses.into_iter().collect();
        self
    }

    pub fn visible_edge_position(&self, edge: EdgeId) -> Option<EdgePosition> {
        self.visible_edge_positions
            .iter()
            .find(|position| position.edge == edge)
            .map(|position| position.position)
    }

    pub fn visible_edge_route_facts(&self, edge: EdgeId) -> Option<&EdgeRouteFacts> {
        self.visible_edge_route_facts
            .iter()
            .find(|facts| facts.edge == edge)
            .map(|facts| &facts.facts)
    }

    pub fn node_measurement_status(&self, node: NodeId) -> NodeMeasurementStatus {
        self.node_measurement_statuses
            .iter()
            .find(|status| status.node == node)
            .map(|status| status.status)
            .unwrap_or(NodeMeasurementStatus::Missing)
    }
}

impl NodeGraphStore {
    /// Returns a short-lived controller for adapter node-internal geometry lifecycle updates.
    pub fn node_internals(&mut self) -> NodeInternalsController<'_> {
        NodeInternalsController::new(self)
    }

    /// Applies non-persisted renderer measurements for one node.
    pub fn report_node_measurement(
        &mut self,
        measurement: NodeMeasurement,
    ) -> Result<NodeMeasurementOutcome, NodeMeasurementError> {
        let measurement = self.validate_node_measurement(measurement)?;
        let Some(entry) = self.lookups_mut().node_lookup.get_mut(&measurement.node) else {
            return Err(NodeMeasurementError::MissingNode(measurement.node));
        };

        if entry.apply_measurement(&measurement) {
            self.publish_layout_facts_changed();
            Ok(NodeMeasurementOutcome::Changed)
        } else {
            Ok(NodeMeasurementOutcome::Unchanged)
        }
    }

    /// Clears non-persisted measurements for one node.
    pub fn clear_node_measurement(&mut self, node: NodeId) -> NodeMeasurementOutcome {
        let Some(entry) = self.lookups_mut().node_lookup.get_mut(&node) else {
            return NodeMeasurementOutcome::Unchanged;
        };

        if entry.clear_measurement() {
            self.publish_layout_facts_changed();
            NodeMeasurementOutcome::Changed
        } else {
            NodeMeasurementOutcome::Unchanged
        }
    }

    /// Marks one or more nodes as requiring adapter remeasurement.
    ///
    /// Existing size facts remain available so unsized runtime-only nodes do not disappear between
    /// render passes, but stale handles and anchors are not used for endpoints or hit targets.
    pub fn invalidate_node_internals(
        &mut self,
        invalidation: NodeInternalsInvalidation,
    ) -> NodeMeasurementOutcome {
        let mut changed = false;
        for node in invalidation.nodes {
            let Some(entry) = self.lookups_mut().node_lookup.get_mut(&node) else {
                continue;
            };
            changed |= entry.mark_measurement_dirty(invalidation.reason);
        }

        if changed {
            self.publish_layout_facts_changed();
            NodeMeasurementOutcome::Changed
        } else {
            NodeMeasurementOutcome::Unchanged
        }
    }

    /// Reads the current non-persisted measurement facts for one node.
    pub fn node_measurement(&self, node: NodeId) -> Option<NodeMeasurement> {
        self.lookups()
            .node_lookup
            .get(&node)
            .and_then(|entry| entry.measurement(node))
    }

    /// Reads whether the latest measurement facts for a node are fresh, dirty, or missing.
    pub fn node_measurement_status(&self, node: NodeId) -> NodeMeasurementStatus {
        self.lookups()
            .node_lookup
            .get(&node)
            .map(|entry| entry.measurement_status())
            .unwrap_or(NodeMeasurementStatus::Missing)
    }

    /// Resolves local handle geometry from fresh measured handles or semantic anchors.
    pub fn resolve_node_handle_measurement(
        &self,
        handle: ConnectionHandleRef,
    ) -> NodeHandleMeasurementResolution {
        resolve_handle_measurement(self.graph(), self.lookups(), handle)
    }

    /// Reads the adapter-facing layout facts for the current store state.
    pub fn layout_facts_query(&self, viewport_size: CanvasSize) -> LayoutFactsQueryResult {
        crate::runtime::query::layout_facts_query(self, viewport_size)
    }

    /// Builds renderer-neutral connection target candidates from reported handle measurements.
    pub fn connection_target_candidates_from_layout_facts(&self) -> Vec<ConnectionTargetCandidate> {
        crate::runtime::query::connection_target_candidates_from_layout_facts(self)
    }

    /// Resolves a connection target using the handle inventory previously reported by adapters.
    pub fn resolve_connection_target_from_layout_facts(
        &self,
        pointer: CanvasPoint,
        from: ConnectionHandleRef,
    ) -> ResolvedConnectionTarget {
        crate::runtime::query::resolve_connection_target_from_layout_facts(self, pointer, from)
    }

    /// Resolves edge endpoint geometry from graph endpoints plus reported measurement facts.
    pub fn edge_position_from_layout_facts(&self, edge: EdgeId) -> Option<EdgePosition> {
        crate::runtime::query::edge_position_from_layout_facts(self, edge)
    }

    fn validate_node_measurement(
        &self,
        measurement: NodeMeasurement,
    ) -> Result<NodeMeasurement, NodeMeasurementError> {
        if !self.graph().nodes().contains_key(&measurement.node) {
            return Err(NodeMeasurementError::MissingNode(measurement.node));
        }
        if let Some(size) = measurement.size
            && !size.is_positive_finite()
        {
            return Err(NodeMeasurementError::InvalidSize {
                node: measurement.node,
                size,
            });
        }

        for measured in &measurement.handles {
            if measured.handle.node != measurement.node {
                return Err(NodeMeasurementError::InvalidHandle {
                    node: measurement.node,
                    handle: measured.handle,
                });
            }
            if !measured.bounds.rect.is_positive_finite() {
                return Err(NodeMeasurementError::InvalidHandleBounds {
                    node: measurement.node,
                    handle: measured.handle,
                });
            }
            let Some(port) = self.graph().ports().get(&measured.handle.port) else {
                return Err(NodeMeasurementError::InvalidHandle {
                    node: measurement.node,
                    handle: measured.handle,
                });
            };
            if port.node != measurement.node || port.dir != measured.handle.direction {
                return Err(NodeMeasurementError::InvalidHandle {
                    node: measurement.node,
                    handle: measured.handle,
                });
            }
        }

        for slot in &measurement.slots {
            if !slot.rect.is_positive_finite() {
                return Err(NodeMeasurementError::InvalidSlotRect {
                    node: measurement.node,
                    slot: slot.key.clone(),
                });
            }
        }

        for anchor in &measurement.anchors {
            if !anchor.rect.is_positive_finite() {
                return Err(NodeMeasurementError::InvalidAnchorRect {
                    node: measurement.node,
                    anchor: anchor.anchor.clone(),
                });
            }
            let Some(port) = anchor
                .port
                .and_then(|port| self.graph().ports().get(&port).map(|model| (port, model)))
            else {
                if anchor.port.is_some() {
                    return Err(NodeMeasurementError::InvalidAnchorTarget {
                        node: measurement.node,
                        anchor: anchor.anchor.clone(),
                    });
                }
                continue;
            };
            if port.1.node != measurement.node {
                return Err(NodeMeasurementError::InvalidAnchorTarget {
                    node: measurement.node,
                    anchor: anchor.anchor.clone(),
                });
            }
            if let Some(port_key) = &anchor.port_key
                && port_key != &port.1.key
            {
                return Err(NodeMeasurementError::InvalidAnchorTarget {
                    node: measurement.node,
                    anchor: anchor.anchor.clone(),
                });
            }
            if !anchor_position_matches_direction(anchor.position, port.1.dir) {
                return Err(NodeMeasurementError::InvalidAnchorTarget {
                    node: measurement.node,
                    anchor: anchor.anchor.clone(),
                });
            }
        }

        Ok(measurement)
    }
}

pub(crate) fn resolve_handle_measurement(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    handle: ConnectionHandleRef,
) -> NodeHandleMeasurementResolution {
    let Some(entry) = lookups.node_lookup.get(&handle.node) else {
        return NodeHandleMeasurementResolution {
            handle,
            bounds: None,
            source: NodeHandleMeasurementSource::Fallback {
                reason: NodeHandleFallbackReason::MissingMeasurement,
            },
            status: NodeMeasurementStatus::Missing,
        };
    };
    let status = entry.measurement_status();
    if !status.is_fresh() {
        let reason = match status {
            NodeMeasurementStatus::Missing => NodeHandleFallbackReason::MissingMeasurement,
            NodeMeasurementStatus::Dirty { .. } => NodeHandleFallbackReason::DirtyMeasurement,
            NodeMeasurementStatus::Fresh { .. } => NodeHandleFallbackReason::MissingHandle,
        };
        return NodeHandleMeasurementResolution {
            handle,
            bounds: None,
            source: NodeHandleMeasurementSource::Fallback { reason },
            status,
        };
    }
    if let Some(measured) = entry
        .measured_handles
        .iter()
        .find(|measured| measured.handle == handle)
    {
        return NodeHandleMeasurementResolution {
            handle,
            bounds: Some(measured.bounds),
            source: NodeHandleMeasurementSource::MeasuredHandle,
            status,
        };
    }

    let anchor = graph.ports().get(&handle.port).and_then(|port| {
        if port.node != handle.node || port.dir != handle.direction {
            return None;
        }
        entry.measured_anchors.iter().find(|anchor| {
            anchor.is_visible()
                && (anchor.port == Some(handle.port) || anchor.port_key.as_ref() == Some(&port.key))
                && anchor_position_matches_direction(anchor.position, port.dir)
        })
    });
    if let Some(anchor) = anchor {
        return NodeHandleMeasurementResolution {
            handle,
            bounds: Some(anchor.bounds()),
            source: NodeHandleMeasurementSource::MeasuredAnchor {
                anchor: anchor.anchor.clone(),
            },
            status,
        };
    }

    NodeHandleMeasurementResolution {
        handle,
        bounds: None,
        source: NodeHandleMeasurementSource::Fallback {
            reason: NodeHandleFallbackReason::MissingHandle,
        },
        status,
    }
}

pub(crate) fn anchor_position_matches_direction(
    position: HandlePosition,
    direction: PortDirection,
) -> bool {
    match direction {
        PortDirection::In => matches!(position, HandlePosition::Left | HandlePosition::Top),
        PortDirection::Out => matches!(position, HandlePosition::Right | HandlePosition::Bottom),
    }
}
