use crate::core::{Edge, EdgeId, Graph, Port, PortId};

use super::ApplyError;

pub(super) fn remove_edge_exact(
    graph: &mut Graph,
    id: EdgeId,
    expected: &Edge,
) -> Result<(), ApplyError> {
    let Some(current) = graph.edges.get(&id) else {
        return Err(ApplyError::MissingEdge { id });
    };
    if current.kind != expected.kind || current.from != expected.from || current.to != expected.to {
        return Err(ApplyError::RemoveEdgeMismatch { id });
    }
    graph.edges.remove(&id);
    Ok(())
}

pub(super) fn remove_port_exact(
    graph: &mut Graph,
    id: PortId,
    expected: &Port,
) -> Result<(), ApplyError> {
    let Some(current) = graph.ports.get(&id) else {
        return Err(ApplyError::MissingPort { id });
    };
    if current.node != expected.node || current.key != expected.key {
        return Err(ApplyError::RemovePortMismatch { id });
    }
    graph.ports.remove(&id);
    Ok(())
}
