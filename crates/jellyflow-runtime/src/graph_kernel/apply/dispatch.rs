use crate::graph_kernel::{GraphOp, GraphTransaction};
use jellyflow_core::core::{Graph, GraphBuilder, validate_graph_storage};

use super::ApplyError;

pub(super) fn apply_transaction(
    graph: &mut Graph,
    tx: &GraphTransaction,
) -> Result<(), ApplyError> {
    let mut scratch = GraphBuilder::from_detached_graph(graph.clone());
    for op in tx.ops() {
        apply_op(&mut scratch, op)?;
    }

    let report = validate_graph_storage(scratch.graph());
    if !report.is_ok() {
        return Err(ApplyError::InvalidTransactionResult {
            errors: report.into_errors(),
        });
    }

    *graph = scratch.build_unchecked();
    Ok(())
}

pub(super) fn apply_prevalidated_transaction_batch(
    graph: &mut Graph,
    tx: &GraphTransaction,
) -> Result<(), ApplyError> {
    let mut scratch = GraphBuilder::from_detached_graph(graph.clone());
    for op in tx.ops() {
        apply_op(&mut scratch, op)?;
    }
    *graph = scratch.build_unchecked();
    Ok(())
}

fn apply_op(graph: &mut GraphBuilder, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddNode { .. }
        | GraphOp::RemoveNode { .. }
        | GraphOp::SetNodePos { .. }
        | GraphOp::SetNodeOrigin { .. }
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
        | GraphOp::SetNodeData { .. } => super::nodes::apply_node_op(graph, op),

        GraphOp::AddPort { .. }
        | GraphOp::RemovePort { .. }
        | GraphOp::SetPortConnectable { .. }
        | GraphOp::SetPortConnectableStart { .. }
        | GraphOp::SetPortConnectableEnd { .. }
        | GraphOp::SetPortType { .. }
        | GraphOp::SetPortData { .. } => super::ports::apply_port_op(graph, op),

        GraphOp::AddEdge { .. }
        | GraphOp::RemoveEdge { .. }
        | GraphOp::SetEdgeKind { .. }
        | GraphOp::SetEdgeSelectable { .. }
        | GraphOp::SetEdgeHidden { .. }
        | GraphOp::SetEdgeInteractionWidth { .. }
        | GraphOp::SetEdgeDeletable { .. }
        | GraphOp::SetEdgeReconnectable { .. }
        | GraphOp::SetEdgeData { .. }
        | GraphOp::SetEdgeView { .. }
        | GraphOp::SetEdgeEndpoints { .. } => super::edges::apply_edge_op(graph, op),

        GraphOp::AddImport { .. }
        | GraphOp::RemoveImport { .. }
        | GraphOp::SetImportAlias { .. } => super::imports::apply_import_op(graph, op),

        GraphOp::AddSymbol { .. }
        | GraphOp::RemoveSymbol { .. }
        | GraphOp::SetSymbolName { .. }
        | GraphOp::SetSymbolType { .. }
        | GraphOp::SetSymbolDefaultValue { .. }
        | GraphOp::SetSymbolMeta { .. } => super::symbols::apply_symbol_op(graph, op),

        GraphOp::AddGroup { .. }
        | GraphOp::RemoveGroup { .. }
        | GraphOp::SetGroupRect { .. }
        | GraphOp::SetGroupTitle { .. }
        | GraphOp::SetGroupColor { .. } => super::groups::apply_group_op(graph, op),

        GraphOp::AddStickyNote { .. }
        | GraphOp::RemoveStickyNote { .. }
        | GraphOp::SetStickyNoteText { .. }
        | GraphOp::SetStickyNoteRect { .. }
        | GraphOp::SetStickyNoteColor { .. } => {
            super::sticky_notes::apply_sticky_note_op(graph, op)
        }

        GraphOp::AddBinding { .. }
        | GraphOp::RemoveBinding { .. }
        | GraphOp::SetBindingSubject { .. }
        | GraphOp::SetBindingTarget { .. }
        | GraphOp::SetBindingKind { .. }
        | GraphOp::SetBindingMeta { .. } => super::bindings::apply_binding_op(graph, op),
    }
}
