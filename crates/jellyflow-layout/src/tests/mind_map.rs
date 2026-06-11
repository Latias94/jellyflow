use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphId, Node, NodeId, NodeKindKey,
    Port, PortCapacity, PortDirection, PortId, PortKey,
};

use crate::{
    LayoutContext, LayoutEngine, LayoutEngineId, LayoutRequest, MIND_MAP_RADIAL_LAYOUT_ENGINE_ID,
    MindMapRadialLayoutEngine, builtin_layout_engine_registry, layout_graph_with_mind_map_radial,
};

#[test]
fn builtin_registry_contains_mind_map_radial_engine() {
    let registry = builtin_layout_engine_registry();

    assert!(registry.get(&LayoutEngineId::dugong()).is_some());
    assert!(registry.get(&LayoutEngineId::mind_map_radial()).is_some());
    assert_eq!(
        LayoutEngineId::mind_map_radial().as_str(),
        MIND_MAP_RADIAL_LAYOUT_ENGINE_ID
    );
}

#[test]
fn wrapper_matches_engine_entry_point() {
    let (graph, _root, _left, _right, _grandchild) = radial_tree_graph();
    let request = LayoutRequest::all();

    let wrapper = layout_graph_with_mind_map_radial(&graph, &request).expect("wrapper");
    let engine = MindMapRadialLayoutEngine
        .layout(&graph, &request, &LayoutContext::default())
        .expect("engine");

    assert_eq!(wrapper, engine);
}

#[test]
fn layout_places_descendants_on_expanding_rings() {
    let (graph, root, left, right, grandchild) = radial_tree_graph();
    let result = layout_graph_with_mind_map_radial(&graph, &LayoutRequest::all()).expect("layout");

    let root = result.node_position(root).expect("root");
    let left = result.node_position(left).expect("left");
    let right = result.node_position(right).expect("right");
    let grandchild = result.node_position(grandchild).expect("grandchild");

    let left_distance = distance(root.center, left.center);
    let right_distance = distance(root.center, right.center);
    let grandchild_distance = distance(left.center, grandchild.center);

    assert!((left_distance - right_distance).abs() <= 1.0e-3);
    assert!(grandchild_distance > left_distance + 1.0e-3);
    assert_eq!(result.edge_routes.len(), 3);
    assert!(result.bounds.is_some());
}

#[test]
fn layout_separates_disconnected_roots() {
    let (graph, first, second) = disconnected_roots_graph();
    let result = layout_graph_with_mind_map_radial(&graph, &LayoutRequest::all()).expect("layout");

    let first = result.node_position(first).expect("first");
    let second = result.node_position(second).expect("second");

    assert!(distance(first.center, second.center) > 1.0);
    assert!(result.edge_routes.is_empty());
}

fn radial_tree_graph() -> (Graph, NodeId, NodeId, NodeId, NodeId) {
    let mut graph = Graph::new(GraphId::from_u128(1));
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

    graph.nodes.insert(root, node("demo.root", vec![root_out]));
    graph
        .nodes
        .insert(left, node("demo.left", vec![left_in, left_out]));
    graph
        .nodes
        .insert(right, node("demo.right", vec![right_in]));
    graph
        .nodes
        .insert(grandchild, node("demo.grandchild", vec![grandchild_in]));

    graph
        .ports
        .insert(root_out, port(root, "out", PortDirection::Out));
    graph
        .ports
        .insert(left_in, port(left, "in", PortDirection::In));
    graph
        .ports
        .insert(left_out, port(left, "out", PortDirection::Out));
    graph
        .ports
        .insert(right_in, port(right, "in", PortDirection::In));
    graph
        .ports
        .insert(grandchild_in, port(grandchild, "in", PortDirection::In));

    graph.edges.insert(first_edge, edge(root_out, left_in));
    graph.edges.insert(second_edge, edge(root_out, right_in));
    graph
        .edges
        .insert(third_edge, edge(left_out, grandchild_in));

    (graph, root, left, right, grandchild)
}

fn disconnected_roots_graph() -> (Graph, NodeId, NodeId) {
    let mut graph = Graph::new(GraphId::from_u128(2));
    let first = NodeId::from_u128(1);
    let second = NodeId::from_u128(2);

    graph.nodes.insert(first, node("demo.first", Vec::new()));
    graph.nodes.insert(second, node("demo.second", Vec::new()));

    (graph, first, second)
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

fn distance(a: CanvasPoint, b: CanvasPoint) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}
