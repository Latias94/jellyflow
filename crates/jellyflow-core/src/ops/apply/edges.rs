use crate::core::{Edge, EdgeId, Graph, PortId};
use crate::ops::GraphOp;

use super::ApplyError;
use super::resources::remove_edge_exact;

pub(super) fn apply_edge_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddEdge { id, edge } => apply_add_edge(graph, *id, edge)?,
        GraphOp::RemoveEdge { id, edge } => {
            remove_edge_exact(graph, *id, edge)?;
        }
        GraphOp::SetEdgeKind { id, to, .. } => {
            edge_mut(graph, *id)?.kind = *to;
        }
        GraphOp::SetEdgeSelectable { id, to, .. } => {
            edge_mut(graph, *id)?.selectable = *to;
        }
        GraphOp::SetEdgeFocusable { id, to, .. } => {
            edge_mut(graph, *id)?.focusable = *to;
        }
        GraphOp::SetEdgeHidden { id, to, .. } => {
            edge_mut(graph, *id)?.hidden = *to;
        }
        GraphOp::SetEdgeInteractionWidth { id, to, .. } => {
            edge_mut(graph, *id)?.interaction_width = *to;
        }
        GraphOp::SetEdgeDeletable { id, to, .. } => {
            edge_mut(graph, *id)?.deletable = *to;
        }
        GraphOp::SetEdgeReconnectable { id, to, .. } => {
            edge_mut(graph, *id)?.reconnectable = *to;
        }
        GraphOp::SetEdgeEndpoints { id, to, .. } => {
            ensure_edge_exists(graph, *id)?;
            ensure_edge_ports_exist(graph, *id, to.from, to.to)?;
            let edge = edge_mut(graph, *id)?;
            edge.from = to.from;
            edge.to = to.to;
        }
        _ => unreachable!("non-edge op routed to edge apply"),
    }
    Ok(())
}

fn apply_add_edge(graph: &mut Graph, id: EdgeId, edge: &Edge) -> Result<(), ApplyError> {
    if graph.edges.contains_key(&id) {
        return Err(ApplyError::EdgeAlreadyExists { id });
    }
    ensure_edge_ports_exist(graph, id, edge.from, edge.to)?;
    graph.edges.insert(id, edge.clone());
    Ok(())
}

fn ensure_edge_exists(graph: &Graph, id: EdgeId) -> Result<(), ApplyError> {
    if graph.edges.contains_key(&id) {
        Ok(())
    } else {
        Err(ApplyError::MissingEdge { id })
    }
}

fn ensure_edge_ports_exist(
    graph: &Graph,
    edge_id: EdgeId,
    from: PortId,
    to: PortId,
) -> Result<(), ApplyError> {
    if !graph.ports.contains_key(&from) {
        return Err(ApplyError::EdgeMissingPort {
            edge: edge_id,
            port: from,
        });
    }
    if !graph.ports.contains_key(&to) {
        return Err(ApplyError::EdgeMissingPort {
            edge: edge_id,
            port: to,
        });
    }
    Ok(())
}

fn edge_mut(graph: &mut Graph, id: EdgeId) -> Result<&mut Edge, ApplyError> {
    graph
        .edges
        .get_mut(&id)
        .ok_or(ApplyError::MissingEdge { id })
}
