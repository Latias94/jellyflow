use jellyflow_core::ops::GraphOp;

pub(super) enum NodeGraphProjectionTarget {
    Node,
    Edge,
    Ignore,
}

impl NodeGraphProjectionTarget {
    pub(super) fn for_op(op: &GraphOp) -> Self {
        match op {
            GraphOp::AddNode { .. }
            | GraphOp::RemoveNode { .. }
            | GraphOp::SetNodePos { .. }
            | GraphOp::SetNodeKind { .. }
            | GraphOp::SetNodeKindVersion { .. }
            | GraphOp::SetNodeSelectable { .. }
            | GraphOp::SetNodeDraggable { .. }
            | GraphOp::SetNodeConnectable { .. }
            | GraphOp::SetNodeDeletable { .. }
            | GraphOp::SetNodeParent { .. }
            | GraphOp::SetNodeExtent { .. }
            | GraphOp::SetNodeExpandParent { .. }
            | GraphOp::SetNodeSize { .. }
            | GraphOp::SetNodeHidden { .. }
            | GraphOp::SetNodeCollapsed { .. }
            | GraphOp::SetNodePorts { .. }
            | GraphOp::SetNodeData { .. }
            | GraphOp::RemoveGroup { .. } => Self::Node,

            GraphOp::RemovePort { .. }
            | GraphOp::AddEdge { .. }
            | GraphOp::RemoveEdge { .. }
            | GraphOp::SetEdgeKind { .. }
            | GraphOp::SetEdgeSelectable { .. }
            | GraphOp::SetEdgeDeletable { .. }
            | GraphOp::SetEdgeReconnectable { .. }
            | GraphOp::SetEdgeEndpoints { .. } => Self::Edge,

            // These variants mutate graph resources outside the XyFlow-style node/edge
            // change-array contract. Full-fidelity controlled integrations should apply the
            // committed GraphTransaction from on_graph_commit.
            GraphOp::AddPort { .. }
            | GraphOp::SetPortConnectable { .. }
            | GraphOp::SetPortConnectableStart { .. }
            | GraphOp::SetPortConnectableEnd { .. }
            | GraphOp::SetPortType { .. }
            | GraphOp::SetPortData { .. }
            | GraphOp::AddImport { .. }
            | GraphOp::RemoveImport { .. }
            | GraphOp::SetImportAlias { .. }
            | GraphOp::AddSymbol { .. }
            | GraphOp::RemoveSymbol { .. }
            | GraphOp::SetSymbolName { .. }
            | GraphOp::SetSymbolType { .. }
            | GraphOp::SetSymbolDefaultValue { .. }
            | GraphOp::SetSymbolMeta { .. }
            | GraphOp::AddGroup { .. }
            | GraphOp::SetGroupRect { .. }
            | GraphOp::SetGroupTitle { .. }
            | GraphOp::SetGroupColor { .. }
            | GraphOp::AddStickyNote { .. }
            | GraphOp::RemoveStickyNote { .. }
            | GraphOp::SetStickyNoteText { .. }
            | GraphOp::SetStickyNoteRect { .. }
            | GraphOp::SetStickyNoteColor { .. } => Self::Ignore,
        }
    }
}
