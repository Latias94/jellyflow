use crate::runtime::xyflow::changes::{ChangesToTransactionError, EdgeChange};
use jellyflow_core::core::{Edge, EdgeId};
use jellyflow_core::ops::{EdgeEndpoints, GraphMutationPlanner, GraphOp};

use super::ChangesTransactionPlanner;

impl<'a> ChangesTransactionPlanner<'a> {
    pub(super) fn push_edge_change(
        &mut self,
        change: &EdgeChange,
    ) -> Result<(), ChangesToTransactionError> {
        match change {
            EdgeChange::Add { id, edge } => {
                self.push_op(GraphOp::AddEdge {
                    id: *id,
                    edge: edge.clone(),
                });
            }
            EdgeChange::Remove { id } => {
                self.push_remove_edge_change(*id)?;
            }
            EdgeChange::Kind { id, kind } => {
                self.push_edge_update(*id, |edge| GraphOp::SetEdgeKind {
                    id: *id,
                    from: edge.kind,
                    to: *kind,
                })?;
            }
            EdgeChange::Selectable { id, selectable } => {
                self.push_edge_update(*id, |edge| GraphOp::SetEdgeSelectable {
                    id: *id,
                    from: edge.selectable,
                    to: *selectable,
                })?;
            }
            EdgeChange::Deletable { id, deletable } => {
                self.push_edge_update(*id, |edge| GraphOp::SetEdgeDeletable {
                    id: *id,
                    from: edge.deletable,
                    to: *deletable,
                })?;
            }
            EdgeChange::Reconnectable { id, reconnectable } => {
                self.push_edge_update(*id, |edge| GraphOp::SetEdgeReconnectable {
                    id: *id,
                    from: edge.reconnectable,
                    to: *reconnectable,
                })?;
            }
            EdgeChange::Endpoints { id, from, to } => {
                self.push_edge_update(*id, |edge| GraphOp::SetEdgeEndpoints {
                    id: *id,
                    from: EdgeEndpoints::from_edge(edge),
                    to: EdgeEndpoints::new(*from, *to),
                })?;
            }
        }
        Ok(())
    }

    fn push_remove_edge_change(&mut self, id: EdgeId) -> Result<(), ChangesToTransactionError> {
        let op = GraphMutationPlanner::new(self.graph)
            .remove_edge_op(id)
            .map_err(|_| ChangesToTransactionError::MissingEdge(id))?;
        self.push_op(op);
        Ok(())
    }

    fn push_edge_update(
        &mut self,
        id: EdgeId,
        build: impl FnOnce(&Edge) -> GraphOp,
    ) -> Result<(), ChangesToTransactionError> {
        let op = {
            let edge = self.existing_edge(id)?;
            build(edge)
        };
        self.push_op(op);
        Ok(())
    }
}
