use super::harness::{HarnessEvent, InteractionHarness};

use crate::io::NodeGraphNodeOrigin;
use crate::runtime::events::{
    NodeGraphGestureEvent, NodeResizeEnd, NodeResizeEndOutcome, NodeResizeStart, NodeResizeUpdate,
};
use crate::runtime::resize::{
    NODE_RESIZE_TRANSACTION_LABEL, NodePointerResizeRequest, NodeResizeAxis, NodeResizeConstraints,
    NodeResizeDirection, NodeResizeItem, NodeResizeRequest, NodeResizeSession,
    NodeResizeSessionUpdateRequest, plan_node_pointer_resize, plan_node_resize,
};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Graph, GraphId, Group, GroupId, Node, NodeExtent, NodeId,
    NodeKindKey, NodeOrigin,
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
        ["set_node_size"],
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

#[test]
fn pointer_resize_grows_from_bottom_right_pointer_delta() {
    let fixture = resize_fixture();
    let harness = InteractionHarness::new("pointer resize bottom right", fixture.graph);

    let plan = harness
        .store()
        .plan_node_pointer_resize(NodePointerResizeRequest::new(
            fixture.enabled,
            CanvasPoint { x: 110.0, y: 80.0 },
            CanvasPoint { x: 140.0, y: 120.0 },
            NodeResizeDirection::BottomRight,
        ))
        .expect("pointer resize plan");

    assert_eq!(plan.from_pos, CanvasPoint { x: 10.0, y: 20.0 });
    assert_eq!(plan.to_pos, CanvasPoint { x: 10.0, y: 20.0 });
    assert_eq!(
        plan.to,
        CanvasSize {
            width: 130.0,
            height: 100.0,
        },
    );
    assert!(
        matches!(
            plan.transaction().ops(),
            [GraphOp::SetNodeSize { id, from, to }]
                if *id == fixture.enabled
                    && *from == Some(CanvasSize { width: 100.0, height: 60.0 })
                    && *to == Some(CanvasSize { width: 130.0, height: 100.0 })
        ),
        "pointer resize should set only node size: {:#?}",
        plan.transaction().ops(),
    );
}

#[test]
fn pointer_resize_from_left_and_top_moves_position_before_size_change() {
    let fixture = resize_fixture();
    let harness = InteractionHarness::new("pointer resize top left", fixture.graph);

    let plan = harness
        .store()
        .plan_node_pointer_resize(NodePointerResizeRequest::new(
            fixture.enabled,
            CanvasPoint { x: 10.0, y: 20.0 },
            CanvasPoint { x: 0.0, y: 0.0 },
            NodeResizeDirection::TopLeft,
        ))
        .expect("pointer top left resize plan");

    assert_eq!(plan.to_pos, CanvasPoint { x: 0.0, y: 0.0 });
    assert_eq!(
        plan.to,
        CanvasSize {
            width: 110.0,
            height: 80.0,
        },
    );
    assert!(
        matches!(
            plan.transaction().ops(),
            [
                GraphOp::SetNodePos { id: pos_id, to: pos_to, .. },
                GraphOp::SetNodeSize { id: size_id, to: size_to, .. },
            ]
                if *pos_id == fixture.enabled
                    && *pos_to == CanvasPoint { x: 0.0, y: 0.0 }
                    && *size_id == fixture.enabled
                    && *size_to == Some(CanvasSize { width: 110.0, height: 80.0 })
        ),
        "top-left pointer resize should move before sizing: {:#?}",
        plan.transaction().ops(),
    );
}

#[test]
fn pointer_resize_commit_preserves_position_before_size_order_and_trace() {
    let fixture = resize_fixture();
    let mut harness = InteractionHarness::new("pointer resize top left commit", fixture.graph);

    let outcome = harness
        .store_mut()
        .apply_node_pointer_resize(NodePointerResizeRequest::new(
            fixture.enabled,
            CanvasPoint { x: 10.0, y: 20.0 },
            CanvasPoint { x: 0.0, y: 0.0 },
            NodeResizeDirection::TopLeft,
        ))
        .expect("pointer resize dispatch succeeds")
        .expect("pointer resize commits");

    assert!(
        matches!(
            outcome.patch.ops(),
            [
                GraphOp::SetNodePos { id: pos_id, to: pos_to, .. },
                GraphOp::SetNodeSize { id: size_id, to: size_to, .. },
            ]
                if *pos_id == fixture.enabled
                    && *pos_to == CanvasPoint { x: 0.0, y: 0.0 }
                    && *size_id == fixture.enabled
                    && *size_to == Some(CanvasSize { width: 110.0, height: 80.0 })
        ),
        "committed pointer resize should move before sizing: {:#?}",
        outcome.patch.ops(),
    );
    assert_eq!(
        harness.store().graph().nodes[&fixture.enabled].pos,
        CanvasPoint { x: 0.0, y: 0.0 },
    );
    assert_eq!(
        harness.store().graph().nodes[&fixture.enabled].size,
        Some(CanvasSize {
            width: 110.0,
            height: 80.0,
        }),
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_RESIZE_TRANSACTION_LABEL),
        ["set_node_pos", "set_node_size"],
    )]);
}

#[test]
fn node_resize_session_emits_lifecycle_around_pointer_resize_commit() {
    let fixture = resize_fixture();
    let mut harness = InteractionHarness::new("node resize session", fixture.graph);
    let session = NodeResizeSession::new(
        fixture.enabled,
        CanvasPoint { x: 110.0, y: 80.0 },
        NodeResizeDirection::BottomRight,
    );
    let update_request = NodeResizeSessionUpdateRequest::new(CanvasPoint { x: 140.0, y: 120.0 });

    let outcome = harness
        .store_mut()
        .apply_node_resize_session(session, update_request)
        .expect("session dispatch succeeds")
        .expect("session dispatch commits");

    assert_eq!(
        outcome.update,
        NodeResizeUpdate {
            node: fixture.enabled,
            direction: NodeResizeDirection::BottomRight,
            pointer: CanvasPoint { x: 140.0, y: 120.0 },
            position: CanvasPoint { x: 10.0, y: 20.0 },
            size: CanvasSize {
                width: 130.0,
                height: 100.0,
            },
        },
    );
    assert_eq!(
        outcome.dispatch.committed().label(),
        Some(NODE_RESIZE_TRANSACTION_LABEL),
    );
    harness.assert_events(&[
        HarnessEvent::gesture(NodeGraphGestureEvent::NodeResizeStart(NodeResizeStart {
            node: fixture.enabled,
            direction: NodeResizeDirection::BottomRight,
            pointer: CanvasPoint { x: 110.0, y: 80.0 },
        })),
        HarnessEvent::graph_commit(Some(NODE_RESIZE_TRANSACTION_LABEL), ["set_node_size"]),
        HarnessEvent::gesture(NodeGraphGestureEvent::NodeResizeUpdate(NodeResizeUpdate {
            node: fixture.enabled,
            direction: NodeResizeDirection::BottomRight,
            pointer: CanvasPoint { x: 140.0, y: 120.0 },
            position: CanvasPoint { x: 10.0, y: 20.0 },
            size: CanvasSize {
                width: 130.0,
                height: 100.0,
            },
        })),
        HarnessEvent::gesture(NodeGraphGestureEvent::NodeResizeEnd(NodeResizeEnd {
            node: fixture.enabled,
            direction: NodeResizeDirection::BottomRight,
            pointer: CanvasPoint { x: 140.0, y: 120.0 },
            outcome: NodeResizeEndOutcome::Committed,
        })),
    ]);
}

#[test]
fn pointer_resize_clamps_to_min_max_constraints() {
    let fixture = resize_fixture();
    let harness = InteractionHarness::new("pointer resize constraints", fixture.graph);
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
        .plan_node_pointer_resize(
            NodePointerResizeRequest::new(
                fixture.enabled,
                CanvasPoint { x: 110.0, y: 80.0 },
                CanvasPoint { x: 400.0, y: 400.0 },
                NodeResizeDirection::BottomRight,
            )
            .with_constraints(constraints),
        )
        .expect("constrained pointer resize plan");

    assert_eq!(
        plan.to,
        CanvasSize {
            width: 120.0,
            height: 70.0,
        },
    );
}

#[test]
fn pointer_resize_preserves_aspect_ratio_for_diagonal_and_axis_handles() {
    let fixture = resize_fixture();
    let harness = InteractionHarness::new("pointer resize aspect ratio", fixture.graph);

    let diagonal = harness
        .store()
        .plan_node_pointer_resize(
            NodePointerResizeRequest::new(
                fixture.enabled,
                CanvasPoint { x: 110.0, y: 80.0 },
                CanvasPoint { x: 170.0, y: 100.0 },
                NodeResizeDirection::BottomRight,
            )
            .with_keep_aspect_ratio(true),
        )
        .expect("diagonal aspect pointer resize plan");
    let right = harness
        .store()
        .plan_node_pointer_resize(
            NodePointerResizeRequest::new(
                fixture.enabled,
                CanvasPoint { x: 110.0, y: 50.0 },
                CanvasPoint { x: 170.0, y: 50.0 },
                NodeResizeDirection::Right,
            )
            .with_keep_aspect_ratio(true),
        )
        .expect("right aspect pointer resize plan");

    let expected = CanvasSize {
        width: 160.0,
        height: 96.0,
    };
    assert_eq!(diagonal.to, expected);
    assert_eq!(right.to, expected);
}

#[test]
fn pointer_resize_axis_filter_keeps_unselected_dimension() {
    let fixture = resize_fixture();
    let harness = InteractionHarness::new("pointer resize axis filter", fixture.graph);

    let plan = harness
        .store()
        .plan_node_pointer_resize(
            NodePointerResizeRequest::new(
                fixture.enabled,
                CanvasPoint { x: 110.0, y: 80.0 },
                CanvasPoint { x: 140.0, y: 120.0 },
                NodeResizeDirection::BottomRight,
            )
            .with_axis(NodeResizeAxis::Horizontal),
        )
        .expect("horizontal pointer resize plan");

    assert_eq!(
        plan.to,
        CanvasSize {
            width: 130.0,
            height: 60.0,
        },
    );
}

#[test]
fn pointer_resize_clamps_to_node_rect_extent() {
    let mut fixture = resize_fixture();
    fixture
        .graph
        .nodes
        .get_mut(&fixture.enabled)
        .unwrap()
        .extent = Some(NodeExtent::Rect {
        rect: CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 130.0,
                height: 100.0,
            },
        },
    });
    let harness = InteractionHarness::new("pointer resize rect extent", fixture.graph);

    let plan = harness
        .store()
        .plan_node_pointer_resize(NodePointerResizeRequest::new(
            fixture.enabled,
            CanvasPoint { x: 110.0, y: 80.0 },
            CanvasPoint { x: 400.0, y: 400.0 },
            NodeResizeDirection::BottomRight,
        ))
        .expect("extent pointer resize plan");

    assert_eq!(
        plan.to,
        CanvasSize {
            width: 120.0,
            height: 80.0,
        },
    );
}

#[test]
fn pointer_resize_clamps_to_parent_group_extent_when_expand_parent_is_false() {
    let mut fixture = resize_fixture();
    let parent = GroupId::from_u128(40);
    fixture.graph.groups.insert(
        parent,
        Group {
            title: "Parent".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 130.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );
    let node = fixture.graph.nodes.get_mut(&fixture.enabled).unwrap();
    node.parent = Some(parent);
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(false);
    let harness = InteractionHarness::new("pointer resize parent extent", fixture.graph);

    let plan = harness
        .store()
        .plan_node_pointer_resize(NodePointerResizeRequest::new(
            fixture.enabled,
            CanvasPoint { x: 110.0, y: 80.0 },
            CanvasPoint { x: 400.0, y: 400.0 },
            NodeResizeDirection::BottomRight,
        ))
        .expect("parent extent pointer resize plan");

    assert_eq!(
        plan.to,
        CanvasSize {
            width: 120.0,
            height: 80.0,
        },
    );
}

#[test]
fn node_resize_clamps_to_parent_group_extent_when_expand_parent_is_false() {
    let mut fixture = resize_fixture();
    let parent = GroupId::from_u128(40);
    fixture.graph.groups.insert(
        parent,
        Group {
            title: "Parent".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 130.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );
    let node = fixture.graph.nodes.get_mut(&fixture.enabled).unwrap();
    node.parent = Some(parent);
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(false);
    let harness = InteractionHarness::new("target resize parent extent", fixture.graph);

    let plan = harness
        .store()
        .plan_node_resize(NodeResizeRequest::new(
            fixture.enabled,
            CanvasSize {
                width: 400.0,
                height: 400.0,
            },
        ))
        .expect("parent extent target resize plan");

    assert_eq!(
        plan.to,
        CanvasSize {
            width: 120.0,
            height: 80.0,
        },
    );
    assert!(
        matches!(
            plan.transaction().ops(),
            [GraphOp::SetNodeSize { id, to, .. }]
                if *id == fixture.enabled
                    && *to == Some(CanvasSize { width: 120.0, height: 80.0 })
        ),
        "disabled parent expansion should clamp target resize without group ops: {:#?}",
        plan.transaction().ops(),
    );
}

#[test]
fn node_resize_expands_parent_group_when_expand_parent_is_true() {
    let mut fixture = resize_fixture();
    let parent = GroupId::from_u128(40);
    let parent_rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 130.0,
            height: 100.0,
        },
    };
    fixture.graph.groups.insert(
        parent,
        Group {
            title: "Parent".to_owned(),
            rect: parent_rect,
            color: None,
        },
    );
    let node = fixture.graph.nodes.get_mut(&fixture.enabled).unwrap();
    node.parent = Some(parent);
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(true);
    let harness = InteractionHarness::new("target resize parent expansion", fixture.graph);
    let target = CanvasSize {
        width: 150.0,
        height: 100.0,
    };
    let expected_parent_rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 160.0,
            height: 120.0,
        },
    };

    let plan = harness
        .store()
        .plan_node_resize(NodeResizeRequest::new(fixture.enabled, target))
        .expect("parent expansion target resize plan");

    assert_eq!(plan.to, target);
    assert!(
        matches!(
            plan.transaction().ops(),
            [
                GraphOp::SetNodeSize { id, to, .. },
                GraphOp::SetGroupRect {
                    id: expanded,
                    from: group_from,
                    to: group_to,
                },
            ] if *id == fixture.enabled
                && *to == Some(target)
                && *expanded == parent
                && *group_from == parent_rect
                && *group_to == expected_parent_rect
        ),
        "enabled parent expansion should expand group for target resize: {:#?}",
        plan.transaction().ops(),
    );
}

#[test]
fn pointer_resize_expands_parent_group_when_expand_parent_is_true() {
    let mut fixture = resize_fixture();
    let parent = GroupId::from_u128(40);
    let parent_rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 130.0,
            height: 100.0,
        },
    };
    fixture.graph.groups.insert(
        parent,
        Group {
            title: "Parent".to_owned(),
            rect: parent_rect,
            color: None,
        },
    );
    let node = fixture.graph.nodes.get_mut(&fixture.enabled).unwrap();
    node.parent = Some(parent);
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(true);

    let mut harness = InteractionHarness::new("pointer resize parent expansion", fixture.graph);
    let target = CanvasSize {
        width: 150.0,
        height: 100.0,
    };
    let expected_parent_rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 160.0,
            height: 120.0,
        },
    };

    let request = NodePointerResizeRequest::new(
        fixture.enabled,
        CanvasPoint { x: 110.0, y: 80.0 },
        CanvasPoint { x: 160.0, y: 120.0 },
        NodeResizeDirection::BottomRight,
    );
    let plan = harness
        .store()
        .plan_node_pointer_resize(request)
        .expect("parent expansion pointer resize plan");

    assert_eq!(plan.to, target);
    assert_eq!(plan.to_pos, CanvasPoint { x: 10.0, y: 20.0 });
    assert!(
        matches!(
            plan.transaction().ops(),
            [
                GraphOp::SetNodeSize { id, from, to },
                GraphOp::SetGroupRect {
                    id: expanded,
                    from: group_from,
                    to: group_to,
                },
            ] if *id == fixture.enabled
                && *from == Some(CanvasSize { width: 100.0, height: 60.0 })
                && *to == Some(target)
                && *expanded == parent
                && *group_from == parent_rect
                && *group_to == expected_parent_rect
        ),
        "enabled parent expansion should resize child and expand group: {:#?}",
        plan.transaction().ops(),
    );

    harness
        .store_mut()
        .apply_node_pointer_resize(request)
        .expect("parent expansion pointer resize dispatch succeeds")
        .expect("parent expansion pointer resize dispatch commits");

    assert_eq!(
        harness.store().graph().nodes[&fixture.enabled].size,
        Some(target)
    );
    assert_eq!(
        harness.store().graph().groups[&parent].rect,
        expected_parent_rect,
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_RESIZE_TRANSACTION_LABEL),
        ["set_node_size", "set_group_rect"],
    )]);
}

#[test]
fn pointer_resize_expands_parent_group_from_top_left_without_sibling_compensation() {
    let mut fixture = resize_fixture();
    let parent = GroupId::from_u128(40);
    let parent_rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 130.0,
            height: 100.0,
        },
    };
    fixture.graph.groups.insert(
        parent,
        Group {
            title: "Parent".to_owned(),
            rect: parent_rect,
            color: None,
        },
    );
    let node = fixture.graph.nodes.get_mut(&fixture.enabled).unwrap();
    node.parent = Some(parent);
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(true);

    let sibling_pos = CanvasPoint { x: 90.0, y: 40.0 };
    let sibling = fixture.graph.nodes.get_mut(&fixture.no_size).unwrap();
    sibling.parent = Some(parent);
    sibling.pos = sibling_pos;

    let mut harness =
        InteractionHarness::new("pointer resize top left parent expansion", fixture.graph);
    let target = CanvasSize {
        width: 130.0,
        height: 90.0,
    };
    let expected_node_pos = CanvasPoint { x: -20.0, y: -10.0 };
    let expected_parent_rect = CanvasRect {
        origin: expected_node_pos,
        size: CanvasSize {
            width: 150.0,
            height: 110.0,
        },
    };

    let request = NodePointerResizeRequest::new(
        fixture.enabled,
        CanvasPoint { x: 10.0, y: 20.0 },
        CanvasPoint { x: -20.0, y: -10.0 },
        NodeResizeDirection::TopLeft,
    );
    let plan = harness
        .store()
        .plan_node_pointer_resize(request)
        .expect("top left parent expansion pointer resize plan");

    assert_eq!(plan.to, target);
    assert_eq!(plan.to_pos, expected_node_pos);
    assert!(
        matches!(
            plan.transaction().ops(),
            [
                GraphOp::SetNodePos {
                    id: moved,
                    to: node_to,
                    ..
                },
                GraphOp::SetNodeSize { id: sized, to: size_to, .. },
                GraphOp::SetGroupRect {
                    id: expanded,
                    from: group_from,
                    to: group_to,
                },
            ] if *moved == fixture.enabled
                && *node_to == expected_node_pos
                && *sized == fixture.enabled
                && *size_to == Some(target)
                && *expanded == parent
                && *group_from == parent_rect
                && *group_to == expected_parent_rect
        ),
        "left/top parent expansion should not add sibling compensation ops: {:#?}",
        plan.transaction().ops(),
    );

    harness
        .store_mut()
        .apply_node_pointer_resize(request)
        .expect("top left parent expansion pointer resize dispatch succeeds")
        .expect("top left parent expansion pointer resize dispatch commits");

    assert_eq!(
        harness.store().graph().nodes[&fixture.enabled].pos,
        expected_node_pos
    );
    assert_eq!(
        harness.store().graph().nodes[&fixture.enabled].size,
        Some(target)
    );
    assert_eq!(
        harness.store().graph().nodes[&fixture.no_size].pos,
        sibling_pos
    );
    assert_eq!(
        harness.store().graph().groups[&parent].rect,
        expected_parent_rect,
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_RESIZE_TRANSACTION_LABEL),
        ["set_node_pos", "set_node_size", "set_group_rect"],
    )]);
}

#[test]
fn pointer_resize_skips_hidden_missing_unsized_noop_and_invalid_requests() {
    let fixture = resize_fixture();
    let harness = InteractionHarness::new("pointer resize rejects", fixture.graph);
    let valid = NodePointerResizeRequest::new(
        fixture.enabled,
        CanvasPoint { x: 110.0, y: 80.0 },
        CanvasPoint { x: 140.0, y: 120.0 },
        NodeResizeDirection::BottomRight,
    );

    for request in [
        NodePointerResizeRequest::new(
            fixture.hidden,
            CanvasPoint { x: 110.0, y: 80.0 },
            CanvasPoint { x: 140.0, y: 120.0 },
            NodeResizeDirection::BottomRight,
        ),
        NodePointerResizeRequest::new(
            fixture.missing,
            CanvasPoint { x: 110.0, y: 80.0 },
            CanvasPoint { x: 140.0, y: 120.0 },
            NodeResizeDirection::BottomRight,
        ),
        NodePointerResizeRequest::new(
            fixture.no_size,
            CanvasPoint { x: 110.0, y: 80.0 },
            CanvasPoint { x: 140.0, y: 120.0 },
            NodeResizeDirection::BottomRight,
        ),
        NodePointerResizeRequest::new(
            fixture.enabled,
            CanvasPoint { x: 110.0, y: 80.0 },
            CanvasPoint { x: 110.0, y: 80.0 },
            NodeResizeDirection::BottomRight,
        ),
        NodePointerResizeRequest::new(
            fixture.enabled,
            CanvasPoint {
                x: f32::INFINITY,
                y: 80.0,
            },
            CanvasPoint { x: 140.0, y: 120.0 },
            NodeResizeDirection::BottomRight,
        ),
        valid.with_constraints(NodeResizeConstraints::new(
            Some(CanvasSize {
                width: 160.0,
                height: 20.0,
            }),
            Some(CanvasSize {
                width: 120.0,
                height: 90.0,
            }),
        )),
    ] {
        assert!(
            plan_node_pointer_resize(harness.store().graph(), request).is_none(),
            "request should not produce a pointer resize plan: {request:?}",
        );
    }
}

struct ResizeFixture {
    graph: Graph,
    enabled: NodeId,
    hidden: NodeId,
    missing: NodeId,
    no_size: NodeId,
}

fn resize_fixture() -> ResizeFixture {
    let mut graph = Graph::new(GraphId::from_u128(1));
    let enabled = NodeId::from_u128(10);
    let hidden = NodeId::from_u128(20);
    let missing = NodeId::from_u128(30);
    let no_size = NodeId::from_u128(40);
    graph.nodes.insert(enabled, resize_node(false));
    graph.nodes.insert(hidden, resize_node(true));
    graph.nodes.insert(
        no_size,
        Node {
            size: None,
            ..resize_node(false)
        },
    );

    ResizeFixture {
        graph,
        enabled,
        hidden,
        missing,
        no_size,
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
