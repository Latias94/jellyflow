use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId,
    Group, GroupId, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
    PortKind,
};

use crate::io::{NodeGraphEditorConfig, NodeGraphNodeOrigin, NodeGraphViewState};
use crate::runtime::measurement::NodeMeasurement;
use crate::runtime::rendering::order::{
    EdgeRenderOrderOptions, GroupRenderOrderOptions, NodeRenderOrderOptions,
    resolve_edge_render_order, resolve_group_render_order, resolve_node_render_order,
};
use crate::runtime::rendering::visibility::{
    VisibleEdgeIdsRequest, VisibleNodeIdsRequest, resolve_visible_edge_ids,
    resolve_visible_edge_render_order, resolve_visible_node_ids, resolve_visible_node_render_order,
};
use crate::runtime::store::NodeGraphStore;
use crate::runtime::viewport::ViewportTransform;

fn node(kind: &str, hidden: bool) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint::default(),
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

fn sized_node(kind: &str, pos: CanvasPoint, size: CanvasSize, hidden: bool) -> Node {
    Node {
        pos,
        size: Some(size),
        ..node(kind, hidden)
    }
}

fn graph_with_three_nodes(hidden_c: bool) -> (Graph, NodeId, NodeId, NodeId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let a = NodeId::from_u128(1);
    let b = NodeId::from_u128(2);
    let c = NodeId::from_u128(3);
    graph.insert_node(a, node("test.a", false));
    graph.insert_node(b, node("test.b", false));
    graph.insert_node(c, node("test.c", hidden_c));
    (graph.into(), a, b, c)
}

fn graph_with_visible_node_fixture() -> (Graph, NodeId, NodeId, NodeId, NodeId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let inside = NodeId::from_u128(1);
    let partial = NodeId::from_u128(2);
    let outside = NodeId::from_u128(3);
    let hidden = NodeId::from_u128(4);
    let size = CanvasSize {
        width: 10.0,
        height: 10.0,
    };

    graph.insert_node(
        inside,
        sized_node("test.inside", CanvasPoint { x: 0.0, y: 0.0 }, size, false),
    );
    graph.insert_node(
        partial,
        sized_node(
            "test.partial",
            CanvasPoint { x: 95.0, y: 95.0 },
            size,
            false,
        ),
    );
    graph.insert_node(
        outside,
        sized_node(
            "test.outside",
            CanvasPoint { x: 140.0, y: 0.0 },
            size,
            false,
        ),
    );
    graph.insert_node(
        hidden,
        sized_node("test.hidden", CanvasPoint { x: 0.0, y: 0.0 }, size, true),
    );

    (graph.into(), inside, partial, outside, hidden)
}

fn group(title: &str) -> Group {
    Group {
        title: title.to_owned(),
        rect: CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 100.0,
                height: 60.0,
            },
        },
        color: None,
    }
}

fn graph_with_three_groups() -> (Graph, GroupId, GroupId, GroupId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let a = GroupId::from_u128(1);
    let b = GroupId::from_u128(2);
    let c = GroupId::from_u128(3);
    graph.insert_group(a, group("group-a"));
    graph.insert_group(b, group("group-b"));
    graph.insert_group(c, group("group-c"));
    (graph.into(), a, b, c)
}

fn port(node: NodeId, dir: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new("p"),
        dir,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn edge(from: PortId, to: PortId, hidden: bool) -> Edge {
    Edge {
        kind: EdgeKind::Data,
        from,
        to,
        hidden,
        selectable: None,
        focusable: None,
        interaction_width: None,
        deletable: None,
        reconnectable: None,
    }
}

fn graph_with_three_edges(hidden_c: bool) -> (Graph, NodeId, EdgeId, EdgeId, EdgeId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let a = NodeId::from_u128(1);
    let b = NodeId::from_u128(2);
    let c = NodeId::from_u128(3);
    let d = NodeId::from_u128(4);
    for (id, kind) in [(a, "test.a"), (b, "test.b"), (c, "test.c"), (d, "test.d")] {
        graph.insert_node(id, node(kind, false));
    }

    let a_out = PortId::from_u128(10);
    let b_in = PortId::from_u128(11);
    let c_out = PortId::from_u128(12);
    let d_in = PortId::from_u128(13);
    let b_out = PortId::from_u128(14);
    let c_in = PortId::from_u128(15);
    for (id, port) in [
        (a_out, port(a, PortDirection::Out)),
        (b_in, port(b, PortDirection::In)),
        (c_out, port(c, PortDirection::Out)),
        (d_in, port(d, PortDirection::In)),
        (b_out, port(b, PortDirection::Out)),
        (c_in, port(c, PortDirection::In)),
    ] {
        graph.insert_port(id, port);
    }

    let e1 = EdgeId::from_u128(1);
    let e2 = EdgeId::from_u128(2);
    let e3 = EdgeId::from_u128(3);
    graph.insert_edge(e1, edge(a_out, b_in, false));
    graph.insert_edge(e2, edge(c_out, d_in, false));
    graph.insert_edge(e3, edge(b_out, c_in, hidden_c));
    (graph.into(), a, e1, e2, e3)
}

fn graph_with_visible_edge_fixture() -> (Graph, EdgeId, EdgeId, EdgeId, EdgeId, EdgeId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(2));
    let size = CanvasSize {
        width: 10.0,
        height: 10.0,
    };

    let left = NodeId::from_u128(10);
    let right = NodeId::from_u128(11);
    let outside_a = NodeId::from_u128(12);
    let outside_b = NodeId::from_u128(13);
    let inside = NodeId::from_u128(14);
    let hidden = NodeId::from_u128(15);
    for (id, pos, hidden) in [
        (left, CanvasPoint { x: -20.0, y: 10.0 }, false),
        (right, CanvasPoint { x: 110.0, y: 10.0 }, false),
        (outside_a, CanvasPoint { x: 140.0, y: 0.0 }, false),
        (outside_b, CanvasPoint { x: 160.0, y: 0.0 }, false),
        (inside, CanvasPoint { x: 20.0, y: 20.0 }, false),
        (hidden, CanvasPoint { x: 0.0, y: 0.0 }, true),
    ] {
        graph.insert_node(id, sized_node("test.edge-endpoint", pos, size, hidden));
    }

    let left_out = PortId::from_u128(20);
    let right_in = PortId::from_u128(21);
    let outside_a_out = PortId::from_u128(22);
    let outside_b_in = PortId::from_u128(23);
    let inside_out = PortId::from_u128(24);
    let inside_in = PortId::from_u128(25);
    let hidden_out = PortId::from_u128(26);
    for (id, port) in [
        (left_out, port(left, PortDirection::Out)),
        (right_in, port(right, PortDirection::In)),
        (outside_a_out, port(outside_a, PortDirection::Out)),
        (outside_b_in, port(outside_b, PortDirection::In)),
        (inside_out, port(inside, PortDirection::Out)),
        (inside_in, port(inside, PortDirection::In)),
        (hidden_out, port(hidden, PortDirection::Out)),
    ] {
        graph.insert_port(id, port);
    }

    let spanning = EdgeId::from_u128(10);
    let outside = EdgeId::from_u128(11);
    let hidden_edge = EdgeId::from_u128(12);
    let hidden_endpoint = EdgeId::from_u128(13);
    let inside_edge = EdgeId::from_u128(14);
    graph.insert_edge(spanning, edge(left_out, right_in, false));
    graph.insert_edge(outside, edge(outside_a_out, outside_b_in, false));
    graph.insert_edge(hidden_edge, edge(inside_out, right_in, true));
    graph.insert_edge(hidden_endpoint, edge(hidden_out, inside_in, false));
    graph.insert_edge(inside_edge, edge(inside_out, left_out, false));

    (
        graph.into(),
        spanning,
        outside,
        hidden_edge,
        hidden_endpoint,
        inside_edge,
    )
}

#[test]
fn node_render_order_respects_draw_order_and_elevates_selected_nodes() {
    let (graph, a, b, c) = graph_with_three_nodes(false);
    let view_state = NodeGraphViewState {
        selected_nodes: vec![c],
        draw_order: vec![c, a, c, NodeId::from_u128(404)],
        ..NodeGraphViewState::default()
    };

    assert_eq!(
        resolve_node_render_order(&graph, &view_state, NodeRenderOrderOptions::default()),
        vec![a, b, c],
        "selected nodes are moved after non-selected nodes while preserving base order"
    );

    assert_eq!(
        resolve_node_render_order(
            &graph,
            &view_state,
            NodeRenderOrderOptions {
                elevate_nodes_on_select: false,
                ..NodeRenderOrderOptions::default()
            },
        ),
        vec![c, a, b],
        "explicit draw order is the base order when elevation is disabled"
    );
}

#[test]
fn node_render_order_filters_hidden_nodes_unless_requested() {
    let (graph, a, b, c) = graph_with_three_nodes(true);
    let view_state = NodeGraphViewState {
        selected_nodes: vec![c],
        draw_order: vec![c, a],
        ..NodeGraphViewState::default()
    };

    assert_eq!(
        resolve_node_render_order(&graph, &view_state, NodeRenderOrderOptions::default()),
        vec![a, b]
    );

    assert_eq!(
        resolve_node_render_order(
            &graph,
            &view_state,
            NodeRenderOrderOptions {
                include_hidden: true,
                ..NodeRenderOrderOptions::default()
            },
        ),
        vec![a, b, c],
        "hidden selected nodes can be included explicitly and still elevate"
    );
}

#[test]
fn group_render_order_respects_group_draw_order_and_elevates_selected_groups() {
    let (graph, a, b, c) = graph_with_three_groups();
    let view_state = NodeGraphViewState {
        selected_groups: vec![c],
        group_draw_order: vec![c, a, c, GroupId::from_u128(404)],
        ..NodeGraphViewState::default()
    };

    assert_eq!(
        resolve_group_render_order(&graph, &view_state, GroupRenderOrderOptions::default()),
        vec![a, b, c],
        "selected groups are moved after non-selected groups while preserving base order"
    );

    assert_eq!(
        resolve_group_render_order(
            &graph,
            &view_state,
            GroupRenderOrderOptions {
                elevate_groups_on_select: false,
            },
        ),
        vec![c, a, b],
        "explicit group draw order is the base order when elevation is disabled"
    );
}

#[test]
fn edge_render_order_elevates_selected_edges_and_edges_connected_to_selected_nodes() {
    let (graph, selected_node, e1, e2, e3) = graph_with_three_edges(false);
    let view_state = NodeGraphViewState {
        selected_nodes: vec![selected_node],
        selected_edges: vec![e2],
        edge_draw_order: vec![e3, e1, e3, EdgeId::from_u128(404)],
        ..NodeGraphViewState::default()
    };

    assert_eq!(
        resolve_edge_render_order(&graph, &view_state, EdgeRenderOrderOptions::default()),
        vec![e3, e1, e2],
        "edges connected to selected nodes and selected edges are moved after non-elevated edges"
    );

    assert_eq!(
        resolve_edge_render_order(
            &graph,
            &view_state,
            EdgeRenderOrderOptions {
                elevate_edges_on_select: false,
                ..EdgeRenderOrderOptions::default()
            },
        ),
        vec![e3, e1, e2],
        "explicit edge draw order is the base order when elevation is disabled"
    );
}

#[test]
fn edge_render_order_filters_hidden_edges_unless_requested() {
    let (graph, selected_node, e1, e2, e3) = graph_with_three_edges(true);
    let view_state = NodeGraphViewState {
        selected_nodes: vec![selected_node],
        selected_edges: vec![e3],
        ..NodeGraphViewState::default()
    };

    assert_eq!(
        resolve_edge_render_order(&graph, &view_state, EdgeRenderOrderOptions::default()),
        vec![e2, e1]
    );

    assert_eq!(
        resolve_edge_render_order(
            &graph,
            &view_state,
            EdgeRenderOrderOptions {
                include_hidden: true,
                ..EdgeRenderOrderOptions::default()
            },
        ),
        vec![e2, e1, e3],
        "hidden selected edges can be included explicitly and still elevate"
    );
}

#[test]
fn visible_node_ids_follow_viewport_and_rendering_tuning() {
    let (graph, inside, partial, outside, hidden) = graph_with_visible_node_fixture();
    let store = NodeGraphStore::new(
        graph.into(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let viewport_size = CanvasSize {
        width: 100.0,
        height: 100.0,
    };
    let request = VisibleNodeIdsRequest::new(
        ViewportTransform::new(CanvasPoint::default(), 1.0).expect("viewport"),
        viewport_size,
    )
    .with_only_render_visible_elements(true);

    assert_eq!(
        resolve_visible_node_ids(store.lookups(), request),
        vec![inside, partial],
        "culling uses the current viewport and includes partially visible nodes"
    );

    let uncull_ids = resolve_visible_node_ids(
        store.lookups(),
        request.with_only_render_visible_elements(false),
    );
    assert_eq!(
        uncull_ids,
        vec![inside, partial, outside],
        "disabled culling returns all non-hidden node ids in deterministic order"
    );
    assert!(!uncull_ids.contains(&hidden));

    assert_eq!(
        store.rendering_query(viewport_size).visible_node_ids,
        vec![inside, partial],
        "store query reads only_render_visible_elements from default runtime tuning"
    );

    let mut uncull_store = store;
    uncull_store.update_editor_config(|config| {
        config.runtime_tuning.only_render_visible_elements = false;
    });
    assert_eq!(
        uncull_store.rendering_query(viewport_size).visible_node_ids,
        vec![inside, partial, outside]
    );

    assert!(
        uncull_store
            .rendering_query(CanvasSize::default())
            .visible_node_ids
            .is_empty()
    );
}

#[test]
fn visible_node_ids_use_transform_node_origin_and_fallback_size() {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let centered = NodeId::from_u128(10);
    let unsized_id = NodeId::from_u128(20);
    graph.insert_node(
        centered,
        sized_node(
            "test.centered",
            CanvasPoint { x: 15.0, y: 0.0 },
            CanvasSize {
                width: 10.0,
                height: 10.0,
            },
            false,
        ),
    );
    graph.insert_node(
        unsized_id,
        Node {
            pos: CanvasPoint { x: 0.0, y: 0.0 },
            ..node("test.unsized", false)
        },
    );

    let mut editor_config = NodeGraphEditorConfig::default();
    editor_config.interaction.node_origin = NodeGraphNodeOrigin { x: 0.5, y: 0.0 };
    let store = NodeGraphStore::new(
        graph.into(),
        NodeGraphViewState {
            pan: CanvasPoint { x: -9.5, y: 0.0 },
            zoom: 1.0,
            ..NodeGraphViewState::default()
        },
        editor_config,
    );

    assert_eq!(
        store
            .rendering_query(CanvasSize {
                width: 1.0,
                height: 10.0,
            })
            .visible_node_ids,
        vec![centered],
        "store query maps logical viewport size through current pan and node origin"
    );

    let request = VisibleNodeIdsRequest::new(
        ViewportTransform::new(CanvasPoint::default(), 1.0).expect("viewport"),
        CanvasSize {
            width: 10.0,
            height: 10.0,
        },
    );
    assert!(resolve_visible_node_ids(store.lookups(), request).is_empty());
    assert_eq!(
        resolve_visible_node_ids(
            store.lookups(),
            request.with_fallback_size(Some(CanvasSize {
                width: 5.0,
                height: 5.0,
            })),
        ),
        vec![unsized_id],
        "unsized nodes participate in culling only when the adapter supplies a fallback size"
    );
}

#[test]
fn visible_node_render_order_filters_visible_ids_through_node_render_order() {
    let (graph, inside, partial, outside, hidden) = graph_with_visible_node_fixture();
    let view_state = NodeGraphViewState {
        selected_nodes: vec![inside],
        draw_order: vec![outside, partial, inside, hidden],
        ..NodeGraphViewState::default()
    };
    let store = NodeGraphStore::new(graph, view_state, NodeGraphEditorConfig::default());
    let viewport_size = CanvasSize {
        width: 100.0,
        height: 100.0,
    };
    let request = VisibleNodeIdsRequest::new(
        ViewportTransform::new(CanvasPoint::default(), 1.0).expect("viewport"),
        viewport_size,
    );

    assert_eq!(
        resolve_visible_node_render_order(
            store.graph(),
            store.lookups(),
            store.view_state(),
            request,
            NodeRenderOrderOptions::default(),
        ),
        vec![partial, inside],
        "outside and hidden nodes are removed before the selected visible node is elevated"
    );
    assert_eq!(
        store
            .rendering_query(viewport_size)
            .visible_node_render_order,
        vec![partial, inside]
    );
}

#[test]
fn visible_edge_ids_follow_endpoint_union_and_rendering_tuning() {
    let (graph, spanning, outside, hidden_edge, hidden_endpoint, inside_edge) =
        graph_with_visible_edge_fixture();
    let store = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let viewport_size = CanvasSize {
        width: 100.0,
        height: 100.0,
    };
    let request = VisibleEdgeIdsRequest::new(
        ViewportTransform::new(CanvasPoint::default(), 1.0).expect("viewport"),
        viewport_size,
    )
    .with_only_render_visible_elements(true);

    assert_eq!(
        resolve_visible_edge_ids(store.graph(), store.lookups(), request),
        vec![spanning, inside_edge],
        "edge culling uses the union of endpoint node bounds, not endpoint-node visibility"
    );

    let uncull_ids = resolve_visible_edge_ids(
        store.graph(),
        store.lookups(),
        request.with_only_render_visible_elements(false),
    );
    assert_eq!(
        uncull_ids,
        vec![spanning, outside, hidden_endpoint, inside_edge],
        "disabled culling returns all non-hidden edge ids in deterministic order"
    );
    assert!(!uncull_ids.contains(&hidden_edge));

    assert_eq!(
        store.rendering_query(viewport_size).visible_edge_ids,
        vec![spanning, inside_edge],
        "store query reads only_render_visible_elements from default runtime tuning"
    );

    let mut uncull_store = store;
    uncull_store.update_editor_config(|config| {
        config.runtime_tuning.only_render_visible_elements = false;
    });
    assert_eq!(
        uncull_store.rendering_query(viewport_size).visible_edge_ids,
        vec![spanning, outside, hidden_endpoint, inside_edge]
    );

    assert!(
        uncull_store
            .rendering_query(CanvasSize::default())
            .visible_edge_ids
            .is_empty()
    );
}

#[test]
fn visible_edge_render_order_filters_visible_ids_through_edge_render_order() {
    let (graph, spanning, outside, _hidden_edge, hidden_endpoint, inside_edge) =
        graph_with_visible_edge_fixture();
    let view_state = NodeGraphViewState {
        selected_edges: vec![spanning],
        edge_draw_order: vec![outside, spanning, inside_edge, hidden_endpoint],
        ..NodeGraphViewState::default()
    };
    let store = NodeGraphStore::new(graph, view_state, NodeGraphEditorConfig::default());
    let viewport_size = CanvasSize {
        width: 100.0,
        height: 100.0,
    };
    let request = VisibleEdgeIdsRequest::new(
        ViewportTransform::new(CanvasPoint::default(), 1.0).expect("viewport"),
        viewport_size,
    );

    assert_eq!(
        resolve_visible_edge_render_order(
            store.graph(),
            store.lookups(),
            store.view_state(),
            request,
            EdgeRenderOrderOptions::default(),
        ),
        vec![inside_edge, spanning],
        "outside and hidden-endpoint edges are removed before the selected visible edge is elevated"
    );
    assert_eq!(
        store
            .rendering_query(viewport_size)
            .visible_edge_render_order,
        vec![inside_edge, spanning]
    );
}

#[test]
fn store_rendering_query_returns_order_and_visibility_together() {
    let (graph, inside, partial, outside, hidden) = graph_with_visible_node_fixture();
    let view_state = NodeGraphViewState {
        selected_nodes: vec![inside],
        draw_order: vec![outside, partial, inside, hidden],
        ..NodeGraphViewState::default()
    };
    let store = NodeGraphStore::new(graph, view_state, NodeGraphEditorConfig::default());

    let query = store.rendering_query(CanvasSize {
        width: 100.0,
        height: 100.0,
    });

    assert_eq!(query.group_order, Vec::<GroupId>::new());
    assert_eq!(query.edge_order, Vec::<EdgeId>::new());
    assert_eq!(query.node_order, vec![outside, partial, inside]);
    assert_eq!(query.visible_node_ids, vec![inside, partial]);
    assert_eq!(query.visible_node_render_order, vec![partial, inside]);
    assert_eq!(query.visible_edge_ids, Vec::<EdgeId>::new());
    assert_eq!(query.visible_edge_render_order, Vec::<EdgeId>::new());
}

#[test]
fn store_rendering_query_returns_visible_edge_order_and_visibility_together() {
    let (graph, spanning, outside, _hidden_edge, hidden_endpoint, inside_edge) =
        graph_with_visible_edge_fixture();
    let view_state = NodeGraphViewState {
        selected_edges: vec![spanning],
        edge_draw_order: vec![outside, spanning, inside_edge, hidden_endpoint],
        ..NodeGraphViewState::default()
    };
    let store = NodeGraphStore::new(graph, view_state, NodeGraphEditorConfig::default());

    let query = store.rendering_query(CanvasSize {
        width: 100.0,
        height: 100.0,
    });

    assert_eq!(
        query.edge_order,
        vec![outside, inside_edge, hidden_endpoint, spanning]
    );
    assert_eq!(query.visible_edge_ids, vec![spanning, inside_edge]);
    assert_eq!(query.visible_edge_render_order, vec![inside_edge, spanning]);
}

#[test]
fn store_rendering_query_combines_measurements_visibility_and_selected_elevation() {
    let mut graph = GraphBuilder::new(GraphId::from_u128(3));
    let measured_selected = NodeId::from_u128(10);
    let sized_middle = NodeId::from_u128(11);
    let sized_right = NodeId::from_u128(12);
    let unmeasured = NodeId::from_u128(13);
    graph.insert_node(
        measured_selected,
        Node {
            pos: CanvasPoint::default(),
            ..node("test.measured-selected", false)
        },
    );
    graph.insert_node(
        sized_middle,
        sized_node(
            "test.sized-middle",
            CanvasPoint { x: 20.0, y: 0.0 },
            CanvasSize {
                width: 10.0,
                height: 10.0,
            },
            false,
        ),
    );
    graph.insert_node(
        sized_right,
        sized_node(
            "test.sized-right",
            CanvasPoint { x: 40.0, y: 0.0 },
            CanvasSize {
                width: 10.0,
                height: 10.0,
            },
            false,
        ),
    );
    graph.insert_node(
        unmeasured,
        Node {
            pos: CanvasPoint { x: 5.0, y: 0.0 },
            ..node("test.unmeasured", false)
        },
    );

    let selected_out = PortId::from_u128(20);
    let middle_in = PortId::from_u128(21);
    let middle_out = PortId::from_u128(22);
    let right_in = PortId::from_u128(23);
    for (id, port) in [
        (selected_out, port(measured_selected, PortDirection::Out)),
        (middle_in, port(sized_middle, PortDirection::In)),
        (middle_out, port(sized_middle, PortDirection::Out)),
        (right_in, port(sized_right, PortDirection::In)),
    ] {
        graph.insert_port(id, port);
    }

    let elevated_edge = EdgeId::from_u128(30);
    let normal_edge = EdgeId::from_u128(31);
    graph.insert_edge(elevated_edge, edge(selected_out, middle_in, false));
    graph.insert_edge(normal_edge, edge(middle_out, right_in, false));

    let view_state = NodeGraphViewState {
        selected_nodes: vec![measured_selected],
        edge_draw_order: vec![elevated_edge, normal_edge],
        draw_order: vec![measured_selected, sized_middle, sized_right, unmeasured],
        ..NodeGraphViewState::default()
    };
    let mut store = NodeGraphStore::new(graph.into(), view_state, NodeGraphEditorConfig::default());
    store
        .report_node_measurement(NodeMeasurement::new(measured_selected).with_size(Some(
            CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        )))
        .expect("node measurement");

    let query = store.rendering_query(CanvasSize {
        width: 100.0,
        height: 100.0,
    });

    assert_eq!(
        query.node_order,
        vec![sized_middle, sized_right, unmeasured, measured_selected],
        "selected node elevation applies to the full non-hidden node paint order"
    );
    assert_eq!(
        query.visible_node_ids,
        vec![measured_selected, sized_middle, sized_right],
        "reported measurement makes the unsized selected node visible while the unmeasured node stays culled"
    );
    assert_eq!(
        query.visible_node_render_order,
        vec![sized_middle, sized_right, measured_selected],
        "visible node order keeps selected elevation after culling"
    );
    assert_eq!(query.edge_order, vec![normal_edge, elevated_edge]);
    assert_eq!(query.visible_edge_ids, vec![elevated_edge, normal_edge]);
    assert_eq!(
        query.visible_edge_render_order,
        vec![normal_edge, elevated_edge],
        "edge visibility and selected-node elevation are resolved in one store-level query"
    );
}

#[test]
fn visible_node_render_order_matches_node_render_order_when_culling_is_disabled() {
    let (graph, inside, partial, outside, hidden) = graph_with_visible_node_fixture();
    let view_state = NodeGraphViewState {
        selected_nodes: vec![inside],
        draw_order: vec![outside, partial, inside, hidden],
        ..NodeGraphViewState::default()
    };
    let mut store = NodeGraphStore::new(graph, view_state, NodeGraphEditorConfig::default());
    store.update_editor_config(|config| {
        config.runtime_tuning.only_render_visible_elements = false;
    });

    assert_eq!(
        store
            .rendering_query(CanvasSize {
                width: 100.0,
                height: 100.0,
            })
            .visible_node_render_order,
        store.node_render_order(),
        "disabled culling keeps the same paint order as the non-culling node render contract"
    );
}

#[test]
fn store_group_render_order_uses_resolved_editor_config() {
    let (graph, a, b, c) = graph_with_three_groups();
    let view_state = NodeGraphViewState {
        selected_groups: vec![c],
        group_draw_order: vec![c, a],
        ..NodeGraphViewState::default()
    };
    let mut store = NodeGraphStore::new(graph, view_state, NodeGraphEditorConfig::default());

    assert_eq!(store.group_render_order(), vec![a, b, c]);

    store.update_editor_config(|config| {
        config.interaction.elevate_nodes_on_select = false;
    });

    assert_eq!(store.group_render_order(), vec![c, a, b]);
}

#[test]
fn store_node_render_order_uses_resolved_editor_config() {
    let (graph, a, b, c) = graph_with_three_nodes(false);
    let view_state = NodeGraphViewState {
        selected_nodes: vec![c],
        draw_order: vec![c, a],
        ..NodeGraphViewState::default()
    };
    let mut store = NodeGraphStore::new(graph, view_state, NodeGraphEditorConfig::default());

    assert_eq!(store.node_render_order(), vec![a, b, c]);

    store.update_editor_config(|config| {
        config.interaction.elevate_nodes_on_select = false;
    });

    assert_eq!(store.node_render_order(), vec![c, a, b]);
}

#[test]
fn store_edge_render_order_uses_resolved_editor_config() {
    let (graph, selected_node, e1, e2, e3) = graph_with_three_edges(false);
    let view_state = NodeGraphViewState {
        selected_nodes: vec![selected_node],
        selected_edges: vec![e2],
        ..NodeGraphViewState::default()
    };
    let mut store = NodeGraphStore::new(graph, view_state, NodeGraphEditorConfig::default());

    assert_eq!(store.edge_render_order(), vec![e3, e1, e2]);

    store.update_editor_config(|config| {
        config.interaction.elevate_edges_on_select = false;
    });

    assert_eq!(store.edge_render_order(), vec![e1, e2, e3]);
}
