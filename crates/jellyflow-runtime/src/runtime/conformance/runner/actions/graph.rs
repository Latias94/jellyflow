use crate::runtime::events::NodeGraphGestureEvent;
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId};
use jellyflow_core::ops::GraphTransaction;

use super::super::super::scenario::ConformanceAction;

pub(super) fn execute_action(
    store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Option<Result<(), String>> {
    Some(match action {
        ConformanceAction::DispatchTransaction { transaction } => {
            dispatch_transaction(store, transaction)
        }
        ConformanceAction::AssertNodePosition { node, expected } => {
            assert_node_position(store, *node, *expected)
        }
        ConformanceAction::SetViewport { pan, zoom } => {
            set_viewport(store, *pan, *zoom);
            Ok(())
        }
        ConformanceAction::SetSelection {
            nodes,
            edges,
            groups,
        } => {
            set_selection(store, nodes, edges, groups);
            Ok(())
        }
        ConformanceAction::EmitGesture { event } => {
            emit_gesture(store, event);
            Ok(())
        }
        _ => return None,
    })
}

pub(super) fn dispatch_transaction(
    store: &mut NodeGraphStore,
    transaction: &GraphTransaction,
) -> Result<(), String> {
    store
        .dispatch_transaction(transaction)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

pub(super) fn set_viewport(store: &mut NodeGraphStore, pan: CanvasPoint, zoom: f32) {
    store.set_viewport(pan, zoom);
}

pub(super) fn set_selection(
    store: &mut NodeGraphStore,
    nodes: &[NodeId],
    edges: &[EdgeId],
    groups: &[GroupId],
) {
    store.set_selection(nodes.to_vec(), edges.to_vec(), groups.to_vec());
}

pub(super) fn emit_gesture(store: &mut NodeGraphStore, event: &NodeGraphGestureEvent) {
    store.emit_gesture(event.clone());
}

pub(super) fn assert_node_position(
    store: &NodeGraphStore,
    node: NodeId,
    expected: CanvasPoint,
) -> Result<(), String> {
    let Some(actual) = store.graph().nodes.get(&node).map(|node| node.pos) else {
        return Err(format!("node not found for position assertion: {node:?}"));
    };

    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "node position resolved to {actual:?}, expected {expected:?}"
        ))
    }
}
