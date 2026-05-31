use std::collections::BTreeSet;

use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{Edge, EdgeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

pub(super) fn connection_changes_from_transaction(tx: &GraphTransaction) -> Vec<ConnectionChange> {
    let mut accumulator = ConnectionChangeAccumulator::new(tx.len());
    for op in tx.ops() {
        accumulator.push_op(op);
    }
    accumulator.finish()
}

struct ConnectionChangeAccumulator {
    out: Vec<ConnectionChange>,
    removed_edges: BTreeSet<EdgeId>,
}

impl ConnectionChangeAccumulator {
    fn new(op_count: usize) -> Self {
        let mut out = Vec::new();
        out.reserve(op_count.min(8));
        Self {
            out,
            removed_edges: BTreeSet::new(),
        }
    }

    fn push_op(&mut self, op: &GraphOp) {
        match op {
            GraphOp::AddEdge { id, edge } => self.push_connected(*id, edge),
            GraphOp::RemoveNode { edges, .. } | GraphOp::RemovePort { edges, .. } => {
                self.push_disconnected_edges(edges);
            }
            GraphOp::RemoveEdge { id, edge } => self.push_disconnected_edge(*id, edge),
            GraphOp::SetEdgeEndpoints { id, from, to } => {
                self.out.push(ConnectionChange::Reconnected {
                    edge: *id,
                    from: *from,
                    to: *to,
                });
            }
            _ => {}
        }
    }

    fn push_connected(&mut self, id: EdgeId, edge: &Edge) {
        self.out
            .push(ConnectionChange::Connected(edge_connection(id, edge)));
    }

    fn push_disconnected_edges(&mut self, edges: &[(EdgeId, Edge)]) {
        for (id, edge) in edges {
            self.push_disconnected_edge(*id, edge);
        }
    }

    fn push_disconnected_edge(&mut self, id: EdgeId, edge: &Edge) {
        if self.removed_edges.insert(id) {
            self.out
                .push(ConnectionChange::Disconnected(edge_connection(id, edge)));
        }
    }

    fn finish(self) -> Vec<ConnectionChange> {
        self.out
    }
}

fn edge_connection(id: EdgeId, edge: &Edge) -> EdgeConnection {
    EdgeConnection {
        edge: id,
        from: edge.from,
        to: edge.to,
        kind: edge.kind,
    }
}
