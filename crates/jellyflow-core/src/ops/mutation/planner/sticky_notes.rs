use crate::core::StickyNoteId;
use crate::ops::{GraphOp, GraphTransaction};

use super::GraphMutationPlanner;
use crate::ops::mutation::GraphMutationError;
use crate::ops::mutation::collect::bindings_for_sticky_note;

impl GraphMutationPlanner<'_> {
    pub fn remove_sticky_note_op(&self, id: StickyNoteId) -> Result<GraphOp, GraphMutationError> {
        let note = self
            .graph
            .sticky_notes
            .get(&id)
            .cloned()
            .ok_or(GraphMutationError::MissingStickyNote(id))?;

        Ok(GraphOp::RemoveStickyNote {
            id,
            note,
            bindings: bindings_for_sticky_note(self.graph, id),
        })
    }

    pub fn remove_sticky_note_tx(
        &self,
        id: StickyNoteId,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction::new()
            .with_label(label)
            .with_ops([self.remove_sticky_note_op(id)?]))
    }
}
