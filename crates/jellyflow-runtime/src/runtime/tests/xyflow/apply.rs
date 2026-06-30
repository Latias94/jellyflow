use super::super::fixtures::make_graph;

use crate::runtime::xyflow::apply::{
    XyFlowDimensionAttribute, XyFlowDimensionsSetAttributes, XyFlowEdgeChange, XyFlowEdgeElement,
    XyFlowNodeChange, XyFlowNodeElement, apply_edge_changes, apply_graph_changes,
    apply_node_changes, apply_xyflow_edge_changes, apply_xyflow_node_changes,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, EdgeId, EdgeKind, EdgeLabelAnchor, EdgeViewDescriptor, NodeId,
    NodeOrigin,
};

#[test]
fn apply_node_changes_removes_ports_and_incident_edges() {
    let (mut g0, a, b, out_port, in_port, eid) = make_graph();

    let report = apply_node_changes(&mut g0, &[NodeChange::Remove { id: a }]);
    assert!(report.did_change());
    assert_eq!(report.ignored(), 0);

    assert!(!g0.nodes().contains_key(&a));
    assert!(g0.nodes().contains_key(&b));

    assert!(!g0.ports().contains_key(&out_port));
    assert!(g0.ports().contains_key(&in_port));

    assert!(!g0.edges().contains_key(&eid));
}

#[test]
fn apply_node_changes_updates_origin_and_ignores_missing() {
    let (mut g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let missing = NodeId::new();

    let report = apply_node_changes(
        &mut g0,
        &[
            NodeChange::Position {
                id: a,
                position: CanvasPoint { x: 12.0, y: 24.0 },
            },
            NodeChange::Origin {
                id: a,
                origin: Some(NodeOrigin { x: 0.5, y: 0.25 }),
            },
            NodeChange::Origin {
                id: missing,
                origin: Some(NodeOrigin { x: 1.0, y: 1.0 }),
            },
        ],
    );

    assert!(report.did_change());
    assert_eq!(report.ignored(), 1);
    assert_eq!(
        g0.nodes().get(&a).unwrap().pos,
        CanvasPoint { x: 12.0, y: 24.0 }
    );
    assert_eq!(
        g0.nodes().get(&a).unwrap().origin,
        Some(NodeOrigin { x: 0.5, y: 0.25 })
    );
}

#[test]
fn apply_edge_changes_updates_kind_and_ignores_missing() {
    let (mut g0, _a, _b, _out_port, _in_port, eid) = make_graph();
    let missing = EdgeId::new();
    let edge_data = serde_json::json!({ "cardinality": "1:n" });
    let edge_view = EdgeViewDescriptor {
        renderer_key: Some("erd-relation".to_owned()),
        label: Some("owns".to_owned()),
        label_anchor: Some(EdgeLabelAnchor::Center),
        source_marker_key: Some("one".to_owned()),
        target_marker_key: Some("many".to_owned()),
        style_token: Some("relation".to_owned()),
        route_kind: None,
        hit_target_width: Some(24.0),
    };

    let report = apply_edge_changes(
        &mut g0,
        &[
            EdgeChange::Kind {
                id: eid,
                kind: EdgeKind::Exec,
            },
            EdgeChange::Hidden {
                id: eid,
                hidden: true,
            },
            EdgeChange::InteractionWidth {
                id: eid,
                interaction_width: Some(30.0),
            },
            EdgeChange::Data {
                id: eid,
                data: edge_data.clone(),
            },
            EdgeChange::View {
                id: eid,
                view: edge_view.clone(),
            },
            EdgeChange::Remove { id: missing },
        ],
    );
    assert!(report.did_change());
    assert_eq!(report.ignored(), 1);
    assert_eq!(g0.edges().get(&eid).unwrap().kind, EdgeKind::Exec);
    assert!(g0.edges().get(&eid).unwrap().hidden);
    assert_eq!(g0.edges().get(&eid).unwrap().interaction_width, Some(30.0));
    assert_eq!(g0.edges().get(&eid).unwrap().data, edge_data);
    assert_eq!(g0.edges().get(&eid).unwrap().view, edge_view);
}

#[test]
fn node_update_changes_apply_and_transaction_paths_agree() {
    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let changes = NodeGraphChanges::from_parts(
        vec![
            NodeChange::Position {
                id: a,
                position: CanvasPoint { x: 12.0, y: 24.0 },
            },
            NodeChange::Origin {
                id: a,
                origin: Some(NodeOrigin { x: 0.5, y: 0.25 }),
            },
            NodeChange::Selectable {
                id: a,
                selectable: Some(false),
            },
            NodeChange::Size {
                id: a,
                size: Some(CanvasSize {
                    width: 88.0,
                    height: 44.0,
                }),
            },
            NodeChange::Hidden {
                id: a,
                hidden: true,
            },
        ],
        Vec::new(),
    );

    let mut applied = g0.clone();
    let report = apply_graph_changes(&mut applied, &changes);
    let tx = changes.to_transaction(&g0).expect("node update tx");
    let mut transacted = g0.clone();
    tx.apply_to(&mut transacted)
        .expect("node update tx applies");

    assert_eq!(report.applied(), 5);
    assert_eq!(report.ignored(), 0);
    let applied_node = applied.nodes().get(&a).expect("applied node");
    let transacted_node = transacted.nodes().get(&a).expect("transacted node");
    assert_eq!(applied_node.pos, transacted_node.pos);
    assert_eq!(applied_node.origin, transacted_node.origin);
    assert_eq!(applied_node.selectable, transacted_node.selectable);
    assert_eq!(applied_node.size, transacted_node.size);
    assert_eq!(applied_node.hidden, transacted_node.hidden);
}

#[test]
fn edge_update_changes_apply_and_transaction_paths_agree() {
    let (g0, _a, _b, out_port, in_port, eid) = make_graph();
    let replacement_from = out_port;
    let replacement_to = in_port;
    let edge_data = serde_json::json!({ "path": "error" });
    let edge_view = EdgeViewDescriptor {
        renderer_key: Some("error-edge".to_owned()),
        label: Some("Error".to_owned()),
        label_anchor: Some(EdgeLabelAnchor::Source),
        source_marker_key: None,
        target_marker_key: Some("arrow".to_owned()),
        style_token: Some("danger".to_owned()),
        route_kind: None,
        hit_target_width: Some(36.0),
    };
    let changes = NodeGraphChanges::from_parts(
        Vec::new(),
        vec![
            EdgeChange::Kind {
                id: eid,
                kind: EdgeKind::Exec,
            },
            EdgeChange::Hidden {
                id: eid,
                hidden: true,
            },
            EdgeChange::InteractionWidth {
                id: eid,
                interaction_width: Some(32.0),
            },
            EdgeChange::Data {
                id: eid,
                data: edge_data.clone(),
            },
            EdgeChange::View {
                id: eid,
                view: edge_view.clone(),
            },
            EdgeChange::Endpoints {
                id: eid,
                from: replacement_from,
                to: replacement_to,
            },
        ],
    );

    let mut applied = g0.clone();
    let report = apply_graph_changes(&mut applied, &changes);
    let tx = changes.to_transaction(&g0).expect("edge update tx");
    let mut transacted = g0.clone();
    tx.apply_to(&mut transacted)
        .expect("edge update tx applies");

    assert_eq!(report.applied(), 6);
    assert_eq!(report.ignored(), 0);
    let applied_edge = applied.edges().get(&eid).expect("applied edge");
    let transacted_edge = transacted.edges().get(&eid).expect("transacted edge");
    assert_eq!(applied_edge.kind, transacted_edge.kind);
    assert_eq!(applied_edge.hidden, transacted_edge.hidden);
    assert_eq!(
        applied_edge.interaction_width,
        transacted_edge.interaction_width
    );
    assert_eq!(applied_edge.from, transacted_edge.from);
    assert_eq!(applied_edge.to, transacted_edge.to);
    assert_eq!(applied_edge.data, transacted_edge.data);
    assert_eq!(applied_edge.view, transacted_edge.view);
    assert_eq!(applied_edge.data, edge_data);
    assert_eq!(applied_edge.view, edge_view);
}

#[test]
fn apply_xyflow_node_changes_preserves_react_ordering_semantics() {
    let (g0, a, b, _out_port, _in_port, _eid) = make_graph();
    let replacement = node_element(&g0, a);
    let inserted = node_element(&g0, NodeId::new());
    let appended = node_element(&g0, NodeId::new());
    let nodes = vec![node_element(&g0, a), node_element(&g0, b)];

    let updated = apply_xyflow_node_changes(
        &[
            XyFlowNodeChange::Position {
                id: a,
                position: Some(CanvasPoint { x: 10.0, y: 20.0 }),
                position_absolute: None,
                dragging: Some(true),
            },
            XyFlowNodeChange::Replace {
                id: a,
                item: replacement.clone(),
            },
            XyFlowNodeChange::Select {
                id: a,
                selected: true,
            },
            XyFlowNodeChange::Position {
                id: b,
                position: Some(CanvasPoint { x: 99.0, y: 100.0 }),
                position_absolute: None,
                dragging: None,
            },
            XyFlowNodeChange::Remove { id: b },
            XyFlowNodeChange::Select {
                id: b,
                selected: true,
            },
            XyFlowNodeChange::Add {
                item: inserted.clone(),
                index: Some(0),
            },
            XyFlowNodeChange::Add {
                item: appended.clone(),
                index: None,
            },
        ],
        &nodes,
    );

    let ids: Vec<NodeId> = updated.iter().map(|node| node.id).collect();
    assert_eq!(ids, vec![inserted.id, replacement.id, appended.id]);
    assert_eq!(updated[1].node.pos, replacement.node.pos);
    assert_eq!(updated[1].selected, None);
    assert_eq!(updated[1].dragging, None);
}

#[test]
fn apply_xyflow_node_changes_updates_ui_state_dimensions_and_position() {
    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let nodes = vec![node_element(&g0, a)];

    let updated = apply_xyflow_node_changes(
        &[
            XyFlowNodeChange::Select {
                id: a,
                selected: true,
            },
            XyFlowNodeChange::Position {
                id: a,
                position: Some(CanvasPoint { x: 12.0, y: 24.0 }),
                position_absolute: Some(CanvasPoint { x: 120.0, y: 240.0 }),
                dragging: Some(true),
            },
            XyFlowNodeChange::Dimensions {
                id: a,
                dimensions: Some(CanvasSize {
                    width: 100.0,
                    height: 80.0,
                }),
                resizing: Some(true),
                set_attributes: Some(XyFlowDimensionsSetAttributes::Bool(true)),
            },
            XyFlowNodeChange::Dimensions {
                id: a,
                dimensions: Some(CanvasSize {
                    width: 120.0,
                    height: 90.0,
                }),
                resizing: Some(false),
                set_attributes: Some(XyFlowDimensionsSetAttributes::Attribute(
                    XyFlowDimensionAttribute::Width,
                )),
            },
            XyFlowNodeChange::Dimensions {
                id: a,
                dimensions: None,
                resizing: Some(true),
                set_attributes: Some(XyFlowDimensionsSetAttributes::Attribute(
                    XyFlowDimensionAttribute::Height,
                )),
            },
        ],
        &nodes,
    );

    let node = &updated[0];
    assert_eq!(node.selected, Some(true));
    assert_eq!(node.dragging, Some(true));
    assert_eq!(node.resizing, Some(true));
    assert_eq!(node.node.pos, CanvasPoint { x: 12.0, y: 24.0 });
    assert_eq!(
        node.measured,
        Some(CanvasSize {
            width: 120.0,
            height: 90.0,
        })
    );
    assert_eq!(node.width, Some(120.0));
    assert_eq!(node.height, Some(80.0));
}

#[test]
fn apply_xyflow_node_changes_accepts_xyflow_camel_case_json_fields() {
    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let change: XyFlowNodeChange = serde_json::from_value(serde_json::json!({
        "type": "dimensions",
        "id": a,
        "dimensions": {
            "width": 64.0,
            "height": 48.0,
        },
        "setAttributes": "height",
        "resizing": true,
    }))
    .expect("xyflow dimension change json");

    let updated = apply_xyflow_node_changes(&[change], &[node_element(&g0, a)]);
    assert_eq!(updated[0].width, None);
    assert_eq!(updated[0].height, Some(48.0));
    assert_eq!(updated[0].resizing, Some(true));

    let encoded = serde_json::to_value(XyFlowNodeChange::Position {
        id: a,
        position: None,
        position_absolute: Some(CanvasPoint { x: 1.0, y: 2.0 }),
        dragging: None,
    })
    .expect("xyflow position json");
    assert!(encoded.get("positionAbsolute").is_some());
    assert!(encoded.get("position_absolute").is_none());
}

#[test]
fn apply_xyflow_edge_changes_preserves_react_ordering_semantics() {
    let (g0, _a, _b, _out_port, _in_port, eid) = make_graph();
    let second = EdgeId::new();
    let removed = EdgeId::new();
    let inserted = EdgeId::new();
    let replacement = edge_element(&g0, eid);
    let edges = vec![
        edge_element(&g0, eid),
        edge_element(&g0, second),
        edge_element(&g0, removed),
    ];

    let inserted_edge = edge_element(&g0, inserted);
    let updated = apply_xyflow_edge_changes(
        &[
            XyFlowEdgeChange::Replace {
                id: eid,
                item: replacement.clone(),
            },
            XyFlowEdgeChange::Select {
                id: eid,
                selected: true,
            },
            XyFlowEdgeChange::Select {
                id: second,
                selected: true,
            },
            XyFlowEdgeChange::Remove { id: removed },
            XyFlowEdgeChange::Select {
                id: removed,
                selected: true,
            },
            XyFlowEdgeChange::Add {
                item: inserted_edge.clone(),
                index: Some(1),
            },
        ],
        &edges,
    );

    let ids: Vec<EdgeId> = updated.iter().map(|edge| edge.id).collect();
    assert_eq!(ids, vec![replacement.id, inserted_edge.id, second]);
    assert_eq!(updated[0].selected, None);
    assert_eq!(updated[2].selected, Some(true));
}

fn node_element(g: &jellyflow_core::core::Graph, id: NodeId) -> XyFlowNodeElement {
    let node = g.nodes().values().next().expect("fixture node").clone();
    XyFlowNodeElement::new(id, node)
}

fn edge_element(g: &jellyflow_core::core::Graph, id: EdgeId) -> XyFlowEdgeElement {
    let edge = g.edges().values().next().expect("fixture edge").clone();
    XyFlowEdgeElement::new(id, edge)
}
