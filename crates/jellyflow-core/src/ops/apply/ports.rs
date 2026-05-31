use crate::core::{Edge, EdgeId, Graph, Port, PortId};
use crate::ops::GraphOp;

use super::ApplyError;
use super::resources::remove_edge_exact;

pub(super) fn apply_port_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddPort { id, port } => apply_add_port(graph, *id, port)?,
        GraphOp::RemovePort { id, port, edges } => apply_remove_port(graph, *id, port, edges)?,
        GraphOp::SetPortConnectable { id, to, .. } => {
            port_mut(graph, *id)?.connectable = *to;
        }
        GraphOp::SetPortConnectableStart { id, to, .. } => {
            port_mut(graph, *id)?.connectable_start = *to;
        }
        GraphOp::SetPortConnectableEnd { id, to, .. } => {
            port_mut(graph, *id)?.connectable_end = *to;
        }
        GraphOp::SetPortType { id, to, .. } => {
            port_mut(graph, *id)?.ty = to.clone();
        }
        GraphOp::SetPortData { id, to, .. } => {
            port_mut(graph, *id)?.data = to.clone();
        }
        _ => unreachable!("non-port op routed to port apply"),
    }
    Ok(())
}

fn apply_add_port(graph: &mut Graph, id: PortId, port: &Port) -> Result<(), ApplyError> {
    if graph.ports.contains_key(&id) {
        return Err(ApplyError::PortAlreadyExists { id });
    }
    if !graph.nodes.contains_key(&port.node) {
        return Err(ApplyError::MissingNode { id: port.node });
    }
    graph.ports.insert(id, port.clone());
    Ok(())
}

fn apply_remove_port(
    graph: &mut Graph,
    id: PortId,
    port: &Port,
    edges: &[(EdgeId, Edge)],
) -> Result<(), ApplyError> {
    let Some(current) = graph.ports.get(&id) else {
        return Err(ApplyError::MissingPort { id });
    };
    if current.node != port.node || current.key != port.key {
        return Err(ApplyError::RemovePortMismatch { id });
    }
    for (edge_id, edge) in edges {
        remove_edge_exact(graph, *edge_id, edge)?;
    }
    graph.ports.remove(&id);
    if let Some(node) = graph.nodes.get_mut(&port.node) {
        node.ports.retain(|p| *p != id);
    }
    Ok(())
}

fn port_mut(graph: &mut Graph, id: PortId) -> Result<&mut Port, ApplyError> {
    graph
        .ports
        .get_mut(&id)
        .ok_or(ApplyError::MissingPort { id })
}
