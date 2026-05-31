use crate::core::Graph;
use crate::ops::GraphOp;

use super::ApplyError;
use super::resources::remove_edge_exact;

pub(super) fn apply_edge_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddEdge { id, edge } => {
            if graph.edges.contains_key(id) {
                return Err(ApplyError::EdgeAlreadyExists { id: *id });
            }
            if !graph.ports.contains_key(&edge.from) {
                return Err(ApplyError::EdgeMissingPort {
                    edge: *id,
                    port: edge.from,
                });
            }
            if !graph.ports.contains_key(&edge.to) {
                return Err(ApplyError::EdgeMissingPort {
                    edge: *id,
                    port: edge.to,
                });
            }
            graph.edges.insert(*id, edge.clone());
        }
        GraphOp::RemoveEdge { id, edge } => {
            remove_edge_exact(graph, *id, edge)?;
        }
        GraphOp::SetEdgeKind { id, to, .. } => {
            let Some(edge) = graph.edges.get_mut(id) else {
                return Err(ApplyError::MissingEdge { id: *id });
            };
            edge.kind = *to;
        }
        GraphOp::SetEdgeSelectable { id, to, .. } => {
            let Some(edge) = graph.edges.get_mut(id) else {
                return Err(ApplyError::MissingEdge { id: *id });
            };
            edge.selectable = *to;
        }
        GraphOp::SetEdgeDeletable { id, to, .. } => {
            let Some(edge) = graph.edges.get_mut(id) else {
                return Err(ApplyError::MissingEdge { id: *id });
            };
            edge.deletable = *to;
        }
        GraphOp::SetEdgeReconnectable { id, to, .. } => {
            let Some(edge) = graph.edges.get_mut(id) else {
                return Err(ApplyError::MissingEdge { id: *id });
            };
            edge.reconnectable = *to;
        }
        GraphOp::SetEdgeEndpoints { id, to, .. } => {
            let Some(edge) = graph.edges.get_mut(id) else {
                return Err(ApplyError::MissingEdge { id: *id });
            };
            if !graph.ports.contains_key(&to.from) {
                return Err(ApplyError::EdgeMissingPort {
                    edge: *id,
                    port: to.from,
                });
            }
            if !graph.ports.contains_key(&to.to) {
                return Err(ApplyError::EdgeMissingPort {
                    edge: *id,
                    port: to.to,
                });
            }
            edge.from = to.from;
            edge.to = to.to;
        }
        _ => unreachable!("non-edge op routed to edge apply"),
    }
    Ok(())
}
