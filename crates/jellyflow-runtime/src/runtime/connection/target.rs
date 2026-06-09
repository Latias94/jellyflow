use serde::{Deserialize, Serialize};

use jellyflow_core::core::{CanvasPoint, CanvasRect, PortDirection};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::runtime::geometry::HandleBounds;

use super::{
    ClosestConnectionHandleInput, ConnectionHandleCandidate, ConnectionHandleRef,
    ConnectionHandleValidity, closest_connection_handle, connection_handle_validity,
};

/// Candidate target handle plus adapter-resolved connectability policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionTargetHandle {
    pub handle: ConnectionHandleRef,
    pub connectable: bool,
    pub connectable_end: bool,
}

impl ConnectionTargetHandle {
    pub fn new(handle: ConnectionHandleRef, connectable: bool, connectable_end: bool) -> Self {
        Self {
            handle,
            connectable,
            connectable_end,
        }
    }
}

/// Renderer-normalized target-handle geometry plus target connectability policy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConnectionTargetCandidate {
    pub target: ConnectionTargetHandle,
    pub node_rect: CanvasRect,
    pub bounds: HandleBounds,
}

impl ConnectionTargetCandidate {
    pub fn new(
        target: ConnectionTargetHandle,
        node_rect: CanvasRect,
        bounds: HandleBounds,
    ) -> Self {
        Self {
            target,
            node_rect,
            bounds,
        }
    }

    fn handle_candidate(self) -> ConnectionHandleCandidate {
        ConnectionHandleCandidate::new(self.target.handle, self.node_rect, self.bounds)
    }
}

/// XyFlow-shaped connection endpoints resolved from a start handle and target handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionHandleConnection {
    pub source: ConnectionHandleRef,
    pub target: ConnectionHandleRef,
}

impl ConnectionHandleConnection {
    pub fn from_start_and_target(from: ConnectionHandleRef, target: ConnectionHandleRef) -> Self {
        if from.direction == PortDirection::In {
            Self {
                source: target,
                target: from,
            }
        } else {
            Self {
                source: from,
                target,
            }
        }
    }
}

/// Input for resolving whether a target handle can complete a connection gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionTargetInput {
    pub from: ConnectionHandleRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ConnectionTargetHandle>,
    pub mode: NodeGraphConnectionMode,
    #[serde(default)]
    pub is_inside_connection_radius: bool,
    #[serde(default = "default_connection_validity")]
    pub is_valid_connection: bool,
}

impl ConnectionTargetInput {
    pub fn new(
        from: ConnectionHandleRef,
        target: Option<ConnectionTargetHandle>,
        mode: NodeGraphConnectionMode,
        is_inside_connection_radius: bool,
    ) -> Self {
        Self {
            from,
            target,
            mode,
            is_inside_connection_radius,
            is_valid_connection: true,
        }
    }

    pub fn with_connection_validity(mut self, is_valid_connection: bool) -> Self {
        self.is_valid_connection = is_valid_connection;
        self
    }
}

/// Input for resolving the current connection target from renderer-normalized handle candidates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionTargetFromHandlesInput<'a> {
    pub pointer: CanvasPoint,
    pub radius: f32,
    pub from: ConnectionHandleRef,
    pub candidates: &'a [ConnectionTargetCandidate],
    pub mode: NodeGraphConnectionMode,
    pub is_valid_connection: bool,
}

impl<'a> ConnectionTargetFromHandlesInput<'a> {
    pub fn new(
        pointer: CanvasPoint,
        radius: f32,
        from: ConnectionHandleRef,
        candidates: &'a [ConnectionTargetCandidate],
        mode: NodeGraphConnectionMode,
    ) -> Self {
        Self {
            pointer,
            radius,
            from,
            candidates,
            mode,
            is_valid_connection: true,
        }
    }

    pub fn with_connection_validity(mut self, is_valid_connection: bool) -> Self {
        self.is_valid_connection = is_valid_connection;
        self
    }
}

fn default_connection_validity() -> bool {
    true
}

/// Resolved target semantics for connection feedback and completion callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResolvedConnectionTarget {
    pub target: Option<ConnectionTargetHandle>,
    pub connection: Option<ConnectionHandleConnection>,
    pub is_handle_valid: bool,
    pub feedback: ConnectionHandleValidity,
}

/// Resolves XyFlow handle target semantics without depending on DOM hit testing.
///
/// Adapters decide which handle is under or near the pointer. This function owns the shared
/// source/target ordering, strict/loose mode checks, target connectability, and feedback state.
pub fn resolve_connection_target(input: ConnectionTargetInput) -> ResolvedConnectionTarget {
    let Some(target) = input.target else {
        return ResolvedConnectionTarget {
            target: None,
            connection: None,
            is_handle_valid: false,
            feedback: connection_handle_validity(input.is_inside_connection_radius, false),
        };
    };

    let connection = ConnectionHandleConnection::from_start_and_target(input.from, target.handle);
    let mode_allows_target = match input.mode {
        NodeGraphConnectionMode::Strict => input.from.direction != target.handle.direction,
        NodeGraphConnectionMode::Loose => {
            input.from.node != target.handle.node || input.from.port != target.handle.port
        }
    };
    let is_handle_valid = target.connectable
        && target.connectable_end
        && mode_allows_target
        && input.is_valid_connection;

    ResolvedConnectionTarget {
        target: Some(target),
        connection: Some(connection),
        is_handle_valid,
        feedback: connection_handle_validity(input.is_inside_connection_radius, is_handle_valid),
    }
}

/// Resolves connection target feedback from renderer-normalized handle geometry and policies.
///
/// Adapters still own DOM/window hit testing and provide the candidate handle inventory. The runtime
/// owns XyFlow-compatible closest-handle tie semantics, strict/loose checks, endpoint ordering, and
/// validity feedback.
pub fn resolve_connection_target_from_handles(
    input: ConnectionTargetFromHandlesInput<'_>,
) -> ResolvedConnectionTarget {
    let handle_candidates = input
        .candidates
        .iter()
        .copied()
        .map(ConnectionTargetCandidate::handle_candidate)
        .collect::<Vec<_>>();
    let closest = closest_connection_handle(ClosestConnectionHandleInput::new(
        input.pointer,
        input.radius,
        input.from,
        &handle_candidates,
    ));
    let target = closest.and_then(|closest| {
        input
            .candidates
            .iter()
            .find(|candidate| candidate.target.handle == closest.handle)
            .map(|candidate| candidate.target)
    });

    resolve_connection_target(
        ConnectionTargetInput::new(input.from, target, input.mode, closest.is_some())
            .with_connection_validity(input.is_valid_connection),
    )
}
