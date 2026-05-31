use crate::core::{CanvasPoint, CanvasRect, CanvasSize, NodeExtent};

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
    for (ix, op) in tx.ops.iter().enumerate() {
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
    fn point_is_finite(p: CanvasPoint) -> bool {
        p.x.is_finite() && p.y.is_finite()
    }

    fn size_is_finite(s: CanvasSize) -> bool {
        s.width.is_finite() && s.height.is_finite()
    }

    fn rect_is_finite(r: CanvasRect) -> bool {
        Self::point_is_finite(r.origin) && Self::size_is_finite(r.size)
    }

    fn endpoints_is_finite(_e: EdgeEndpoints) -> bool {
        true
    }

    fn op_field(op: &GraphOp) -> Option<&'static str> {
        match op {
            GraphOp::AddNode { node, .. } => node
                .size
                .and_then(|s| (!Self::size_is_finite(s)).then_some("AddNode.node.size"))
                .or_else(|| (!Self::point_is_finite(node.pos)).then_some("AddNode.node.pos"))
                .or_else(|| match node.extent {
                    Some(NodeExtent::Rect { rect }) => {
                        (!Self::rect_is_finite(rect)).then_some("AddNode.node.extent.rect")
                    }
                    Some(NodeExtent::Parent) | None => None,
                }),
            GraphOp::AddGroup { group, .. } => {
                (!Self::rect_is_finite(group.rect)).then_some("AddGroup.rect")
            }
            GraphOp::AddStickyNote { note, .. } => {
                (!Self::rect_is_finite(note.rect)).then_some("AddStickyNote.rect")
            }

            GraphOp::SetNodePos { to, .. } => {
                (!Self::point_is_finite(*to)).then_some("SetNodePos.to")
            }
            GraphOp::SetGroupRect { to, .. } => {
                (!Self::rect_is_finite(*to)).then_some("SetGroupRect.to")
            }
            GraphOp::SetStickyNoteRect { to, .. } => {
                (!Self::rect_is_finite(*to)).then_some("SetStickyNoteRect.to")
            }
            GraphOp::SetNodeSize { to, .. } => {
                to.and_then(|s| (!Self::size_is_finite(s)).then_some("SetNodeSize.to"))
            }
            GraphOp::SetNodeExtent { to, .. } => match to {
                Some(NodeExtent::Rect { rect }) => {
                    (!Self::rect_is_finite(*rect)).then_some("SetNodeExtent.to.rect")
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
    fn size_is_valid(s: CanvasSize) -> bool {
        s.width.is_finite() && s.height.is_finite() && s.width > 0.0 && s.height > 0.0
    }

    fn op_field(op: &GraphOp) -> Option<&'static str> {
        match op {
            GraphOp::AddNode { node, .. } => node
                .size
                .and_then(|s| (!Self::size_is_valid(s)).then_some("AddNode.node.size")),
            GraphOp::SetNodeSize { to, .. } => {
                to.and_then(|s| (!Self::size_is_valid(s)).then_some("SetNodeSize.to"))
            }
            _ => None,
        }
    }
}
