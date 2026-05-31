use crate::core::Graph;
use crate::ops::GraphOp;

use super::ApplyError;
use super::resources::{remove_edge_exact, remove_port_exact};

pub(super) fn apply_node_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddNode { id, node } => {
            if graph.nodes.contains_key(id) {
                return Err(ApplyError::NodeAlreadyExists { id: *id });
            }
            graph.nodes.insert(*id, node.clone());
        }
        GraphOp::RemoveNode {
            id,
            node,
            ports,
            edges,
        } => {
            let Some(current) = graph.nodes.get(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            if current.kind != node.kind || current.kind_version != node.kind_version {
                return Err(ApplyError::RemoveNodeMismatch { id: *id });
            }

            for (edge_id, edge) in edges {
                remove_edge_exact(graph, *edge_id, edge)?;
            }
            for (port_id, port) in ports {
                remove_port_exact(graph, *port_id, port)?;
            }

            graph.nodes.remove(id);
        }
        GraphOp::SetNodePos { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.pos = *to;
        }
        GraphOp::SetNodeKind { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.kind = to.clone();
        }
        GraphOp::SetNodeKindVersion { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.kind_version = *to;
        }
        GraphOp::SetNodeSelectable { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.selectable = *to;
        }
        GraphOp::SetNodeDraggable { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.draggable = *to;
        }
        GraphOp::SetNodeConnectable { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.connectable = *to;
        }
        GraphOp::SetNodeDeletable { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.deletable = *to;
        }
        GraphOp::SetNodeParent { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            if let Some(group) = to
                && !graph.groups.contains_key(group)
            {
                return Err(ApplyError::NodeParentMissingGroup {
                    node: *id,
                    group: *group,
                });
            }
            node.parent = *to;
        }
        GraphOp::SetNodeExtent { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.extent = *to;
        }
        GraphOp::SetNodeExpandParent { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.expand_parent = *to;
        }
        GraphOp::SetNodeSize { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.size = *to;
        }
        GraphOp::SetNodeHidden { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.hidden = *to;
        }
        GraphOp::SetNodeCollapsed { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.collapsed = *to;
        }
        GraphOp::SetNodePorts { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            for port_id in to {
                let Some(port) = graph.ports.get(port_id) else {
                    return Err(ApplyError::NodePortsUnknownPort {
                        node: *id,
                        port: *port_id,
                    });
                };
                if port.node != *id {
                    return Err(ApplyError::NodePortsUnknownPort {
                        node: *id,
                        port: *port_id,
                    });
                }
            }
            node.ports = to.clone();
        }
        GraphOp::SetNodeData { id, to, .. } => {
            let Some(node) = graph.nodes.get_mut(id) else {
                return Err(ApplyError::MissingNode { id: *id });
            };
            node.data = to.clone();
        }
        _ => unreachable!("non-node op routed to node apply"),
    }
    Ok(())
}
