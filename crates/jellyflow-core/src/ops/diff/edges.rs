use super::GraphDiffPlanner;
use crate::ops::{EdgeEndpoints, GraphOp};

impl<'a> GraphDiffPlanner<'a> {
    pub(super) fn diff_edges(&mut self) {
        let from = self.from;
        let to = self.to;
        let tx = &mut self.tx;
        let removed_edges_by_cascade = &self.removed_edges_by_cascade;

        for (id, edge_to) in &to.edges {
            if let Some(edge_from) = from.edges.get(id) {
                if edge_from.kind != edge_to.kind {
                    tx.ops.push(GraphOp::SetEdgeKind {
                        id: *id,
                        from: edge_from.kind,
                        to: edge_to.kind,
                    });
                }
                let from_ep = EdgeEndpoints {
                    from: edge_from.from,
                    to: edge_from.to,
                };
                let to_ep = EdgeEndpoints {
                    from: edge_to.from,
                    to: edge_to.to,
                };
                if from_ep != to_ep {
                    tx.ops.push(GraphOp::SetEdgeEndpoints {
                        id: *id,
                        from: from_ep,
                        to: to_ep,
                    });
                }

                if edge_from.selectable != edge_to.selectable {
                    tx.ops.push(GraphOp::SetEdgeSelectable {
                        id: *id,
                        from: edge_from.selectable,
                        to: edge_to.selectable,
                    });
                }
                if edge_from.deletable != edge_to.deletable {
                    tx.ops.push(GraphOp::SetEdgeDeletable {
                        id: *id,
                        from: edge_from.deletable,
                        to: edge_to.deletable,
                    });
                }
                if edge_from.reconnectable != edge_to.reconnectable {
                    tx.ops.push(GraphOp::SetEdgeReconnectable {
                        id: *id,
                        from: edge_from.reconnectable,
                        to: edge_to.reconnectable,
                    });
                }
            } else {
                tx.ops.push(GraphOp::AddEdge {
                    id: *id,
                    edge: edge_to.clone(),
                });
            }
        }

        for (id, edge_from) in &from.edges {
            if !to.edges.contains_key(id) {
                if removed_edges_by_cascade.contains(id) {
                    continue;
                }
                tx.ops.push(GraphOp::RemoveEdge {
                    id: *id,
                    edge: edge_from.clone(),
                });
            }
        }
    }
}
