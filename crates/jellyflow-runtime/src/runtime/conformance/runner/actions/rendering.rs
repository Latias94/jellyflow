use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{CanvasSize, EdgeId, NodeId};

use super::super::super::scenario::ConformanceAction;

pub(super) fn execute_action(
    store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Option<Result<(), String>> {
    Some(match action {
        ConformanceAction::AssertVisibleNodeIds {
            viewport_size,
            expected,
        } => assert_visible_node_ids(store, *viewport_size, expected),
        ConformanceAction::AssertVisibleNodeRenderOrder {
            viewport_size,
            expected,
        } => assert_visible_node_render_order(store, *viewport_size, expected),
        ConformanceAction::AssertVisibleEdgeIds {
            viewport_size,
            expected,
        } => assert_visible_edge_ids(store, *viewport_size, expected),
        ConformanceAction::AssertVisibleEdgeRenderOrder {
            viewport_size,
            expected,
        } => assert_visible_edge_render_order(store, *viewport_size, expected),
        _ => return None,
    })
}

pub(super) fn assert_visible_node_ids(
    store: &NodeGraphStore,
    viewport_size: CanvasSize,
    expected: &[NodeId],
) -> Result<(), String> {
    let actual = store.visible_node_ids(viewport_size);
    if actual.as_slice() == expected {
        Ok(())
    } else {
        Err(format!(
            "visible node ids resolved to {actual:?}, expected {expected:?}"
        ))
    }
}

pub(super) fn assert_visible_node_render_order(
    store: &NodeGraphStore,
    viewport_size: CanvasSize,
    expected: &[NodeId],
) -> Result<(), String> {
    let actual = store.visible_node_render_order(viewport_size);
    if actual.as_slice() == expected {
        Ok(())
    } else {
        Err(format!(
            "visible node render order resolved to {actual:?}, expected {expected:?}"
        ))
    }
}

pub(super) fn assert_visible_edge_ids(
    store: &NodeGraphStore,
    viewport_size: CanvasSize,
    expected: &[EdgeId],
) -> Result<(), String> {
    let actual = store.visible_edge_ids(viewport_size);
    if actual.as_slice() == expected {
        Ok(())
    } else {
        Err(format!(
            "visible edge ids resolved to {actual:?}, expected {expected:?}"
        ))
    }
}

pub(super) fn assert_visible_edge_render_order(
    store: &NodeGraphStore,
    viewport_size: CanvasSize,
    expected: &[EdgeId],
) -> Result<(), String> {
    let actual = store.visible_edge_render_order(viewport_size);
    if actual.as_slice() == expected {
        Ok(())
    } else {
        Err(format!(
            "visible edge render order resolved to {actual:?}, expected {expected:?}"
        ))
    }
}
