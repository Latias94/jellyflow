use crate::ops::GraphOp;

pub(super) fn try_coalesce_port_setter(last: &mut GraphOp, next: &GraphOp) -> bool {
    match (last, next) {
        (
            GraphOp::SetPortConnectable {
                id: a, to: last_to, ..
            },
            GraphOp::SetPortConnectable { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetPortConnectableStart {
                id: a, to: last_to, ..
            },
            GraphOp::SetPortConnectableStart { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetPortConnectableEnd {
                id: a, to: last_to, ..
            },
            GraphOp::SetPortConnectableEnd { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetPortType {
                id: a, to: last_to, ..
            },
            GraphOp::SetPortType { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        (
            GraphOp::SetPortData {
                id: a, to: last_to, ..
            },
            GraphOp::SetPortData { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        _ => false,
    }
}
