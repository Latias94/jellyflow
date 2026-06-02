use super::*;

#[test]
fn adapter_conformance_fixture_runner_records_node_drag_gesture_transaction_and_callbacks() {
    let (graph, node_id, _b, _out_port, _in_port, _eid) = make_graph();

    let start = NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
    };
    let start_event = NodeGraphGestureEvent::NodeDragStart(start.clone());

    let target = CanvasPoint { x: 32.0, y: 16.0 };
    let update = NodeDragUpdate {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
    };
    let update_event = NodeGraphGestureEvent::NodeDragUpdate(update.clone());

    let end = NodeDragEnd {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
        outcome: NodeDragEndOutcome::Committed,
    };
    let end_event = NodeGraphGestureEvent::NodeDragEnd(end.clone());
    let scenario = ConformanceScenario::new("node drag gesture transaction callbacks", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::emit_gesture(start_event.clone()),
            ConformanceAction::apply_node_drag(node_id, target),
            ConformanceAction::emit_gesture(update_event.clone()),
            ConformanceAction::emit_gesture(end_event.clone()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDragStart(start)),
            ConformanceTraceEvent::graph_commit(
                Some(NODE_DRAG_TRANSACTION_LABEL),
                ["set_node_pos"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
            ConformanceTraceEvent::gesture(update_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDrag(update)),
            ConformanceTraceEvent::gesture(end_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDragEnd(end)),
        ]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_records_node_drag_parent_expansion_transaction() {
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

    let target = CanvasPoint { x: 95.0, y: 95.0 };
    let scenario = ConformanceScenario::new("node drag parent expansion", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_node_drag(node_id, target)])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_DRAG_TRANSACTION_LABEL),
                ["set_node_pos", "set_group_rect"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ]);

    assert_conformance_trace(&scenario);
}
