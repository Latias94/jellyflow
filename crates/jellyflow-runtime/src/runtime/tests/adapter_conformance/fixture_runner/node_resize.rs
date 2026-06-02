use super::*;

#[test]
fn adapter_conformance_fixture_runner_records_node_resize_direction_transaction() {
    let (mut graph, node_id, _b, _out_port, _in_port, _eid) = make_graph();
    graph.nodes.get_mut(&node_id).expect("node exists").size = Some(CanvasSize {
        width: 100.0,
        height: 60.0,
    });

    let scenario = ConformanceScenario::new("node resize direction transaction", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
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
