mod edges;
mod groups;
mod nodes;
mod ports;

use super::NodeGraphLookups;
use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

impl NodeGraphLookups {
    pub fn apply_transaction(&mut self, graph: &Graph, tx: &GraphTransaction) {
        for op in tx.ops() {
            if !self.apply_op(graph, op) {
                self.rebuild_from(graph);
                return;
            }
        }
    }

    fn apply_op(&mut self, graph: &Graph, op: &GraphOp) -> bool {
        match op {
            GraphOp::AddNode { id, node } => self.apply_add_node(*id, node),
            GraphOp::RemoveNode { id, edges, .. } => self.apply_remove_node(*id, edges),
            GraphOp::SetNodePos { id, to, .. } => self.apply_set_node_pos(graph, *id, *to),
            GraphOp::SetNodeOrigin { id, to, .. } => self.apply_set_node_origin(*id, *to),
            GraphOp::SetNodeKind { id, to, .. } => self.apply_set_node_kind(graph, *id, to),
            GraphOp::SetNodeKindVersion { id, to, .. } => {
                self.apply_set_node_kind_version(graph, *id, *to)
            }
            GraphOp::SetNodeParent { id, to, .. } => self.apply_set_node_parent(*id, *to),
            GraphOp::SetNodeSize { id, to, .. } => self.apply_set_node_size(*id, *to),
            GraphOp::SetNodeHidden { id, to, .. } => self.apply_set_node_hidden(*id, *to),
            GraphOp::SetNodeCollapsed { id, to, .. } => self.apply_set_node_collapsed(*id, *to),
            GraphOp::SetNodePorts { id, to, .. } => self.apply_set_node_ports(*id, to),
            GraphOp::RemovePort {
                id, port, edges, ..
            } => self.apply_remove_port(*id, port, edges),
            GraphOp::AddEdge { id, .. } => self.apply_add_edge(graph, *id),
            GraphOp::RemoveEdge { id, .. } => self.apply_remove_edge(*id),
            GraphOp::SetEdgeKind { id, to, .. } => self.apply_set_edge_kind(graph, *id, *to),
            GraphOp::SetEdgeReconnectable { id, to, .. } => {
                self.apply_set_edge_reconnectable(graph, *id, *to)
            }
            GraphOp::SetEdgeEndpoints { id, from, to } => {
                self.apply_set_edge_endpoints(graph, *id, *from, *to)
            }
            GraphOp::RemoveGroup { detached, .. } => self.apply_remove_group(detached),

            GraphOp::SetNodeSelectable { .. }
            | GraphOp::SetNodeFocusable { .. }
            | GraphOp::SetNodeDraggable { .. }
            | GraphOp::SetNodeConnectable { .. }
            | GraphOp::SetNodeDeletable { .. }
            | GraphOp::SetNodeExtent { .. }
            | GraphOp::SetNodeExpandParent { .. }
            | GraphOp::SetNodeData { .. }
            | GraphOp::AddPort { .. }
            | GraphOp::SetPortConnectable { .. }
            | GraphOp::SetPortConnectableStart { .. }
            | GraphOp::SetPortConnectableEnd { .. }
            | GraphOp::SetPortType { .. }
            | GraphOp::SetPortData { .. }
            | GraphOp::SetEdgeSelectable { .. }
            | GraphOp::SetEdgeFocusable { .. }
            | GraphOp::SetEdgeHidden { .. }
            | GraphOp::SetEdgeInteractionWidth { .. }
            | GraphOp::SetEdgeDeletable { .. }
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
            | GraphOp::SetStickyNoteColor { .. }
            | GraphOp::AddBinding { .. }
            | GraphOp::RemoveBinding { .. }
            | GraphOp::SetBindingSubject { .. }
            | GraphOp::SetBindingTarget { .. }
            | GraphOp::SetBindingKind { .. }
            | GraphOp::SetBindingMeta { .. } => true,
        }
    }
}
