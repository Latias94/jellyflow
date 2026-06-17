use crate::runtime::xyflow::changes::EdgeChange;
use jellyflow_core::core::{Edge, EdgeId};
use jellyflow_core::ops::{EdgeEndpoints, GraphOp};

pub(in crate::runtime::xyflow) fn edge_update_id(change: &EdgeChange) -> Option<EdgeId> {
    match change {
        EdgeChange::Add { .. } | EdgeChange::Remove { .. } => None,
        EdgeChange::Kind { id, .. }
        | EdgeChange::Selectable { id, .. }
        | EdgeChange::Focusable { id, .. }
        | EdgeChange::Hidden { id, .. }
        | EdgeChange::InteractionWidth { id, .. }
        | EdgeChange::Deletable { id, .. }
        | EdgeChange::Reconnectable { id, .. }
        | EdgeChange::Data { id, .. }
        | EdgeChange::View { id, .. }
        | EdgeChange::Endpoints { id, .. } => Some(*id),
    }
}

pub(in crate::runtime::xyflow) fn edge_update_op(
    change: &EdgeChange,
    edge: &Edge,
) -> Option<GraphOp> {
    Some(match change {
        EdgeChange::Add { .. } | EdgeChange::Remove { .. } => return None,
        EdgeChange::Kind { id, kind } => GraphOp::SetEdgeKind {
            id: *id,
            from: edge.kind,
            to: *kind,
        },
        EdgeChange::Selectable { id, selectable } => GraphOp::SetEdgeSelectable {
            id: *id,
            from: edge.selectable,
            to: *selectable,
        },
        EdgeChange::Focusable { id, focusable } => GraphOp::SetEdgeFocusable {
            id: *id,
            from: edge.focusable,
            to: *focusable,
        },
        EdgeChange::Hidden { id, hidden } => GraphOp::SetEdgeHidden {
            id: *id,
            from: edge.hidden,
            to: *hidden,
        },
        EdgeChange::InteractionWidth {
            id,
            interaction_width,
        } => GraphOp::SetEdgeInteractionWidth {
            id: *id,
            from: edge.interaction_width,
            to: *interaction_width,
        },
        EdgeChange::Deletable { id, deletable } => GraphOp::SetEdgeDeletable {
            id: *id,
            from: edge.deletable,
            to: *deletable,
        },
        EdgeChange::Reconnectable { id, reconnectable } => GraphOp::SetEdgeReconnectable {
            id: *id,
            from: edge.reconnectable,
            to: *reconnectable,
        },
        EdgeChange::Data { id, data } => GraphOp::SetEdgeData {
            id: *id,
            from: edge.data.clone(),
            to: data.clone(),
        },
        EdgeChange::View { id, view } => GraphOp::SetEdgeView {
            id: *id,
            from: edge.view.clone(),
            to: view.clone(),
        },
        EdgeChange::Endpoints { id, from, to } => GraphOp::SetEdgeEndpoints {
            id: *id,
            from: EdgeEndpoints::from_edge(edge),
            to: EdgeEndpoints::new(*from, *to),
        },
    })
}

pub(in crate::runtime::xyflow) fn edge_update_change_from_op(op: &GraphOp) -> Option<EdgeChange> {
    Some(match op {
        GraphOp::SetEdgeKind { id, to, .. } => EdgeChange::Kind { id: *id, kind: *to },
        GraphOp::SetEdgeSelectable { id, to, .. } => EdgeChange::Selectable {
            id: *id,
            selectable: *to,
        },
        GraphOp::SetEdgeFocusable { id, to, .. } => EdgeChange::Focusable {
            id: *id,
            focusable: *to,
        },
        GraphOp::SetEdgeHidden { id, to, .. } => EdgeChange::Hidden {
            id: *id,
            hidden: *to,
        },
        GraphOp::SetEdgeInteractionWidth { id, to, .. } => EdgeChange::InteractionWidth {
            id: *id,
            interaction_width: *to,
        },
        GraphOp::SetEdgeDeletable { id, to, .. } => EdgeChange::Deletable {
            id: *id,
            deletable: *to,
        },
        GraphOp::SetEdgeReconnectable { id, to, .. } => EdgeChange::Reconnectable {
            id: *id,
            reconnectable: *to,
        },
        GraphOp::SetEdgeData { id, to, .. } => EdgeChange::Data {
            id: *id,
            data: to.clone(),
        },
        GraphOp::SetEdgeView { id, to, .. } => EdgeChange::View {
            id: *id,
            view: to.clone(),
        },
        GraphOp::SetEdgeEndpoints { id, to, .. } => EdgeChange::Endpoints {
            id: *id,
            from: to.from,
            to: to.to,
        },
        _ => return None,
    })
}
