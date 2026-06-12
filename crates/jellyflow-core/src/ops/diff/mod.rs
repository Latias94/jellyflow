mod edges;
mod metadata;
mod nodes;
mod ports;

use std::collections::BTreeSet;

use crate::core::{BindingId, EdgeId, Graph, NodeId, PortId};
use crate::ops::{GraphOp, GraphTransaction, normalize_transaction};

/// Computes a deterministic patch transaction that transforms `from` into `to`.
///
/// This is intended as a collaboration-friendly patch unit and as a conformance gate for refactors.
/// It prefers correctness + determinism over minimality.
pub fn graph_diff(from: &Graph, to: &Graph) -> GraphTransaction {
    GraphDiffPlanner::new(from, to).finish()
}

struct GraphDiffPlanner<'a> {
    from: &'a Graph,
    to: &'a Graph,
    tx: GraphTransaction,
    removed_ports_by_cascade: BTreeSet<PortId>,
    removed_edges_by_cascade: BTreeSet<EdgeId>,
    removed_bindings_by_cascade: BTreeSet<BindingId>,
    restored_edges_by_cascade: BTreeSet<EdgeId>,
    nodes_requiring_port_order_restore: BTreeSet<NodeId>,
    replaced_ports_requiring_port_order_restore: BTreeSet<PortId>,
}

impl<'a> GraphDiffPlanner<'a> {
    fn new(from: &'a Graph, to: &'a Graph) -> Self {
        Self {
            from,
            to,
            tx: GraphTransaction::new(),
            removed_ports_by_cascade: BTreeSet::new(),
            removed_edges_by_cascade: BTreeSet::new(),
            removed_bindings_by_cascade: BTreeSet::new(),
            restored_edges_by_cascade: BTreeSet::new(),
            nodes_requiring_port_order_restore: BTreeSet::new(),
            replaced_ports_requiring_port_order_restore: BTreeSet::new(),
        }
    }

    fn finish(mut self) -> GraphTransaction {
        self.diff_imports();
        self.diff_symbols();
        self.diff_groups();

        // Nodes/ports/edges: MVP focuses on headless collaboration patching. We keep the phase
        // order apply-safe (edges last because they reference ports).
        self.diff_nodes();
        self.diff_ports();
        self.restore_target_port_orders();
        self.diff_edges();
        self.diff_bindings();
        self.diff_sticky_notes();

        normalize_transaction(self.tx)
    }

    fn push_op(&mut self, op: GraphOp) {
        self.tx.push(op);
    }

    fn stable_existing_port_order(&self, ports: &[PortId]) -> Vec<PortId> {
        ports
            .iter()
            .copied()
            .filter(|port_id| self.to.ports.contains_key(port_id))
            .collect()
    }

    fn stable_restored_port_order(&self, ports: &[PortId]) -> Vec<PortId> {
        self.stable_existing_port_order(ports)
            .into_iter()
            .filter(|port_id| {
                !self
                    .replaced_ports_requiring_port_order_restore
                    .contains(port_id)
            })
            .collect()
    }

    fn node_port_order_needs_post_restore(&self, node: NodeId, ports: &[PortId]) -> bool {
        ports
            .iter()
            .any(|port_id| match self.from.ports.get(port_id) {
                Some(port_from) => port_from.node != node,
                None => true,
            })
    }

    fn target_removed_bindings(
        &self,
        bindings: &[(BindingId, crate::core::Binding)],
    ) -> Vec<(BindingId, crate::core::Binding)> {
        bindings
            .iter()
            .filter(|(binding_id, _)| !self.to.bindings.contains_key(binding_id))
            .cloned()
            .collect()
    }

    fn with_target_removed_bindings(&self, op: GraphOp) -> GraphOp {
        match op {
            GraphOp::RemoveNode {
                id,
                node,
                ports,
                edges,
                bindings,
            } => GraphOp::RemoveNode {
                id,
                node,
                ports,
                edges,
                bindings: self.target_removed_bindings(&bindings),
            },
            GraphOp::RemovePort {
                id,
                port,
                edges,
                bindings,
            } => GraphOp::RemovePort {
                id,
                port,
                edges,
                bindings: self.target_removed_bindings(&bindings),
            },
            GraphOp::RemoveEdge { id, edge, bindings } => GraphOp::RemoveEdge {
                id,
                edge,
                bindings: self.target_removed_bindings(&bindings),
            },
            GraphOp::RemoveGroup {
                id,
                group,
                detached,
                bindings,
            } => GraphOp::RemoveGroup {
                id,
                group,
                detached,
                bindings: self.target_removed_bindings(&bindings),
            },
            GraphOp::RemoveStickyNote { id, note, bindings } => GraphOp::RemoveStickyNote {
                id,
                note,
                bindings: self.target_removed_bindings(&bindings),
            },
            other => other,
        }
    }
}
