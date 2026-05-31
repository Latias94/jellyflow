use crate::runtime::xyflow::changes::{NodeChange, NodeGraphChanges};
use jellyflow_core::ops::GraphOp;

use super::edges::push_removed_edge_changes;

pub(super) fn push_node_change(op: &GraphOp, out: &mut NodeGraphChanges) {
    match op {
        GraphOp::AddNode { id, node } => out.nodes.push(NodeChange::Add {
            id: *id,
            node: node.clone(),
        }),
        GraphOp::RemoveNode { id, edges, .. } => {
            out.nodes.push(NodeChange::Remove { id: *id });
            push_removed_edge_changes(edges, &mut out.edges);
        }
        GraphOp::SetNodePos { id, to, .. } => out.nodes.push(NodeChange::Position {
            id: *id,
            position: *to,
        }),
        GraphOp::SetNodeKind { id, to, .. } => out.nodes.push(NodeChange::Kind {
            id: *id,
            kind: to.clone(),
        }),
        GraphOp::SetNodeKindVersion { id, to, .. } => out.nodes.push(NodeChange::KindVersion {
            id: *id,
            kind_version: *to,
        }),
        GraphOp::SetNodeSelectable { id, to, .. } => out.nodes.push(NodeChange::Selectable {
            id: *id,
            selectable: *to,
        }),
        GraphOp::SetNodeDraggable { id, to, .. } => out.nodes.push(NodeChange::Draggable {
            id: *id,
            draggable: *to,
        }),
        GraphOp::SetNodeConnectable { id, to, .. } => out.nodes.push(NodeChange::Connectable {
            id: *id,
            connectable: *to,
        }),
        GraphOp::SetNodeDeletable { id, to, .. } => out.nodes.push(NodeChange::Deletable {
            id: *id,
            deletable: *to,
        }),
        GraphOp::SetNodeParent { id, to, .. } => out.nodes.push(NodeChange::Parent {
            id: *id,
            parent: *to,
        }),
        GraphOp::SetNodeExtent { id, to, .. } => out.nodes.push(NodeChange::Extent {
            id: *id,
            extent: *to,
        }),
        GraphOp::SetNodeExpandParent { id, to, .. } => out.nodes.push(NodeChange::ExpandParent {
            id: *id,
            expand_parent: *to,
        }),
        GraphOp::SetNodeSize { id, to, .. } => {
            out.nodes.push(NodeChange::Size { id: *id, size: *to })
        }
        GraphOp::SetNodeHidden { id, to, .. } => out.nodes.push(NodeChange::Hidden {
            id: *id,
            hidden: *to,
        }),
        GraphOp::SetNodeCollapsed { id, to, .. } => out.nodes.push(NodeChange::Collapsed {
            id: *id,
            collapsed: *to,
        }),
        GraphOp::SetNodeData { id, to, .. } => out.nodes.push(NodeChange::Data {
            id: *id,
            data: to.clone(),
        }),
        GraphOp::SetNodePorts { id, to, .. } => out.nodes.push(NodeChange::Ports {
            id: *id,
            ports: to.clone(),
        }),
        GraphOp::RemoveGroup { detached, .. } => {
            for (node_id, _previous_parent) in detached {
                out.nodes.push(NodeChange::Parent {
                    id: *node_id,
                    parent: None,
                });
            }
        }
        _ => unreachable!("node projection called with non-node graph operation"),
    }
}
