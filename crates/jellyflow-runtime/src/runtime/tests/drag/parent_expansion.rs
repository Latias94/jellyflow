use super::super::harness::{HarnessEvent, InteractionHarness};
use super::support::drag_fixture;

use crate::io::NodeGraphViewState;
use crate::runtime::drag::{NODE_DRAG_TRANSACTION_LABEL, NodeDragItem, NodeDragRequest};
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, Group, GroupId, NodeExtent};
use jellyflow_core::ops::GraphOp;

#[test]
fn drag_parent_expansion_keeps_parent_extent_clamp_when_disabled() {
    let mut fixture = drag_fixture();
    let child = fixture.child_in_selected_group;
    let parent = fixture.selected_group;
    let parent_rect = fixture.graph.groups[&parent].rect;
    let node = fixture.graph.nodes.get_mut(&child).unwrap();
    node.size = Some(CanvasSize {
        width: 20.0,
        height: 20.0,
    });
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(false);

    let mut harness = InteractionHarness::new("parent extent clamp", fixture.graph);
    let plan = harness
        .store()
        .plan_node_drag(NodeDragRequest {
            node: child,
            to: CanvasPoint { x: 350.0, y: 50.0 },
        })
        .expect("parent extent drag plan");

    assert_eq!(plan.to, CanvasPoint { x: 340.0, y: 40.0 });
    assert_eq!(
        plan.items(),
        &[NodeDragItem {
            node: child,
            from: CanvasPoint { x: 300.0, y: 0.0 },
            to: CanvasPoint { x: 340.0, y: 40.0 },
        }],
    );
    assert!(
        matches!(
            plan.transaction().ops(),
            [GraphOp::SetNodePos { id, from, to }]
                if *id == child
                    && *from == CanvasPoint { x: 300.0, y: 0.0 }
                    && *to == CanvasPoint { x: 340.0, y: 40.0 }
        ),
        "disabled parent expansion should only clamp the child: {:#?}",
        plan.transaction().ops(),
    );

    harness
        .store_mut()
        .apply_node_drag(NodeDragRequest {
            node: child,
            to: CanvasPoint { x: 350.0, y: 50.0 },
        })
        .expect("parent extent drag dispatch succeeds")
        .expect("parent extent drag dispatch commits");

    assert_eq!(harness.store().graph().groups[&parent].rect, parent_rect);
    assert_eq!(
        harness.store().graph().nodes[&child].pos,
        CanvasPoint { x: 340.0, y: 40.0 },
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_DRAG_TRANSACTION_LABEL),
        &["set_node_pos"],
    )]);
}

#[test]
fn drag_parent_expansion_expands_parent_group_when_enabled() {
    let mut fixture = drag_fixture();
    let child = fixture.child_in_selected_group;
    let parent = fixture.selected_group;
    let parent_rect = fixture.graph.groups[&parent].rect;
    let node = fixture.graph.nodes.get_mut(&child).unwrap();
    node.size = Some(CanvasSize {
        width: 20.0,
        height: 20.0,
    });
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(true);

    let expected_parent_rect = CanvasRect {
        origin: CanvasPoint { x: 280.0, y: -20.0 },
        size: CanvasSize {
            width: 90.0,
            height: 90.0,
        },
    };
    let mut harness = InteractionHarness::new("parent expansion", fixture.graph);
    let target = CanvasPoint { x: 350.0, y: 50.0 };

    let plan = harness
        .store()
        .plan_node_drag(NodeDragRequest {
            node: child,
            to: target,
        })
        .expect("parent expansion drag plan");

    assert_eq!(plan.to, target);
    assert_eq!(
        plan.items(),
        &[NodeDragItem {
            node: child,
            from: CanvasPoint { x: 300.0, y: 0.0 },
            to: target,
        }],
    );
    assert!(
        matches!(
            plan.transaction().ops(),
            [
                GraphOp::SetNodePos {
                    id: moved,
                    from: child_from,
                    to: child_to,
                },
                GraphOp::SetGroupRect {
                    id: expanded,
                    from: group_from,
                    to: group_to,
                },
            ] if *moved == child
                && *child_from == CanvasPoint { x: 300.0, y: 0.0 }
                && *child_to == target
                && *expanded == parent
                && *group_from == parent_rect
                && *group_to == expected_parent_rect
        ),
        "enabled parent expansion should move child and expand parent: {:#?}",
        plan.transaction().ops(),
    );

    harness
        .store_mut()
        .apply_node_drag(NodeDragRequest {
            node: child,
            to: target,
        })
        .expect("parent expansion dispatch succeeds")
        .expect("parent expansion dispatch commits");

    assert_eq!(
        harness.store().graph().groups[&parent].rect,
        expected_parent_rect,
    );
    assert_eq!(harness.store().graph().nodes[&child].pos, target);
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_DRAG_TRANSACTION_LABEL),
        &["set_node_pos", "set_group_rect"],
    )]);
}

#[test]
fn drag_parent_expansion_expands_multiple_parent_groups_in_sorted_order() {
    let mut fixture = drag_fixture();
    let first_child = fixture.child_in_selected_group;
    let first_parent = fixture.selected_group;
    let second_child = fixture.enabled;
    let second_parent = GroupId::from_u128(101);
    fixture.graph.groups.insert(
        second_parent,
        Group {
            title: "Second Group".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 70.0,
                    height: 80.0,
                },
            },
            color: None,
        },
    );

    let first = fixture.graph.nodes.get_mut(&first_child).unwrap();
    first.size = Some(CanvasSize {
        width: 20.0,
        height: 20.0,
    });
    first.extent = Some(NodeExtent::Parent);
    first.expand_parent = Some(true);

    let second = fixture.graph.nodes.get_mut(&second_child).unwrap();
    second.parent = Some(second_parent);
    second.size = Some(CanvasSize {
        width: 30.0,
        height: 20.0,
    });
    second.extent = Some(NodeExtent::Parent);
    second.expand_parent = Some(true);

    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![second_child], Vec::new(), Vec::new());
    let harness =
        InteractionHarness::with_view_state("multi parent expansion", fixture.graph, view_state);
    let target = CanvasPoint { x: 350.0, y: 50.0 };

    let plan = harness
        .store()
        .plan_node_drag(NodeDragRequest {
            node: first_child,
            to: target,
        })
        .expect("multi parent expansion drag plan");

    assert!(
        matches!(
            plan.transaction().ops(),
            [
                GraphOp::SetNodePos {
                    id: moved_second,
                    to: second_to,
                    ..
                },
                GraphOp::SetNodePos {
                    id: moved_first,
                    to: first_to,
                    ..
                },
                GraphOp::SetGroupRect {
                    id: expanded_first,
                    to: first_rect,
                    ..
                },
                GraphOp::SetGroupRect {
                    id: expanded_second,
                    to: second_rect,
                    ..
                },
            ] if *moved_second == second_child
                && *second_to == CanvasPoint { x: 60.0, y: 70.0 }
                && *moved_first == first_child
                && *first_to == target
                && *expanded_first == first_parent
                && *first_rect == CanvasRect {
                    origin: CanvasPoint { x: 280.0, y: -20.0 },
                    size: CanvasSize {
                        width: 90.0,
                        height: 90.0,
                    },
                }
                && *expanded_second == second_parent
                && *second_rect == CanvasRect {
                    origin: CanvasPoint { x: 0.0, y: 0.0 },
                    size: CanvasSize {
                        width: 90.0,
                        height: 90.0,
                    },
                }
        ),
        "multi-parent expansion should sort node ops by node id and group ops by group id: {:#?}",
        plan.transaction().ops(),
    );
}

#[test]
fn drag_parent_expansion_left_top_preserves_absolute_sibling_positions_without_compensation() {
    let mut fixture = drag_fixture();
    let child = fixture.child_in_selected_group;
    let sibling = fixture.enabled;
    let parent = fixture.selected_group;
    let child_node = fixture.graph.nodes.get_mut(&child).unwrap();
    child_node.size = Some(CanvasSize {
        width: 20.0,
        height: 20.0,
    });
    child_node.extent = Some(NodeExtent::Parent);
    child_node.expand_parent = Some(true);

    let sibling_pos = CanvasPoint { x: 320.0, y: 10.0 };
    let sibling_node = fixture.graph.nodes.get_mut(&sibling).unwrap();
    sibling_node.parent = Some(parent);
    sibling_node.pos = sibling_pos;

    let mut harness = InteractionHarness::new("left top parent expansion", fixture.graph);
    let target = CanvasPoint { x: 270.0, y: -30.0 };
    let expected_parent_rect = CanvasRect {
        origin: CanvasPoint { x: 270.0, y: -30.0 },
        size: CanvasSize {
            width: 90.0,
            height: 90.0,
        },
    };

    let plan = harness
        .store()
        .plan_node_drag(NodeDragRequest {
            node: child,
            to: target,
        })
        .expect("left top parent expansion drag plan");

    assert!(
        matches!(
            plan.transaction().ops(),
            [
                GraphOp::SetNodePos {
                    id: moved,
                    to: child_to,
                    ..
                },
                GraphOp::SetGroupRect {
                    id: expanded,
                    to: group_to,
                    ..
                },
            ] if *moved == child
                && *child_to == target
                && *expanded == parent
                && *group_to == expected_parent_rect
        ),
        "left/top expansion should not add sibling compensation ops for absolute Jellyflow coordinates: {:#?}",
        plan.transaction().ops(),
    );

    harness
        .store_mut()
        .apply_node_drag(NodeDragRequest {
            node: child,
            to: target,
        })
        .expect("left top parent expansion dispatch succeeds")
        .expect("left top parent expansion dispatch commits");

    assert_eq!(
        harness.store().graph().groups[&parent].rect,
        expected_parent_rect,
    );
    assert_eq!(harness.store().graph().nodes[&child].pos, target);
    assert_eq!(harness.store().graph().nodes[&sibling].pos, sibling_pos);
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_DRAG_TRANSACTION_LABEL),
        &["set_node_pos", "set_group_rect"],
    )]);
}
