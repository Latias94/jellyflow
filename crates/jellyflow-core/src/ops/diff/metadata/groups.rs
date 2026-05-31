use super::super::GraphDiffPlanner;
use crate::ops::{GraphMutationPlanner, GraphOp};

impl<'a> GraphDiffPlanner<'a> {
    pub(crate) fn diff_groups(&mut self) {
        let from = self.from;
        let to = self.to;
        let tx = &mut self.tx;

        for (id, group_to) in &to.groups {
            if let Some(group_from) = from.groups.get(id) {
                if group_from.color != group_to.color {
                    tx.ops.push(GraphOp::SetGroupColor {
                        id: *id,
                        from: group_from.color.clone(),
                        to: group_to.color.clone(),
                    });
                }

                if group_from.rect != group_to.rect {
                    tx.ops.push(GraphOp::SetGroupRect {
                        id: *id,
                        from: group_from.rect,
                        to: group_to.rect,
                    });
                }
                if group_from.title != group_to.title {
                    tx.ops.push(GraphOp::SetGroupTitle {
                        id: *id,
                        from: group_from.title.clone(),
                        to: group_to.title.clone(),
                    });
                }
            } else {
                tx.ops.push(GraphOp::AddGroup {
                    id: *id,
                    group: group_to.clone(),
                });
            }
        }

        for (id, group_from) in &from.groups {
            if !to.groups.contains_key(id) {
                if let Ok(op) = GraphMutationPlanner::new(from).remove_group_op(*id) {
                    tx.ops.push(op);
                } else {
                    let detached: Vec<(crate::core::NodeId, Option<crate::core::GroupId>)> = from
                        .nodes
                        .iter()
                        .filter_map(|(node_id, node)| {
                            (node.parent == Some(*id)).then_some((*node_id, Some(*id)))
                        })
                        .collect();
                    tx.ops.push(GraphOp::RemoveGroup {
                        id: *id,
                        group: group_from.clone(),
                        detached,
                    });
                }
            }
        }
    }
}
