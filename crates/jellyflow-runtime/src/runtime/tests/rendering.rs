use jellyflow_core::core::{CanvasPoint, Graph, GraphId, Node, NodeId, NodeKindKey};

use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use crate::runtime::rendering::{NodeRenderOrderOptions, resolve_node_render_order};
use crate::runtime::store::NodeGraphStore;

fn node(kind: &str, hidden: bool) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint::default(),
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
