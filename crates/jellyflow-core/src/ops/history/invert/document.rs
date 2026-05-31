use crate::ops::GraphOp;

pub(super) fn invert_document_op(op: &GraphOp) -> Vec<GraphOp> {
    match op {
        GraphOp::AddImport { id, import } => vec![GraphOp::RemoveImport {
            id: *id,
            import: import.clone(),
        }],
        GraphOp::RemoveImport { id, import } => vec![GraphOp::AddImport {
            id: *id,
            import: import.clone(),
        }],
        GraphOp::SetImportAlias { id, from, to } => vec![GraphOp::SetImportAlias {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],

        GraphOp::AddSymbol { id, symbol } => vec![GraphOp::RemoveSymbol {
            id: *id,
            symbol: symbol.clone(),
        }],
        GraphOp::RemoveSymbol { id, symbol } => vec![GraphOp::AddSymbol {
            id: *id,
            symbol: symbol.clone(),
        }],
        GraphOp::SetSymbolName { id, from, to } => vec![GraphOp::SetSymbolName {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        GraphOp::SetSymbolType { id, from, to } => vec![GraphOp::SetSymbolType {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        GraphOp::SetSymbolDefaultValue { id, from, to } => vec![GraphOp::SetSymbolDefaultValue {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        GraphOp::SetSymbolMeta { id, from, to } => vec![GraphOp::SetSymbolMeta {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],

        GraphOp::AddGroup { id, group } => vec![GraphOp::RemoveGroup {
            id: *id,
            group: group.clone(),
            detached: Vec::new(),
        }],
        GraphOp::RemoveGroup {
            id,
            group,
            detached,
        } => {
            let mut out: Vec<GraphOp> = Vec::new();
            out.push(GraphOp::AddGroup {
                id: *id,
                group: group.clone(),
            });
            for (node_id, parent) in detached {
                out.push(GraphOp::SetNodeParent {
                    id: *node_id,
                    from: None,
                    to: *parent,
                });
            }
            out
        }
        GraphOp::SetGroupRect { id, from, to } => vec![GraphOp::SetGroupRect {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetGroupTitle { id, from, to } => vec![GraphOp::SetGroupTitle {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        GraphOp::SetGroupColor { id, from, to } => vec![GraphOp::SetGroupColor {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],

        GraphOp::AddStickyNote { id, note } => vec![GraphOp::RemoveStickyNote {
            id: *id,
            note: note.clone(),
        }],
        GraphOp::RemoveStickyNote { id, note } => vec![GraphOp::AddStickyNote {
            id: *id,
            note: note.clone(),
        }],
        GraphOp::SetStickyNoteText { id, from, to } => vec![GraphOp::SetStickyNoteText {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        GraphOp::SetStickyNoteRect { id, from, to } => vec![GraphOp::SetStickyNoteRect {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetStickyNoteColor { id, from, to } => vec![GraphOp::SetStickyNoteColor {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        _ => unreachable!("document invert handler received element operation"),
    }
}
