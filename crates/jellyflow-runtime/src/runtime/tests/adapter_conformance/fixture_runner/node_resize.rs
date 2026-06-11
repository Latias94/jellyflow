use super::*;

#[test]
fn adapter_conformance_fixture_runner_records_node_resize_direction_transaction() {
    let (mut graph, node_id, _b, _out_port, _in_port, _eid) = make_graph();
    graph.nodes.get_mut(&node_id).expect("node exists").size = Some(CanvasSize {
        width: 100.0,
        height: 60.0,
    });

    let scenario = ConformanceScenario::new("node resize direction transaction", graph)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_node_resize(
            NodeResizeRequest::new(
                node_id,
                CanvasSize {
                    width: 140.0,
                    height: 60.0,
                },
            )
            .with_direction(NodeResizeDirection::Left),
        )])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_RESIZE_TRANSACTION_LABEL),
                ["set_node_pos", "set_node_size"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_RESIZE_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 2,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 2 }),
        ]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_records_node_resize_parent_expansion_transaction() {
    let (mut graph, node_id, _b, _out_port, _in_port, _eid) = make_graph();
    let parent_id = GroupId::from_u128(200);
    graph.groups.insert(
        parent_id,
        Group {
            title: "Parent".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );
    let node = graph.nodes.get_mut(&node_id).expect("node exists");
    node.parent = Some(parent_id);
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(true);
    node.size = Some(CanvasSize {
        width: 20.0,
        height: 20.0,
    });

    let scenario = ConformanceScenario::new("node resize parent expansion", graph)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_node_pointer_resize(
            NodePointerResizeRequest::new(
                node_id,
                CanvasPoint { x: 20.0, y: 20.0 },
                CanvasPoint { x: 120.0, y: 115.0 },
                NodeResizeDirection::BottomRight,
            ),
        )])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_RESIZE_TRANSACTION_LABEL),
                ["set_node_size", "set_group_rect"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_RESIZE_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ]);

    assert_conformance_trace(&scenario);
}
