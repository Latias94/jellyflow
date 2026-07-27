use std::fmt;

use jellyflow_core::{CanvasPoint, CanvasRect, EdgeId, PortId};
use jellyflow_runtime::geometry::{
    EdgeEndpointPosition, EdgePathAnchor, EdgePathSample, HandlePosition, handle_anchor_position,
};
use jellyflow_runtime::{graph::EdgeEndpoint, view::GeometrySnapshot};

const ENDPOINT_POINT_EPSILON: f32 = 0.001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PublishedEndpointSource {
    PublishedPort,
    PublishedRoute,
    ExplicitFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PublishedRouteEndpointIdentity {
    edge: EdgeId,
    endpoint: EdgeEndpoint,
}

impl PublishedRouteEndpointIdentity {
    pub(super) const fn new(edge: EdgeId, endpoint: EdgeEndpoint) -> Self {
        Self { edge, endpoint }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PublishedEndpointIdentity {
    layout_revision: u64,
    port: PortId,
    route: Option<PublishedRouteEndpointIdentity>,
}

impl PublishedEndpointIdentity {
    #[cfg(test)]
    pub(super) const fn layout_revision(self) -> u64 {
        self.layout_revision
    }

    #[cfg(test)]
    pub(super) const fn port(self) -> PortId {
        self.port
    }

    #[cfg(test)]
    pub(super) const fn route(self) -> Option<PublishedRouteEndpointIdentity> {
        self.route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PublishedEndpointDiagnosticKind {
    MissingPublishedRoute,
    MissingPublishedPort,
    MissingPublishedRouteAndPort,
    PublishedRoutePortDisagreement,
    PublishedRoutePathDisagreement,
    PublishedRoutePathUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PublishedEndpointDiagnostic {
    identity: PublishedEndpointIdentity,
    kind: PublishedEndpointDiagnosticKind,
    selected_source: PublishedEndpointSource,
}

impl PublishedEndpointDiagnostic {
    #[cfg(test)]
    pub(super) const fn kind(self) -> PublishedEndpointDiagnosticKind {
        self.kind
    }

    #[cfg(test)]
    pub(super) const fn selected_source(self) -> PublishedEndpointSource {
        self.selected_source
    }
}

impl fmt::Display for PublishedEndpointDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let issue = match self.kind {
            PublishedEndpointDiagnosticKind::MissingPublishedRoute => {
                "the exact published route endpoint is unavailable"
            }
            PublishedEndpointDiagnosticKind::MissingPublishedPort => {
                "the exact published port geometry is unavailable"
            }
            PublishedEndpointDiagnosticKind::MissingPublishedRouteAndPort => {
                "the exact published route endpoint and port geometry are unavailable"
            }
            PublishedEndpointDiagnosticKind::PublishedRoutePortDisagreement => {
                "the published route endpoint disagrees with its published port anchor"
            }
            PublishedEndpointDiagnosticKind::PublishedRoutePathDisagreement => {
                "the published route path endpoint disagrees with its route facts"
            }
            PublishedEndpointDiagnosticKind::PublishedRoutePathUnavailable => {
                "the published route path endpoint cannot be sampled"
            }
        };
        write!(
            formatter,
            "layout revision {} endpoint port={:?} route={:?}: {issue}; selected {:?}",
            self.identity.layout_revision,
            self.identity.port,
            self.identity
                .route
                .map(|route| (route.edge, route.endpoint)),
            self.selected_source,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ResolvedEndpoint {
    identity: PublishedEndpointIdentity,
    position: EdgeEndpointPosition,
    tangent: CanvasPoint,
    source: PublishedEndpointSource,
    diagnostic: Option<PublishedEndpointDiagnostic>,
}

impl ResolvedEndpoint {
    pub(super) const fn position(self) -> EdgeEndpointPosition {
        self.position
    }

    pub(super) const fn point(self) -> CanvasPoint {
        self.position.point
    }

    pub(super) const fn outward(self) -> HandlePosition {
        self.position.position
    }

    pub(super) const fn tangent(self) -> CanvasPoint {
        self.tangent
    }

    #[cfg(test)]
    pub(super) const fn source(self) -> PublishedEndpointSource {
        self.source
    }

    pub(super) fn diagnostic(self) -> Option<PublishedEndpointDiagnostic> {
        if let Some(diagnostic) = self.diagnostic {
            debug_assert_eq!(diagnostic.identity, self.identity);
            debug_assert_eq!(diagnostic.selected_source, self.source);
        }
        self.diagnostic
    }

    #[cfg(test)]
    pub(super) const fn identity(self) -> PublishedEndpointIdentity {
        self.identity
    }

    pub(super) fn with_route_sample(mut self, sample: EdgePathSample) -> Self {
        self.position.point = sample.point;
        self.tangent = sample.tangent;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ExplicitEndpointFallback {
    layout_revision: u64,
    position: EdgeEndpointPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndpointAuthority {
    Port,
    Route,
}

#[derive(Debug, Clone, Copy)]
struct RouteEndpointCandidate {
    position: EdgeEndpointPosition,
    tangent: CanvasPoint,
    diagnostic: Option<PublishedEndpointDiagnosticKind>,
}

pub(super) struct PublishedEndpointResolver<'a> {
    geometry: &'a GeometrySnapshot,
}

impl<'a> PublishedEndpointResolver<'a> {
    pub(super) const fn new(geometry: &'a GeometrySnapshot) -> Self {
        Self { geometry }
    }

    pub(super) const fn geometry(&self) -> &'a GeometrySnapshot {
        self.geometry
    }

    pub(super) fn explicit_node_box_fallback(
        &self,
        bounds: CanvasRect,
        outward: HandlePosition,
    ) -> Option<ExplicitEndpointFallback> {
        let position = handle_anchor_position(bounds, None, outward)?;
        Some(ExplicitEndpointFallback {
            layout_revision: self.geometry.layout_revision(),
            position,
        })
    }

    pub(super) fn resolve_port(
        &self,
        port: PortId,
        route: Option<PublishedRouteEndpointIdentity>,
        fallback: Option<ExplicitEndpointFallback>,
    ) -> Option<ResolvedEndpoint> {
        self.resolve(port, route, fallback, EndpointAuthority::Port)
    }

    pub(super) fn resolve_route(
        &self,
        port: PortId,
        route: PublishedRouteEndpointIdentity,
        fallback: Option<ExplicitEndpointFallback>,
    ) -> Option<ResolvedEndpoint> {
        self.resolve(port, Some(route), fallback, EndpointAuthority::Route)
    }

    fn resolve(
        &self,
        port: PortId,
        route: Option<PublishedRouteEndpointIdentity>,
        fallback: Option<ExplicitEndpointFallback>,
        authority: EndpointAuthority,
    ) -> Option<ResolvedEndpoint> {
        let identity = PublishedEndpointIdentity {
            layout_revision: self.geometry.layout_revision(),
            port,
            route,
        };
        let port = self
            .geometry
            .port(port)
            .map(|published| EdgeEndpointPosition {
                point: published.route_anchor(),
                position: published.outward(),
            });
        let route = route.and_then(|route| self.route_candidate(route));
        resolve_candidates(identity, authority, port, route, fallback)
    }

    fn route_candidate(
        &self,
        identity: PublishedRouteEndpointIdentity,
    ) -> Option<RouteEndpointCandidate> {
        let route = self.geometry.route(identity.edge)?;
        let facts = route.facts();
        let (position, anchor) = match identity.endpoint {
            EdgeEndpoint::From => (facts.endpoints.source, EdgePathAnchor::Source),
            EdgeEndpoint::To => (facts.endpoints.target, EdgePathAnchor::Target),
        };
        match facts.path.sample(anchor) {
            Ok(sample) => {
                let diagnostic = (!points_match(position.point, sample.point))
                    .then_some(PublishedEndpointDiagnosticKind::PublishedRoutePathDisagreement);
                Some(RouteEndpointCandidate {
                    position: EdgeEndpointPosition {
                        point: sample.point,
                        position: position.position,
                    },
                    tangent: sample.tangent,
                    diagnostic,
                })
            }
            Err(_) => Some(RouteEndpointCandidate {
                position,
                tangent: route_tangent(position.position, identity.endpoint),
                diagnostic: Some(PublishedEndpointDiagnosticKind::PublishedRoutePathUnavailable),
            }),
        }
    }
}

fn resolve_candidates(
    identity: PublishedEndpointIdentity,
    authority: EndpointAuthority,
    port: Option<EdgeEndpointPosition>,
    route: Option<RouteEndpointCandidate>,
    fallback: Option<ExplicitEndpointFallback>,
) -> Option<ResolvedEndpoint> {
    let disagreement = port.zip(route).and_then(|(port, route)| {
        (!positions_match(port, route.position))
            .then_some(PublishedEndpointDiagnosticKind::PublishedRoutePortDisagreement)
    });
    if let Some(route) = route.filter(|_| disagreement.is_some()) {
        return Some(resolved_route(
            identity,
            route,
            disagreement.or(route.diagnostic),
        ));
    }

    match authority {
        EndpointAuthority::Port => {
            if let Some(port) = port {
                return Some(ResolvedEndpoint {
                    identity,
                    position: port,
                    tangent: identity.route.map_or_else(
                        || outward_tangent(port.position),
                        |route| route_tangent(port.position, route.endpoint),
                    ),
                    source: PublishedEndpointSource::PublishedPort,
                    diagnostic: route.and_then(|route| {
                        route.diagnostic.map(|kind| PublishedEndpointDiagnostic {
                            identity,
                            kind,
                            selected_source: PublishedEndpointSource::PublishedPort,
                        })
                    }),
                });
            }
            if let Some(route) = route {
                return Some(resolved_route(
                    identity,
                    route,
                    Some(PublishedEndpointDiagnosticKind::MissingPublishedPort),
                ));
            }
            resolved_fallback(
                identity,
                fallback,
                PublishedEndpointDiagnosticKind::MissingPublishedPort,
            )
        }
        EndpointAuthority::Route => {
            if let Some(route) = route {
                return Some(resolved_route(identity, route, route.diagnostic));
            }
            if let Some(port) = port {
                return Some(ResolvedEndpoint {
                    identity,
                    position: port,
                    tangent: route_tangent(
                        port.position,
                        identity
                            .route
                            .map_or(EdgeEndpoint::From, |route| route.endpoint),
                    ),
                    source: PublishedEndpointSource::PublishedPort,
                    diagnostic: Some(PublishedEndpointDiagnostic {
                        identity,
                        kind: PublishedEndpointDiagnosticKind::MissingPublishedRoute,
                        selected_source: PublishedEndpointSource::PublishedPort,
                    }),
                });
            }
            resolved_fallback(
                identity,
                fallback,
                PublishedEndpointDiagnosticKind::MissingPublishedRouteAndPort,
            )
        }
    }
}

fn resolved_route(
    identity: PublishedEndpointIdentity,
    route: RouteEndpointCandidate,
    diagnostic: Option<PublishedEndpointDiagnosticKind>,
) -> ResolvedEndpoint {
    ResolvedEndpoint {
        identity,
        position: route.position,
        tangent: route.tangent,
        source: PublishedEndpointSource::PublishedRoute,
        diagnostic: diagnostic.map(|kind| PublishedEndpointDiagnostic {
            identity,
            kind,
            selected_source: PublishedEndpointSource::PublishedRoute,
        }),
    }
}

fn resolved_fallback(
    identity: PublishedEndpointIdentity,
    fallback: Option<ExplicitEndpointFallback>,
    kind: PublishedEndpointDiagnosticKind,
) -> Option<ResolvedEndpoint> {
    let fallback =
        fallback.filter(|fallback| fallback.layout_revision == identity.layout_revision)?;
    Some(ResolvedEndpoint {
        identity,
        position: fallback.position,
        tangent: identity.route.map_or_else(
            || outward_tangent(fallback.position.position),
            |route| route_tangent(fallback.position.position, route.endpoint),
        ),
        source: PublishedEndpointSource::ExplicitFallback,
        diagnostic: Some(PublishedEndpointDiagnostic {
            identity,
            kind,
            selected_source: PublishedEndpointSource::ExplicitFallback,
        }),
    })
}

fn positions_match(left: EdgeEndpointPosition, right: EdgeEndpointPosition) -> bool {
    left.position == right.position && points_match(left.point, right.point)
}

fn points_match(left: CanvasPoint, right: CanvasPoint) -> bool {
    (left.x - right.x).abs() <= ENDPOINT_POINT_EPSILON
        && (left.y - right.y).abs() <= ENDPOINT_POINT_EPSILON
}

fn route_tangent(outward: HandlePosition, endpoint: EdgeEndpoint) -> CanvasPoint {
    let tangent = outward_tangent(outward);
    match endpoint {
        EdgeEndpoint::From => tangent,
        EdgeEndpoint::To => CanvasPoint {
            x: -tangent.x,
            y: -tangent.y,
        },
    }
}

pub(super) fn outward_tangent(outward: HandlePosition) -> CanvasPoint {
    match outward {
        HandlePosition::Top => CanvasPoint { x: 0.0, y: -1.0 },
        HandlePosition::Right => CanvasPoint { x: 1.0, y: 0.0 },
        HandlePosition::Bottom => CanvasPoint { x: 0.0, y: 1.0 },
        HandlePosition::Left => CanvasPoint { x: -1.0, y: 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_runtime::viewport::ViewportTransform;

    use super::*;

    fn port() -> PortId {
        PortId::new()
    }

    fn edge() -> EdgeId {
        EdgeId::new()
    }

    fn position(x: f32, y: f32, outward: HandlePosition) -> EdgeEndpointPosition {
        EdgeEndpointPosition {
            point: CanvasPoint { x, y },
            position: outward,
        }
    }

    fn identity(
        revision: u64,
        port: PortId,
        edge: EdgeId,
        endpoint: EdgeEndpoint,
    ) -> PublishedEndpointIdentity {
        PublishedEndpointIdentity {
            layout_revision: revision,
            port,
            route: Some(PublishedRouteEndpointIdentity::new(edge, endpoint)),
        }
    }

    fn route(position: EdgeEndpointPosition) -> RouteEndpointCandidate {
        RouteEndpointCandidate {
            position,
            tangent: route_tangent(position.position, EdgeEndpoint::From),
            diagnostic: None,
        }
    }

    #[test]
    fn complete_reconnect_endpoint_matches_preview_and_committed_geometry() {
        let port = port();
        let edge = edge();
        let identity = identity(7, port, edge, EdgeEndpoint::From);
        let endpoint = position(80.0, 42.0, HandlePosition::Right);

        let preview = resolve_candidates(
            identity,
            EndpointAuthority::Port,
            Some(endpoint),
            Some(route(endpoint)),
            None,
        )
        .expect("matching preview endpoint");
        let reconnect = resolve_candidates(
            identity,
            EndpointAuthority::Route,
            Some(endpoint),
            Some(route(endpoint)),
            None,
        )
        .expect("matching reconnect endpoint");
        let committed = resolve_candidates(
            identity,
            EndpointAuthority::Route,
            Some(endpoint),
            Some(route(endpoint)),
            None,
        )
        .expect("matching committed endpoint");

        assert_eq!(preview.identity(), committed.identity());
        assert_eq!(reconnect.identity(), committed.identity());
        assert_eq!(preview.identity().layout_revision(), 7);
        assert_eq!(preview.identity().port(), port);
        assert_eq!(
            preview.identity().route(),
            Some(PublishedRouteEndpointIdentity::new(
                edge,
                EdgeEndpoint::From
            ))
        );
        assert_eq!(preview.position(), committed.position());
        assert_eq!(reconnect.position(), committed.position());
        assert_eq!(preview.tangent(), committed.tangent());
        assert_eq!(reconnect.tangent(), committed.tangent());
        assert_eq!(preview.source(), PublishedEndpointSource::PublishedPort);
        assert_eq!(reconnect.source(), PublishedEndpointSource::PublishedRoute);
        assert_eq!(committed.source(), PublishedEndpointSource::PublishedRoute);
        assert_eq!(preview.diagnostic(), None);
        assert_eq!(reconnect.diagnostic(), None);
        assert_eq!(committed.diagnostic(), None);
    }

    #[test]
    fn disagreed_reconnect_endpoint_uses_same_route_authority_as_preview_and_commit() {
        let identity = identity(9, port(), edge(), EdgeEndpoint::To);
        let published_port = position(10.0, 20.0, HandlePosition::Left);
        let published_route = position(12.0, 20.0, HandlePosition::Left);

        let preview = resolve_candidates(
            identity,
            EndpointAuthority::Port,
            Some(published_port),
            Some(route(published_route)),
            None,
        )
        .expect("route-authoritative preview endpoint");
        let reconnect = resolve_candidates(
            identity,
            EndpointAuthority::Route,
            Some(published_port),
            Some(route(published_route)),
            None,
        )
        .expect("route-authoritative reconnect endpoint");
        let committed = resolve_candidates(
            identity,
            EndpointAuthority::Route,
            Some(published_port),
            Some(route(published_route)),
            None,
        )
        .expect("route-authoritative committed endpoint");

        for resolved in [preview, reconnect, committed] {
            assert_eq!(resolved.position(), published_route);
            assert_eq!(resolved.source(), PublishedEndpointSource::PublishedRoute);
            let diagnostic = resolved.diagnostic().expect("disagreement diagnostic");
            assert_eq!(
                diagnostic.kind(),
                PublishedEndpointDiagnosticKind::PublishedRoutePortDisagreement
            );
            assert_eq!(
                diagnostic.selected_source(),
                PublishedEndpointSource::PublishedRoute
            );
        }
        assert_eq!(preview.identity(), committed.identity());
        assert_eq!(reconnect.identity(), committed.identity());
        assert_eq!(preview.tangent(), committed.tangent());
        assert_eq!(reconnect.tangent(), committed.tangent());
    }

    #[test]
    fn reconnect_target_keeps_outward_normal_and_path_tangent_distinct() {
        let identity = identity(11, port(), edge(), EdgeEndpoint::To);
        let endpoint = position(90.0, 28.0, HandlePosition::Left);
        let route = RouteEndpointCandidate {
            position: endpoint,
            tangent: CanvasPoint { x: 1.0, y: 0.0 },
            diagnostic: None,
        };

        let preview = resolve_candidates(
            identity,
            EndpointAuthority::Port,
            Some(endpoint),
            Some(route),
            None,
        )
        .expect("reconnect preview endpoint");
        let committed = resolve_candidates(
            identity,
            EndpointAuthority::Route,
            Some(endpoint),
            Some(route),
            None,
        )
        .expect("committed target endpoint");

        assert_eq!(preview.outward(), HandlePosition::Left);
        assert_eq!(committed.outward(), HandlePosition::Left);
        assert_eq!(preview.tangent(), CanvasPoint { x: 1.0, y: 0.0 });
        assert_eq!(preview.tangent(), committed.tangent());
    }

    #[test]
    fn missing_route_reconnect_endpoint_matches_same_revision_port_fallback() {
        let identity = identity(13, port(), edge(), EdgeEndpoint::From);
        let published_port = position(18.0, 24.0, HandlePosition::Right);

        let preview = resolve_candidates(
            identity,
            EndpointAuthority::Port,
            Some(published_port),
            None,
            None,
        )
        .expect("published preview endpoint");
        let reconnect = resolve_candidates(
            identity,
            EndpointAuthority::Route,
            Some(published_port),
            None,
            None,
        )
        .expect("reconnect port fallback");
        let committed = resolve_candidates(
            identity,
            EndpointAuthority::Route,
            Some(published_port),
            None,
            None,
        )
        .expect("committed port fallback");

        assert_eq!(preview.source(), PublishedEndpointSource::PublishedPort);
        assert_eq!(preview.position(), published_port);
        assert_eq!(preview.diagnostic(), None);
        for resolved in [reconnect, committed] {
            assert_eq!(resolved.source(), PublishedEndpointSource::PublishedPort);
            assert_eq!(resolved.position(), published_port);
            assert_eq!(
                resolved.diagnostic().map(PublishedEndpointDiagnostic::kind),
                Some(PublishedEndpointDiagnosticKind::MissingPublishedRoute)
            );
        }
        assert_eq!(preview.identity(), committed.identity());
        assert_eq!(reconnect.identity(), committed.identity());
        assert_eq!(preview.tangent(), committed.tangent());
        assert_eq!(reconnect.tangent(), committed.tangent());
        assert_eq!(
            reconnect
                .diagnostic()
                .map(PublishedEndpointDiagnostic::kind),
            Some(PublishedEndpointDiagnosticKind::MissingPublishedRoute)
        );
        let message = reconnect
            .diagnostic()
            .expect("fallback diagnostic")
            .to_string();
        assert!(message.contains("layout revision 13"));
        assert!(message.contains("selected PublishedPort"));
    }

    #[test]
    fn missing_port_uses_only_a_same_revision_explicit_fallback() {
        let identity = identity(21, port(), edge(), EdgeEndpoint::From);
        let fallback = ExplicitEndpointFallback {
            layout_revision: 21,
            position: position(30.0, 40.0, HandlePosition::Right),
        };

        let resolved = resolve_candidates(
            identity,
            EndpointAuthority::Port,
            None,
            None,
            Some(fallback),
        )
        .expect("explicit node-box fallback");

        assert_eq!(resolved.source(), PublishedEndpointSource::ExplicitFallback);
        assert_eq!(
            resolved.diagnostic().map(PublishedEndpointDiagnostic::kind),
            Some(PublishedEndpointDiagnosticKind::MissingPublishedPort)
        );

        let stale = ExplicitEndpointFallback {
            layout_revision: 20,
            ..fallback
        };
        assert!(
            resolve_candidates(identity, EndpointAuthority::Port, None, None, Some(stale),)
                .is_none(),
            "an explicit fallback from another layout revision must not be mixed in"
        );
    }

    #[test]
    fn canvas_endpoint_identity_is_stable_across_zoom_levels() {
        let identity = identity(34, port(), edge(), EdgeEndpoint::From);
        let published = position(120.0, 64.0, HandlePosition::Right);
        let resolved = resolve_candidates(
            identity,
            EndpointAuthority::Route,
            Some(published),
            Some(route(published)),
            None,
        )
        .expect("published endpoint");

        for zoom in [0.5, 1.0, 2.0] {
            let transform = ViewportTransform::new(CanvasPoint { x: 7.0, y: -3.0 }, zoom)
                .expect("valid transform");
            let projected = transform
                .screen_point_for_canvas(resolved.point())
                .expect("finite projected endpoint");
            assert_eq!(resolved.identity(), identity);
            assert!((projected.x - ((120.0 + 7.0) * zoom)).abs() <= f32::EPSILON);
            assert!((projected.y - ((64.0 - 3.0) * zoom)).abs() <= f32::EPSILON);
        }
    }
}
