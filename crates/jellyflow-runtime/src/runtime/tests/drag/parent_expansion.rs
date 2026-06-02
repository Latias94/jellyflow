use super::super::harness::{HarnessEvent, InteractionHarness};
use super::support::drag_fixture;

use crate::runtime::drag::{NODE_DRAG_TRANSACTION_LABEL, NodeDragItem, NodeDragRequest};
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeExtent};
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
