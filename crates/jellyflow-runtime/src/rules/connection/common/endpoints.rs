use crate::rules::ConnectPlan;
use jellyflow_core::core::{EdgeKind, Graph, Port, PortDirection, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::kinds::edge_kind_for_port_kind;
use super::rejections::{
    reject_incompatible_port_kind, reject_incompatible_port_kinds, reject_missing_port,
    reject_opposite_directions_required, reject_self_connection,
};

pub(in crate::rules::connection) struct ConnectionEndpoints<'a> {
    pub from_id: PortId,
    pub to_id: PortId,
    pub from: &'a Port,
    pub to: &'a Port,
    pub edge_kind: EdgeKind,
}

pub(in crate::rules::connection) fn resolve_connection_endpoints(
    graph: &Graph,
    a: PortId,
    b: PortId,
    mode: NodeGraphConnectionMode,
) -> Result<ConnectionEndpoints<'_>, ConnectPlan> {
    if a == b {
        return Err(reject_self_connection());
    }

    let Some(port_a) = graph.ports.get(&a) else {
        return Err(reject_missing_port(a));
    };
    let Some(port_b) = graph.ports.get(&b) else {
        return Err(reject_missing_port(b));
    };

    let (from_id, to_id) = match mode {
        NodeGraphConnectionMode::Strict => match (port_a.dir, port_b.dir) {
            (PortDirection::Out, PortDirection::In) => (a, b),
            (PortDirection::In, PortDirection::Out) => (b, a),
            _ => {
                return Err(reject_opposite_directions_required());
            }
        },
        NodeGraphConnectionMode::Loose => match port_a.dir {
            PortDirection::Out => (a, b),
            PortDirection::In => (b, a),
        },
    };

    let (from, to) = connection_ports(graph, from_id, to_id)?;

    if from.kind != to.kind {
        return Err(reject_incompatible_port_kinds(from.kind, to.kind));
    }

    let Some(edge_kind) = edge_kind_for_port_kind(from.kind) else {
        return Err(reject_incompatible_port_kind());
    };

    Ok(ConnectionEndpoints {
        from_id,
        to_id,
        from,
        to,
        edge_kind,
    })
}

pub(in crate::rules::connection) fn connection_ports(
    graph: &Graph,
    from_id: PortId,
    to_id: PortId,
) -> Result<(&Port, &Port), ConnectPlan> {
    let Some(from) = graph.ports.get(&from_id) else {
        return Err(reject_missing_port(from_id));
    };
    let Some(to) = graph.ports.get(&to_id) else {
        return Err(reject_missing_port(to_id));
    };
    Ok((from, to))
}
