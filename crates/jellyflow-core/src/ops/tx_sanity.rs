use crate::core::NodeExtent;

use super::{EdgeEndpoints, GraphOp, GraphTransaction};

pub fn find_non_finite_in_tx(tx: &GraphTransaction) -> Option<(String, String)> {
    find_tx_sanity_issue(
        tx,
        "tx.non_finite",
        "non-finite geometry",
        NonFiniteGeometry::op_field,
    )
}

pub fn find_invalid_size_in_tx(tx: &GraphTransaction) -> Option<(String, String)> {
    find_tx_sanity_issue(
        tx,
        "tx.invalid_size",
        "invalid size",
        InvalidNodeSize::op_field,
    )
}

fn find_tx_sanity_issue(
    tx: &GraphTransaction,
    code: &'static str,
    label: &'static str,
    mut op_field: impl FnMut(&GraphOp) -> Option<&'static str>,
) -> Option<(String, String)> {
    for (ix, op) in tx.ops().iter().enumerate() {
        if let Some(field) = op_field(op) {
            return Some((
                code.to_string(),
                format!("transaction contains {label} at op[{ix}] ({field})"),
            ));
        }
    }
    None
}

struct NonFiniteGeometry;

impl NonFiniteGeometry {
    fn endpoints_is_finite(_e: EdgeEndpoints) -> bool {
        true
    }

    fn op_field(op: &GraphOp) -> Option<&'static str> {
        match op {
            GraphOp::AddNode { node, .. } => node
                .size
                .and_then(|s| (!s.is_finite()).then_some("AddNode.node.size"))
                .or_else(|| (!node.pos.is_finite()).then_some("AddNode.node.pos"))
                .or_else(|| match node.extent {
                    Some(NodeExtent::Rect { rect }) => {
                        (!rect.is_finite()).then_some("AddNode.node.extent.rect")
                    }
                    Some(NodeExtent::Parent) | None => None,
                }),
            GraphOp::AddGroup { group, .. } => (!group.rect.is_finite()).then_some("AddGroup.rect"),
            GraphOp::AddStickyNote { note, .. } => {
                (!note.rect.is_finite()).then_some("AddStickyNote.rect")
            }

            GraphOp::SetNodePos { to, .. } => (!to.is_finite()).then_some("SetNodePos.to"),
            GraphOp::SetGroupRect { to, .. } => (!to.is_finite()).then_some("SetGroupRect.to"),
            GraphOp::SetStickyNoteRect { to, .. } => {
                (!to.is_finite()).then_some("SetStickyNoteRect.to")
            }
            GraphOp::SetNodeSize { to, .. } => {
                to.and_then(|s| (!s.is_finite()).then_some("SetNodeSize.to"))
            }
            GraphOp::SetNodeExtent { to, .. } => match to {
                Some(NodeExtent::Rect { rect }) => {
                    (!rect.is_finite()).then_some("SetNodeExtent.to.rect")
                }
                Some(NodeExtent::Parent) | None => None,
            },

            GraphOp::SetEdgeEndpoints { from, to, .. } => (!Self::endpoints_is_finite(*from))
                .then_some("SetEdgeEndpoints.from")
                .or_else(|| (!Self::endpoints_is_finite(*to)).then_some("SetEdgeEndpoints.to")),

            GraphOp::AddImport { .. }
            | GraphOp::RemoveImport { .. }
            | GraphOp::SetImportAlias { .. }
            | GraphOp::SetSymbolName { .. }
            | GraphOp::SetSymbolType { .. }
            | GraphOp::SetSymbolDefaultValue { .. }
            | GraphOp::SetNodeSelectable { .. }
            | GraphOp::SetNodeDraggable { .. }
            | GraphOp::SetNodeConnectable { .. }
            | GraphOp::SetNodeDeletable { .. }
            | GraphOp::SetNodeExpandParent { .. }
            | GraphOp::SetNodeHidden { .. }
            | GraphOp::SetPortConnectable { .. }
            | GraphOp::SetPortConnectableStart { .. }
            | GraphOp::SetPortConnectableEnd { .. }
            | GraphOp::SetPortType { .. }
            | GraphOp::SetPortData { .. }
            | GraphOp::SetEdgeSelectable { .. }
            | GraphOp::SetEdgeDeletable { .. }
            | GraphOp::SetEdgeReconnectable { .. }
            | GraphOp::RemoveNode { .. }
            | GraphOp::SetNodeKind { .. }
            | GraphOp::SetNodeKindVersion { .. }
            | GraphOp::SetNodeParent { .. }
            | GraphOp::SetNodeCollapsed { .. }
            | GraphOp::SetNodePorts { .. }
            | GraphOp::SetNodeData { .. }
            | GraphOp::SetGroupTitle { .. }
            | GraphOp::SetGroupColor { .. }
            | GraphOp::SetStickyNoteText { .. }
            | GraphOp::SetStickyNoteColor { .. }
            | GraphOp::AddPort { .. }
            | GraphOp::RemovePort { .. }
            | GraphOp::AddEdge { .. }
            | GraphOp::RemoveEdge { .. }
            | GraphOp::SetEdgeKind { .. }
            | GraphOp::AddSymbol { .. }
            | GraphOp::RemoveSymbol { .. }
            | GraphOp::SetSymbolMeta { .. }
            | GraphOp::RemoveGroup { .. }
            | GraphOp::RemoveStickyNote { .. } => None,
        }
    }
}

struct InvalidNodeSize;

impl InvalidNodeSize {
    fn op_field(op: &GraphOp) -> Option<&'static str> {
        match op {
            GraphOp::AddNode { node, .. } => node
                .size
                .and_then(|s| (!s.is_positive_finite()).then_some("AddNode.node.size")),
            GraphOp::SetNodeSize { to, .. } => {
                to.and_then(|s| (!s.is_positive_finite()).then_some("SetNodeSize.to"))
            }
            _ => None,
        }
    }
}
