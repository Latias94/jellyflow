use crate::core::Graph;
use crate::ops::GraphOp;

use super::ApplyError;

pub(super) fn apply_group_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddGroup { id, group } => {
            if graph.groups.contains_key(id) {
                return Err(ApplyError::GroupAlreadyExists { id: *id });
            }
            graph.groups.insert(*id, group.clone());
        }
        GraphOp::RemoveGroup {
            id,
            group,
            detached,
        } => {
            let Some(current) = graph.groups.get(id) else {
                return Err(ApplyError::MissingGroup { id: *id });
            };
            if current.title != group.title
                || current.rect != group.rect
                || current.color != group.color
            {
                return Err(ApplyError::RemoveGroupMismatch { id: *id });
            }

            for (node_id, expected_parent) in detached {
                let Some(node) = graph.nodes.get_mut(node_id) else {
                    return Err(ApplyError::MissingNode { id: *node_id });
                };
                if node.parent != *expected_parent {
                    return Err(ApplyError::RemoveGroupDetachedMismatch {
                        group: *id,
                        node: *node_id,
                        expected: *expected_parent,
                    });
                }
                node.parent = None;
            }

            graph.groups.remove(id);
        }
        GraphOp::SetGroupRect { id, to, .. } => {
            let Some(group) = graph.groups.get_mut(id) else {
                return Err(ApplyError::MissingGroup { id: *id });
            };
            group.rect = *to;
        }
        GraphOp::SetGroupTitle { id, to, .. } => {
            let Some(group) = graph.groups.get_mut(id) else {
                return Err(ApplyError::MissingGroup { id: *id });
            };
            group.title = to.clone();
        }
        GraphOp::SetGroupColor { id, to, .. } => {
            let Some(group) = graph.groups.get_mut(id) else {
                return Err(ApplyError::MissingGroup { id: *id });
            };
            group.color = to.clone();
        }
        _ => unreachable!("non-group op routed to group apply"),
    }
    Ok(())
}
