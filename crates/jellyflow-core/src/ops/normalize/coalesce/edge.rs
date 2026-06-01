use crate::ops::GraphOp;

use super::coalesce_value;

pub(super) fn try_coalesce_edge_setter(last: &mut GraphOp, next: &GraphOp) -> bool {
    match (last, next) {
        (
            GraphOp::SetEdgeKind {
                id: a, to: last_to, ..
            },
            GraphOp::SetEdgeKind { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetEdgeSelectable {
                id: a, to: last_to, ..
            },
            GraphOp::SetEdgeSelectable { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetEdgeFocusable {
                id: a, to: last_to, ..
            },
            GraphOp::SetEdgeFocusable { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetEdgeDeletable {
                id: a, to: last_to, ..
            },
            GraphOp::SetEdgeDeletable { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetEdgeReconnectable {
                id: a, to: last_to, ..
            },
            GraphOp::SetEdgeReconnectable { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetEdgeEndpoints {
                id: a, to: last_to, ..
            },
            GraphOp::SetEdgeEndpoints { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        _ => false,
    }
}
