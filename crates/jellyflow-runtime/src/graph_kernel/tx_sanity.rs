use jellyflow_core::core::NodeExtent;

use super::{GraphOp, GraphTransaction};

pub fn find_non_finite_in_tx(tx: &GraphTransaction) -> Option<(String, String)> {
    find_tx_sanity_issue(
        tx,
        "tx.non_finite",
        "non-finite geometry",
        NonFiniteGeometry::op_field,
    )
}

pub fn find_invalid_size_in_tx(tx: &GraphTransaction) -> Option<(String, String)> {
    find_tx_sanity_issue(
        tx,
        "tx.invalid_size",
        "invalid size",
        InvalidNodeSize::op_field,
    )
}

pub fn find_invalid_edge_route_in_tx(tx: &GraphTransaction) -> Option<(String, String)> {
    for (ix, op) in tx.ops().iter().enumerate() {
        let (field, route) = match op {
            GraphOp::AddEdge { edge, .. } => ("AddEdge.edge.view.route", edge.view.route),
            GraphOp::SetEdgeView { to, .. } => ("SetEdgeView.to.route", to.route),
            _ => continue,
        };
        let Some(source) = route.and_then(|route| route.validate().err()) else {
            continue;
        };
        return Some((
            "tx.invalid_edge_route".to_owned(),
            format!(
                "transaction contains invalid edge route specification at op[{ix}] ({field}.{}): {source}",
                source.field(),
            ),
        ));
    }
    None
}

fn find_tx_sanity_issue(
    tx: &GraphTransaction,
    code: &'static str,
    label: &'static str,
    mut op_field: impl FnMut(&GraphOp) -> Option<&'static str>,
) -> Option<(String, String)> {
    for (ix, op) in tx.ops().iter().enumerate() {
        if let Some(field) = op_field(op) {
            return Some((
                code.to_string(),
                format!("transaction contains {label} at op[{ix}] ({field})"),
            ));
        }
    }
    None
}

struct NonFiniteGeometry;

impl NonFiniteGeometry {
    fn op_field(op: &GraphOp) -> Option<&'static str> {
        match op {
            GraphOp::AddNode { node, .. } => node
                .size
                .and_then(|s| (!s.is_finite()).then_some("AddNode.node.size"))
                .or_else(|| {
                    node.origin
                        .and_then(|origin| (!origin.is_finite()).then_some("AddNode.node.origin"))
                })
                .or_else(|| (!node.pos.is_finite()).then_some("AddNode.node.pos"))
                .or_else(|| match node.extent {
                    Some(NodeExtent::Rect { rect }) => {
                        (!rect.is_finite()).then_some("AddNode.node.extent.rect")
                    }
                    Some(NodeExtent::Parent) | None => None,
                }),
            GraphOp::AddGroup { group, .. } => (!group.rect.is_finite()).then_some("AddGroup.rect"),
            GraphOp::AddStickyNote { note, .. } => {
                (!note.rect.is_finite()).then_some("AddStickyNote.rect")
            }
            GraphOp::AddEdge { edge, .. } => {
                edge_non_finite_field(edge, "AddEdge.edge.interaction_width")
            }

            GraphOp::SetNodePos { to, .. } => (!to.is_finite()).then_some("SetNodePos.to"),
            GraphOp::SetNodeOrigin { to, .. } => {
                to.and_then(|origin| (!origin.is_finite()).then_some("SetNodeOrigin.to"))
            }
            GraphOp::SetGroupRect { to, .. } => (!to.is_finite()).then_some("SetGroupRect.to"),
            GraphOp::SetStickyNoteRect { to, .. } => {
                (!to.is_finite()).then_some("SetStickyNoteRect.to")
            }
            GraphOp::SetNodeSize { to, .. } => {
                to.and_then(|s| (!s.is_finite()).then_some("SetNodeSize.to"))
            }
            GraphOp::SetNodeExtent { to, .. } => match to {
                Some(NodeExtent::Rect { rect }) => {
                    (!rect.is_finite()).then_some("SetNodeExtent.to.rect")
                }
                Some(NodeExtent::Parent) | None => None,
            },
            GraphOp::SetEdgeInteractionWidth { to, .. } => {
                option_non_finite(*to).then_some("SetEdgeInteractionWidth.to")
            }
            GraphOp::SetEdgeView { .. } => None,
            GraphOp::AddImport { .. }
            | GraphOp::RemoveImport { .. }
            | GraphOp::SetImportAlias { .. }
            | GraphOp::SetSymbolName { .. }
            | GraphOp::SetSymbolType { .. }
            | GraphOp::SetSymbolDefaultValue { .. }
            | GraphOp::SetNodeSelectable { .. }
            | GraphOp::SetNodeDraggable { .. }
            | GraphOp::SetNodeConnectable { .. }
            | GraphOp::SetNodeDeletable { .. }
            | GraphOp::SetNodeExpandParent { .. }
            | GraphOp::SetNodeHidden { .. }
            | GraphOp::SetPortConnectable { .. }
            | GraphOp::SetPortConnectableStart { .. }
            | GraphOp::SetPortConnectableEnd { .. }
            | GraphOp::SetPortType { .. }
            | GraphOp::SetPortData { .. }
            | GraphOp::SetEdgeSelectable { .. }
            | GraphOp::SetEdgeHidden { .. }
            | GraphOp::SetEdgeDeletable { .. }
            | GraphOp::SetEdgeReconnectable { .. }
            | GraphOp::SetEdgeData { .. }
            | GraphOp::RemoveNode { .. }
            | GraphOp::SetNodeKind { .. }
            | GraphOp::SetNodeKindVersion { .. }
            | GraphOp::SetNodeParent { .. }
            | GraphOp::SetNodeCollapsed { .. }
            | GraphOp::SetNodePorts { .. }
            | GraphOp::SetNodeData { .. }
            | GraphOp::SetGroupTitle { .. }
            | GraphOp::SetGroupColor { .. }
            | GraphOp::SetStickyNoteText { .. }
            | GraphOp::SetStickyNoteColor { .. }
            | GraphOp::AddBinding { .. }
            | GraphOp::RemoveBinding { .. }
            | GraphOp::SetBindingSubject { .. }
            | GraphOp::SetBindingTarget { .. }
            | GraphOp::SetBindingKind { .. }
            | GraphOp::SetBindingMeta { .. }
            | GraphOp::AddPort { .. }
            | GraphOp::RemovePort { .. }
            | GraphOp::RemoveEdge { .. }
            | GraphOp::SetEdgeEndpoints { .. }
            | GraphOp::SetEdgeKind { .. }
            | GraphOp::AddSymbol { .. }
            | GraphOp::RemoveSymbol { .. }
            | GraphOp::SetSymbolMeta { .. }
            | GraphOp::RemoveGroup { .. }
            | GraphOp::RemoveStickyNote { .. } => None,
        }
    }
}

fn edge_non_finite_field(
    edge: &jellyflow_core::core::Edge,
    interaction_width_field: &'static str,
) -> Option<&'static str> {
    option_non_finite(edge.interaction_width).then_some(interaction_width_field)
}

fn option_non_finite(value: Option<f32>) -> bool {
    value.is_some_and(|value| !value.is_finite())
}

struct InvalidNodeSize;

impl InvalidNodeSize {
    fn op_field(op: &GraphOp) -> Option<&'static str> {
        match op {
            GraphOp::AddNode { node, .. } => node
                .size
                .and_then(|s| (!s.is_positive_finite()).then_some("AddNode.node.size")),
            GraphOp::SetNodeSize { to, .. } => {
                to.and_then(|s| (!s.is_positive_finite()).then_some("SetNodeSize.to"))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{Edge, EdgeId, EdgeKind, EdgeRouteSpec, EdgeViewDescriptor, PortId};

    use super::*;

    #[test]
    fn every_invalid_route_parameter_is_preflighted_for_add_and_set_operations() {
        let invalid_routes = [
            EdgeRouteSpec::Bezier {
                curvature: f32::NAN,
            },
            EdgeRouteSpec::Orthogonal {
                offset: -1.0,
                step_position: 0.5,
                center: None,
            },
            EdgeRouteSpec::Orthogonal {
                offset: 20.0,
                step_position: 1.1,
                center: None,
            },
            EdgeRouteSpec::SmoothStep {
                offset: 20.0,
                step_position: 0.5,
                border_radius: f32::MAX,
                center: None,
            },
        ];

        for route in invalid_routes {
            let invalid_view = EdgeViewDescriptor::new().with_route(route);
            let mut invalid_edge = Edge::new(EdgeKind::Data, PortId::new(), PortId::new());
            invalid_edge.view = invalid_view.clone();
            for (op, field) in [
                (
                    GraphOp::AddEdge {
                        id: EdgeId::new(),
                        edge: invalid_edge.clone(),
                    },
                    "AddEdge.edge.view.route",
                ),
                (
                    GraphOp::SetEdgeView {
                        id: EdgeId::new(),
                        from: EdgeViewDescriptor::default(),
                        to: invalid_view.clone(),
                    },
                    "SetEdgeView.to.route",
                ),
            ] {
                let (code, message) =
                    find_invalid_edge_route_in_tx(&GraphTransaction::from_ops([op]))
                        .expect("invalid route must fail transaction preflight");
                assert_eq!(code, "tx.invalid_edge_route");
                assert!(message.contains(field), "unexpected diagnostic: {message}");
            }
        }
    }
}
