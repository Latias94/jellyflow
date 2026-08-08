use crate::rules::{ConnectPlan, Diagnostic, DiagnosticTarget};
use jellyflow_core::core::{EdgeId, EdgeKind, NodeId, PortId, PortKind};

pub(in crate::rules::connection) fn reject_diagnostic(
    key: &'static str,
    target: DiagnosticTarget,
    message: impl Into<String>,
) -> ConnectPlan {
    ConnectPlan::reject_with_diagnostic(Diagnostic::error(key, target, message))
}

pub(in crate::rules::connection) fn reject_duplicate_connection(target: PortId) -> ConnectPlan {
    reject_diagnostic(
        "connect.duplicate",
        DiagnosticTarget::Port { id: target },
        "duplicate connection already exists",
    )
}

pub(in crate::rules::connection) fn reject_edge_kind_incompatible_with_ports(
    edge_kind: EdgeKind,
    expected: EdgeKind,
    target: PortId,
) -> ConnectPlan {
    reject_diagnostic(
        "connect.edge_kind_mismatch",
        DiagnosticTarget::Port { id: target },
        format!("edge kind is incompatible with ports: edge={edge_kind:?} expected={expected:?}"),
    )
}

pub(in crate::rules::connection) fn reject_incompatible_port_kinds(
    from: PortKind,
    to: PortKind,
    target: PortId,
) -> ConnectPlan {
    reject_diagnostic(
        "connect.port_kind_mismatch",
        DiagnosticTarget::Port { id: target },
        format!("port kinds are incompatible: from={from:?} to={to:?}"),
    )
}

pub(in crate::rules::connection) fn reject_missing_edge(edge_id: EdgeId) -> ConnectPlan {
    reject_diagnostic(
        "connect.edge_missing",
        DiagnosticTarget::Edge { id: edge_id },
        format!("missing edge: {edge_id:?}"),
    )
}

pub(in crate::rules::connection) fn reject_missing_port(port_id: PortId) -> ConnectPlan {
    reject_diagnostic(
        "connect.port_missing",
        DiagnosticTarget::Port { id: port_id },
        format!("missing port: {port_id:?}"),
    )
}

pub(in crate::rules::connection) fn reject_missing_port_owner_node(node_id: NodeId) -> ConnectPlan {
    reject_diagnostic(
        "connect.port_owner_missing",
        DiagnosticTarget::Node { id: node_id },
        format!("missing port owner node: {node_id:?}"),
    )
}

pub(in crate::rules::connection) fn reject_opposite_directions_required(
    target: PortId,
) -> ConnectPlan {
    reject_diagnostic(
        "connect.direction_mismatch",
        DiagnosticTarget::Port { id: target },
        "ports must have opposite directions (in/out)",
    )
}

pub(in crate::rules::connection) fn reject_reconnect_directions_required(
    target: PortId,
) -> ConnectPlan {
    reject_diagnostic(
        "connect.reconnect_direction_mismatch",
        DiagnosticTarget::Port { id: target },
        "ports must be out -> in for reconnection",
    )
}

pub(in crate::rules::connection) fn reject_self_connection(port: PortId) -> ConnectPlan {
    reject_diagnostic(
        "connect.self_connection",
        DiagnosticTarget::Port { id: port },
        "cannot connect a port to itself",
    )
}

pub(in crate::rules::connection) fn reject_port_not_connectable(
    port: PortId,
    source: bool,
) -> ConnectPlan {
    let (key, message) = if source {
        (
            "connect.source_not_connectable",
            "source port is not connectable",
        )
    } else {
        (
            "connect.target_not_connectable",
            "target port is not connectable",
        )
    };
    reject_diagnostic(key, DiagnosticTarget::Port { id: port }, message)
}

pub(in crate::rules::connection) fn reject_edge_not_reconnectable(
    edge: EdgeId,
    source: bool,
) -> ConnectPlan {
    let message = if source {
        "edge source endpoint is not reconnectable"
    } else {
        "edge target endpoint is not reconnectable"
    };
    reject_diagnostic(
        "connect.edge_not_reconnectable",
        DiagnosticTarget::Edge { id: edge },
        message,
    )
}
