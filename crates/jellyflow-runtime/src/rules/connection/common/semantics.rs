use jellyflow_core::core::{PortDirection, PortKind};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::{TypeCompatibility, TypeCompatibilityResult, TypeDesc};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ConnectionPortFacts<'a> {
    direction: PortDirection,
    kind: PortKind,
    ty: Option<&'a TypeDesc>,
}

impl<'a> ConnectionPortFacts<'a> {
    pub(crate) fn new(direction: PortDirection, kind: PortKind, ty: Option<&'a TypeDesc>) -> Self {
        Self {
            direction,
            kind,
            ty,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::rules::connection) enum ConnectionOrder {
    FirstToSecond,
    SecondToFirst,
}

pub(in crate::rules::connection) fn resolve_connection_order(
    first: PortDirection,
    second: PortDirection,
    mode: NodeGraphConnectionMode,
) -> Option<ConnectionOrder> {
    match mode {
        NodeGraphConnectionMode::Strict => match (first, second) {
            (PortDirection::Out, PortDirection::In) => Some(ConnectionOrder::FirstToSecond),
            (PortDirection::In, PortDirection::Out) => Some(ConnectionOrder::SecondToFirst),
            (PortDirection::In, PortDirection::In) | (PortDirection::Out, PortDirection::Out) => {
                None
            }
        },
        NodeGraphConnectionMode::Loose => Some(match first {
            PortDirection::Out => ConnectionOrder::FirstToSecond,
            PortDirection::In => ConnectionOrder::SecondToFirst,
        }),
    }
}

pub(in crate::rules::connection) fn port_kinds_are_compatible(
    first: PortKind,
    second: PortKind,
) -> bool {
    first == second
}

pub(in crate::rules::connection) fn resolved_types_compatibility(
    from: &TypeDesc,
    to: &TypeDesc,
    compatibility: &dyn TypeCompatibility,
) -> TypeCompatibilityResult {
    compatibility.compatible(from, to)
}

pub(crate) fn connection_semantics_accepts(
    first: ConnectionPortFacts<'_>,
    second: ConnectionPortFacts<'_>,
    mode: NodeGraphConnectionMode,
    compatibility: &dyn TypeCompatibility,
) -> bool {
    let Some(order) = resolve_connection_order(first.direction, second.direction, mode) else {
        return false;
    };
    if !port_kinds_are_compatible(first.kind, second.kind) {
        return false;
    }
    if first.kind != PortKind::Data {
        return true;
    }

    let (from, to) = match order {
        ConnectionOrder::FirstToSecond => (first.ty, second.ty),
        ConnectionOrder::SecondToFirst => (second.ty, first.ty),
    };
    match (from, to) {
        (Some(from), Some(to)) => {
            resolved_types_compatibility(from, to, compatibility).is_compatible()
        }
        (None, _) | (_, None) => true,
    }
}
