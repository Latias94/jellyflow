use crate::core::{Group, GroupId, NodeId};
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
            bindings: Vec::new(),
        }],
        GraphOp::RemoveGroup {
            id,
            group,
            detached,
            bindings,
        } => restore_removed_group(*id, group, detached, bindings),
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
            bindings: Vec::new(),
        }],
        GraphOp::RemoveStickyNote { id, note, bindings } => {
            let mut out = vec![GraphOp::AddStickyNote {
                id: *id,
                note: note.clone(),
            }];
            for (binding_id, binding) in bindings {
                out.push(GraphOp::AddBinding {
                    id: *binding_id,
                    binding: binding.clone(),
                });
            }
            out
        }
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

fn restore_removed_group(
    id: GroupId,
    group: &Group,
    detached: &[(NodeId, Option<GroupId>)],
    bindings: &[(crate::core::BindingId, crate::core::Binding)],
) -> Vec<GraphOp> {
    let mut out: Vec<GraphOp> = Vec::new();
    out.push(GraphOp::AddGroup {
        id,
        group: group.clone(),
    });
    for (node_id, parent) in detached {
        out.push(GraphOp::SetNodeParent {
            id: *node_id,
            from: None,
            to: *parent,
        });
    }
    for (binding_id, binding) in bindings {
        out.push(GraphOp::AddBinding {
            id: *binding_id,
            binding: binding.clone(),
        });
    }
    out
}
