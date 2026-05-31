use crate::ops::GraphOp;

pub(super) fn try_coalesce_node_setter(last: &mut GraphOp, next: &GraphOp) -> bool {
    match (last, next) {
        (
            GraphOp::SetNodePos {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodePos { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodeKind {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeKind { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        (
            GraphOp::SetNodeKindVersion {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeKindVersion { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodeSelectable {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeSelectable { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodeDraggable {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeDraggable { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodeConnectable {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeConnectable { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodeDeletable {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeDeletable { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodeParent {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeParent { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodeExtent {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeExtent { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodeExpandParent {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeExpandParent { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodeSize {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeSize { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodeHidden {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeHidden { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodeCollapsed {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeCollapsed { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetNodePorts {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodePorts { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        (
            GraphOp::SetNodeData {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeData { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        _ => false,
    }
}
