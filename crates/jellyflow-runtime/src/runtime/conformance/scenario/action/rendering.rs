use jellyflow_core::core::{CanvasSize, EdgeId, NodeId};

use super::ConformanceAction;

pub(super) fn kind(action: &ConformanceAction) -> Option<&'static str> {
    Some(match action {
        ConformanceAction::AssertVisibleNodeIds { .. } => "assert_visible_node_ids",
        ConformanceAction::AssertVisibleNodeRenderOrder { .. } => {
            "assert_visible_node_render_order"
        }
        ConformanceAction::AssertVisibleEdgeIds { .. } => "assert_visible_edge_ids",
        ConformanceAction::AssertVisibleEdgeRenderOrder { .. } => {
            "assert_visible_edge_render_order"
        }
        _ => return None,
    })
}

impl ConformanceAction {
    pub fn assert_visible_node_ids(
        viewport_size: CanvasSize,
        expected: impl IntoIterator<Item = NodeId>,
    ) -> Self {
        Self::AssertVisibleNodeIds {
            viewport_size,
            expected: expected.into_iter().collect(),
        }
    }

    pub fn assert_visible_node_render_order(
        viewport_size: CanvasSize,
        expected: impl IntoIterator<Item = NodeId>,
    ) -> Self {
        Self::AssertVisibleNodeRenderOrder {
            viewport_size,
            expected: expected.into_iter().collect(),
        }
    }

    pub fn assert_visible_edge_ids(
        viewport_size: CanvasSize,
        expected: impl IntoIterator<Item = EdgeId>,
    ) -> Self {
        Self::AssertVisibleEdgeIds {
            viewport_size,
            expected: expected.into_iter().collect(),
        }
    }

    pub fn assert_visible_edge_render_order(
        viewport_size: CanvasSize,
        expected: impl IntoIterator<Item = EdgeId>,
    ) -> Self {
        Self::AssertVisibleEdgeRenderOrder {
            viewport_size,
            expected: expected.into_iter().collect(),
        }
    }
}
