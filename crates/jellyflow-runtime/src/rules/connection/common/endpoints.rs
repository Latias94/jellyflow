use crate::graph_kernel::EdgeEndpoints;
use crate::rules::ConnectPlan;
use jellyflow_core::core::{EdgeKind, Graph, Port, PortDirection, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::rejections::{
    reject_incompatible_port_kinds, reject_missing_port, reject_opposite_directions_required,
    reject_self_connection,
};
use super::{ConnectionOrder, port_kinds_are_compatible, resolve_connection_order};

pub(in crate::rules::connection) struct ConnectionEndpoints<'a> {
    pub from_id: PortId,
    pub to_id: PortId,
    pub from: &'a Port,
    pub to: &'a Port,
    pub edge_kind: EdgeKind,
}

impl ConnectionEndpoints<'_> {
    pub(in crate::rules::connection) fn edge_endpoints(&self) -> EdgeEndpoints {
        EdgeEndpoints::new(self.from_id, self.to_id)
    }

    pub(in crate::rules::connection) fn is_out_to_in(&self) -> bool {
        self.from.dir == PortDirection::Out && self.to.dir == PortDirection::In
    }
}

pub(in crate::rules::connection) fn resolve_connection_endpoints(
    graph: &Graph,
    a: PortId,
    b: PortId,
    mode: NodeGraphConnectionMode,
) -> Result<ConnectionEndpoints<'_>, ConnectPlan> {
    if a == b {
        return Err(reject_self_connection(a));
    }

    let Some(port_a) = graph.ports().get(&a) else {
        return Err(reject_missing_port(a));
    };
    let Some(port_b) = graph.ports().get(&b) else {
        return Err(reject_missing_port(b));
    };

    let Some(order) = resolve_connection_order(port_a.dir, port_b.dir, mode) else {
        return Err(reject_opposite_directions_required(b));
    };
    let (from_id, to_id) = match order {
        ConnectionOrder::FirstToSecond => (a, b),
        ConnectionOrder::SecondToFirst => (b, a),
    };

    resolve_ordered_connection_endpoints(graph, from_id, to_id)
}

pub(in crate::rules::connection) fn resolve_ordered_connection_endpoints(
    graph: &Graph,
    from_id: PortId,
    to_id: PortId,
) -> Result<ConnectionEndpoints<'_>, ConnectPlan> {
    if from_id == to_id {
        return Err(reject_self_connection(from_id));
    }

    let (from, to) = connection_ports(graph, from_id, to_id)?;

    if !port_kinds_are_compatible(from.kind, to.kind) {
        return Err(reject_incompatible_port_kinds(from.kind, to.kind, to_id));
    }

    let edge_kind = from.kind.edge_kind();

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
    let Some(from) = graph.ports().get(&from_id) else {
        return Err(reject_missing_port(from_id));
    };
    let Some(to) = graph.ports().get(&to_id) else {
        return Err(reject_missing_port(to_id));
    };
    Ok((from, to))
}
