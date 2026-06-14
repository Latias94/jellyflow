use crate::core::Graph;
use crate::ops::GraphOp;

use super::ApplyError;
use super::resources::remove_bindings_exact;

pub(super) fn apply_sticky_note_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddStickyNote { id, note } => {
            if graph.sticky_notes().contains_key(id) {
                return Err(ApplyError::StickyNoteAlreadyExists { id: *id });
            }
            graph.insert_sticky_note(*id, note.clone());
        }
        GraphOp::RemoveStickyNote { id, note, bindings } => {
            let Some(current) = graph.sticky_notes().get(id) else {
                return Err(ApplyError::MissingStickyNote { id: *id });
            };
            if current.text != note.text || current.rect != note.rect || current.color != note.color
            {
                return Err(ApplyError::RemoveStickyNoteMismatch { id: *id });
            }
            remove_bindings_exact(graph, bindings)?;
            graph.remove_sticky_note(id);
        }
        GraphOp::SetStickyNoteText { id, to, .. } => {
            let Some(note) = graph.sticky_note_mut(id) else {
                return Err(ApplyError::MissingStickyNote { id: *id });
            };
            note.text = to.clone();
        }
        GraphOp::SetStickyNoteRect { id, to, .. } => {
            let Some(note) = graph.sticky_note_mut(id) else {
                return Err(ApplyError::MissingStickyNote { id: *id });
            };
            note.rect = *to;
        }
        GraphOp::SetStickyNoteColor { id, to, .. } => {
            let Some(note) = graph.sticky_note_mut(id) else {
                return Err(ApplyError::MissingStickyNote { id: *id });
            };
            note.color = to.clone();
        }
        _ => unreachable!("non-sticky-note op routed to sticky-note apply"),
    }
    Ok(())
}
