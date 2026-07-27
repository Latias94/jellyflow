use super::{
    BezierEdgeOptions, EdgeHitTestOptions, EdgePath, EdgePosition, OrthogonalEdgeOptions,
    SmoothStepEdgeOptions, bezier_edge_path, orthogonal_edge_path, smoothstep_edge_path,
    straight_edge_path,
};
use jellyflow_core::core::{Edge, EdgeKind, EdgeLabelAnchor, EdgeReconnectable, EdgeRouteSpec};

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
    /// Semantic graph edge kind used by adapter painter fallbacks.
    pub edge_kind: EdgeKind,
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
    pub fn with_interaction(mut self, interaction: EdgeInteractionFacts) -> Self {
        self.interaction = interaction;
        self
    }
}

/// Resolves the exact persisted route specification for an edge.
pub fn resolve_edge_route_path(edge: &Edge, endpoints: EdgePosition) -> Option<EdgeRouteFacts> {
    let route = edge.view.route.unwrap_or_default();
    route.validate().ok()?;
    let path = match route {
        EdgeRouteSpec::Straight => straight_edge_path(endpoints.source, endpoints.target),
        EdgeRouteSpec::Bezier { curvature } => bezier_edge_path(
            endpoints.source,
            endpoints.target,
            BezierEdgeOptions { curvature },
        ),
        EdgeRouteSpec::Orthogonal {
            offset,
            step_position,
            center,
        } => orthogonal_edge_path(
            endpoints.source,
            endpoints.target,
            OrthogonalEdgeOptions {
                offset,
                step_position,
                center,
            },
        ),
        EdgeRouteSpec::SmoothStep {
            offset,
            step_position,
            border_radius,
            center,
        } => smoothstep_edge_path(
            endpoints.source,
            endpoints.target,
            SmoothStepEdgeOptions {
                offset,
                step_position,
                border_radius,
                center,
            },
        ),
    }?;

    Some(EdgeRouteFacts {
        edge_kind: edge.kind,
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
            .with_route(jellyflow_core::core::EdgeRouteSpec::default_smooth_step());
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

    #[test]
    fn stepped_route_facts_honor_the_persisted_center() {
        let endpoints = EdgePosition {
            source: EdgeEndpointPosition {
                point: CanvasPoint { x: 780.0, y: 300.0 },
                position: HandlePosition::Right,
            },
            target: EdgeEndpointPosition {
                point: CanvasPoint { x: 100.0, y: 220.0 },
                position: HandlePosition::Left,
            },
        };
        let center = CanvasPoint { x: 440.0, y: 620.0 };
        let mut edge = Edge::new(EdgeKind::Exec, PortId::from_u128(1), PortId::from_u128(2));
        edge.view = jellyflow_core::core::EdgeViewDescriptor::new().with_route(
            jellyflow_core::core::EdgeRouteSpec::smooth_step(20.0, 0.5, 5.0, Some(center))
                .expect("valid route"),
        );

        let route = resolve_edge_route_path(&edge, endpoints).expect("stepped route");

        assert_eq!(route.path.label.point, center);
        assert!(route.path.commands.iter().any(|command| {
            matches!(command, crate::runtime::geometry::PathCommand::LineTo(point) if point.y == center.y)
        }));
    }

    #[test]
    fn every_route_parameter_changes_the_shared_resolved_path() {
        let forward = EdgePosition {
            source: EdgeEndpointPosition {
                point: CanvasPoint { x: 0.0, y: 0.0 },
                position: HandlePosition::Right,
            },
            target: EdgeEndpointPosition {
                point: CanvasPoint { x: 320.0, y: 120.0 },
                position: HandlePosition::Left,
            },
        };
        let backward = EdgePosition {
            source: EdgeEndpointPosition {
                point: CanvasPoint { x: 320.0, y: 0.0 },
                position: HandlePosition::Right,
            },
            target: EdgeEndpointPosition {
                point: CanvasPoint { x: 0.0, y: 120.0 },
                position: HandlePosition::Left,
            },
        };

        assert_ne!(
            route_path(EdgeRouteSpec::bezier(0.1).expect("valid curve"), backward),
            route_path(EdgeRouteSpec::bezier(0.8).expect("valid curve"), backward),
        );
        assert_ne!(
            route_path(
                EdgeRouteSpec::orthogonal(8.0, 0.5, None).expect("valid orthogonal route"),
                forward,
            ),
            route_path(
                EdgeRouteSpec::orthogonal(48.0, 0.5, None).expect("valid orthogonal route"),
                forward,
            ),
        );
        assert_ne!(
            route_path(
                EdgeRouteSpec::orthogonal(20.0, 0.2, None).expect("valid orthogonal route"),
                forward,
            ),
            route_path(
                EdgeRouteSpec::orthogonal(20.0, 0.8, None).expect("valid orthogonal route"),
                forward,
            ),
        );
        assert_ne!(
            route_path(
                EdgeRouteSpec::orthogonal(20.0, 0.5, None).expect("valid orthogonal route"),
                forward,
            ),
            route_path(
                EdgeRouteSpec::orthogonal(20.0, 0.5, Some(CanvasPoint { x: 240.0, y: 80.0 }),)
                    .expect("valid orthogonal route"),
                forward,
            ),
        );
        assert_ne!(
            route_path(
                EdgeRouteSpec::smooth_step(8.0, 0.5, 5.0, None).expect("valid smooth-step route"),
                forward,
            ),
            route_path(
                EdgeRouteSpec::smooth_step(48.0, 0.5, 5.0, None).expect("valid smooth-step route"),
                forward,
            ),
        );
        assert_ne!(
            route_path(
                EdgeRouteSpec::smooth_step(20.0, 0.2, 5.0, None).expect("valid smooth-step route"),
                forward,
            ),
            route_path(
                EdgeRouteSpec::smooth_step(20.0, 0.8, 5.0, None).expect("valid smooth-step route"),
                forward,
            ),
        );
        assert_ne!(
            route_path(
                EdgeRouteSpec::smooth_step(20.0, 0.5, 0.0, None).expect("valid smooth-step route"),
                forward,
            ),
            route_path(
                EdgeRouteSpec::smooth_step(20.0, 0.5, 24.0, None).expect("valid smooth-step route"),
                forward,
            ),
        );
        assert_ne!(
            route_path(
                EdgeRouteSpec::smooth_step(20.0, 0.5, 5.0, None)
                    .expect("valid smooth-step route"),
                forward,
            ),
            route_path(
                EdgeRouteSpec::smooth_step(
                    20.0,
                    0.5,
                    5.0,
                    Some(CanvasPoint { x: 240.0, y: 80.0 }),
                )
                .expect("valid smooth-step route"),
                forward,
            ),
        );
    }

    #[test]
    fn default_and_explicit_bezier_share_one_resolver_default() {
        let edge = Edge::new(EdgeKind::Data, PortId::from_u128(1), PortId::from_u128(2));
        let default_path = resolve_edge_route_path(&edge, endpoints())
            .expect("default route")
            .path;

        assert_eq!(
            default_path,
            route_path(EdgeRouteSpec::default(), endpoints())
        );
    }

    fn route_path(spec: EdgeRouteSpec, endpoints: EdgePosition) -> EdgePath {
        let mut edge = Edge::new(EdgeKind::Data, PortId::from_u128(1), PortId::from_u128(2));
        edge.view = jellyflow_core::core::EdgeViewDescriptor::new().with_route(spec);
        resolve_edge_route_path(&edge, endpoints)
            .expect("valid route")
            .path
    }
}
