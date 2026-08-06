use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, NodeOrigin, Port, PortCapacity, PortDirection, PortId, PortKey,
};

use crate::{
    LayoutContext, LayoutEngineRequest, LayoutError, LayoutRequest, LayoutResult,
    builtin_layout_engine_registry,
};

#[test]
fn layout_places_descendants_on_expanding_rings() {
    let (graph, root, left, right, grandchild) = radial_tree_graph();
    let graph = graph.build_unchecked();
    let result = run_default_mind_map_layout(&graph, &LayoutRequest::all()).expect("layout");

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
    let result = run_default_mind_map_layout(&graph, &LayoutRequest::all()).expect("layout");

    let first = result.node_position(first).expect("first");
    let second = result.node_position(second).expect("second");

    assert!(distance(first.center, second.center) > 1.0);
    assert!(result.edge_routes.is_empty());
}

#[test]
fn radial_layout_keeps_a_nonzero_pinned_anchor_in_canvas_coordinates() {
    let (mut graph, root, _, _, _) = radial_tree_graph();
    let root_node = graph.node_mut(&root).expect("root exists");
    root_node.pos = CanvasPoint { x: 240.0, y: 120.0 };
    root_node.origin = Some(NodeOrigin { x: 0.5, y: 0.5 });
    let graph = graph.build_unchecked();

    let result = run_mind_map_layout(
        &graph,
        &LayoutRequest::all(),
        &LayoutContext::new().with_pinned_nodes([root]),
    )
    .expect("layout");

    let root = result.node_position(root).expect("root placement");
    assert_eq!(root.pos, CanvasPoint { x: 240.0, y: 120.0 });
    assert_eq!(root.center, CanvasPoint { x: 240.0, y: 120.0 });
}

#[test]
fn mind_map_layout_handles_a_deep_chain_without_recursive_traversal() {
    const NODE_COUNT: usize = 4_096;

    let graph = deep_chain_graph(NODE_COUNT);
    let result = run_default_mind_map_layout(&graph, &LayoutRequest::all()).expect("layout");

    assert_eq!(result.nodes.len(), NODE_COUNT);
    assert_eq!(result.edge_routes.len(), NODE_COUNT - 1);
    assert!(
        result
            .nodes
            .iter()
            .all(|node| node.pos.x.is_finite() && node.pos.y.is_finite())
    );
}

fn run_default_mind_map_layout(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<LayoutResult, LayoutError> {
    run_mind_map_layout(graph, request, &LayoutContext::default())
}

fn run_mind_map_layout(
    graph: &Graph,
    request: &LayoutRequest,
    context: &LayoutContext,
) -> Result<LayoutResult, LayoutError> {
    builtin_layout_engine_registry().layout(
        graph,
        &LayoutEngineRequest::mind_map_radial(request.clone()),
        context,
    )
}

fn radial_tree_graph() -> (GraphBuilder, NodeId, NodeId, NodeId, NodeId) {
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

    (graph, root, left, right, grandchild)
}

fn disconnected_roots_graph() -> (Graph, NodeId, NodeId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(2));
    let first = NodeId::from_u128(1);
    let second = NodeId::from_u128(2);

    graph.insert_node(first, node("demo.first", Vec::new()));
    graph.insert_node(second, node("demo.second", Vec::new()));

    (graph.build_unchecked(), first, second)
}

fn deep_chain_graph(node_count: usize) -> Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(3));

    for index in 0..node_count {
        let node_id = NodeId::from_u128(index as u128 + 1);
        let incoming = (index > 0).then(|| PortId::from_u128(10_001 + (index as u128 - 1) * 2));
        let outgoing =
            (index + 1 < node_count).then(|| PortId::from_u128(10_000 + index as u128 * 2));
        let mut ports = Vec::with_capacity(2);
        if let Some(incoming) = incoming {
            ports.push(incoming);
        }
        if let Some(outgoing) = outgoing {
            ports.push(outgoing);
        }

        graph.insert_node(node_id, node("demo.chain", ports));
        if let Some(incoming) = incoming {
            graph.insert_port(incoming, port(node_id, "in", PortDirection::In));
        }
        if let Some(outgoing) = outgoing {
            graph.insert_port(outgoing, port(node_id, "out", PortDirection::Out));
        }
    }

    for index in 0..node_count.saturating_sub(1) {
        let source = PortId::from_u128(10_000 + index as u128 * 2);
        let target = PortId::from_u128(10_001 + index as u128 * 2);
        graph.insert_edge(
            EdgeId::from_u128(20_000 + index as u128),
            edge(source, target),
        );
    }

    graph.build_unchecked()
}

fn node(kind: &str, ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
        origin: None,
        selectable: None,
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
    Edge::new(EdgeKind::Data, from, to)
}

fn distance(a: CanvasPoint, b: CanvasPoint) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}
