use crate::core::{Edge, EdgeId, Graph, GroupId, Node, NodeId, Port, PortId};
use crate::ops::GraphOp;

use super::ApplyError;
use super::resources::{remove_edge_exact, remove_port_exact};

pub(super) fn apply_node_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddNode { id, node } => apply_add_node(graph, *id, node)?,
        GraphOp::RemoveNode {
            id,
            node,
            ports,
            edges,
        } => apply_remove_node(graph, *id, node, ports, edges)?,
        GraphOp::SetNodePos { id, to, .. } => {
            node_mut(graph, *id)?.pos = *to;
        }
        GraphOp::SetNodeKind { id, to, .. } => {
            node_mut(graph, *id)?.kind = to.clone();
        }
        GraphOp::SetNodeKindVersion { id, to, .. } => {
            node_mut(graph, *id)?.kind_version = *to;
        }
        GraphOp::SetNodeSelectable { id, to, .. } => {
            node_mut(graph, *id)?.selectable = *to;
        }
        GraphOp::SetNodeFocusable { id, to, .. } => {
            node_mut(graph, *id)?.focusable = *to;
        }
        GraphOp::SetNodeDraggable { id, to, .. } => {
            node_mut(graph, *id)?.draggable = *to;
        }
        GraphOp::SetNodeConnectable { id, to, .. } => {
            node_mut(graph, *id)?.connectable = *to;
        }
        GraphOp::SetNodeDeletable { id, to, .. } => {
            node_mut(graph, *id)?.deletable = *to;
        }
        GraphOp::SetNodeParent { id, to, .. } => {
            apply_set_node_parent(graph, *id, *to)?;
        }
        GraphOp::SetNodeExtent { id, to, .. } => {
            node_mut(graph, *id)?.extent = *to;
        }
        GraphOp::SetNodeExpandParent { id, to, .. } => {
            node_mut(graph, *id)?.expand_parent = *to;
        }
        GraphOp::SetNodeSize { id, to, .. } => {
            node_mut(graph, *id)?.size = *to;
        }
        GraphOp::SetNodeHidden { id, to, .. } => {
            node_mut(graph, *id)?.hidden = *to;
        }
        GraphOp::SetNodeCollapsed { id, to, .. } => {
            node_mut(graph, *id)?.collapsed = *to;
        }
        GraphOp::SetNodePorts { id, to, .. } => {
            apply_set_node_ports(graph, *id, to)?;
        }
        GraphOp::SetNodeData { id, to, .. } => {
            node_mut(graph, *id)?.data = to.clone();
        }
        _ => unreachable!("non-node op routed to node apply"),
    }
    Ok(())
}

fn apply_add_node(graph: &mut Graph, id: NodeId, node: &Node) -> Result<(), ApplyError> {
    if graph.nodes.contains_key(&id) {
        return Err(ApplyError::NodeAlreadyExists { id });
    }
    graph.nodes.insert(id, node.clone());
    Ok(())
}

fn apply_remove_node(
    graph: &mut Graph,
    id: NodeId,
    node: &Node,
    ports: &[(PortId, Port)],
    edges: &[(EdgeId, Edge)],
) -> Result<(), ApplyError> {
    let Some(current) = graph.nodes.get(&id) else {
        return Err(ApplyError::MissingNode { id });
    };
    if current.kind != node.kind || current.kind_version != node.kind_version {
        return Err(ApplyError::RemoveNodeMismatch { id });
    }

    for (edge_id, edge) in edges {
        remove_edge_exact(graph, *edge_id, edge)?;
    }
    for (port_id, port) in ports {
        remove_port_exact(graph, *port_id, port)?;
    }

    graph.nodes.remove(&id);
    Ok(())
}

fn apply_set_node_parent(
    graph: &mut Graph,
    id: NodeId,
    parent: Option<GroupId>,
) -> Result<(), ApplyError> {
    ensure_node_exists(graph, id)?;
    if let Some(group) = parent
        && !graph.groups.contains_key(&group)
    {
        return Err(ApplyError::NodeParentMissingGroup { node: id, group });
    }

    node_mut(graph, id)?.parent = parent;
    Ok(())
}

fn apply_set_node_ports(graph: &mut Graph, id: NodeId, ports: &[PortId]) -> Result<(), ApplyError> {
    ensure_node_exists(graph, id)?;
    for port_id in ports {
        let Some(port) = graph.ports.get(port_id) else {
            return Err(ApplyError::NodePortsUnknownPort {
                node: id,
                port: *port_id,
            });
        };
        if port.node != id {
            return Err(ApplyError::NodePortsUnknownPort {
                node: id,
                port: *port_id,
            });
        }
    }

    node_mut(graph, id)?.ports = ports.to_vec();
    Ok(())
}

fn ensure_node_exists(graph: &Graph, id: NodeId) -> Result<(), ApplyError> {
    if graph.nodes.contains_key(&id) {
        Ok(())
    } else {
        Err(ApplyError::MissingNode { id })
    }
}

fn node_mut(graph: &mut Graph, id: NodeId) -> Result<&mut Node, ApplyError> {
    graph
        .nodes
        .get_mut(&id)
        .ok_or(ApplyError::MissingNode { id })
}
