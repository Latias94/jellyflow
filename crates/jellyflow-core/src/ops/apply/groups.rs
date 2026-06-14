use crate::core::{Binding, BindingId, Graph, Group, GroupId, NodeId};
use crate::ops::GraphOp;

use super::ApplyError;
use super::resources::remove_bindings_exact;

pub(super) fn apply_group_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddGroup { id, group } => apply_add_group(graph, *id, group)?,
        GraphOp::RemoveGroup {
            id,
            group,
            detached,
            bindings,
        } => apply_remove_group(graph, *id, group, detached, bindings)?,
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
    if graph.groups().contains_key(&id) {
        return Err(ApplyError::GroupAlreadyExists { id });
    }
    graph.insert_group(id, group.clone());
    Ok(())
}

fn apply_remove_group(
    graph: &mut Graph,
    id: GroupId,
    group: &Group,
    detached: &[(NodeId, Option<GroupId>)],
    bindings: &[(BindingId, Binding)],
) -> Result<(), ApplyError> {
    let Some(current) = graph.groups().get(&id) else {
        return Err(ApplyError::MissingGroup { id });
    };
    if current.title != group.title || current.rect != group.rect || current.color != group.color {
        return Err(ApplyError::RemoveGroupMismatch { id });
    }

    remove_bindings_exact(graph, bindings)?;
    for (node_id, expected_parent) in detached {
        let Some(node) = graph.node_mut(node_id) else {
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

    graph.remove_group(&id);
    Ok(())
}

fn group_mut(graph: &mut Graph, id: GroupId) -> Result<&mut Group, ApplyError> {
    graph.group_mut(&id).ok_or(ApplyError::MissingGroup { id })
}
