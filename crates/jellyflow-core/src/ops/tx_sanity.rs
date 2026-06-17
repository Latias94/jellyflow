use crate::core::NodeExtent;

use super::{GraphOp, GraphTransaction};

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
    fn op_field(op: &GraphOp) -> Option<&'static str> {
        match op {
            GraphOp::AddNode { node, .. } => node
                .size
                .and_then(|s| (!s.is_finite()).then_some("AddNode.node.size"))
                .or_else(|| {
                    node.origin
                        .and_then(|origin| (!origin.is_finite()).then_some("AddNode.node.origin"))
                })
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
            GraphOp::AddEdge { edge, .. } => edge_non_finite_field(
                edge,
                "AddEdge.edge.interaction_width",
                "AddEdge.edge.view.hit_target_width",
            ),

            GraphOp::SetNodePos { to, .. } => (!to.is_finite()).then_some("SetNodePos.to"),
            GraphOp::SetNodeOrigin { to, .. } => {
                to.and_then(|origin| (!origin.is_finite()).then_some("SetNodeOrigin.to"))
            }
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
            GraphOp::SetEdgeInteractionWidth { to, .. } => {
                option_non_finite(*to).then_some("SetEdgeInteractionWidth.to")
            }
            GraphOp::SetEdgeView { to, .. } => {
                edge_view_non_finite_field(to, "SetEdgeView.to.hit_target_width")
            }

            GraphOp::AddImport { .. }
            | GraphOp::RemoveImport { .. }
            | GraphOp::SetImportAlias { .. }
            | GraphOp::SetSymbolName { .. }
            | GraphOp::SetSymbolType { .. }
            | GraphOp::SetSymbolDefaultValue { .. }
            | GraphOp::SetNodeSelectable { .. }
            | GraphOp::SetNodeFocusable { .. }
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
            | GraphOp::SetEdgeFocusable { .. }
            | GraphOp::SetEdgeHidden { .. }
            | GraphOp::SetEdgeDeletable { .. }
            | GraphOp::SetEdgeReconnectable { .. }
            | GraphOp::SetEdgeData { .. }
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
            | GraphOp::AddBinding { .. }
            | GraphOp::RemoveBinding { .. }
            | GraphOp::SetBindingSubject { .. }
            | GraphOp::SetBindingTarget { .. }
            | GraphOp::SetBindingKind { .. }
            | GraphOp::SetBindingMeta { .. }
            | GraphOp::AddPort { .. }
            | GraphOp::RemovePort { .. }
            | GraphOp::RemoveEdge { .. }
            | GraphOp::SetEdgeEndpoints { .. }
            | GraphOp::SetEdgeKind { .. }
            | GraphOp::AddSymbol { .. }
            | GraphOp::RemoveSymbol { .. }
            | GraphOp::SetSymbolMeta { .. }
            | GraphOp::RemoveGroup { .. }
            | GraphOp::RemoveStickyNote { .. } => None,
        }
    }
}

fn edge_non_finite_field(
    edge: &crate::core::Edge,
    interaction_width_field: &'static str,
    view_hit_target_width_field: &'static str,
) -> Option<&'static str> {
    option_non_finite(edge.interaction_width)
        .then_some(interaction_width_field)
        .or_else(|| edge_view_non_finite_field(&edge.view, view_hit_target_width_field))
}

fn edge_view_non_finite_field(
    view: &crate::core::EdgeViewDescriptor,
    hit_target_width_field: &'static str,
) -> Option<&'static str> {
    option_non_finite(view.hit_target_width).then_some(hit_target_width_field)
}

fn option_non_finite(value: Option<f32>) -> bool {
    value.is_some_and(|value| !value.is_finite())
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
