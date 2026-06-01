mod document;
mod edge;
mod node;
mod port;

use crate::ops::{GraphOp, GraphTransaction};

/// Builds an inverse transaction that restores the graph state before `tx`.
pub fn invert_transaction(tx: &GraphTransaction) -> GraphTransaction {
    let mut out = GraphTransaction::new();
    for op in tx.ops().iter().rev() {
        out.extend(invert_op(op));
    }
    out
}

fn invert_op(op: &GraphOp) -> Vec<GraphOp> {
    match op {
        GraphOp::AddNode { .. }
        | GraphOp::RemoveNode { .. }
        | GraphOp::SetNodePos { .. }
        | GraphOp::SetNodeKind { .. }
        | GraphOp::SetNodeKindVersion { .. }
        | GraphOp::SetNodeSelectable { .. }
        | GraphOp::SetNodeFocusable { .. }
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
        | GraphOp::SetNodeData { .. } => node::invert_node_op(op),

        GraphOp::AddPort { .. }
        | GraphOp::RemovePort { .. }
        | GraphOp::SetPortConnectable { .. }
        | GraphOp::SetPortConnectableStart { .. }
        | GraphOp::SetPortConnectableEnd { .. }
        | GraphOp::SetPortType { .. }
        | GraphOp::SetPortData { .. } => port::invert_port_op(op),

        GraphOp::AddEdge { .. }
        | GraphOp::RemoveEdge { .. }
        | GraphOp::SetEdgeKind { .. }
        | GraphOp::SetEdgeSelectable { .. }
        | GraphOp::SetEdgeFocusable { .. }
        | GraphOp::SetEdgeHidden { .. }
        | GraphOp::SetEdgeDeletable { .. }
        | GraphOp::SetEdgeReconnectable { .. }
        | GraphOp::SetEdgeEndpoints { .. } => edge::invert_edge_op(op),

        GraphOp::AddImport { .. }
        | GraphOp::RemoveImport { .. }
        | GraphOp::SetImportAlias { .. }
        | GraphOp::AddSymbol { .. }
        | GraphOp::RemoveSymbol { .. }
        | GraphOp::SetSymbolName { .. }
        | GraphOp::SetSymbolType { .. }
        | GraphOp::SetSymbolDefaultValue { .. }
        | GraphOp::SetSymbolMeta { .. }
        | GraphOp::AddGroup { .. }
        | GraphOp::RemoveGroup { .. }
        | GraphOp::SetGroupRect { .. }
        | GraphOp::SetGroupTitle { .. }
        | GraphOp::SetGroupColor { .. }
        | GraphOp::AddStickyNote { .. }
        | GraphOp::RemoveStickyNote { .. }
        | GraphOp::SetStickyNoteText { .. }
        | GraphOp::SetStickyNoteRect { .. }
        | GraphOp::SetStickyNoteColor { .. } => document::invert_document_op(op),
    }
}
