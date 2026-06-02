use super::harness::{HarnessEvent, InteractionHarness};

use crate::io::NodeGraphNodeOrigin;
use crate::runtime::resize::{
    NODE_RESIZE_TRANSACTION_LABEL, NodeResizeConstraints, NodeResizeDirection, NodeResizeItem,
    NodeResizeRequest, plan_node_resize,
};
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Graph, GraphId, Node, NodeId, NodeKindKey, NodeOrigin,
};
use jellyflow_core::ops::GraphOp;

#[test]
fn single_node_resize_commits_set_node_size_transaction_and_trace() {
    let fixture = resize_fixture();
    let mut harness = InteractionHarness::new("single node resize", fixture.graph);
    let target = CanvasSize {
        width: 140.0,
        height: 80.0,
    };

    let plan = plan_node_resize(
        harness.store().graph(),
        NodeResizeRequest::new(fixture.enabled, target),
    )
    .expect("enabled node resize plan");

    assert_eq!(plan.node, fixture.enabled);
    assert_eq!(
        plan.from,
        Some(CanvasSize {
            width: 100.0,
            height: 60.0,
        }),
    );
    assert_eq!(plan.to, target);
    assert_eq!(
        plan.transaction().label(),
        Some(NODE_RESIZE_TRANSACTION_LABEL)
    );
    assert!(
        matches!(
            plan.transaction().ops(),
            [GraphOp::SetNodeSize { id, from, to }]
                if *id == fixture.enabled
                    && *from == Some(CanvasSize { width: 100.0, height: 60.0 })
                    && *to == Some(target)
        ),
        "resize plan should be a single SetNodeSize op: {:#?}",
        plan.transaction().ops(),
    );

    let outcome = harness
        .store_mut()
        .apply_node_resize(NodeResizeRequest::new(fixture.enabled, target))
        .expect("resize dispatch succeeds")
        .expect("resize dispatch commits");

    assert_eq!(
        outcome.committed().label(),
        Some(NODE_RESIZE_TRANSACTION_LABEL)
    );
    assert_eq!(
        harness.store().graph().nodes[&fixture.enabled].size,
        Some(target)
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_RESIZE_TRANSACTION_LABEL),
        &["set_node_size"],
    )]);
}

#[test]
fn single_node_resize_clamps_to_min_and_max_bounds() {
    let fixture = resize_fixture();
    let harness = InteractionHarness::new("single node resize constraints", fixture.graph);
    let constraints = NodeResizeConstraints::new(
        Some(CanvasSize {
            width: 80.0,
            height: 50.0,
        }),
        Some(CanvasSize {
            width: 120.0,
            height: 70.0,
        }),
    );

    let plan = harness
        .store()
        .plan_node_resize(
            NodeResizeRequest::new(
                fixture.enabled,
                CanvasSize {
                    width: 40.0,
                    height: 120.0,
                },
            )
            .with_constraints(constraints),
        )
        .expect("constrained resize plan");

    assert_eq!(
        plan.to,
        CanvasSize {
            width: 80.0,
            height: 70.0,
        },
    );
    assert_eq!(
        plan.items(),
        &[NodeResizeItem {
            node: fixture.enabled,
            from: Some(CanvasSize {
                width: 100.0,
                height: 60.0,
            }),
            to: CanvasSize {
                width: 80.0,
                height: 70.0,
            },
        }],
    );
}

#[test]
fn left_edge_resize_moves_node_position_before_size_change() {
    let fixture = resize_fixture();
    let harness = InteractionHarness::new("left edge node resize", fixture.graph);
    let target = CanvasSize {
        width: 140.0,
        height: 60.0,
    };

    let plan = harness
        .store()
        .plan_node_resize(
            NodeResizeRequest::new(fixture.enabled, target)
                .with_direction(NodeResizeDirection::Left),
        )
        .expect("left resize plan");

    assert_eq!(plan.from_pos, CanvasPoint { x: 10.0, y: 20.0 });
    assert_eq!(plan.to_pos, CanvasPoint { x: -30.0, y: 20.0 });
    assert!(
        matches!(
            plan.transaction().ops(),
            [
                GraphOp::SetNodePos {
                    id: pos_id,
                    from: pos_from,
                    to: pos_to,
                },
                GraphOp::SetNodeSize {
                    id: size_id,
                    from: size_from,
                    to: size_to,
                },
            ]
                if *pos_id == fixture.enabled
                    && *pos_from == CanvasPoint { x: 10.0, y: 20.0 }
                    && *pos_to == CanvasPoint { x: -30.0, y: 20.0 }
                    && *size_id == fixture.enabled
                    && *size_from == Some(CanvasSize { width: 100.0, height: 60.0 })
                    && *size_to == Some(target)
        ),
        "left resize should move position before setting size: {:#?}",
        plan.transaction().ops(),
    );
}

#[test]
fn top_left_resize_uses_store_node_origin_fallback() {
    let fixture = resize_fixture();
    let mut harness =
        InteractionHarness::new("top left node resize origin fallback", fixture.graph);
    harness.store_mut().update_editor_config(|editor_config| {
        editor_config.interaction.node_origin = NodeGraphNodeOrigin { x: 0.5, y: 0.5 };
    });

    let plan = harness
        .store()
        .plan_node_resize(
            NodeResizeRequest::new(
                fixture.enabled,
                CanvasSize {
                    width: 140.0,
                    height: 80.0,
                },
            )
            .with_direction(NodeResizeDirection::TopLeft),
        )
        .expect("top left resize plan");

    assert_eq!(plan.from_pos, CanvasPoint { x: 10.0, y: 20.0 });
    assert_eq!(plan.to_pos, CanvasPoint { x: -10.0, y: 10.0 });
}

#[test]
fn top_left_resize_uses_node_origin_override() {
    let mut fixture = resize_fixture();
    fixture
        .graph
        .nodes
        .get_mut(&fixture.enabled)
        .unwrap()
        .origin = Some(NodeOrigin { x: 1.0, y: 1.0 });
    let mut harness =
        InteractionHarness::new("top left node resize origin override", fixture.graph);
    harness.store_mut().update_editor_config(|editor_config| {
        editor_config.interaction.node_origin = NodeGraphNodeOrigin { x: 0.5, y: 0.5 };
    });

    let plan = harness
        .store()
        .plan_node_resize(
            NodeResizeRequest::new(
                fixture.enabled,
                CanvasSize {
                    width: 140.0,
                    height: 80.0,
                },
            )
            .with_direction(NodeResizeDirection::TopLeft),
        )
        .expect("top left resize plan");

    assert_eq!(plan.from_pos, CanvasPoint { x: 10.0, y: 20.0 });
    assert_eq!(plan.to_pos, CanvasPoint { x: 10.0, y: 20.0 });
}

#[test]
fn single_node_resize_skips_hidden_missing_noop_and_invalid_requests() {
    let fixture = resize_fixture();
    let harness = InteractionHarness::new("single node resize rejects", fixture.graph);

    let valid_target = CanvasSize {
        width: 140.0,
        height: 80.0,
    };
    for request in [
        NodeResizeRequest::new(fixture.hidden, valid_target),
        NodeResizeRequest::new(fixture.missing, valid_target),
        NodeResizeRequest::new(
            fixture.enabled,
            CanvasSize {
                width: 100.0,
                height: 60.0,
            },
        ),
        NodeResizeRequest::new(
            fixture.enabled,
            CanvasSize {
                width: 0.0,
                height: 80.0,
            },
        ),
        NodeResizeRequest::new(
            fixture.enabled,
            CanvasSize {
                width: f32::INFINITY,
                height: 80.0,
            },
        ),
        NodeResizeRequest::new(fixture.enabled, valid_target).with_constraints(
            NodeResizeConstraints::new(
                Some(CanvasSize {
                    width: -1.0,
                    height: 20.0,
                }),
                None,
            ),
        ),
        NodeResizeRequest::new(fixture.enabled, valid_target).with_constraints(
            NodeResizeConstraints::new(
                Some(CanvasSize {
                    width: 160.0,
                    height: 20.0,
                }),
                Some(CanvasSize {
                    width: 120.0,
                    height: 90.0,
                }),
            ),
        ),
    ] {
        assert!(
            harness.store().plan_node_resize(request).is_none(),
            "request should not produce a resize plan: {request:?}",
        );
    }
}

struct ResizeFixture {
    graph: Graph,
    enabled: NodeId,
    hidden: NodeId,
    missing: NodeId,
}

fn resize_fixture() -> ResizeFixture {
    let mut graph = Graph::new(GraphId::from_u128(1));
    let enabled = NodeId::from_u128(10);
    let hidden = NodeId::from_u128(20);
    let missing = NodeId::from_u128(30);
    graph.nodes.insert(enabled, resize_node(false));
    graph.nodes.insert(hidden, resize_node(true));

    ResizeFixture {
        graph,
        enabled,
        hidden,
        missing,
    }
}

fn resize_node(hidden: bool) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos: CanvasPoint { x: 10.0, y: 20.0 },
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
            width: 100.0,
            height: 60.0,
        }),
        hidden,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}
