use crate::io::NodeGraphInteractionState;
use crate::rules::{ConnectPlan, InsertNodeSpec};
use crate::runtime::policy::{NodeGraphPortInteractionPolicy, resolve_port_interaction_policy};
use jellyflow_core::core::{
    Edge, EdgeId, EdgeKind, Graph, NodeId, Port, PortCapacity, PortDirection, PortId, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::{GraphMutationError, GraphMutationPlanner, GraphOp};

pub(super) fn port_kind_for_edge_kind(edge_kind: EdgeKind) -> PortKind {
    match edge_kind {
        EdgeKind::Data => PortKind::Data,
        EdgeKind::Exec => PortKind::Exec,
    }
}

pub(super) fn edge_kind_for_port_kind(port_kind: PortKind) -> Option<EdgeKind> {
    match port_kind {
        PortKind::Data => Some(EdgeKind::Data),
        PortKind::Exec => Some(EdgeKind::Exec),
    }
}

pub(super) fn reject_mutation_error(error: GraphMutationError) -> ConnectPlan {
    ConnectPlan::reject(error.to_string())
}

fn remove_edge_op(graph: &Graph, edge_id: EdgeId) -> GraphOp {
    GraphMutationPlanner::new(graph)
        .remove_edge_op(edge_id)
        .expect("edge id came from the current graph")
}

pub(super) fn add_existing_ports_edge_op(
    graph: &Graph,
    edge_id: EdgeId,
    edge: Edge,
) -> Result<GraphOp, ConnectPlan> {
    GraphMutationPlanner::new(graph)
        .add_edge_op(edge_id, edge)
        .map_err(reject_mutation_error)
}

pub(super) fn edge_between(kind: EdgeKind, from: PortId, to: PortId) -> Edge {
    Edge {
        kind,
        from,
        to,
        selectable: None,
        deletable: None,
        reconnectable: None,
    }
}

pub(super) fn edge_like(edge: &Edge, from: PortId, to: PortId) -> Edge {
    Edge {
        kind: edge.kind,
        from,
        to,
        selectable: edge.selectable,
        deletable: edge.deletable,
        reconnectable: edge.reconnectable,
    }
}

pub(super) struct ConnectionEndpoints<'a> {
    pub from_id: PortId,
    pub to_id: PortId,
    pub from: &'a Port,
    pub to: &'a Port,
    pub edge_kind: EdgeKind,
}

pub(super) struct ValidatedInsertNodeSpec {
    pub input: PortId,
    pub output: PortId,
}

pub(super) fn validate_insert_node_spec(
    graph: &Graph,
    inserted: &InsertNodeSpec,
    source_node: NodeId,
    target_node: NodeId,
    expected_port_kind: PortKind,
) -> Result<ValidatedInsertNodeSpec, ConnectPlan> {
    if inserted.node_id == source_node || inserted.node_id == target_node {
        return Err(ConnectPlan::reject(
            "inserted node id must be distinct from endpoints",
        ));
    }
    if graph.nodes.contains_key(&inserted.node_id) {
        return Err(ConnectPlan::reject(format!(
            "node already exists: {:?}",
            inserted.node_id
        )));
    }
    for (port_id, _) in &inserted.ports {
        if graph.ports.contains_key(port_id) {
            return Err(ConnectPlan::reject(format!(
                "port already exists: {port_id:?}"
            )));
        }
    }

    if inserted.input == inserted.output {
        return Err(ConnectPlan::reject(
            "inserted input/output ports must be distinct",
        ));
    }

    let mut inserted_in: Option<&Port> = None;
    let mut inserted_out: Option<&Port> = None;
    for (port_id, port) in &inserted.ports {
        if port.node != inserted.node_id {
            return Err(ConnectPlan::reject(format!(
                "inserted port has wrong node: port={port_id:?} expected={:?} got={:?}",
                inserted.node_id, port.node
            )));
        }
        if port.kind != expected_port_kind {
            return Err(ConnectPlan::reject(format!(
                "inserted port kind is incompatible: port={port_id:?} kind={:?} expected={:?}",
                port.kind, expected_port_kind
            )));
        }
        if *port_id == inserted.input {
            inserted_in = Some(port);
        }
        if *port_id == inserted.output {
            inserted_out = Some(port);
        }
    }

    let Some(inserted_in) = inserted_in else {
        return Err(ConnectPlan::reject(
            "inserted input port is missing from spec",
        ));
    };
    let Some(inserted_out) = inserted_out else {
        return Err(ConnectPlan::reject(
            "inserted output port is missing from spec",
        ));
    };

    if inserted_in.dir != PortDirection::In || inserted_out.dir != PortDirection::Out {
        return Err(ConnectPlan::reject("inserted ports must be in -> out"));
    }

    Ok(ValidatedInsertNodeSpec {
        input: inserted.input,
        output: inserted.output,
    })
}

pub(super) fn resolve_connection_endpoints(
    graph: &Graph,
    a: PortId,
    b: PortId,
    mode: NodeGraphConnectionMode,
) -> Result<ConnectionEndpoints<'_>, ConnectPlan> {
    if a == b {
        return Err(ConnectPlan::reject("cannot connect a port to itself"));
    }

    let Some(port_a) = graph.ports.get(&a) else {
        return Err(ConnectPlan::reject(format!("missing port: {a:?}")));
    };
    let Some(port_b) = graph.ports.get(&b) else {
        return Err(ConnectPlan::reject(format!("missing port: {b:?}")));
    };

    let (from_id, to_id) = match mode {
        NodeGraphConnectionMode::Strict => match (port_a.dir, port_b.dir) {
            (PortDirection::Out, PortDirection::In) => (a, b),
            (PortDirection::In, PortDirection::Out) => (b, a),
            _ => {
                return Err(ConnectPlan::reject(
                    "ports must have opposite directions (in/out)",
                ));
            }
        },
        NodeGraphConnectionMode::Loose => match port_a.dir {
            PortDirection::Out => (a, b),
            PortDirection::In => (b, a),
        },
    };

    let Some(from) = graph.ports.get(&from_id) else {
        return Err(ConnectPlan::reject(format!("missing port: {from_id:?}")));
    };
    let Some(to) = graph.ports.get(&to_id) else {
        return Err(ConnectPlan::reject(format!("missing port: {to_id:?}")));
    };

    if from.kind != to.kind {
        return Err(ConnectPlan::reject(format!(
            "port kinds are incompatible: from={:?} to={:?}",
            from.kind, to.kind
        )));
    }

    let Some(edge_kind) = edge_kind_for_port_kind(from.kind) else {
        return Err(ConnectPlan::reject("port kinds are incompatible"));
    };

    Ok(ConnectionEndpoints {
        from_id,
        to_id,
        from,
        to,
        edge_kind,
    })
}

pub(super) fn disconnect_for_capacity(
    graph: &Graph,
    edge_kind: EdgeKind,
    from_id: PortId,
    from_capacity: PortCapacity,
    to_id: PortId,
    to_capacity: PortCapacity,
    skip_edge: Option<EdgeId>,
) -> Vec<GraphOp> {
    let mut ops: Vec<GraphOp> = Vec::new();

    if from_capacity == PortCapacity::Single {
        for (edge_id, edge) in graph.edges.iter() {
            if Some(*edge_id) == skip_edge {
                continue;
            }
            if edge.kind == edge_kind && edge.from == from_id {
                ops.push(remove_edge_op(graph, *edge_id));
            }
        }
    }

    if to_capacity == PortCapacity::Single {
        for (edge_id, edge) in graph.edges.iter() {
            if Some(*edge_id) == skip_edge {
                continue;
            }
            if edge.kind == edge_kind && edge.to == to_id {
                ops.push(remove_edge_op(graph, *edge_id));
            }
        }
    }

    ops
}

fn port_policy_or_reject(
    graph: &Graph,
    port_id: PortId,
    state: &NodeGraphInteractionState,
) -> Result<NodeGraphPortInteractionPolicy, ConnectPlan> {
    let Some(port) = graph.ports.get(&port_id) else {
        return Err(ConnectPlan::reject(format!("missing port: {port_id:?}")));
    };
    let Some(node) = graph.nodes.get(&port.node) else {
        return Err(ConnectPlan::reject(format!(
            "missing port owner node: {:?}",
            port.node
        )));
    };
    Ok(resolve_port_interaction_policy(node, port, state))
}

pub(super) fn reject_if_connection_policy_disallows(
    graph: &Graph,
    from_id: PortId,
    to_id: PortId,
    state: &NodeGraphInteractionState,
) -> Option<ConnectPlan> {
    let from_policy = match port_policy_or_reject(graph, from_id, state) {
        Ok(policy) => policy,
        Err(plan) => return Some(plan),
    };
    if !from_policy.connectable_start {
        return Some(ConnectPlan::reject("source port is not connectable"));
    }

    let to_policy = match port_policy_or_reject(graph, to_id, state) {
        Ok(policy) => policy,
        Err(plan) => return Some(plan),
    };
    if !to_policy.connectable_end {
        return Some(ConnectPlan::reject("target port is not connectable"));
    }

    None
}
