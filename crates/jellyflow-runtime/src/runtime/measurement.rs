//! Renderer-neutral measurement facts reported by adapters.
//!
//! The graph document remains the persisted source of truth. Measurements live in runtime lookups
//! so adapters can report layout facts once and reuse shared rendering, endpoint, and connection
//! target behavior without copying geometry rules.

use serde::{Deserialize, Serialize};

use crate::runtime::connection::{
    ConnectionHandleRef, ConnectionTargetCandidate, ResolvedConnectionTarget,
};
use crate::runtime::geometry::{EdgePosition, HandleBounds};
use crate::runtime::rendering::RenderingQueryResult;
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeId, NodeId};

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

/// Renderer-neutral measurement facts for one node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeMeasurement {
    pub node: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<CanvasSize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handles: Vec<MeasuredHandle>,
}

impl NodeMeasurement {
    pub fn new(node: NodeId) -> Self {
        Self {
            node,
            size: None,
            handles: Vec::new(),
        }
    }

    pub fn with_size(mut self, size: Option<CanvasSize>) -> Self {
        self.size = size;
        self
    }

    pub fn with_handles(mut self, handles: impl IntoIterator<Item = MeasuredHandle>) -> Self {
        self.handles = handles.into_iter().collect();
        self
    }
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

/// Store-level layout facts derived from the graph, view state, and reported measurements.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutFactsQueryResult {
    pub revision: u64,
    pub rendering: RenderingQueryResult,
    pub visible_edge_positions: Vec<LayoutEdgePosition>,
    pub connection_target_candidates: Vec<ConnectionTargetCandidate>,
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
            connection_target_candidates,
        }
    }

    pub fn visible_edge_position(&self, edge: EdgeId) -> Option<EdgePosition> {
        self.visible_edge_positions
            .iter()
            .find(|position| position.edge == edge)
            .map(|position| position.position)
    }
}

impl NodeGraphStore {
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

    /// Reads the current non-persisted measurement facts for one node.
    pub fn node_measurement(&self, node: NodeId) -> Option<NodeMeasurement> {
        self.lookups()
            .node_lookup
            .get(&node)
            .and_then(|entry| entry.measurement(node))
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
        if !self.graph().nodes.contains_key(&measurement.node) {
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
            let Some(port) = self.graph().ports.get(&measured.handle.port) else {
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

        Ok(measurement)
    }
}
