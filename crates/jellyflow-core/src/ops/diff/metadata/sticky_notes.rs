use super::super::GraphDiffPlanner;
use crate::core::{StickyNote, StickyNoteId};
use crate::ops::{GraphMutationPlanner, GraphOp};

impl<'a> GraphDiffPlanner<'a> {
    pub(crate) fn diff_sticky_notes(&mut self) {
        let from = self.from;
        let to = self.to;

        for (id, note_to) in &to.sticky_notes {
            if let Some(note_from) = from.sticky_notes.get(id) {
                self.diff_existing_sticky_note(*id, note_from, note_to);
            } else {
                self.push_op(GraphOp::AddStickyNote {
                    id: *id,
                    note: note_to.clone(),
                });
            }
        }

        for (id, note_from) in &from.sticky_notes {
            if !to.sticky_notes.contains_key(id) {
                let op = GraphMutationPlanner::new(from)
                    .remove_sticky_note_op(*id)
                    .unwrap_or_else(|_| GraphOp::RemoveStickyNote {
                        id: *id,
                        note: note_from.clone(),
                        bindings: Vec::new(),
                    });
                let op = self.with_target_removed_bindings(op);
                if let GraphOp::RemoveStickyNote { bindings, .. } = &op {
                    self.removed_bindings_by_cascade
                        .extend(bindings.iter().map(|(id, _)| *id));
                }
                self.push_op(op);
            }
        }
    }

    fn diff_existing_sticky_note(
        &mut self,
        id: StickyNoteId,
        note_from: &StickyNote,
        note_to: &StickyNote,
    ) {
        if note_from.text != note_to.text {
            self.push_op(GraphOp::SetStickyNoteText {
                id,
                from: note_from.text.clone(),
                to: note_to.text.clone(),
            });
        }
        if note_from.rect != note_to.rect {
            self.push_op(GraphOp::SetStickyNoteRect {
                id,
                from: note_from.rect,
                to: note_to.rect,
            });
        }
        if note_from.color != note_to.color {
            self.push_op(GraphOp::SetStickyNoteColor {
                id,
                from: note_from.color.clone(),
                to: note_to.color.clone(),
            });
        }
    }
}
