mod edges;
mod nodes;

use crate::runtime::xyflow::changes::NodeGraphChanges;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

pub(super) fn node_graph_changes_from_transaction(tx: &GraphTransaction) -> NodeGraphChanges {
    let mut out = NodeGraphChanges::default();
    for op in &tx.ops {
        push_node_graph_change(op, &mut out);
    }
    out
}

fn push_node_graph_change(op: &GraphOp, out: &mut NodeGraphChanges) {
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
        | GraphOp::RemoveGroup { .. } => nodes::push_node_change(op, out),

        GraphOp::RemovePort { .. }
        | GraphOp::AddEdge { .. }
        | GraphOp::RemoveEdge { .. }
        | GraphOp::SetEdgeKind { .. }
        | GraphOp::SetEdgeSelectable { .. }
        | GraphOp::SetEdgeDeletable { .. }
        | GraphOp::SetEdgeReconnectable { .. }
        | GraphOp::SetEdgeEndpoints { .. } => edges::push_edge_change(op, &mut out.edges),

        // These variants mutate graph resources that are outside the XyFlow-style
        // node/edge change-array contract. Full-fidelity controlled integrations should
        // apply the committed GraphTransaction from on_graph_commit.
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
        | GraphOp::SetStickyNoteColor { .. } => {}
    }
}
