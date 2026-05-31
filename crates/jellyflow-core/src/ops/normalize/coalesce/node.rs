use crate::ops::GraphOp;

use super::coalesce_value;

pub(super) fn try_coalesce_node_setter(last: &mut GraphOp, next: &GraphOp) -> bool {
    match (last, next) {
        (
            GraphOp::SetNodePos {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodePos { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeKind {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeKind { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeKindVersion {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeKindVersion { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeSelectable {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeSelectable { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeDraggable {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeDraggable { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeConnectable {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeConnectable { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeDeletable {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeDeletable { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeParent {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeParent { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeExtent {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeExtent { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeExpandParent {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeExpandParent { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeSize {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeSize { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeHidden {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeHidden { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeCollapsed {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeCollapsed { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodePorts {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodePorts { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetNodeData {
                id: a, to: last_to, ..
            },
            GraphOp::SetNodeData { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        _ => false,
    }
}
