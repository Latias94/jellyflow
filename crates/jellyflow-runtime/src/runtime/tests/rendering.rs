use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphId, Group, GroupId,
    Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

use crate::io::{NodeGraphEditorConfig, NodeGraphNodeOrigin, NodeGraphViewState};
use crate::runtime::rendering::{
    EdgeRenderOrderOptions, GroupRenderOrderOptions, NodeRenderOrderOptions, VisibleNodeIdsRequest,
    resolve_edge_render_order, resolve_group_render_order, resolve_node_render_order,
    resolve_visible_node_ids, resolve_visible_node_render_order,
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
    let mut graph = Graph::new(GraphId::from_u128(1));
    let a = NodeId::from_u128(1);
    let b = NodeId::from_u128(2);
    let c = NodeId::from_u128(3);
    graph.nodes.insert(a, node("test.a", false));
    graph.nodes.insert(b, node("test.b", false));
    graph.nodes.insert(c, node("test.c", hidden_c));
    (graph, a, b, c)
}

fn graph_with_visible_node_fixture() -> (Graph, NodeId, NodeId, NodeId, NodeId) {
    let mut graph = Graph::new(GraphId::from_u128(1));
    let inside = NodeId::from_u128(1);
    let partial = NodeId::from_u128(2);
    let outside = NodeId::from_u128(3);
    let hidden = NodeId::from_u128(4);
    let size = CanvasSize {
        width: 10.0,
        height: 10.0,
    };

    graph.nodes.insert(
        inside,
        sized_node("test.inside", CanvasPoint { x: 0.0, y: 0.0 }, size, false),
    );
    graph.nodes.insert(
        partial,
        sized_node(
            "test.partial",
            CanvasPoint { x: 95.0, y: 95.0 },
            size,
            false,
        ),
    );
    graph.nodes.insert(
        outside,
        sized_node(
            "test.outside",
            CanvasPoint { x: 140.0, y: 0.0 },
            size,
            false,
        ),
    );
    graph.nodes.insert(
        hidden,
        sized_node("test.hidden", CanvasPoint { x: 0.0, y: 0.0 }, size, true),
    );

    (graph, inside, partial, outside, hidden)
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
    let mut graph = Graph::new(GraphId::from_u128(1));
    let a = GroupId::from_u128(1);
    let b = GroupId::from_u128(2);
    let c = GroupId::from_u128(3);
    graph.groups.insert(a, group("group-a"));
    graph.groups.insert(b, group("group-b"));
    graph.groups.insert(c, group("group-c"));
    (graph, a, b, c)
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
    let mut graph = Graph::new(GraphId::from_u128(1));
    let a = NodeId::from_u128(1);
    let b = NodeId::from_u128(2);
    let c = NodeId::from_u128(3);
    let d = NodeId::from_u128(4);
    for (id, kind) in [(a, "test.a"), (b, "test.b"), (c, "test.c"), (d, "test.d")] {
        graph.nodes.insert(id, node(kind, false));
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
        graph.ports.insert(id, port);
    }

    let e1 = EdgeId::from_u128(1);
    let e2 = EdgeId::from_u128(2);
    let e3 = EdgeId::from_u128(3);
    graph.edges.insert(e1, edge(a_out, b_in, false));
    graph.edges.insert(e2, edge(c_out, d_in, false));
    graph.edges.insert(e3, edge(b_out, c_in, hidden_c));
    (graph, a, e1, e2, e3)
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
        graph,
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
        store.visible_node_ids(viewport_size),
        vec![inside, partial],
        "store helper reads only_render_visible_elements from default runtime tuning"
    );

    let mut uncull_store = store;
    uncull_store.update_editor_config(|config| {
        config.runtime_tuning.only_render_visible_elements = false;
    });
    assert_eq!(
        uncull_store.visible_node_ids(viewport_size),
        vec![inside, partial, outside]
    );

    assert!(
        uncull_store
            .visible_node_ids(CanvasSize::default())
            .is_empty()
    );
}

#[test]
fn visible_node_ids_use_transform_node_origin_and_fallback_size() {
    let mut graph = Graph::new(GraphId::from_u128(1));
    let centered = NodeId::from_u128(10);
    let unsized_id = NodeId::from_u128(20);
    graph.nodes.insert(
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
    graph.nodes.insert(
        unsized_id,
        Node {
            pos: CanvasPoint { x: 0.0, y: 0.0 },
            ..node("test.unsized", false)
        },
    );

    let mut editor_config = NodeGraphEditorConfig::default();
    editor_config.interaction.node_origin = NodeGraphNodeOrigin { x: 0.5, y: 0.0 };
    let store = NodeGraphStore::new(
        graph,
        NodeGraphViewState {
            pan: CanvasPoint { x: -9.5, y: 0.0 },
            zoom: 1.0,
            ..NodeGraphViewState::default()
        },
        editor_config,
    );

    assert_eq!(
        store.visible_node_ids(CanvasSize {
            width: 1.0,
            height: 10.0,
        }),
        vec![centered],
        "store helper maps logical viewport size through current pan and node origin"
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
        store.visible_node_render_order(viewport_size),
        vec![partial, inside]
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
        store.visible_node_render_order(CanvasSize {
            width: 100.0,
            height: 100.0,
        }),
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
