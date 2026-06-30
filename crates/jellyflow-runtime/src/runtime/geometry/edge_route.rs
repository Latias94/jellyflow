use jellyflow_core::core::{Edge, EdgeLabelAnchor, EdgeRouteKind};
use serde::{Deserialize, Serialize};

use super::{
    BezierEdgeOptions, EdgeHitTestOptions, EdgePath, EdgePosition, SmoothStepEdgeOptions,
    bezier_edge_path, edge_path_contains_point, smoothstep_edge_path, straight_edge_path,
};

/// Effective route kind used by runtime geometry projections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedEdgeRouteKind {
    Straight,
    Orthogonal,
    Bezier,
    SmoothStep,
}

impl From<EdgeRouteKind> for ResolvedEdgeRouteKind {
    fn from(value: EdgeRouteKind) -> Self {
        match value {
            EdgeRouteKind::Straight => Self::Straight,
            EdgeRouteKind::Orthogonal => Self::Orthogonal,
            EdgeRouteKind::Bezier => Self::Bezier,
            EdgeRouteKind::SmoothStep => Self::SmoothStep,
        }
    }
}

impl From<ResolvedEdgeRouteKind> for EdgeRouteKind {
    fn from(value: ResolvedEdgeRouteKind) -> Self {
        match value {
            ResolvedEdgeRouteKind::Straight => Self::Straight,
            ResolvedEdgeRouteKind::Orthogonal => Self::Orthogonal,
            ResolvedEdgeRouteKind::Bezier => Self::Bezier,
            ResolvedEdgeRouteKind::SmoothStep => Self::SmoothStep,
        }
    }
}

/// Adapter-facing interaction facts for an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeInteractionFacts {
    pub selectable: bool,
    pub selected: bool,
    pub focusable: bool,
    pub deletable: bool,
    pub reconnect_source: bool,
    pub reconnect_target: bool,
}

impl EdgeInteractionFacts {
    pub fn can_reconnect(self) -> bool {
        self.reconnect_source || self.reconnect_target
    }
}

/// Renderer-neutral route and interaction facts for one edge.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeRouteFacts {
    pub kind: ResolvedEdgeRouteKind,
    pub endpoints: EdgePosition,
    pub path: EdgePath,
    pub label_anchor: EdgeLabelAnchor,
    pub hit_test: EdgeHitTestOptions,
    pub interaction: EdgeInteractionFacts,
}

impl EdgeRouteFacts {
    pub fn with_hit_test(mut self, hit_test: EdgeHitTestOptions) -> Self {
        self.hit_test = hit_test;
        self
    }

    pub fn with_interaction(mut self, interaction: EdgeInteractionFacts) -> Self {
        self.interaction = interaction;
        self
    }

    pub fn contains_point(&self, point: jellyflow_core::core::CanvasPoint) -> bool {
        edge_path_contains_point(&self.path, point, self.hit_test)
    }
}

/// Resolves the route style and path for an edge from persisted view hints.
pub fn resolve_edge_route_path(edge: &Edge, endpoints: EdgePosition) -> Option<EdgeRouteFacts> {
    let kind = edge
        .view
        .route_kind
        .map(ResolvedEdgeRouteKind::from)
        .unwrap_or(ResolvedEdgeRouteKind::Bezier);
    let path = match kind {
        ResolvedEdgeRouteKind::Straight => straight_edge_path(endpoints.source, endpoints.target),
        ResolvedEdgeRouteKind::Orthogonal | ResolvedEdgeRouteKind::SmoothStep => {
            smoothstep_edge_path(
                endpoints.source,
                endpoints.target,
                SmoothStepEdgeOptions::default(),
            )
        }
        ResolvedEdgeRouteKind::Bezier => bezier_edge_path(
            endpoints.source,
            endpoints.target,
            BezierEdgeOptions::default(),
        ),
    }?;

    Some(EdgeRouteFacts {
        kind,
        endpoints,
        path,
        label_anchor: edge.view.label_anchor.unwrap_or(EdgeLabelAnchor::Center),
        hit_test: EdgeHitTestOptions::default(),
        interaction: EdgeInteractionFacts {
            selectable: false,
            selected: false,
            focusable: false,
            deletable: false,
            reconnect_source: false,
            reconnect_target: false,
        },
    })
}
