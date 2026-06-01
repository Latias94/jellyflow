use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphId, Group, GroupId,
    Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use crate::runtime::rendering::{
    EdgeRenderOrderOptions, GroupRenderOrderOptions, NodeRenderOrderOptions,
    resolve_edge_render_order, resolve_group_render_order, resolve_node_render_order,
};
use crate::runtime::store::NodeGraphStore;

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
