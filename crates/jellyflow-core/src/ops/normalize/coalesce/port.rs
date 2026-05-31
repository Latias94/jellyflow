use crate::ops::GraphOp;

use super::coalesce_value;

pub(super) fn try_coalesce_port_setter(last: &mut GraphOp, next: &GraphOp) -> bool {
    match (last, next) {
        (
            GraphOp::SetPortConnectable {
                id: a, to: last_to, ..
            },
            GraphOp::SetPortConnectable { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetPortConnectableStart {
                id: a, to: last_to, ..
            },
            GraphOp::SetPortConnectableStart { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetPortConnectableEnd {
                id: a, to: last_to, ..
            },
            GraphOp::SetPortConnectableEnd { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetPortType {
                id: a, to: last_to, ..
            },
            GraphOp::SetPortType { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetPortData {
                id: a, to: last_to, ..
            },
            GraphOp::SetPortData { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        _ => false,
    }
}
