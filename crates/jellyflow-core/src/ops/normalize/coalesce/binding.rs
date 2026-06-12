use crate::ops::GraphOp;

use super::coalesce_value;

pub(super) fn try_coalesce_binding_setter(last: &mut GraphOp, next: &GraphOp) -> bool {
    match (last, next) {
        (
            GraphOp::SetBindingSubject {
                id: a, to: last_to, ..
            },
            GraphOp::SetBindingSubject { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetBindingTarget {
                id: a, to: last_to, ..
            },
            GraphOp::SetBindingTarget { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetBindingKind {
                id: a, to: last_to, ..
            },
            GraphOp::SetBindingKind { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetBindingMeta {
                id: a, to: last_to, ..
            },
            GraphOp::SetBindingMeta { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        _ => false,
    }
}
