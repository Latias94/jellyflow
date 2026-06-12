use super::super::GraphDiffPlanner;
use crate::core::{Binding, BindingId};
use crate::ops::GraphOp;

impl<'a> GraphDiffPlanner<'a> {
    pub(crate) fn diff_bindings(&mut self) {
        let from = self.from;
        let to = self.to;

        for (id, binding_to) in &to.bindings {
            if self.removed_bindings_by_cascade.contains(id) {
                self.push_op(GraphOp::AddBinding {
                    id: *id,
                    binding: binding_to.clone(),
                });
                continue;
            }

            if let Some(binding_from) = from.bindings.get(id) {
                self.diff_existing_binding(*id, binding_from, binding_to);
            } else {
                self.push_op(GraphOp::AddBinding {
                    id: *id,
                    binding: binding_to.clone(),
                });
            }
        }

        for (id, binding_from) in &from.bindings {
            if to.bindings.contains_key(id) || self.removed_bindings_by_cascade.contains(id) {
                continue;
            }
            self.push_op(GraphOp::RemoveBinding {
                id: *id,
                binding: binding_from.clone(),
            });
        }
    }

    fn diff_existing_binding(
        &mut self,
        id: BindingId,
        binding_from: &Binding,
        binding_to: &Binding,
    ) {
        if binding_from.subject != binding_to.subject {
            self.push_op(GraphOp::SetBindingSubject {
                id,
                from: binding_from.subject.clone(),
                to: binding_to.subject.clone(),
            });
        }
        if binding_from.target != binding_to.target {
            self.push_op(GraphOp::SetBindingTarget {
                id,
                from: binding_from.target.clone(),
                to: binding_to.target.clone(),
            });
        }
        if binding_from.kind != binding_to.kind {
            self.push_op(GraphOp::SetBindingKind {
                id,
                from: binding_from.kind.clone(),
                to: binding_to.kind.clone(),
            });
        }
        if binding_from.meta != binding_to.meta {
            self.push_op(GraphOp::SetBindingMeta {
                id,
                from: binding_from.meta.clone(),
                to: binding_to.meta.clone(),
            });
        }
    }
}
