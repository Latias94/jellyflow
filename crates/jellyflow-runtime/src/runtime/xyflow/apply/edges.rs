use crate::runtime::xyflow::changes::EdgeChange;
use crate::runtime::xyflow::dialect::{
    apply_edge_update_change as apply_edge_update, edge_update_id,
};
use jellyflow_core::core::EdgeId;

use super::ApplyChangesPlanner;

impl<'a> ApplyChangesPlanner<'a> {
    pub(super) fn apply_edges(&mut self, changes: &[EdgeChange]) {
        for change in changes {
            self.apply_edge_change(change);
        }
    }

    fn apply_edge_change(&mut self, change: &EdgeChange) {
        match change {
            EdgeChange::Add { id, edge } => {
                self.graph.edges.insert(*id, edge.clone());
                self.mark_applied();
            }
            EdgeChange::Remove { id } => {
                self.remove_edge_change(*id);
            }
            _ => self.apply_edge_update_change(change),
        }
    }

    fn apply_edge_update_change(&mut self, change: &EdgeChange) {
        let Some(id) = edge_update_id(change) else {
            self.mark_ignored();
            return;
        };
        let Some(edge) = self.graph.edges.get_mut(&id) else {
            self.mark_ignored();
            return;
        };
        if apply_edge_update(change, edge) {
            self.mark_applied();
        } else {
            self.mark_ignored();
        }
    }

    fn remove_edge_change(&mut self, id: EdgeId) {
        if self.graph.edges.remove(&id).is_some() {
            self.mark_applied();
        } else {
            self.mark_ignored();
        }
    }
}
