use crate::runtime::events::NodeGraphGestureEvent;
use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId};
use jellyflow_core::ops::GraphTransaction;

use super::ConformanceAction;

pub(super) fn kind(action: &ConformanceAction) -> Option<&'static str> {
    Some(match action {
        ConformanceAction::DispatchTransaction { .. } => "dispatch_transaction",
        ConformanceAction::AssertNodePosition { .. } => "assert_node_position",
        ConformanceAction::SetViewport { .. } => "set_viewport",
        ConformanceAction::SetSelection { .. } => "set_selection",
        ConformanceAction::EmitGesture { .. } => "emit_gesture",
        _ => return None,
    })
}

impl ConformanceAction {
    /// Builds the low-level transaction fixture action.
    ///
    /// Prefer the interaction-specific constructors when checking adapter behavior.
    pub fn dispatch_transaction(transaction: GraphTransaction) -> Self {
        Self::DispatchTransaction { transaction }
    }

    pub fn assert_node_position(node: NodeId, expected: CanvasPoint) -> Self {
        Self::AssertNodePosition { node, expected }
    }

    pub fn set_viewport(pan: CanvasPoint, zoom: f32) -> Self {
        Self::SetViewport { pan, zoom }
    }

    pub fn set_selection(
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
        groups: impl IntoIterator<Item = GroupId>,
    ) -> Self {
        Self::SetSelection {
            nodes: nodes.into_iter().collect(),
            edges: edges.into_iter().collect(),
            groups: groups.into_iter().collect(),
        }
    }

    pub fn emit_gesture(event: NodeGraphGestureEvent) -> Self {
        Self::EmitGesture { event }
    }
}
