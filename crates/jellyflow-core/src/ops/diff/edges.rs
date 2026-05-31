use super::GraphDiffPlanner;
use crate::core::{Edge, EdgeId};
use crate::ops::{EdgeEndpoints, GraphOp};

impl<'a> GraphDiffPlanner<'a> {
    pub(super) fn diff_edges(&mut self) {
        let from = self.from;
        let to = self.to;

        for (id, edge_to) in &to.edges {
            if let Some(edge_from) = from.edges.get(id) {
                self.diff_existing_edge(*id, edge_from, edge_to);
            } else {
                self.tx.ops.push(GraphOp::AddEdge {
                    id: *id,
                    edge: edge_to.clone(),
                });
            }
        }

        self.diff_removed_edges();
    }

    fn diff_existing_edge(&mut self, id: EdgeId, edge_from: &Edge, edge_to: &Edge) {
        self.diff_edge_kind(id, edge_from, edge_to);
        self.diff_edge_endpoints(id, edge_from, edge_to);
        self.diff_edge_policy_fields(id, edge_from, edge_to);
    }

    fn diff_edge_kind(&mut self, id: EdgeId, edge_from: &Edge, edge_to: &Edge) {
        if edge_from.kind != edge_to.kind {
            self.tx.ops.push(GraphOp::SetEdgeKind {
                id,
                from: edge_from.kind,
                to: edge_to.kind,
            });
        }
    }

    fn diff_edge_endpoints(&mut self, id: EdgeId, edge_from: &Edge, edge_to: &Edge) {
        let from = edge_endpoints(edge_from);
        let to = edge_endpoints(edge_to);
        if from != to {
            self.tx.ops.push(GraphOp::SetEdgeEndpoints { id, from, to });
        }
    }

    fn diff_edge_policy_fields(&mut self, id: EdgeId, edge_from: &Edge, edge_to: &Edge) {
        if edge_from.selectable != edge_to.selectable {
            self.tx.ops.push(GraphOp::SetEdgeSelectable {
                id,
                from: edge_from.selectable,
                to: edge_to.selectable,
            });
        }
        if edge_from.deletable != edge_to.deletable {
            self.tx.ops.push(GraphOp::SetEdgeDeletable {
                id,
                from: edge_from.deletable,
                to: edge_to.deletable,
            });
        }
        if edge_from.reconnectable != edge_to.reconnectable {
            self.tx.ops.push(GraphOp::SetEdgeReconnectable {
                id,
                from: edge_from.reconnectable,
                to: edge_to.reconnectable,
            });
        }
    }

    fn diff_removed_edges(&mut self) {
        let from = self.from;
        let to = self.to;

        for (id, edge_from) in &from.edges {
            if to.edges.contains_key(id) || self.removed_edges_by_cascade.contains(id) {
                continue;
            }

            self.tx.ops.push(GraphOp::RemoveEdge {
                id: *id,
                edge: edge_from.clone(),
            });
        }
    }
}

fn edge_endpoints(edge: &Edge) -> EdgeEndpoints {
    EdgeEndpoints {
        from: edge.from,
        to: edge.to,
    }
}
