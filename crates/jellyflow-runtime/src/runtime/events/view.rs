use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId};

/// View-state projection change events.
///
/// These are the B-layer equivalent of XyFlow's selection/viewport updates (which are embedded in
/// their node/edge arrays). In Jellyflow, view-state is intentionally separate from the serialized
/// graph document.
///
/// Only viewport/selection changes are surfaced here. Other persisted editor configuration is
/// observable through selector subscriptions on [`super::NodeGraphStoreSnapshot`].
#[derive(Debug, Clone)]
pub enum ViewChange {
    Viewport {
        pan: CanvasPoint,
        zoom: f32,
    },
    Selection {
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    },
}

impl ViewChange {
    pub fn viewport(pan: CanvasPoint, zoom: f32) -> Self {
        Self::Viewport { pan, zoom }
    }

    pub fn selection(nodes: Vec<NodeId>, edges: Vec<EdgeId>, groups: Vec<GroupId>) -> Self {
        Self::Selection {
            nodes,
            edges,
            groups,
        }
    }
}
