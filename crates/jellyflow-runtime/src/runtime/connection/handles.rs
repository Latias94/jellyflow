use serde::{Deserialize, Serialize};

use jellyflow_core::core::{CanvasPoint, CanvasRect, NodeId, PortDirection, PortId};

use crate::runtime::geometry::{HandleBounds, HandlePosition, handle_center_position};

/// Stable identity for a renderer handle participating in a connection gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionHandleRef {
    pub node: NodeId,
    pub port: PortId,
    pub direction: PortDirection,
}

impl ConnectionHandleRef {
    pub fn new(node: NodeId, port: PortId, direction: PortDirection) -> Self {
        Self {
            node,
            port,
            direction,
        }
    }
}

/// Renderer-normalized handle geometry in canvas coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConnectionHandleCandidate {
    pub handle: ConnectionHandleRef,
    pub node_rect: CanvasRect,
    pub bounds: HandleBounds,
}

impl ConnectionHandleCandidate {
    pub fn new(handle: ConnectionHandleRef, node_rect: CanvasRect, bounds: HandleBounds) -> Self {
        Self {
            handle,
            node_rect,
            bounds,
        }
    }
}

/// Input for resolving the closest connection handle near a pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClosestConnectionHandleInput<'a> {
    pub pointer: CanvasPoint,
    pub radius: f32,
    pub from: ConnectionHandleRef,
    pub candidates: &'a [ConnectionHandleCandidate],
}

impl<'a> ClosestConnectionHandleInput<'a> {
    pub fn new(
        pointer: CanvasPoint,
        radius: f32,
        from: ConnectionHandleRef,
        candidates: &'a [ConnectionHandleCandidate],
    ) -> Self {
        Self {
            pointer,
            radius,
            from,
            candidates,
        }
    }
}

/// Closest handle resolution result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClosestConnectionHandle {
    pub handle: ConnectionHandleRef,
    pub center: CanvasPoint,
    pub position: HandlePosition,
    pub distance: f32,
}

/// XyFlow-compatible validity state for a connection target candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionHandleValidity {
    /// A candidate handle is present and can accept the connection.
    Valid,
    /// A candidate handle is present or inside the connection radius, but cannot accept it.
    Invalid,
    /// No handle is close enough to report valid or invalid feedback.
    NoHandle,
}

/// Returns the nearest handle inside `connection_radius`, matching XyFlow tie semantics.
///
/// XyFlow skips the starting handle, measures distance to handle centers, keeps all equal-distance
/// nearest handles, and prefers the opposite handle type when handles overlap at the same distance.
pub fn closest_connection_handle(
    input: ClosestConnectionHandleInput<'_>,
) -> Option<ClosestConnectionHandle> {
    if !input.pointer.is_finite() || !input.radius.is_finite() || input.radius < 0.0 {
        return None;
    }

    let mut closest: Vec<ClosestConnectionHandle> = Vec::new();
    let mut min_distance = f32::INFINITY;

    for candidate in input.candidates {
        if candidate.handle == input.from {
            continue;
        }

        let Some(center) = handle_center_position(
            candidate.node_rect,
            Some(candidate.bounds),
            candidate.bounds.position,
        ) else {
            continue;
        };
        let distance = (center.x - input.pointer.x).hypot(center.y - input.pointer.y);
        if !distance.is_finite() || distance > input.radius {
            continue;
        }

        let resolved = ClosestConnectionHandle {
            handle: candidate.handle,
            center,
            position: candidate.bounds.position,
            distance,
        };
        if distance < min_distance {
            closest.clear();
            closest.push(resolved);
            min_distance = distance;
        } else if distance == min_distance {
            closest.push(resolved);
        }
    }

    let preferred_direction = opposite_direction(input.from.direction);
    closest
        .iter()
        .find(|candidate| candidate.handle.direction == preferred_direction)
        .copied()
        .or_else(|| closest.first().copied())
}

fn opposite_direction(direction: PortDirection) -> PortDirection {
    match direction {
        PortDirection::In => PortDirection::Out,
        PortDirection::Out => PortDirection::In,
    }
}

/// Resolves XyFlow's `true | false | null` connection feedback into a Rust enum.
///
/// A valid handle wins even if the adapter did not separately mark it inside the radius. Otherwise,
/// an inside-radius candidate is invalid, and no candidate remains neutral.
pub fn connection_handle_validity(
    is_inside_connection_radius: bool,
    is_handle_valid: bool,
) -> ConnectionHandleValidity {
    if is_handle_valid {
        ConnectionHandleValidity::Valid
    } else if is_inside_connection_radius {
        ConnectionHandleValidity::Invalid
    } else {
        ConnectionHandleValidity::NoHandle
    }
}
