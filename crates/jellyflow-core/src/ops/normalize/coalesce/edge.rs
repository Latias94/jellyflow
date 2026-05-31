use crate::ops::GraphOp;

pub(super) fn try_coalesce_edge_setter(last: &mut GraphOp, next: &GraphOp) -> bool {
    match (last, next) {
        (
            GraphOp::SetEdgeKind {
                id: a, to: last_to, ..
            },
            GraphOp::SetEdgeKind { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetEdgeSelectable {
                id: a, to: last_to, ..
            },
            GraphOp::SetEdgeSelectable { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetEdgeDeletable {
                id: a, to: last_to, ..
            },
            GraphOp::SetEdgeDeletable { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetEdgeReconnectable {
                id: a, to: last_to, ..
            },
            GraphOp::SetEdgeReconnectable { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetEdgeEndpoints {
                id: a, to: last_to, ..
            },
            GraphOp::SetEdgeEndpoints { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        _ => false,
    }
}
