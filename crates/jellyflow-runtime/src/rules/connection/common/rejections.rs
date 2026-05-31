use crate::rules::ConnectPlan;
use jellyflow_core::core::{EdgeId, EdgeKind, NodeId, PortId, PortKind};

pub(in crate::rules::connection) fn reject_duplicate_connection() -> ConnectPlan {
    ConnectPlan::reject("duplicate connection already exists")
}

pub(in crate::rules::connection) fn reject_edge_kind_incompatible_with_ports(
    edge_kind: EdgeKind,
    expected: EdgeKind,
) -> ConnectPlan {
    ConnectPlan::reject(format!(
        "edge kind is incompatible with ports: edge={edge_kind:?} expected={expected:?}"
    ))
}

pub(in crate::rules::connection) fn reject_edge_kind_incompatible() -> ConnectPlan {
    ConnectPlan::reject("edge kind is incompatible with ports")
}

pub(in crate::rules::connection) fn reject_incompatible_port_kind() -> ConnectPlan {
    ConnectPlan::reject("port kinds are incompatible")
}

pub(in crate::rules::connection) fn reject_incompatible_port_kinds(
    from: PortKind,
    to: PortKind,
) -> ConnectPlan {
    ConnectPlan::reject(format!(
        "port kinds are incompatible: from={from:?} to={to:?}"
    ))
}

pub(in crate::rules::connection) fn reject_missing_edge(edge_id: EdgeId) -> ConnectPlan {
    ConnectPlan::reject(format!("missing edge: {edge_id:?}"))
}

pub(in crate::rules::connection) fn reject_missing_port(port_id: PortId) -> ConnectPlan {
    ConnectPlan::reject(format!("missing port: {port_id:?}"))
}

pub(in crate::rules::connection) fn reject_missing_port_owner_node(node_id: NodeId) -> ConnectPlan {
    ConnectPlan::reject(format!("missing port owner node: {node_id:?}"))
}

pub(in crate::rules::connection) fn reject_opposite_directions_required() -> ConnectPlan {
    ConnectPlan::reject("ports must have opposite directions (in/out)")
}

pub(in crate::rules::connection) fn reject_reconnect_directions_required() -> ConnectPlan {
    ConnectPlan::reject("ports must be out -> in for reconnection")
}

pub(in crate::rules::connection) fn reject_self_connection() -> ConnectPlan {
    ConnectPlan::reject("cannot connect a port to itself")
}
