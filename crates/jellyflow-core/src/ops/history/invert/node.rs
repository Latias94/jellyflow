use crate::core::{Edge, EdgeId, Node, NodeId, Port, PortId};
use crate::ops::GraphOp;

pub(super) fn invert_node_op(op: &GraphOp) -> Vec<GraphOp> {
    match op {
        GraphOp::AddNode { id, node } => vec![GraphOp::RemoveNode {
            id: *id,
            node: node.clone(),
            ports: Vec::new(),
            edges: Vec::new(),
        }],
        GraphOp::RemoveNode {
            id,
            node,
            ports,
            edges,
        } => restore_removed_node(*id, node, ports, edges),
        GraphOp::SetNodePos { id, from, to } => vec![GraphOp::SetNodePos {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodeKind { id, from, to } => vec![GraphOp::SetNodeKind {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        GraphOp::SetNodeKindVersion { id, from, to } => vec![GraphOp::SetNodeKindVersion {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodeSelectable { id, from, to } => vec![GraphOp::SetNodeSelectable {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodeDraggable { id, from, to } => vec![GraphOp::SetNodeDraggable {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodeConnectable { id, from, to } => vec![GraphOp::SetNodeConnectable {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodeDeletable { id, from, to } => vec![GraphOp::SetNodeDeletable {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodeParent { id, from, to } => vec![GraphOp::SetNodeParent {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodeExtent { id, from, to } => vec![GraphOp::SetNodeExtent {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodeExpandParent { id, from, to } => vec![GraphOp::SetNodeExpandParent {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodeSize { id, from, to } => vec![GraphOp::SetNodeSize {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodeHidden { id, from, to } => vec![GraphOp::SetNodeHidden {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodeCollapsed { id, from, to } => vec![GraphOp::SetNodeCollapsed {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetNodePorts { id, from, to } => vec![GraphOp::SetNodePorts {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        GraphOp::SetNodeData { id, from, to } => vec![GraphOp::SetNodeData {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        _ => unreachable!("node invert handler received non-node operation"),
    }
}

fn restore_removed_node(
    id: NodeId,
    node: &Node,
    ports: &[(PortId, Port)],
    edges: &[(EdgeId, Edge)],
) -> Vec<GraphOp> {
    let mut out: Vec<GraphOp> = Vec::new();
    out.push(GraphOp::AddNode {
        id,
        node: node.clone(),
    });
    for (port_id, port) in ports {
        out.push(GraphOp::AddPort {
            id: *port_id,
            port: port.clone(),
        });
    }
    for (edge_id, edge) in edges {
        out.push(GraphOp::AddEdge {
            id: *edge_id,
            edge: edge.clone(),
        });
    }
    out
}
