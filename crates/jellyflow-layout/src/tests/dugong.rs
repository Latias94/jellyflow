use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

use crate::{
    DugongLayoutEngine, LayoutContext, LayoutDirection, LayoutEngine, LayoutError, LayoutOptions,
    LayoutRequest, LayoutResult, LayoutSpacing,
};

#[test]
fn dugong_layout_emits_node_positions() {
    let (mut graph, a, b, _edge) = connected_graph();
    graph
        .node_mut(&a)
        .map(|node| {
            node.pos = CanvasPoint {
                x: 1000.0,
                y: 1000.0,
            }
        })
        .expect("node exists");
    graph
        .node_mut(&b)
        .map(|node| {
            node.pos = CanvasPoint {
                x: 2000.0,
                y: 2000.0,
            }
        })
        .expect("node exists");
    let request = LayoutRequest::all().with_options(LayoutOptions {
        default_node_size: size(100.0, 40.0),
        ..LayoutOptions::default()
    });

    let result = run_default_dugong_layout(&graph, &request).expect("layout");

    assert_eq!(result.nodes.len(), 2);
    assert!(
        result
            .nodes
            .iter()
            .any(|position| position.node == a && position.pos != graph.nodes()[&a].pos)
    );
    assert!(
        result
            .nodes
            .iter()
            .any(|position| position.node == b && position.pos != graph.nodes()[&b].pos)
    );
}

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
fn node_origin_controls_written_position_from_dugong_center() {
    let (mut graph, a, _b, _edge) = connected_graph();
    graph
        .node_mut(&a)
        .map(|node| node.origin = Some(jellyflow_core::NodeOrigin { x: 0.5, y: 0.5 }))
        .expect("node exists");
    let request = LayoutRequest::all().with_options(LayoutOptions {
        default_node_size: size(100.0, 40.0),
        ..LayoutOptions::default()
    });

    let result = run_default_dugong_layout(&graph, &request).expect("layout");
    let node = result.node_position(a).expect("node");

    assert!((node.pos.x - node.center.x).abs() <= 1.0e-3);
    assert!((node.pos.y - node.center.y).abs() <= 1.0e-3);
}

#[test]
fn missing_node_origin_uses_top_left_anchor() {
    let (graph, a, _b, _edge) = connected_graph();
    let request = LayoutRequest::nodes([a]).with_options(LayoutOptions {
        default_node_size: size(100.0, 40.0),
        ..LayoutOptions::default()
    });

    let result = DugongLayoutEngine
        .layout(&graph, &request, &LayoutContext::new())
        .expect("layout");
    let node = result.node_position(a).expect("node");

    assert!((node.pos.x - (node.center.x - node.size.width * 0.5)).abs() <= 1.0e-3);
    assert!((node.pos.y - (node.center.y - node.size.height * 0.5)).abs() <= 1.0e-3);
}

#[test]
fn layout_scope_uses_only_requested_nodes_and_internal_edges() {
    let (graph, a, b, _edge) = connected_graph();

    let result = run_default_dugong_layout(&graph, &LayoutRequest::nodes([a])).expect("layout");

    assert!(result.node_position(a).is_some());
    assert!(result.node_position(b).is_none());
    assert!(result.edge_routes.is_empty());
}

#[test]
fn node_size_resolution_prefers_graph_then_request_then_context_then_default() {
    let (mut graph, a, b, _edge) = connected_graph();
    let graph_size = size(300.0, 70.0);
    let request_size = size(80.0, 50.0);
    let context_size = size(60.0, 30.0);
    graph
        .node_mut(&a)
        .map(|node| node.size = Some(graph_size))
        .expect("node exists");
    let request = LayoutRequest::all()
        .with_measured_node_sizes([(a, size(10.0, 10.0)), (b, request_size)])
        .with_options(LayoutOptions {
            default_node_size: size(20.0, 20.0),
            ..LayoutOptions::default()
        });
    let context = LayoutContext::new().with_measured_node_sizes([(b, context_size)]);

    let result = DugongLayoutEngine
        .layout(&graph, &request, &context)
        .expect("layout");

    assert_eq!(result.node_position(a).expect("a").size, graph_size);
    assert_eq!(result.node_position(b).expect("b").size, request_size);
}

#[test]
fn context_measured_size_is_used_when_request_has_none() {
    let (graph, a, _b, _edge) = connected_graph();
    let context_size = size(220.0, 90.0);
    let request = LayoutRequest::nodes([a]).with_options(LayoutOptions {
        default_node_size: size(20.0, 20.0),
        ..LayoutOptions::default()
    });
    let context = LayoutContext::new().with_measured_node_sizes([(a, context_size)]);

    let result = DugongLayoutEngine
        .layout(&graph, &request, &context)
        .expect("layout");

    assert_eq!(result.node_position(a).expect("a").size, context_size);
}

#[test]
fn hidden_nodes_and_edges_are_excluded_from_projection() {
    let (mut graph, a, b, edge) = connected_graph();
    graph
        .node_mut(&b)
        .map(|node| node.hidden = true)
        .expect("node exists");

    let hidden_node_result =
        run_default_dugong_layout(&graph, &LayoutRequest::all()).expect("hidden node layout");

    assert!(hidden_node_result.node_position(a).is_some());
    assert!(hidden_node_result.node_position(b).is_none());
    assert!(hidden_node_result.edge_routes.is_empty());

    graph
        .node_mut(&b)
        .map(|node| node.hidden = false)
        .expect("node exists");
    graph
        .edge_mut(&edge)
        .map(|edge| edge.hidden = true)
        .expect("edge exists");

    let hidden_edge_result =
        run_default_dugong_layout(&graph, &LayoutRequest::all()).expect("hidden edge layout");

    assert!(hidden_edge_result.node_position(a).is_some());
    assert!(hidden_edge_result.node_position(b).is_some());
    assert!(hidden_edge_result.edge_routes.is_empty());
}

#[test]
fn layout_reports_projected_edge_routes() {
    let (graph, _a, _b, edge) = connected_graph();

    let result = run_default_dugong_layout(&graph, &LayoutRequest::all()).expect("layout");
    let route = result
        .edge_routes
        .iter()
        .find(|route| route.edge == edge)
        .expect("edge route");

    assert!(!route.points.is_empty());
    assert!(route.points.iter().all(|point| point.is_finite()));
}

#[test]
fn parallel_edges_between_same_nodes_keep_distinct_routes() {
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
    assert!(
        result
            .edge_routes
            .iter()
            .any(|route| route.edge == first_edge)
    );
    assert!(
        result
            .edge_routes
            .iter()
            .any(|route| route.edge == second_edge)
    );
}

#[test]
fn empty_graph_layout_is_empty() {
    let graph = GraphBuilder::new(GraphId::from_u128(42));

    let result = run_default_dugong_layout(&graph, &LayoutRequest::all()).expect("layout");

    assert!(result.nodes.is_empty());
    assert!(result.edge_routes.is_empty());
    assert!(result.bounds.is_none());
}

#[test]
fn invalid_size_is_reported_before_layout() {
    let (graph, a, _b, _edge) = connected_graph();
    let request = LayoutRequest::all().with_measured_node_sizes([(
        a,
        CanvasSize {
            width: 0.0,
            height: 1.0,
        },
    )]);

    let err = run_default_dugong_layout(&graph, &request).expect_err("invalid size");

    assert_eq!(
        err,
        LayoutError::InvalidNodeSize {
            node: a,
            size: CanvasSize {
                width: 0.0,
                height: 1.0,
            },
        }
    );
}

#[test]
fn unused_context_measured_sizes_do_not_fail_scoped_layout() {
    let (mut graph, a, b, _edge) = connected_graph();
    graph
        .node_mut(&b)
        .map(|node| node.hidden = true)
        .expect("node exists");
    let context = LayoutContext::new().with_measured_node_sizes([
        (
            b,
            CanvasSize {
                width: 0.0,
                height: 1.0,
            },
        ),
        (
            NodeId::from_u128(99),
            CanvasSize {
                width: f32::NAN,
                height: 1.0,
            },
        ),
    ]);

    let result = DugongLayoutEngine
        .layout(&graph, &LayoutRequest::nodes([a]), &context)
        .expect("layout");

    assert!(result.node_position(a).is_some());
    assert!(result.node_position(b).is_none());
}

#[test]
fn invalid_spacing_and_margin_are_reported_before_layout() {
    let (graph, _a, _b, _edge) = connected_graph();
    let spacing = LayoutSpacing {
        nodesep: -1.0,
        ..LayoutSpacing::default()
    };
    let spacing_request = LayoutRequest::all().with_options(LayoutOptions {
        spacing,
        ..LayoutOptions::default()
    });

    let err = run_default_dugong_layout(&graph, &spacing_request).expect_err("spacing");

    assert_eq!(err, LayoutError::InvalidSpacing(spacing));

    let margin = CanvasSize {
        width: f32::INFINITY,
        height: 0.0,
    };
    let margin_request = LayoutRequest::all().with_options(LayoutOptions {
        margin,
        ..LayoutOptions::default()
    });

    let err = run_default_dugong_layout(&graph, &margin_request).expect_err("margin");

    assert_eq!(err, LayoutError::InvalidMargin(margin));
}

#[test]
fn invalid_scope_node_is_reported_before_layout() {
    let (graph, _a, _b, _edge) = connected_graph();
    let missing = NodeId::from_u128(99);

    let err = run_default_dugong_layout(&graph, &LayoutRequest::nodes([missing]))
        .expect_err("missing scope node");

    assert_eq!(err, LayoutError::MissingScopeNode(missing));
}

#[test]
fn missing_source_and_target_ports_are_reported() {
    let (mut missing_source, _a, _b, edge) = connected_graph();
    let source_port = missing_source.edges().get(&edge).unwrap().from;
    missing_source.remove_port(&source_port);
    let missing_source = missing_source.build_unchecked();

    let err = run_default_dugong_layout(&missing_source, &LayoutRequest::all())
        .expect_err("missing source port");

    assert_eq!(err, LayoutError::MissingSourcePort(edge));

    let (mut missing_target, _a, _b, edge) = connected_graph();
    let target_port = missing_target.edges().get(&edge).unwrap().to;
    missing_target.remove_port(&target_port);
    let missing_target = missing_target.build_unchecked();

    let err = run_default_dugong_layout(&missing_target, &LayoutRequest::all())
        .expect_err("missing target port");

    assert_eq!(err, LayoutError::MissingTargetPort(edge));
}

#[test]
fn missing_source_and_target_nodes_are_reported() {
    let (mut missing_source, a, _b, edge) = connected_graph();
    missing_source.remove_node(&a);
    let missing_source = missing_source.build_unchecked();

    let err = run_default_dugong_layout(&missing_source, &LayoutRequest::all())
        .expect_err("missing source node");

    assert_eq!(err, LayoutError::MissingSourceNode { edge });

    let (mut missing_target, _a, b, edge) = connected_graph();
    missing_target.remove_node(&b);
    let missing_target = missing_target.build_unchecked();

    let err = run_default_dugong_layout(&missing_target, &LayoutRequest::all())
        .expect_err("missing target node");

    assert_eq!(err, LayoutError::MissingTargetNode { edge });
}

#[test]
fn bounds_track_visual_rect_independent_of_node_origin_anchor() {
    let (mut graph, a, _b, _edge) = connected_graph();
    graph
        .node_mut(&a)
        .map(|node| node.origin = Some(jellyflow_core::NodeOrigin { x: 1.0, y: 1.0 }))
        .expect("node exists");
    let request = LayoutRequest::nodes([a]).with_options(LayoutOptions {
        default_node_size: size(100.0, 40.0),
        ..LayoutOptions::default()
    });

    let result = run_default_dugong_layout(&graph, &request).expect("layout");
    let node = result.node_position(a).expect("node");
    let bounds = result.bounds.expect("bounds");

    assert!((node.pos.x - (node.center.x + node.size.width * 0.5)).abs() <= 1.0e-3);
    assert!((node.pos.y - (node.center.y + node.size.height * 0.5)).abs() <= 1.0e-3);
    assert!((bounds.origin.x - (node.center.x - node.size.width * 0.5)).abs() <= 1.0e-3);
    assert!((bounds.origin.y - (node.center.y - node.size.height * 0.5)).abs() <= 1.0e-3);
    assert_eq!(bounds.size, node.size);
}

fn run_default_dugong_layout(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<LayoutResult, LayoutError> {
    DugongLayoutEngine.layout(graph, request, &LayoutContext::default())
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

fn size(width: f32, height: f32) -> CanvasSize {
    CanvasSize { width, height }
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
