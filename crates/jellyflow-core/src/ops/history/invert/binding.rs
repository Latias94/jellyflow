use crate::ops::GraphOp;

pub(super) fn invert_binding_op(op: &GraphOp) -> Vec<GraphOp> {
    match op {
        GraphOp::AddBinding { id, binding } => vec![GraphOp::RemoveBinding {
            id: *id,
            binding: binding.clone(),
        }],
        GraphOp::RemoveBinding { id, binding } => vec![GraphOp::AddBinding {
            id: *id,
            binding: binding.clone(),
        }],
        GraphOp::SetBindingSubject { id, from, to } => vec![GraphOp::SetBindingSubject {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        GraphOp::SetBindingTarget { id, from, to } => vec![GraphOp::SetBindingTarget {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        GraphOp::SetBindingKind { id, from, to } => vec![GraphOp::SetBindingKind {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        GraphOp::SetBindingMeta { id, from, to } => vec![GraphOp::SetBindingMeta {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        _ => unreachable!("binding invert handler received non-binding operation"),
    }
}
