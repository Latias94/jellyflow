use super::super::GraphDiffPlanner;
use crate::ops::GraphOp;

impl<'a> GraphDiffPlanner<'a> {
    pub(crate) fn diff_sticky_notes(&mut self) {
        let from = self.from;
        let to = self.to;
        let tx = &mut self.tx;

        for (id, note_to) in &to.sticky_notes {
            if let Some(note_from) = from.sticky_notes.get(id) {
                if note_from.text != note_to.text {
                    tx.ops.push(GraphOp::SetStickyNoteText {
                        id: *id,
                        from: note_from.text.clone(),
                        to: note_to.text.clone(),
                    });
                }
                if note_from.rect != note_to.rect {
                    tx.ops.push(GraphOp::SetStickyNoteRect {
                        id: *id,
                        from: note_from.rect,
                        to: note_to.rect,
                    });
                }
                if note_from.color != note_to.color {
                    tx.ops.push(GraphOp::SetStickyNoteColor {
                        id: *id,
                        from: note_from.color.clone(),
                        to: note_to.color.clone(),
                    });
                }
            } else {
                tx.ops.push(GraphOp::AddStickyNote {
                    id: *id,
                    note: note_to.clone(),
                });
            }
        }

        for (id, note_from) in &from.sticky_notes {
            if !to.sticky_notes.contains_key(id) {
                tx.ops.push(GraphOp::RemoveStickyNote {
                    id: *id,
                    note: note_from.clone(),
                });
            }
        }
    }
}
