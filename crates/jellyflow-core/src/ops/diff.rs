mod edges;
mod metadata;
mod nodes;
mod ports;

use std::collections::BTreeSet;

use crate::core::{EdgeId, Graph, PortId};
use crate::ops::{GraphTransaction, normalize_transaction};

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
}

impl<'a> GraphDiffPlanner<'a> {
    fn new(from: &'a Graph, to: &'a Graph) -> Self {
        Self {
            from,
            to,
            tx: GraphTransaction::new(),
            removed_ports_by_cascade: BTreeSet::new(),
            removed_edges_by_cascade: BTreeSet::new(),
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
        self.diff_edges();
        self.diff_sticky_notes();

        normalize_transaction(self.tx)
    }
}
