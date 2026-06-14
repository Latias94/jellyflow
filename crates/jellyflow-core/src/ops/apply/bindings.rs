use crate::core::{Binding, BindingId, Graph};
use crate::ops::GraphOp;

use super::ApplyError;

pub(super) fn apply_binding_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddBinding { id, binding } => apply_add_binding(graph, *id, binding)?,
        GraphOp::RemoveBinding { id, binding } => remove_binding_exact(graph, *id, binding)?,
        GraphOp::SetBindingSubject { id, to, .. } => {
            binding_mut(graph, *id)?.subject = to.clone();
        }
        GraphOp::SetBindingTarget { id, to, .. } => {
            binding_mut(graph, *id)?.target = to.clone();
        }
        GraphOp::SetBindingKind { id, to, .. } => {
            binding_mut(graph, *id)?.kind = to.clone();
        }
        GraphOp::SetBindingMeta { id, to, .. } => {
            binding_mut(graph, *id)?.meta = to.clone();
        }
        _ => unreachable!("non-binding op routed to binding apply"),
    }
    Ok(())
}

pub(super) fn remove_binding_exact(
    graph: &mut Graph,
    id: BindingId,
    expected: &Binding,
) -> Result<(), ApplyError> {
    let Some(current) = graph.bindings().get(&id) else {
        return Err(ApplyError::MissingBinding { id });
    };
    if current != expected {
        return Err(ApplyError::RemoveBindingMismatch { id });
    }
    graph.remove_binding(&id);
    Ok(())
}

fn apply_add_binding(
    graph: &mut Graph,
    id: BindingId,
    binding: &Binding,
) -> Result<(), ApplyError> {
    if graph.bindings().contains_key(&id) {
        return Err(ApplyError::BindingAlreadyExists { id });
    }
    graph.insert_binding(id, binding.clone());
    Ok(())
}

fn binding_mut(graph: &mut Graph, id: BindingId) -> Result<&mut Binding, ApplyError> {
    graph
        .binding_mut(&id)
        .ok_or(ApplyError::MissingBinding { id })
}
