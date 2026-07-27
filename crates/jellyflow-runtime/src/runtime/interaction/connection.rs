use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, EdgeId, NodeId, PortDirection, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::rules::{Diagnostic, EdgeEndpoint};
use crate::runtime::commands::ConnectionEdit;
use crate::runtime::geometry::HandlePosition;
use crate::runtime::graph_runtime::{
    EdgeInstanceKey, GraphRevision, GraphRuntimePolicy, GraphSnapshot, GraphVersion,
    NodeLifetimeKey, PortInstanceKey, RuntimeProvenance,
};
use crate::runtime::policy::{resolve_edge_policy, resolve_port_policy};

/// Stable identity of one connection target lifetime retained across pointer probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionProbeTargetIdentity(PortInstanceKey);

impl ConnectionProbeTargetIdentity {
    pub(crate) const fn new(instance: PortInstanceKey) -> Self {
        Self(instance)
    }

    pub const fn port(self) -> PortId {
        self.0.port()
    }

    pub const fn instance(self) -> PortInstanceKey {
        self.0
    }
}

/// Exact graph authority retained for the complete lifetime of one connection gesture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionGestureBinding(Arc<ConnectionGestureAuthority>);

#[derive(Debug, PartialEq, Eq)]
enum ConnectionGestureAuthority {
    Connect {
        origin: PortInstanceKey,
        origin_endpoint: EdgeEndpoint,
        prospective_edge: EdgeId,
        mode: NodeGraphConnectionMode,
    },
    Reconnect {
        edge: EdgeInstanceKey,
        endpoint: EdgeEndpoint,
        detached: PortInstanceKey,
        fixed: PortInstanceKey,
        mode: NodeGraphConnectionMode,
    },
}

impl ConnectionGestureBinding {
    pub(crate) fn connect(
        origin: PortInstanceKey,
        origin_endpoint: EdgeEndpoint,
        prospective_edge: EdgeId,
        mode: NodeGraphConnectionMode,
    ) -> Self {
        Self(Arc::new(ConnectionGestureAuthority::Connect {
            origin,
            origin_endpoint,
            prospective_edge,
            mode,
        }))
    }

    #[cfg(test)]
    pub(crate) fn connect_for_test(
        prospective_edge: EdgeId,
        origin: PortId,
        mode: NodeGraphConnectionMode,
    ) -> Self {
        Self::connect(
            PortInstanceKey::unavailable(NodeId::from_u128(0), origin),
            EdgeEndpoint::From,
            prospective_edge,
            mode,
        )
    }

    pub(crate) fn reconnect(
        edge: EdgeInstanceKey,
        endpoint: EdgeEndpoint,
        detached: PortInstanceKey,
        fixed: PortInstanceKey,
        mode: NodeGraphConnectionMode,
    ) -> Option<Self> {
        (edge.provenance() == detached.provenance()
            && edge.provenance() == fixed.provenance()
            && detached.port() != fixed.port())
        .then(|| {
            Self(Arc::new(ConnectionGestureAuthority::Reconnect {
                edge,
                endpoint,
                detached,
                fixed,
                mode,
            }))
        })
    }

    pub(crate) fn edge_id(&self) -> EdgeId {
        match self.0.as_ref() {
            ConnectionGestureAuthority::Connect {
                prospective_edge, ..
            } => *prospective_edge,
            ConnectionGestureAuthority::Reconnect { edge, .. } => edge.edge(),
        }
    }

    pub fn provenance(&self) -> RuntimeProvenance {
        match self.0.as_ref() {
            ConnectionGestureAuthority::Connect { origin, .. } => origin.provenance(),
            ConnectionGestureAuthority::Reconnect { edge, .. } => edge.provenance(),
        }
    }

    pub fn owner(&self) -> NodeLifetimeKey {
        match self.0.as_ref() {
            ConnectionGestureAuthority::Connect { origin, .. } => origin.owner(),
            ConnectionGestureAuthority::Reconnect { detached, .. } => detached.owner(),
        }
    }

    pub fn mode(&self) -> NodeGraphConnectionMode {
        match self.0.as_ref() {
            ConnectionGestureAuthority::Connect { mode, .. }
            | ConnectionGestureAuthority::Reconnect { mode, .. } => *mode,
        }
    }

    pub fn fixed_port_instance(&self) -> PortInstanceKey {
        match self.0.as_ref() {
            ConnectionGestureAuthority::Connect { origin, .. } => *origin,
            ConnectionGestureAuthority::Reconnect { fixed, .. } => *fixed,
        }
    }

    pub fn detached_port_instance(&self) -> Option<PortInstanceKey> {
        match self.0.as_ref() {
            ConnectionGestureAuthority::Connect { .. } => None,
            ConnectionGestureAuthority::Reconnect { detached, .. } => Some(*detached),
        }
    }

    pub fn reconnect_edge(&self) -> Option<EdgeInstanceKey> {
        match self.0.as_ref() {
            ConnectionGestureAuthority::Connect { .. } => None,
            ConnectionGestureAuthority::Reconnect { edge, .. } => Some(*edge),
        }
    }

    pub fn fixed_endpoint(&self) -> EdgeEndpoint {
        match self.0.as_ref() {
            ConnectionGestureAuthority::Connect {
                origin_endpoint, ..
            } => *origin_endpoint,
            ConnectionGestureAuthority::Reconnect { endpoint, .. } => match endpoint {
                EdgeEndpoint::From => EdgeEndpoint::To,
                EdgeEndpoint::To => EdgeEndpoint::From,
            },
        }
    }

    pub(crate) fn is_current(&self, snapshot: &GraphSnapshot) -> bool {
        if self.provenance() != snapshot.provenance()
            || !snapshot.node_lifetime_is_current(self.owner())
        {
            return false;
        }
        match self.0.as_ref() {
            ConnectionGestureAuthority::Connect {
                origin,
                origin_endpoint,
                prospective_edge,
                ..
            } => {
                snapshot.port_instance_is_current(*origin)
                    && snapshot.graph().edge(prospective_edge).is_none()
                    && snapshot.graph().port(&origin.port()).is_some_and(|port| {
                        matches!(
                            (origin_endpoint, port.dir),
                            (&EdgeEndpoint::From, PortDirection::Out)
                                | (&EdgeEndpoint::To, PortDirection::In)
                        )
                    })
            }
            ConnectionGestureAuthority::Reconnect {
                edge,
                endpoint,
                detached,
                fixed,
                ..
            } => {
                if !snapshot.edge_instance_is_current(*edge)
                    || !snapshot.port_instance_is_current(*detached)
                    || !snapshot.port_instance_is_current(*fixed)
                {
                    return false;
                }
                let Some(model) = snapshot.graph().edge(&edge.edge()) else {
                    return false;
                };
                match endpoint {
                    EdgeEndpoint::From => model.from == detached.port() && model.to == fixed.port(),
                    EdgeEndpoint::To => model.to == detached.port() && model.from == fixed.port(),
                }
            }
        }
    }

    pub(crate) fn is_interactive(
        &self,
        snapshot: &GraphSnapshot,
        policy: GraphRuntimePolicy,
    ) -> bool {
        if !self.is_current(snapshot) {
            return false;
        }
        let dragged = self
            .detached_port_instance()
            .unwrap_or_else(|| self.fixed_port_instance());
        let Some(port) = snapshot.graph().port(&dragged.port()) else {
            return false;
        };
        let Some(owner) = snapshot.graph().node(&dragged.node()) else {
            return false;
        };
        let port_policy = resolve_port_policy(owner, port, &policy);
        let port_allows_drag = match port.dir {
            PortDirection::Out => port_policy.can_start_connection(),
            PortDirection::In => port_policy.can_accept_connection(),
        };
        if !port_allows_drag {
            return false;
        }
        self.reconnect_edge().is_none_or(|edge| {
            let Some(model) = snapshot.graph().edge(&edge.edge()) else {
                return false;
            };
            let edge_policy = resolve_edge_policy(model, &policy);
            match self.0.as_ref() {
                ConnectionGestureAuthority::Reconnect { endpoint, .. } => match endpoint {
                    EdgeEndpoint::From => edge_policy.can_reconnect_source(),
                    EdgeEndpoint::To => edge_policy.can_reconnect_target(),
                },
                ConnectionGestureAuthority::Connect { .. } => true,
            }
        })
    }

    pub(crate) fn edit_for_target(&self, target: PortId) -> ConnectionEdit {
        match self.0.as_ref() {
            ConnectionGestureAuthority::Connect {
                prospective_edge,
                origin,
                origin_endpoint,
                mode,
                ..
            } => match origin_endpoint {
                EdgeEndpoint::From => {
                    ConnectionEdit::connect(*prospective_edge, origin.port(), target, *mode)
                }
                EdgeEndpoint::To => {
                    ConnectionEdit::connect(*prospective_edge, target, origin.port(), *mode)
                }
            },
            ConnectionGestureAuthority::Reconnect {
                edge,
                endpoint,
                mode,
                ..
            } => ConnectionEdit::reconnect(edge.edge(), *endpoint, target, *mode),
        }
    }
}

/// Geometry and deterministic ordering data for the port under a connection pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionProbeTarget {
    identity: ConnectionProbeTargetIdentity,
    route_anchor: CanvasPoint,
    outward: HandlePosition,
    distance_in_view: f32,
}

impl ConnectionProbeTarget {
    pub(crate) const fn new(
        instance: PortInstanceKey,
        route_anchor: CanvasPoint,
        outward: HandlePosition,
        distance_in_view: f32,
    ) -> Self {
        Self {
            identity: ConnectionProbeTargetIdentity::new(instance),
            route_anchor,
            outward,
            distance_in_view,
        }
    }

    pub const fn port(self) -> PortId {
        self.identity.port()
    }

    pub const fn identity(self) -> ConnectionProbeTargetIdentity {
        self.identity
    }

    pub const fn node(self) -> NodeId {
        self.identity.instance().node()
    }

    pub const fn route_anchor(self) -> CanvasPoint {
        self.route_anchor
    }

    pub const fn outward(self) -> HandlePosition {
        self.outward
    }

    pub const fn distance_in_view(self) -> f32 {
        self.distance_in_view
    }
}

/// Completed connection target probe. Incomplete work is represented by the query outcome.
#[derive(Debug, Clone)]
pub enum ConnectionProbe {
    None,
    Invalid {
        version: GraphVersion,
        target: ConnectionProbeTarget,
        diagnostics: Vec<Diagnostic>,
    },
    Valid {
        version: GraphVersion,
        target: ConnectionProbeTarget,
    },
}

impl ConnectionProbe {
    pub const fn target(&self) -> Option<ConnectionProbeTarget> {
        match self {
            Self::None => None,
            Self::Invalid { target, .. } | Self::Valid { target, .. } => Some(*target),
        }
    }

    pub const fn revision(&self) -> Option<GraphRevision> {
        match self {
            Self::None => None,
            Self::Invalid { version, .. } | Self::Valid { version, .. } => Some(version.revision()),
        }
    }

    pub const fn version(&self) -> Option<GraphVersion> {
        match self {
            Self::None => None,
            Self::Invalid { version, .. } | Self::Valid { version, .. } => Some(*version),
        }
    }

    pub const fn is_valid(&self) -> bool {
        matches!(self, Self::Valid { .. })
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::Invalid { diagnostics, .. } => diagnostics,
            Self::None | Self::Valid { .. } => &[],
        }
    }
}
