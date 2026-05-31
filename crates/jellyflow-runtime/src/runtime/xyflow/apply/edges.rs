use crate::runtime::xyflow::changes::EdgeChange;

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
                if self.graph.edges.remove(id).is_some() {
                    self.mark_applied();
                } else {
                    self.mark_ignored();
                }
            }
            EdgeChange::Kind { id, kind } => {
                self.mutate_existing_edge(*id, |edge| edge.kind = *kind);
            }
            EdgeChange::Selectable { id, selectable } => {
                self.mutate_existing_edge(*id, |edge| edge.selectable = *selectable);
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
}
