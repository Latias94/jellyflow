use super::super::GraphDiffPlanner;
use crate::core::{Group, GroupId, NodeId};
use crate::ops::{GraphMutationPlanner, GraphOp};

impl<'a> GraphDiffPlanner<'a> {
    pub(crate) fn diff_groups(&mut self) {
        let from = self.from;
        let to = self.to;

        for (id, group_to) in &to.groups {
            if let Some(group_from) = from.groups.get(id) {
                self.diff_existing_group(*id, group_from, group_to);
            } else {
                self.push_op(GraphOp::AddGroup {
                    id: *id,
                    group: group_to.clone(),
                });
            }
        }

        for (id, group_from) in &from.groups {
            if !to.groups.contains_key(id) {
                self.diff_removed_group(*id, group_from);
            }
        }
    }

    fn diff_existing_group(&mut self, id: GroupId, group_from: &Group, group_to: &Group) {
        if group_from.color != group_to.color {
            self.push_op(GraphOp::SetGroupColor {
                id,
                from: group_from.color.clone(),
                to: group_to.color.clone(),
            });
        }

        if group_from.rect != group_to.rect {
            self.push_op(GraphOp::SetGroupRect {
                id,
                from: group_from.rect,
                to: group_to.rect,
            });
        }
        if group_from.title != group_to.title {
            self.push_op(GraphOp::SetGroupTitle {
                id,
                from: group_from.title.clone(),
                to: group_to.title.clone(),
            });
        }
    }

    fn diff_removed_group(&mut self, id: GroupId, group_from: &Group) {
        let op = GraphMutationPlanner::new(self.from)
            .remove_group_op(id)
            .unwrap_or_else(|_| self.fallback_remove_group_op(id, group_from));
        let op = self.with_target_removed_bindings(op);
        if let GraphOp::RemoveGroup { bindings, .. } = &op {
            self.removed_bindings_by_cascade
                .extend(bindings.iter().map(|(id, _)| *id));
        }
        self.push_op(op);
    }

    fn fallback_remove_group_op(&self, id: GroupId, group_from: &Group) -> GraphOp {
        let detached: Vec<(NodeId, Option<GroupId>)> = self
            .from
            .nodes
            .iter()
            .filter_map(|(node_id, node)| (node.parent == Some(id)).then_some((*node_id, Some(id))))
            .collect();
        GraphOp::RemoveGroup {
            id,
            group: group_from.clone(),
            detached,
            bindings: Vec::new(),
        }
    }
}
