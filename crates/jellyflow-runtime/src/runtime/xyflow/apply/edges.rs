use crate::runtime::xyflow::changes::EdgeChange;
use crate::runtime::xyflow::dialect::{edge_update_id, edge_update_op};
use jellyflow_core::core::EdgeId;
use jellyflow_core::ops::{GraphMutationPlanner, GraphOp, GraphTransaction};

use super::ApplyChangesPlanner;

impl<'a> ApplyChangesPlanner<'a> {
    pub(super) fn apply_edges(&mut self, changes: &[EdgeChange]) {
        for change in changes {
            self.apply_edge_change(change);
        }
    }

    fn apply_edge_change(&mut self, change: &EdgeChange) {
        let Some(op) = self.edge_change_op(change) else {
            self.mark_ignored();
            return;
        };
        if GraphTransaction::from_ops([op])
            .apply_to(self.graph)
            .is_ok()
        {
            self.mark_applied();
        } else {
            self.mark_ignored();
        }
    }

    fn edge_change_op(&self, change: &EdgeChange) -> Option<GraphOp> {
        Some(match change {
            EdgeChange::Add { id, edge } => {
                if self.graph.edges().contains_key(id) {
                    return None;
                }
                GraphOp::AddEdge {
                    id: *id,
                    edge: edge.clone(),
                }
            }
            EdgeChange::Remove { id } => self.remove_edge_change_op(*id)?,
            _ => self.edge_update_change_op(change)?,
        })
    }

    fn edge_update_change_op(&self, change: &EdgeChange) -> Option<GraphOp> {
        let id = edge_update_id(change)?;
        let edge = self.graph.edges().get(&id)?;
        edge_update_op(change, edge)
    }

    fn remove_edge_change_op(&self, id: EdgeId) -> Option<GraphOp> {
        GraphMutationPlanner::new(self.graph)
            .remove_edge_op(id)
            .ok()
    }
}
