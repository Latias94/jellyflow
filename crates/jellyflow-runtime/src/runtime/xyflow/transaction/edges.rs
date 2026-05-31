use crate::runtime::xyflow::changes::{ChangesToTransactionError, EdgeChange};
use jellyflow_core::core::Edge;
use jellyflow_core::ops::{EdgeEndpoints, GraphMutationPlanner, GraphOp};

use super::ChangesTransactionPlanner;

impl<'a> ChangesTransactionPlanner<'a> {
    pub(super) fn push_edge_change(
        &mut self,
        change: &EdgeChange,
    ) -> Result<(), ChangesToTransactionError> {
        match change {
            EdgeChange::Add { id, edge } => self.tx.push(GraphOp::AddEdge {
                id: *id,
                edge: edge.clone(),
            }),
            EdgeChange::Remove { id } => {
                let op = GraphMutationPlanner::new(self.graph)
                    .remove_edge_op(*id)
                    .map_err(|_| ChangesToTransactionError::MissingEdge(*id))?;
                self.tx.push(op);
            }
            EdgeChange::Kind { id, kind } => {
                let from = self.existing_edge(*id)?.kind;
                self.tx.push(GraphOp::SetEdgeKind {
                    id: *id,
                    from,
                    to: *kind,
                });
            }
            EdgeChange::Selectable { id, selectable } => {
                let from = self.existing_edge(*id)?.selectable;
                self.tx.push(GraphOp::SetEdgeSelectable {
                    id: *id,
                    from,
                    to: *selectable,
                });
            }
            EdgeChange::Deletable { id, deletable } => {
                let from = self.existing_edge(*id)?.deletable;
                self.tx.push(GraphOp::SetEdgeDeletable {
                    id: *id,
                    from,
                    to: *deletable,
                });
            }
            EdgeChange::Reconnectable { id, reconnectable } => {
                let from = self.existing_edge(*id)?.reconnectable;
                self.tx.push(GraphOp::SetEdgeReconnectable {
                    id: *id,
                    from,
                    to: *reconnectable,
                });
            }
            EdgeChange::Endpoints { id, from, to } => {
                let edge = self.existing_edge(*id)?;
                self.tx.push(GraphOp::SetEdgeEndpoints {
                    id: *id,
                    from: edge_endpoints(edge),
                    to: EdgeEndpoints {
                        from: *from,
                        to: *to,
                    },
                });
            }
        }
        Ok(())
    }
}

fn edge_endpoints(edge: &Edge) -> EdgeEndpoints {
    EdgeEndpoints {
        from: edge.from,
        to: edge.to,
    }
}
