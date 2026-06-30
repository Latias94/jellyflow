use serde::{Deserialize, Serialize};

use crate::runtime::connection::{ConnectionHandleRef, ResolvedConnectionTarget};
use crate::runtime::geometry::{EdgePosition, HandlePosition, ResolvedEdgeRouteKind};
use crate::runtime::measurement::NodeMeasurement;
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeId, NodeId};

use super::ConformanceAction;

pub(super) fn kind(action: &ConformanceAction) -> Option<&'static str> {
    Some(match action {
        ConformanceAction::ReportNodeMeasurement { .. } => "report_node_measurement",
        ConformanceAction::AssertLayoutFacts { .. } => "assert_layout_facts",
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceLayoutFactsExpectation {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visible_node_ids: Vec<NodeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visible_edge_ids: Vec<EdgeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_positions: Vec<ConformanceLayoutEdgePosition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_routes: Vec<ConformanceLayoutEdgeRouteFacts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_target: Option<ConformanceLayoutFactsConnectionTargetExpectation>,
}

impl ConformanceLayoutFactsExpectation {
    pub fn new(
        visible_node_ids: impl IntoIterator<Item = NodeId>,
        visible_edge_ids: impl IntoIterator<Item = EdgeId>,
    ) -> Self {
        Self {
            visible_node_ids: visible_node_ids.into_iter().collect(),
            visible_edge_ids: visible_edge_ids.into_iter().collect(),
            edge_positions: Vec::new(),
            edge_routes: Vec::new(),
            connection_target: None,
        }
    }

    pub fn with_edge_positions(
        mut self,
        edge_positions: impl IntoIterator<Item = ConformanceLayoutEdgePosition>,
    ) -> Self {
        self.edge_positions = edge_positions.into_iter().collect();
        self
    }

    pub fn with_edge_routes(
        mut self,
        edge_routes: impl IntoIterator<Item = ConformanceLayoutEdgeRouteFacts>,
    ) -> Self {
        self.edge_routes = edge_routes.into_iter().collect();
        self
    }

    pub fn with_connection_target(
        mut self,
        connection_target: ConformanceLayoutFactsConnectionTargetExpectation,
    ) -> Self {
        self.connection_target = Some(connection_target);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConformanceLayoutEdgePosition {
    pub edge: EdgeId,
    pub source: ConformanceEdgeEndpointPosition,
    pub target: ConformanceEdgeEndpointPosition,
}

impl ConformanceLayoutEdgePosition {
    pub fn new(
        edge: EdgeId,
        source: ConformanceEdgeEndpointPosition,
        target: ConformanceEdgeEndpointPosition,
    ) -> Self {
        Self {
            edge,
            source,
            target,
        }
    }

    pub fn from_edge_position(edge: EdgeId, position: EdgePosition) -> Self {
        Self {
            edge,
            source: ConformanceEdgeEndpointPosition::new(
                position.source.point,
                position.source.position,
            ),
            target: ConformanceEdgeEndpointPosition::new(
                position.target.point,
                position.target.position,
            ),
        }
    }

    pub(crate) fn matches_edge_position(self, position: EdgePosition) -> bool {
        self.source.matches_endpoint(position.source)
            && self.target.matches_endpoint(position.target)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConformanceEdgeEndpointPosition {
    pub point: CanvasPoint,
    pub position: HandlePosition,
}

impl ConformanceEdgeEndpointPosition {
    pub fn new(point: CanvasPoint, position: HandlePosition) -> Self {
        Self { point, position }
    }

    fn matches_endpoint(self, endpoint: crate::runtime::geometry::EdgeEndpointPosition) -> bool {
        self.point == endpoint.point && self.position == endpoint.position
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConformanceLayoutEdgeRouteFacts {
    pub edge: EdgeId,
    pub kind: ResolvedEdgeRouteKind,
    pub interaction_width: f32,
    pub selected: bool,
}

impl ConformanceLayoutEdgeRouteFacts {
    pub fn new(
        edge: EdgeId,
        kind: ResolvedEdgeRouteKind,
        interaction_width: f32,
        selected: bool,
    ) -> Self {
        Self {
            edge,
            kind,
            interaction_width,
            selected,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceLayoutFactsConnectionTargetExpectation {
    pub pointer: CanvasPoint,
    pub from: ConnectionHandleRef,
    pub expected: ResolvedConnectionTarget,
}

impl ConformanceLayoutFactsConnectionTargetExpectation {
    pub fn new(
        pointer: CanvasPoint,
        from: ConnectionHandleRef,
        expected: ResolvedConnectionTarget,
    ) -> Self {
        Self {
            pointer,
            from,
            expected,
        }
    }
}

impl ConformanceAction {
    pub fn report_node_measurement(measurement: NodeMeasurement) -> Self {
        Self::ReportNodeMeasurement { measurement }
    }

    pub fn assert_layout_facts(
        viewport_size: CanvasSize,
        expected: ConformanceLayoutFactsExpectation,
    ) -> Self {
        Self::AssertLayoutFacts {
            viewport_size,
            expected,
        }
    }
}
