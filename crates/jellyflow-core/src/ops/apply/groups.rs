use crate::core::{Graph, Group, GroupId, NodeId};
use crate::ops::GraphOp;

use super::ApplyError;

pub(super) fn apply_group_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddGroup { id, group } => apply_add_group(graph, *id, group)?,
        GraphOp::RemoveGroup {
            id,
            group,
            detached,
        } => apply_remove_group(graph, *id, group, detached)?,
        GraphOp::SetGroupRect { id, to, .. } => {
            group_mut(graph, *id)?.rect = *to;
        }
        GraphOp::SetGroupTitle { id, to, .. } => {
            group_mut(graph, *id)?.title = to.clone();
        }
        GraphOp::SetGroupColor { id, to, .. } => {
            group_mut(graph, *id)?.color = to.clone();
        }
        _ => unreachable!("non-group op routed to group apply"),
    }
    Ok(())
}

fn apply_add_group(graph: &mut Graph, id: GroupId, group: &Group) -> Result<(), ApplyError> {
    if graph.groups.contains_key(&id) {
        return Err(ApplyError::GroupAlreadyExists { id });
    }
    graph.groups.insert(id, group.clone());
    Ok(())
}

fn apply_remove_group(
    graph: &mut Graph,
    id: GroupId,
    group: &Group,
    detached: &[(NodeId, Option<GroupId>)],
) -> Result<(), ApplyError> {
    let Some(current) = graph.groups.get(&id) else {
        return Err(ApplyError::MissingGroup { id });
    };
    if current.title != group.title || current.rect != group.rect || current.color != group.color {
        return Err(ApplyError::RemoveGroupMismatch { id });
    }

    for (node_id, expected_parent) in detached {
        let Some(node) = graph.nodes.get_mut(node_id) else {
            return Err(ApplyError::MissingNode { id: *node_id });
        };
        if node.parent != *expected_parent {
            return Err(ApplyError::RemoveGroupDetachedMismatch {
                group: id,
                node: *node_id,
                expected: *expected_parent,
            });
        }
        node.parent = None;
    }

    graph.groups.remove(&id);
    Ok(())
}

fn group_mut(graph: &mut Graph, id: GroupId) -> Result<&mut Group, ApplyError> {
    graph
        .groups
        .get_mut(&id)
        .ok_or(ApplyError::MissingGroup { id })
}
