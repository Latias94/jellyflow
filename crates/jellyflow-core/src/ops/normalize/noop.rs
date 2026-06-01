use crate::ops::GraphOp;

pub(super) fn op_is_noop(op: &GraphOp) -> bool {
    match op {
        GraphOp::SetNodePos { from, to, .. } => from == to,
        GraphOp::SetNodeKind { from, to, .. } => from == to,
        GraphOp::SetNodeKindVersion { from, to, .. } => from == to,
        GraphOp::SetNodeSelectable { from, to, .. } => from == to,
        GraphOp::SetNodeFocusable { from, to, .. } => from == to,
        GraphOp::SetNodeDraggable { from, to, .. } => from == to,
        GraphOp::SetNodeConnectable { from, to, .. } => from == to,
        GraphOp::SetNodeDeletable { from, to, .. } => from == to,
        GraphOp::SetNodeParent { from, to, .. } => from == to,
        GraphOp::SetNodeExtent { from, to, .. } => from == to,
        GraphOp::SetNodeExpandParent { from, to, .. } => from == to,
        GraphOp::SetNodeSize { from, to, .. } => from == to,
        GraphOp::SetNodeHidden { from, to, .. } => from == to,
        GraphOp::SetNodeCollapsed { from, to, .. } => from == to,
        GraphOp::SetNodePorts { from, to, .. } => from == to,
        GraphOp::SetNodeData { from, to, .. } => from == to,

        GraphOp::SetPortConnectable { from, to, .. } => from == to,
        GraphOp::SetPortConnectableStart { from, to, .. } => from == to,
        GraphOp::SetPortConnectableEnd { from, to, .. } => from == to,
        GraphOp::SetPortType { from, to, .. } => from == to,
        GraphOp::SetPortData { from, to, .. } => from == to,

        GraphOp::SetEdgeKind { from, to, .. } => from == to,
        GraphOp::SetEdgeSelectable { from, to, .. } => from == to,
        GraphOp::SetEdgeFocusable { from, to, .. } => from == to,
        GraphOp::SetEdgeDeletable { from, to, .. } => from == to,
        GraphOp::SetEdgeReconnectable { from, to, .. } => from == to,
        GraphOp::SetEdgeEndpoints { from, to, .. } => from == to,

        GraphOp::SetImportAlias { from, to, .. } => from == to,

        GraphOp::SetSymbolName { from, to, .. } => from == to,
        GraphOp::SetSymbolType { from, to, .. } => from == to,
        GraphOp::SetSymbolDefaultValue { from, to, .. } => from == to,
        GraphOp::SetSymbolMeta { from, to, .. } => from == to,

        GraphOp::SetGroupRect { from, to, .. } => from == to,
        GraphOp::SetGroupTitle { from, to, .. } => from == to,
        GraphOp::SetGroupColor { from, to, .. } => from == to,
        GraphOp::SetStickyNoteText { from, to, .. } => from == to,
        GraphOp::SetStickyNoteRect { from, to, .. } => from == to,
        GraphOp::SetStickyNoteColor { from, to, .. } => from == to,

        GraphOp::AddNode { .. }
        | GraphOp::RemoveNode { .. }
        | GraphOp::AddPort { .. }
        | GraphOp::RemovePort { .. }
        | GraphOp::AddEdge { .. }
        | GraphOp::RemoveEdge { .. }
        | GraphOp::AddImport { .. }
        | GraphOp::RemoveImport { .. }
        | GraphOp::AddSymbol { .. }
        | GraphOp::RemoveSymbol { .. }
        | GraphOp::AddGroup { .. }
        | GraphOp::RemoveGroup { .. }
        | GraphOp::AddStickyNote { .. }
        | GraphOp::RemoveStickyNote { .. } => false,
    }
}
