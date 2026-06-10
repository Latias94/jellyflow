use crate::runtime::xyflow::changes::NodeChange;
use jellyflow_core::core::{Node, NodeId};
use jellyflow_core::ops::GraphOp;

pub(in crate::runtime::xyflow) fn node_update_id(change: &NodeChange) -> Option<NodeId> {
    match change {
        NodeChange::Add { .. } | NodeChange::Remove { .. } => None,
        NodeChange::Position { id, .. }
        | NodeChange::Origin { id, .. }
        | NodeChange::Kind { id, .. }
        | NodeChange::KindVersion { id, .. }
        | NodeChange::Selectable { id, .. }
        | NodeChange::Focusable { id, .. }
        | NodeChange::Draggable { id, .. }
        | NodeChange::Connectable { id, .. }
        | NodeChange::Deletable { id, .. }
        | NodeChange::Parent { id, .. }
        | NodeChange::Extent { id, .. }
        | NodeChange::ExpandParent { id, .. }
        | NodeChange::Size { id, .. }
        | NodeChange::Hidden { id, .. }
        | NodeChange::Collapsed { id, .. }
        | NodeChange::Data { id, .. }
        | NodeChange::Ports { id, .. } => Some(*id),
    }
}

pub(in crate::runtime::xyflow) fn apply_node_update_change(
    change: &NodeChange,
    node: &mut Node,
) -> bool {
    match change {
        NodeChange::Add { .. } | NodeChange::Remove { .. } => false,
        NodeChange::Position { position, .. } => {
            node.pos = *position;
            true
        }
        NodeChange::Origin { origin, .. } => {
            node.origin = *origin;
            true
        }
        NodeChange::Kind { kind, .. } => {
            node.kind = kind.clone();
            true
        }
        NodeChange::KindVersion { kind_version, .. } => {
            node.kind_version = *kind_version;
            true
        }
        NodeChange::Selectable { selectable, .. } => {
            node.selectable = *selectable;
            true
        }
        NodeChange::Focusable { focusable, .. } => {
            node.focusable = *focusable;
            true
        }
        NodeChange::Draggable { draggable, .. } => {
            node.draggable = *draggable;
            true
        }
        NodeChange::Connectable { connectable, .. } => {
            node.connectable = *connectable;
            true
        }
        NodeChange::Deletable { deletable, .. } => {
            node.deletable = *deletable;
            true
        }
        NodeChange::Parent { parent, .. } => {
            node.parent = *parent;
            true
        }
        NodeChange::Extent { extent, .. } => {
            node.extent = *extent;
            true
        }
        NodeChange::ExpandParent { expand_parent, .. } => {
            node.expand_parent = *expand_parent;
            true
        }
        NodeChange::Size { size, .. } => {
            node.size = *size;
            true
        }
        NodeChange::Hidden { hidden, .. } => {
            node.hidden = *hidden;
            true
        }
        NodeChange::Collapsed { collapsed, .. } => {
            node.collapsed = *collapsed;
            true
        }
        NodeChange::Data { data, .. } => {
            node.data = data.clone();
            true
        }
        NodeChange::Ports { ports, .. } => {
            node.ports = ports.clone();
            true
        }
    }
}

pub(in crate::runtime::xyflow) fn node_update_op(
    change: &NodeChange,
    node: &Node,
) -> Option<GraphOp> {
    Some(match change {
        NodeChange::Add { .. } | NodeChange::Remove { .. } => return None,
        NodeChange::Position { id, position } => GraphOp::SetNodePos {
            id: *id,
            from: node.pos,
            to: *position,
        },
        NodeChange::Origin { id, origin } => GraphOp::SetNodeOrigin {
            id: *id,
            from: node.origin,
            to: *origin,
        },
        NodeChange::Kind { id, kind } => GraphOp::SetNodeKind {
            id: *id,
            from: node.kind.clone(),
            to: kind.clone(),
        },
        NodeChange::KindVersion { id, kind_version } => GraphOp::SetNodeKindVersion {
            id: *id,
            from: node.kind_version,
            to: *kind_version,
        },
        NodeChange::Selectable { id, selectable } => GraphOp::SetNodeSelectable {
            id: *id,
            from: node.selectable,
            to: *selectable,
        },
        NodeChange::Focusable { id, focusable } => GraphOp::SetNodeFocusable {
            id: *id,
            from: node.focusable,
            to: *focusable,
        },
        NodeChange::Draggable { id, draggable } => GraphOp::SetNodeDraggable {
            id: *id,
            from: node.draggable,
            to: *draggable,
        },
        NodeChange::Connectable { id, connectable } => GraphOp::SetNodeConnectable {
            id: *id,
            from: node.connectable,
            to: *connectable,
        },
        NodeChange::Deletable { id, deletable } => GraphOp::SetNodeDeletable {
            id: *id,
            from: node.deletable,
            to: *deletable,
        },
        NodeChange::Parent { id, parent } => GraphOp::SetNodeParent {
            id: *id,
            from: node.parent,
            to: *parent,
        },
        NodeChange::Extent { id, extent } => GraphOp::SetNodeExtent {
            id: *id,
            from: node.extent,
            to: *extent,
        },
        NodeChange::ExpandParent { id, expand_parent } => GraphOp::SetNodeExpandParent {
            id: *id,
            from: node.expand_parent,
            to: *expand_parent,
        },
        NodeChange::Size { id, size } => GraphOp::SetNodeSize {
            id: *id,
            from: node.size,
            to: *size,
        },
        NodeChange::Hidden { id, hidden } => GraphOp::SetNodeHidden {
            id: *id,
            from: node.hidden,
            to: *hidden,
        },
        NodeChange::Collapsed { id, collapsed } => GraphOp::SetNodeCollapsed {
            id: *id,
            from: node.collapsed,
            to: *collapsed,
        },
        NodeChange::Data { id, data } => GraphOp::SetNodeData {
            id: *id,
            from: node.data.clone(),
            to: data.clone(),
        },
        NodeChange::Ports { id, ports } => GraphOp::SetNodePorts {
            id: *id,
            from: node.ports.clone(),
            to: ports.clone(),
        },
    })
}

pub(in crate::runtime::xyflow) fn node_update_change_from_op(op: &GraphOp) -> Option<NodeChange> {
    Some(match op {
        GraphOp::SetNodePos { id, to, .. } => NodeChange::Position {
            id: *id,
            position: *to,
        },
        GraphOp::SetNodeOrigin { id, to, .. } => NodeChange::Origin {
            id: *id,
            origin: *to,
        },
        GraphOp::SetNodeKind { id, to, .. } => NodeChange::Kind {
            id: *id,
            kind: to.clone(),
        },
        GraphOp::SetNodeKindVersion { id, to, .. } => NodeChange::KindVersion {
            id: *id,
            kind_version: *to,
        },
        GraphOp::SetNodeSelectable { id, to, .. } => NodeChange::Selectable {
            id: *id,
            selectable: *to,
        },
        GraphOp::SetNodeFocusable { id, to, .. } => NodeChange::Focusable {
            id: *id,
            focusable: *to,
        },
        GraphOp::SetNodeDraggable { id, to, .. } => NodeChange::Draggable {
            id: *id,
            draggable: *to,
        },
        GraphOp::SetNodeConnectable { id, to, .. } => NodeChange::Connectable {
            id: *id,
            connectable: *to,
        },
        GraphOp::SetNodeDeletable { id, to, .. } => NodeChange::Deletable {
            id: *id,
            deletable: *to,
        },
        GraphOp::SetNodeParent { id, to, .. } => NodeChange::Parent {
            id: *id,
            parent: *to,
        },
        GraphOp::SetNodeExtent { id, to, .. } => NodeChange::Extent {
            id: *id,
            extent: *to,
        },
        GraphOp::SetNodeExpandParent { id, to, .. } => NodeChange::ExpandParent {
            id: *id,
            expand_parent: *to,
        },
        GraphOp::SetNodeSize { id, to, .. } => NodeChange::Size { id: *id, size: *to },
        GraphOp::SetNodeHidden { id, to, .. } => NodeChange::Hidden {
            id: *id,
            hidden: *to,
        },
        GraphOp::SetNodeCollapsed { id, to, .. } => NodeChange::Collapsed {
            id: *id,
            collapsed: *to,
        },
        GraphOp::SetNodeData { id, to, .. } => NodeChange::Data {
            id: *id,
            data: to.clone(),
        },
        GraphOp::SetNodePorts { id, to, .. } => NodeChange::Ports {
            id: *id,
            ports: to.clone(),
        },
        _ => return None,
    })
}
