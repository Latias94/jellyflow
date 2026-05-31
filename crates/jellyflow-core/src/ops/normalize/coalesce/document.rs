use crate::ops::GraphOp;

use super::coalesce_value;

pub(super) fn try_coalesce_document_setter(last: &mut GraphOp, next: &GraphOp) -> bool {
    match (last, next) {
        (
            GraphOp::SetImportAlias {
                id: a, to: last_to, ..
            },
            GraphOp::SetImportAlias { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetSymbolMeta {
                id: a, to: last_to, ..
            },
            GraphOp::SetSymbolMeta { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetSymbolName {
                id: a, to: last_to, ..
            },
            GraphOp::SetSymbolName { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetSymbolType {
                id: a, to: last_to, ..
            },
            GraphOp::SetSymbolType { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetSymbolDefaultValue {
                id: a, to: last_to, ..
            },
            GraphOp::SetSymbolDefaultValue { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetGroupRect {
                id: a, to: last_to, ..
            },
            GraphOp::SetGroupRect { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetGroupColor {
                id: a, to: last_to, ..
            },
            GraphOp::SetGroupColor { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetStickyNoteText {
                id: a, to: last_to, ..
            },
            GraphOp::SetStickyNoteText { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetStickyNoteRect {
                id: a, to: last_to, ..
            },
            GraphOp::SetStickyNoteRect { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        (
            GraphOp::SetStickyNoteColor {
                id: a, to: last_to, ..
            },
            GraphOp::SetStickyNoteColor { id: b, from, to },
        ) => coalesce_value(a, last_to, b, from, to),
        _ => false,
    }
}
