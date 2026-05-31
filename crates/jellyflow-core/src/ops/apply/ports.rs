use crate::core::Graph;
use crate::ops::GraphOp;

use super::ApplyError;
use super::resources::remove_edge_exact;

pub(super) fn apply_port_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddPort { id, port } => {
            if graph.ports.contains_key(id) {
                return Err(ApplyError::PortAlreadyExists { id: *id });
            }
            if !graph.nodes.contains_key(&port.node) {
                return Err(ApplyError::MissingNode { id: port.node });
            }
            graph.ports.insert(*id, port.clone());
        }
        GraphOp::RemovePort { id, port, edges } => {
            let Some(current) = graph.ports.get(id) else {
                return Err(ApplyError::MissingPort { id: *id });
            };
            if current.node != port.node || current.key != port.key {
                return Err(ApplyError::RemovePortMismatch { id: *id });
            }
            for (edge_id, edge) in edges {
                remove_edge_exact(graph, *edge_id, edge)?;
            }
            graph.ports.remove(id);
            if let Some(node) = graph.nodes.get_mut(&port.node) {
                node.ports.retain(|p| p != id);
            }
        }
        GraphOp::SetPortConnectable { id, to, .. } => {
            let Some(port) = graph.ports.get_mut(id) else {
                return Err(ApplyError::MissingPort { id: *id });
            };
            port.connectable = *to;
        }
        GraphOp::SetPortConnectableStart { id, to, .. } => {
            let Some(port) = graph.ports.get_mut(id) else {
                return Err(ApplyError::MissingPort { id: *id });
            };
            port.connectable_start = *to;
        }
        GraphOp::SetPortConnectableEnd { id, to, .. } => {
            let Some(port) = graph.ports.get_mut(id) else {
                return Err(ApplyError::MissingPort { id: *id });
            };
            port.connectable_end = *to;
        }
        GraphOp::SetPortType { id, to, .. } => {
            let Some(port) = graph.ports.get_mut(id) else {
                return Err(ApplyError::MissingPort { id: *id });
            };
            port.ty = to.clone();
        }
        GraphOp::SetPortData { id, to, .. } => {
            let Some(port) = graph.ports.get_mut(id) else {
                return Err(ApplyError::MissingPort { id: *id });
            };
            port.data = to.clone();
        }
        _ => unreachable!("non-port op routed to port apply"),
    }
    Ok(())
}
