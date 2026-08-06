use jellyflow_core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
    Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

use crate::{
    LayoutContext, LayoutDirection, LayoutEngineRequest, LayoutError, LayoutOptions, LayoutRequest,
    LayoutResult, builtin_layout_engine_registry,
};

#[test]
fn layout_direction_changes_axis_ordering() {
    let (graph, a, b, _edge) = connected_graph();
    let tb = run_default_dugong_layout(&graph, &LayoutRequest::all()).expect("tb layout");
    let lr = run_default_dugong_layout(
        &graph,
        &LayoutRequest::all()
            .with_options(LayoutOptions::default().with_direction(LayoutDirection::LeftToRight)),
    )
    .expect("lr layout");

    let tb_a = tb.node_position(a).expect("tb a");
    let tb_b = tb.node_position(b).expect("tb b");
    let lr_a = lr.node_position(a).expect("lr a");
    let lr_b = lr.node_position(b).expect("lr b");

    assert!(tb_b.center.y > tb_a.center.y);
    assert!((tb_b.center.x - tb_a.center.x).abs() <= 1.0e-3);
    assert!(lr_b.center.x > lr_a.center.x);
    assert!((lr_b.center.y - lr_a.center.y).abs() <= 1.0e-3);
}

#[test]
fn parallel_edges_keep_separate_finite_route_records() {
    let (mut graph, _a, _b, first_edge) = connected_graph();
    let second_edge = EdgeId::from_u128(6);
    let endpoints = {
        let edge = graph.edges().get(&first_edge).expect("first edge");
        (edge.from, edge.to)
    };
    graph.insert_edge(second_edge, data_edge(endpoints.0, endpoints.1));
    let graph = graph.build_unchecked();

    let result = run_default_dugong_layout(&graph, &LayoutRequest::all()).expect("layout");

    assert_eq!(result.edge_routes.len(), 2);
    for edge in [first_edge, second_edge] {
        let route = result
            .edge_routes
            .iter()
            .find(|route| route.edge == edge)
            .expect("parallel edge route");
        assert!(!route.points.is_empty());
        assert!(route.points.iter().all(|point| point.is_finite()));
    }
}

fn run_default_dugong_layout(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<LayoutResult, LayoutError> {
    builtin_layout_engine_registry().layout(
        graph,
        &LayoutEngineRequest::dugong(request.clone()),
        &LayoutContext::default(),
    )
}

fn connected_graph() -> (GraphBuilder, NodeId, NodeId, EdgeId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let a = NodeId::from_u128(1);
    let b = NodeId::from_u128(2);
    let out = PortId::from_u128(3);
    let inn = PortId::from_u128(4);
    let edge = EdgeId::from_u128(5);

    graph.insert_node(a, node("demo.a", vec![out]));
    graph.insert_node(b, node("demo.b", vec![inn]));
    graph.insert_port(out, port(a, "out", PortDirection::Out));
    graph.insert_port(inn, port(b, "in", PortDirection::In));
    graph.insert_edge(edge, data_edge(out, inn));

    (graph, a, b, edge)
}

fn data_edge(from: PortId, to: PortId) -> Edge {
    Edge::new(EdgeKind::Data, from, to)
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
        size: None,
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
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}
