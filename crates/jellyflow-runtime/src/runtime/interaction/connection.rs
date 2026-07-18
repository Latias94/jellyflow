use jellyflow_core::core::{CanvasPoint, EdgeId, NodeId, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::rules::{Diagnostic, EdgeEndpoint};
use crate::runtime::commands::ConnectionEdit;
use crate::runtime::geometry::HandlePosition;
use crate::runtime::graph_runtime::GraphRevision;

/// One connection gesture independent of native pointer and capture mechanics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionGestureIntent {
    Connect {
        edge: EdgeId,
        source: PortId,
        mode: NodeGraphConnectionMode,
    },
    Reconnect {
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        detached_port: PortId,
        fixed_port: PortId,
        mode: NodeGraphConnectionMode,
    },
}

impl ConnectionGestureIntent {
    pub const fn connect(edge: EdgeId, source: PortId, mode: NodeGraphConnectionMode) -> Self {
        Self::Connect { edge, source, mode }
    }

    pub const fn reconnect(
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        detached_port: PortId,
        fixed_port: PortId,
        mode: NodeGraphConnectionMode,
    ) -> Self {
        Self::Reconnect {
            edge,
            endpoint,
            detached_port,
            fixed_port,
            mode,
        }
    }

    pub const fn edge(self) -> EdgeId {
        match self {
            Self::Connect { edge, .. } | Self::Reconnect { edge, .. } => edge,
        }
    }

    pub const fn mode(self) -> NodeGraphConnectionMode {
        match self {
            Self::Connect { mode, .. } | Self::Reconnect { mode, .. } => mode,
        }
    }

    /// Returns the anchored port that cannot be selected as the moving endpoint's target.
    pub const fn fixed_port(self) -> PortId {
        match self {
            Self::Connect { source, .. } => source,
            Self::Reconnect { fixed_port, .. } => fixed_port,
        }
    }

    pub const fn detached_port(self) -> Option<PortId> {
        match self {
            Self::Connect { .. } => None,
            Self::Reconnect { detached_port, .. } => Some(detached_port),
        }
    }

    pub fn edit_for_target(self, target: PortId) -> ConnectionEdit {
        match self {
            Self::Connect { edge, source, mode } => {
                ConnectionEdit::connect(edge, source, target, mode)
            }
            Self::Reconnect {
                edge,
                endpoint,
                mode,
                ..
            } => ConnectionEdit::reconnect(edge, endpoint, target, mode),
        }
    }
}

/// Geometry and deterministic ordering data for the port under a connection pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionProbeTarget {
    port: PortId,
    node: NodeId,
    route_anchor: CanvasPoint,
    outward: HandlePosition,
    distance_in_view: f32,
}

impl ConnectionProbeTarget {
    pub(crate) const fn new(
        port: PortId,
        node: NodeId,
        route_anchor: CanvasPoint,
        outward: HandlePosition,
        distance_in_view: f32,
    ) -> Self {
        Self {
            port,
            node,
            route_anchor,
            outward,
            distance_in_view,
        }
    }

    pub const fn port(self) -> PortId {
        self.port
    }

    pub const fn node(self) -> NodeId {
        self.node
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
        revision: GraphRevision,
        target: ConnectionProbeTarget,
        diagnostics: Vec<Diagnostic>,
    },
    Valid {
        revision: GraphRevision,
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
            Self::Invalid { revision, .. } | Self::Valid { revision, .. } => Some(*revision),
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
