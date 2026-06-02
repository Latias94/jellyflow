use serde::{Deserialize, Serialize};

use jellyflow_core::core::PortDirection;
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::{ConnectionHandleRef, ConnectionHandleValidity, connection_handle_validity};

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
