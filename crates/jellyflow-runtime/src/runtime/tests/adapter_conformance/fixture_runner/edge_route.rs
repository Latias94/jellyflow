use super::*;

#[test]
fn adapter_conformance_fixture_runner_asserts_edge_route_facts() {
    let (graph, source_node, target_node, out_port, in_port, edge) = route_graph();
    let source = ConnectionHandleRef::new(source_node, out_port, PortDirection::Out);
    let target = ConnectionHandleRef::new(target_node, in_port, PortDirection::In);
    let scenario = ConformanceScenario::new("edge route facts assertion", graph)
        .with_actions([
            ConformanceAction::dispatch_transaction(GraphTransaction::from_ops([
                GraphOp::SetEdgeView {
                    id: edge,
                    from: EdgeViewDescriptor::default(),
                    to: EdgeViewDescriptor::new()
                        .with_route_kind(EdgeRouteKind::Straight)
                        .with_hit_target_width(28.0),
                },
            ])),
            ConformanceAction::set_selection([], [edge], []),
            ConformanceAction::report_node_measurement(
                NodeMeasurement::new(source_node)
                    .with_size(Some(CanvasSize {
                        width: 100.0,
                        height: 80.0,
                    }))
                    .with_handles([MeasuredHandle::new(
                        source,
                        handle_bounds(CanvasPoint { x: 90.0, y: 30.0 }),
                    )]),
            ),
            ConformanceAction::report_node_measurement(
                NodeMeasurement::new(target_node)
                    .with_size(Some(CanvasSize {
                        width: 100.0,
                        height: 80.0,
                    }))
                    .with_handles([MeasuredHandle::new(
                        target,
                        HandleBounds {
                            rect: CanvasRect {
                                origin: CanvasPoint { x: 0.0, y: 30.0 },
                                size: CanvasSize {
                                    width: 10.0,
                                    height: 10.0,
                                },
                            },
                            position: HandlePosition::Left,
                        },
                    )]),
            ),
            ConformanceAction::assert_layout_facts(
                CanvasSize {
                    width: 400.0,
                    height: 240.0,
                },
                ConformanceLayoutFactsExpectation::new([source_node, target_node], [edge])
                    .with_edge_routes([ConformanceLayoutEdgeRouteFacts::new(
                        edge,
                        crate::runtime::geometry::ResolvedEdgeRouteKind::Straight,
                        28.0,
                        true,
                    )]),
            ),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(None::<String>, ["set_edge_view"]),
            ConformanceTraceEvent::selection(vec![], vec![edge], vec![]),
        ]);

    assert_conformance_trace(&scenario);
}

fn route_graph() -> (Graph, NodeId, NodeId, PortId, PortId, EdgeId) {
    let source_node = NodeId::from_u128(10);
    let target_node = NodeId::from_u128(11);
    let out_port = PortId::from_u128(20);
    let in_port = PortId::from_u128(21);
    let edge = EdgeId::from_u128(30);
    let mut graph = GraphBuilder::new(GraphId::from_u128(100));

    graph.insert_node(
        source_node,
        Node {
            kind: NodeKindKey::new("demo.source"),
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
            size: None,
            hidden: false,
            collapsed: false,
            ports: vec![out_port],
            data: serde_json::Value::Null,
        },
    );
    graph.insert_node(
        target_node,
        Node {
            kind: NodeKindKey::new("demo.target"),
            kind_version: 1,
            pos: CanvasPoint { x: 100.0, y: 0.0 },
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
            hidden: false,
            collapsed: false,
            ports: vec![in_port],
            data: serde_json::Value::Null,
        },
    );
    graph.insert_port(
        out_port,
        Port {
            node: source_node,
            key: PortKey::new("out"),
            dir: PortDirection::Out,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );
    graph.insert_port(
        in_port,
        Port {
            node: target_node,
            key: PortKey::new("in"),
            dir: PortDirection::In,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );
    graph.insert_edge(edge, Edge::new(EdgeKind::Data, out_port, in_port));

    (
        graph.into(),
        source_node,
        target_node,
        out_port,
        in_port,
        edge,
    )
}

fn handle_bounds(origin: CanvasPoint) -> HandleBounds {
    HandleBounds {
        rect: CanvasRect {
            origin,
            size: CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        },
        position: HandlePosition::Right,
    }
}
