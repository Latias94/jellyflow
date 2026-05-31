use crate::runtime::xyflow::changes::{NodeChange, NodeGraphChanges};
use jellyflow_core::ops::GraphOp;

use super::edges::push_removed_edge_changes;

pub(super) fn try_push_node_change(op: &GraphOp, out: &mut NodeGraphChanges) -> bool {
    match op {
        GraphOp::AddNode { id, node } => {
            out.push_node(NodeChange::Add {
                id: *id,
                node: node.clone(),
            });
        }
        GraphOp::RemoveNode { id, edges, .. } => {
            out.push_node(NodeChange::Remove { id: *id });
            push_removed_edge_changes(edges, out);
        }
        GraphOp::SetNodePos { id, to, .. } => out.push_node(NodeChange::Position {
            id: *id,
            position: *to,
        }),
        GraphOp::SetNodeKind { id, to, .. } => out.push_node(NodeChange::Kind {
            id: *id,
            kind: to.clone(),
        }),
        GraphOp::SetNodeKindVersion { id, to, .. } => out.push_node(NodeChange::KindVersion {
            id: *id,
            kind_version: *to,
        }),
        GraphOp::SetNodeSelectable { id, to, .. } => out.push_node(NodeChange::Selectable {
            id: *id,
            selectable: *to,
        }),
        GraphOp::SetNodeDraggable { id, to, .. } => out.push_node(NodeChange::Draggable {
            id: *id,
            draggable: *to,
        }),
        GraphOp::SetNodeConnectable { id, to, .. } => out.push_node(NodeChange::Connectable {
            id: *id,
            connectable: *to,
        }),
        GraphOp::SetNodeDeletable { id, to, .. } => out.push_node(NodeChange::Deletable {
            id: *id,
            deletable: *to,
        }),
        GraphOp::SetNodeParent { id, to, .. } => out.push_node(NodeChange::Parent {
            id: *id,
            parent: *to,
        }),
        GraphOp::SetNodeExtent { id, to, .. } => out.push_node(NodeChange::Extent {
            id: *id,
            extent: *to,
        }),
        GraphOp::SetNodeExpandParent { id, to, .. } => out.push_node(NodeChange::ExpandParent {
            id: *id,
            expand_parent: *to,
        }),
        GraphOp::SetNodeSize { id, to, .. } => {
            out.push_node(NodeChange::Size { id: *id, size: *to })
        }
        GraphOp::SetNodeHidden { id, to, .. } => out.push_node(NodeChange::Hidden {
            id: *id,
            hidden: *to,
        }),
        GraphOp::SetNodeCollapsed { id, to, .. } => out.push_node(NodeChange::Collapsed {
            id: *id,
            collapsed: *to,
        }),
        GraphOp::SetNodeData { id, to, .. } => out.push_node(NodeChange::Data {
            id: *id,
            data: to.clone(),
        }),
        GraphOp::SetNodePorts { id, to, .. } => out.push_node(NodeChange::Ports {
            id: *id,
            ports: to.clone(),
        }),
        GraphOp::RemoveGroup { detached, .. } => {
            for (node_id, _previous_parent) in detached {
                out.push_node(NodeChange::Parent {
                    id: *node_id,
                    parent: None,
                });
            }
        }
        _ => return false,
    }
    true
}
