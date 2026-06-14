use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
};

use crate::{
    LayoutContext, LayoutDirection, LayoutEngine, LayoutEngineId, LayoutOptions, LayoutRequest,
    TIDY_TREE_LAYOUT_ENGINE_ID, TidyTreeLayoutEngine, builtin_layout_engine_registry,
    layout_graph_with_tidy_tree,
};

#[test]
fn builtin_registry_contains_tidy_tree_engine() {
    let registry = builtin_layout_engine_registry();

    assert!(registry.get(&LayoutEngineId::tidy_tree()).is_some());
    assert_eq!(
        LayoutEngineId::tidy_tree().as_str(),
        TIDY_TREE_LAYOUT_ENGINE_ID
    );
    assert!(
        registry
            .metadata(&LayoutEngineId::tidy_tree())
            .expect("metadata")
            .name
            .contains("Tidy")
    );
}

#[test]
fn wrapper_matches_engine_entry_point() {
    let (graph, _root, _left, _right, _grandchild) = tidy_tree_graph();
    let request = LayoutRequest::all();

    let wrapper = layout_graph_with_tidy_tree(&graph, &request).expect("wrapper");
    let engine = TidyTreeLayoutEngine
        .layout(&graph, &request, &LayoutContext::default())
        .expect("engine");

    assert_eq!(wrapper, engine);
}

#[test]
fn top_to_bottom_layout_centers_parent_above_children() {
    let (graph, root, left, right, grandchild) = tidy_tree_graph();
    let result = layout_graph_with_tidy_tree(&graph, &LayoutRequest::all()).expect("layout");

    let root = result.node_position(root).expect("root");
    let left = result.node_position(left).expect("left");
    let right = result.node_position(right).expect("right");
    let grandchild = result.node_position(grandchild).expect("grandchild");

    assert!(left.center.y > root.center.y);
    assert!(right.center.y > root.center.y);
    assert!(grandchild.center.y > left.center.y);
    assert!((root.center.x - midpoint(left.center.x, right.center.x)).abs() <= 1.0e-3);
    assert!(right.center.x > left.center.x);
    assert_eq!(result.edge_routes.len(), 3);
    assert!(result.bounds.is_some());
}

#[test]
fn left_to_right_layout_changes_growth_axis() {
    let (graph, root, left, right, _grandchild) = tidy_tree_graph();
    let result = layout_graph_with_tidy_tree(
        &graph,
        &LayoutRequest::all()
            .with_options(LayoutOptions::default().with_direction(LayoutDirection::LeftToRight)),
    )
    .expect("layout");

    let root = result.node_position(root).expect("root");
    let left = result.node_position(left).expect("left");
    let right = result.node_position(right).expect("right");

    assert!(left.center.x > root.center.x);
    assert!(right.center.x > root.center.x);
    assert!(right.center.y > left.center.y);
}

#[test]
fn siblings_do_not_overlap_with_default_spacing() {
    let (graph, _root, left, right, _grandchild) = tidy_tree_graph();
    let result = layout_graph_with_tidy_tree(&graph, &LayoutRequest::all()).expect("layout");

    let left = result.node_position(left).expect("left");
    let right = result.node_position(right).expect("right");
    let left_right_edge = left.center.x + left.size.width * 0.5;
    let right_left_edge = right.center.x - right.size.width * 0.5;

    assert!(right_left_edge >= left_right_edge);
}

#[test]
fn disconnected_roots_are_separated() {
    let (graph, first, second) = disconnected_roots_graph();
    let result = layout_graph_with_tidy_tree(&graph, &LayoutRequest::all()).expect("layout");

    let first = result.node_position(first).expect("first");
    let second = result.node_position(second).expect("second");

    assert!(second.center.x > first.center.x);
    assert!(result.edge_routes.is_empty());
}

#[test]
fn hidden_nodes_edges_and_scope_are_excluded() {
    let (mut graph, root, left, right, _grandchild) = tidy_tree_graph();
    graph
        .update_node(&right, |node| node.hidden = true)
        .expect("node exists");

    let hidden_node_result =
        layout_graph_with_tidy_tree(&graph, &LayoutRequest::all()).expect("hidden node");

    assert!(hidden_node_result.node_position(root).is_some());
    assert!(hidden_node_result.node_position(left).is_some());
    assert!(hidden_node_result.node_position(right).is_none());
    assert_eq!(hidden_node_result.edge_routes.len(), 2);

    let scoped_result =
        layout_graph_with_tidy_tree(&graph, &LayoutRequest::nodes([root, right])).expect("scope");

    assert!(scoped_result.node_position(root).is_some());
    assert!(scoped_result.node_position(left).is_none());
    assert!(scoped_result.node_position(right).is_none());
    assert!(scoped_result.edge_routes.is_empty());
}

#[test]
fn cycles_are_projected_as_stable_tree_without_looping() {
    let (graph, root, left, right, _grandchild) = tidy_tree_graph();
    let mut graph = GraphBuilder::from_graph(graph);
    let left_out = PortId::from_u128(30);
    let root_in = PortId::from_u128(31);
    let cycle_edge = EdgeId::from_u128(32);

    graph
        .update_node(&left, |node| node.ports.push(left_out))
        .expect("left node exists");
    graph
        .update_node(&root, |node| node.ports.push(root_in))
        .expect("root node exists");
    graph.insert_port(left_out, port(left, "cycle-out", PortDirection::Out));
    graph.insert_port(root_in, port(root, "cycle-in", PortDirection::In));
    graph.insert_edge(cycle_edge, edge(left_out, root_in));

    let graph = graph.build_unchecked();
    let result = layout_graph_with_tidy_tree(&graph, &LayoutRequest::all()).expect("layout");

    assert_eq!(result.nodes.len(), 4);
    assert!(result.node_position(root).is_some());
    assert!(result.node_position(left).is_some());
    assert!(result.node_position(right).is_some());
    assert!(
        result
            .edge_routes
            .iter()
            .any(|route| route.edge == cycle_edge)
    );
}

fn tidy_tree_graph() -> (Graph, NodeId, NodeId, NodeId, NodeId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let root = NodeId::from_u128(1);
    let left = NodeId::from_u128(2);
    let right = NodeId::from_u128(3);
    let grandchild = NodeId::from_u128(4);
    let root_out = PortId::from_u128(10);
    let left_in = PortId::from_u128(11);
    let left_out = PortId::from_u128(12);
    let right_in = PortId::from_u128(13);
    let grandchild_in = PortId::from_u128(14);
    let first_edge = EdgeId::from_u128(20);
    let second_edge = EdgeId::from_u128(21);
    let third_edge = EdgeId::from_u128(22);

    graph.insert_node(root, node("demo.root", vec![root_out]));
    graph.insert_node(left, node("demo.left", vec![left_in, left_out]));
    graph.insert_node(right, node("demo.right", vec![right_in]));
    graph.insert_node(grandchild, node("demo.grandchild", vec![grandchild_in]));

    graph.insert_port(root_out, port(root, "out", PortDirection::Out));
    graph.insert_port(left_in, port(left, "in", PortDirection::In));
    graph.insert_port(left_out, port(left, "out", PortDirection::Out));
    graph.insert_port(right_in, port(right, "in", PortDirection::In));
    graph.insert_port(grandchild_in, port(grandchild, "in", PortDirection::In));

    graph.insert_edge(first_edge, edge(root_out, left_in));
    graph.insert_edge(second_edge, edge(root_out, right_in));
    graph.insert_edge(third_edge, edge(left_out, grandchild_in));

    (graph.build_unchecked(), root, left, right, grandchild)
}

fn disconnected_roots_graph() -> (Graph, NodeId, NodeId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(2));
    let first = NodeId::from_u128(1);
    let second = NodeId::from_u128(2);

    graph.insert_node(first, node("demo.first", Vec::new()));
    graph.insert_node(second, node("demo.second", Vec::new()));

    (graph.build_unchecked(), first, second)
}

fn node(kind: &str, ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 160.0,
            height: 80.0,
        }),
        hidden: false,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}

fn port(node: NodeId, key: &str, dir: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir,
        kind: jellyflow_core::PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn edge(from: PortId, to: PortId) -> Edge {
    Edge {
        kind: EdgeKind::Data,
        from,
        to,
        hidden: false,
        selectable: None,
        focusable: None,
        interaction_width: None,
        deletable: None,
        reconnectable: None,
    }
}

fn midpoint(left: f32, right: f32) -> f32 {
    (left + right) * 0.5
}
