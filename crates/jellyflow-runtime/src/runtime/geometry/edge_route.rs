use jellyflow_core::core::{Edge, EdgeLabelAnchor, EdgeReconnectable, EdgeRouteKind};
use serde::{Deserialize, Serialize};

use super::{
    BezierEdgeOptions, EdgeHitTestOptions, EdgePath, EdgePosition, OrthogonalEdgeOptions,
    SmoothStepEdgeOptions, bezier_edge_path, edge_path_contains_point, orthogonal_edge_path,
    smoothstep_edge_path, straight_edge_path,
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
    pub presentation: EdgePresentationFacts,
    pub hit_test: EdgeHitTestOptions,
    pub interaction: EdgeInteractionFacts,
}

/// Renderer-neutral presentation keys carried with authoritative route geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgePresentationFacts {
    pub renderer_key: Option<String>,
    pub label: Option<String>,
    pub label_anchor: EdgeLabelAnchor,
    pub source_marker_key: Option<String>,
    pub target_marker_key: Option<String>,
    pub style_token: Option<String>,
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

    pub fn contains_point(
        &self,
        point: jellyflow_core::core::CanvasPoint,
    ) -> Result<bool, super::EdgePathFlattenError> {
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
        ResolvedEdgeRouteKind::Orthogonal => orthogonal_edge_path(
            endpoints.source,
            endpoints.target,
            OrthogonalEdgeOptions::default(),
        ),
        ResolvedEdgeRouteKind::SmoothStep => smoothstep_edge_path(
            endpoints.source,
            endpoints.target,
            SmoothStepEdgeOptions::default(),
        ),
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
        presentation: EdgePresentationFacts {
            renderer_key: edge.view.renderer_key.clone(),
            label: edge.view.label.clone(),
            label_anchor: edge.view.label_anchor.unwrap_or(EdgeLabelAnchor::Center),
            source_marker_key: edge.view.source_marker_key.clone(),
            target_marker_key: edge.view.target_marker_key.clone(),
            style_token: edge.view.style_token.clone(),
        },
        hit_test: EdgeHitTestOptions::new(
            edge.interaction_width
                .unwrap_or(super::DEFAULT_EDGE_INTERACTION_WIDTH),
            EdgeHitTestOptions::default().flatten.max_error,
        ),
        interaction: default_edge_interaction(edge),
    })
}

fn default_edge_interaction(edge: &Edge) -> EdgeInteractionFacts {
    if edge.hidden {
        return EdgeInteractionFacts {
            selectable: false,
            selected: false,
            deletable: false,
            reconnect_source: false,
            reconnect_target: false,
        };
    }
    let (reconnect_source, reconnect_target) = edge
        .reconnectable
        .unwrap_or(EdgeReconnectable::Bool(true))
        .endpoint_flags();
    EdgeInteractionFacts {
        selectable: edge.selectable.unwrap_or(true),
        selected: false,
        deletable: edge.deletable.unwrap_or(true),
        reconnect_source,
        reconnect_target,
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{
        CanvasPoint, Edge, EdgeKind, EdgeReconnectable, EdgeReconnectableEndpoint, PortId,
    };

    use super::*;
    use crate::runtime::geometry::{EdgeEndpointPosition, HandlePosition};

    fn endpoints() -> EdgePosition {
        EdgePosition {
            source: EdgeEndpointPosition {
                point: CanvasPoint { x: 0.0, y: 0.0 },
                position: HandlePosition::Right,
            },
            target: EdgeEndpointPosition {
                point: CanvasPoint { x: 100.0, y: 0.0 },
                position: HandlePosition::Left,
            },
        }
    }

    #[test]
    fn route_facts_use_one_interaction_width_and_real_edge_defaults() {
        let mut default_edge =
            Edge::new(EdgeKind::Data, PortId::from_u128(1), PortId::from_u128(2));
        default_edge.view = jellyflow_core::core::EdgeViewDescriptor::new()
            .with_renderer_key("data.flow")
            .with_label("value")
            .with_label_anchor(jellyflow_core::core::EdgeLabelAnchor::Target)
            .with_source_marker_key("source.dot")
            .with_target_marker_key("target.arrow")
            .with_style_token("data.primary")
            .with_route_kind(jellyflow_core::core::EdgeRouteKind::SmoothStep);
        let defaults = resolve_edge_route_path(&default_edge, endpoints()).expect("default route");
        assert!(defaults.interaction.selectable);
        assert!(defaults.interaction.deletable);
        assert!(defaults.interaction.reconnect_source);
        assert!(defaults.interaction.reconnect_target);
        assert_eq!(defaults.hit_test.interaction_width, 18.0);
        assert_eq!(
            defaults.presentation.renderer_key.as_deref(),
            Some("data.flow")
        );
        assert_eq!(defaults.presentation.label.as_deref(), Some("value"));
        assert_eq!(
            defaults.presentation.label_anchor,
            jellyflow_core::core::EdgeLabelAnchor::Target
        );
        assert_eq!(
            defaults.presentation.source_marker_key.as_deref(),
            Some("source.dot")
        );
        assert_eq!(
            defaults.presentation.target_marker_key.as_deref(),
            Some("target.arrow")
        );
        assert_eq!(
            defaults.presentation.style_token.as_deref(),
            Some("data.primary")
        );

        let mut restricted = default_edge;
        restricted.selectable = Some(false);
        restricted.deletable = Some(false);
        restricted.reconnectable = Some(EdgeReconnectable::Endpoint(
            EdgeReconnectableEndpoint::Source,
        ));
        restricted.interaction_width = Some(26.0);
        let restricted =
            resolve_edge_route_path(&restricted, endpoints()).expect("restricted route");
        assert!(!restricted.interaction.selectable);
        assert!(!restricted.interaction.deletable);
        assert!(restricted.interaction.reconnect_source);
        assert!(!restricted.interaction.reconnect_target);
        assert_eq!(restricted.hit_test.interaction_width, 26.0);
    }
}
