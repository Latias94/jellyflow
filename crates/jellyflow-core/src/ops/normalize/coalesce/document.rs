use crate::ops::GraphOp;

pub(super) fn try_coalesce_document_setter(last: &mut GraphOp, next: &GraphOp) -> bool {
    match (last, next) {
        (
            GraphOp::SetImportAlias {
                id: a, to: last_to, ..
            },
            GraphOp::SetImportAlias { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        (
            GraphOp::SetSymbolMeta {
                id: a, to: last_to, ..
            },
            GraphOp::SetSymbolMeta { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        (
            GraphOp::SetSymbolName {
                id: a, to: last_to, ..
            },
            GraphOp::SetSymbolName { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        (
            GraphOp::SetSymbolType {
                id: a, to: last_to, ..
            },
            GraphOp::SetSymbolType { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        (
            GraphOp::SetSymbolDefaultValue {
                id: a, to: last_to, ..
            },
            GraphOp::SetSymbolDefaultValue { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        (
            GraphOp::SetGroupRect {
                id: a, to: last_to, ..
            },
            GraphOp::SetGroupRect { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetGroupColor {
                id: a, to: last_to, ..
            },
            GraphOp::SetGroupColor { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        (
            GraphOp::SetStickyNoteText {
                id: a, to: last_to, ..
            },
            GraphOp::SetStickyNoteText { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        (
            GraphOp::SetStickyNoteRect {
                id: a, to: last_to, ..
            },
            GraphOp::SetStickyNoteRect { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = *to;
            true
        }
        (
            GraphOp::SetStickyNoteColor {
                id: a, to: last_to, ..
            },
            GraphOp::SetStickyNoteColor { id: b, from, to },
        ) if a == b && last_to == from => {
            *last_to = to.clone();
            true
        }
        _ => false,
    }
}
