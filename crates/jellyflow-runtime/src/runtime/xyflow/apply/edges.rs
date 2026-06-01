use crate::runtime::xyflow::changes::EdgeChange;
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
            EdgeChange::Kind { id, kind } => {
                self.mutate_existing_edge(*id, |edge| edge.kind = *kind);
            }
            EdgeChange::Selectable { id, selectable } => {
                self.mutate_existing_edge(*id, |edge| edge.selectable = *selectable);
            }
            EdgeChange::Focusable { id, focusable } => {
                self.mutate_existing_edge(*id, |edge| edge.focusable = *focusable);
            }
            EdgeChange::Deletable { id, deletable } => {
                self.mutate_existing_edge(*id, |edge| edge.deletable = *deletable);
            }
            EdgeChange::Reconnectable { id, reconnectable } => {
                self.mutate_existing_edge(*id, |edge| edge.reconnectable = *reconnectable);
            }
            EdgeChange::Endpoints { id, from, to } => {
                self.mutate_existing_edge(*id, |edge| {
                    edge.from = *from;
                    edge.to = *to;
                });
            }
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
