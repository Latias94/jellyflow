use super::super::GraphDiffPlanner;
use crate::core::{StickyNote, StickyNoteId};
use crate::ops::GraphOp;

impl<'a> GraphDiffPlanner<'a> {
    pub(crate) fn diff_sticky_notes(&mut self) {
        let from = self.from;
        let to = self.to;

        for (id, note_to) in &to.sticky_notes {
            if let Some(note_from) = from.sticky_notes.get(id) {
                self.diff_existing_sticky_note(*id, note_from, note_to);
            } else {
                self.tx.ops.push(GraphOp::AddStickyNote {
                    id: *id,
                    note: note_to.clone(),
                });
            }
        }

        for (id, note_from) in &from.sticky_notes {
            if !to.sticky_notes.contains_key(id) {
                self.tx.ops.push(GraphOp::RemoveStickyNote {
                    id: *id,
                    note: note_from.clone(),
                });
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
            self.tx.ops.push(GraphOp::SetStickyNoteText {
                id,
                from: note_from.text.clone(),
                to: note_to.text.clone(),
            });
        }
        if note_from.rect != note_to.rect {
            self.tx.ops.push(GraphOp::SetStickyNoteRect {
                id,
                from: note_from.rect,
                to: note_to.rect,
            });
        }
        if note_from.color != note_to.color {
            self.tx.ops.push(GraphOp::SetStickyNoteColor {
                id,
                from: note_from.color.clone(),
                to: note_to.color.clone(),
            });
        }
    }
}
