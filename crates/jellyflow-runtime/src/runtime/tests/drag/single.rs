use super::super::fixtures::default_editor_config;
use super::super::harness::{HarnessEvent, InteractionHarness};
use super::support::drag_fixture;

use crate::io::NodeGraphNodeOrigin;
use crate::runtime::drag::{
    NODE_DRAG_TRANSACTION_LABEL, NodeDragItem, NodeDragRequest, plan_node_drag,
};
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeExtent, NodeOrigin};
use jellyflow_core::ops::GraphOp;

#[test]
fn single_node_drag_commits_set_node_pos_transaction_and_trace() {
    let fixture = drag_fixture();
    let mut harness = InteractionHarness::new("single node drag", fixture.graph);
    let target = CanvasPoint { x: 30.0, y: 40.0 };

    let plan = plan_node_drag(
        harness.store().graph(),
        harness.store().view_state(),
        &harness.store().resolved_interaction_state(),
        NodeDragRequest {
            node: fixture.enabled,
            to: target,
        },
    )
    .expect("enabled node drag plan");

    assert_eq!(plan.node, fixture.enabled);
    assert_eq!(plan.from, CanvasPoint { x: 10.0, y: 20.0 });
    assert_eq!(plan.to, target);
    assert_eq!(
        plan.transaction().label(),
        Some(NODE_DRAG_TRANSACTION_LABEL)
    );
    assert!(
        matches!(
            plan.transaction().ops(),
            [GraphOp::SetNodePos { id, from, to }]
                if *id == fixture.enabled
                    && *from == CanvasPoint { x: 10.0, y: 20.0 }
                    && *to == target
        ),
        "drag plan should be a single SetNodePos op: {:#?}",
        plan.transaction().ops(),
    );

    let outcome = harness
        .store_mut()
        .apply_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: target,
        })
        .expect("drag dispatch succeeds")
        .expect("drag dispatch commits");

    assert_eq!(
        outcome.committed().label(),
        Some(NODE_DRAG_TRANSACTION_LABEL)
    );
    assert_eq!(
        harness.store().graph().nodes[&fixture.enabled].pos,
        CanvasPoint { x: 30.0, y: 40.0 },
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_DRAG_TRANSACTION_LABEL),
        ["set_node_pos"],
    )]);
}

#[test]
fn single_node_drag_skips_non_draggable_hidden_noop_missing_and_invalid_targets() {
    let fixture = drag_fixture();
    let store = NodeGraphStore::new(fixture.graph, Default::default(), default_editor_config());
    let interaction = store.resolved_interaction_state();

    for request in [
        NodeDragRequest {
            node: fixture.disabled,
            to: CanvasPoint { x: 30.0, y: 40.0 },
        },
        NodeDragRequest {
            node: fixture.hidden,
            to: CanvasPoint { x: 30.0, y: 40.0 },
        },
        NodeDragRequest {
            node: fixture.missing,
            to: CanvasPoint { x: 30.0, y: 40.0 },
        },
        NodeDragRequest {
            node: fixture.enabled,
            to: CanvasPoint { x: 10.0, y: 20.0 },
        },
        NodeDragRequest {
            node: fixture.enabled,
            to: CanvasPoint {
                x: f32::INFINITY,
                y: 40.0,
            },
        },
    ] {
        assert!(
            plan_node_drag(store.graph(), store.view_state(), &interaction, request).is_none(),
            "request should not produce a drag plan: {request:?}",
        );
    }
}

#[test]
fn single_node_drag_respects_global_nodes_draggable_policy_without_committing() {
    let fixture = drag_fixture();
    let mut editor_config = default_editor_config();
    editor_config.interaction.nodes_draggable = false;
    let mut harness = InteractionHarness::new("global drag disabled", fixture.graph);
    harness.store_mut().replace_editor_config(editor_config);

    let result = harness
        .store_mut()
        .apply_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: CanvasPoint { x: 30.0, y: 40.0 },
        })
        .expect("disabled drag does not error");

    assert!(result.is_none());
    assert_eq!(
        harness.store().graph().nodes[&fixture.enabled].pos,
        CanvasPoint { x: 10.0, y: 20.0 },
    );
    harness.assert_events(&[]);
}

#[test]
fn single_node_drag_clamps_per_node_rect_with_node_origin() {
    let mut fixture = drag_fixture();
    let node = fixture.graph.nodes.get_mut(&fixture.enabled).unwrap();
    node.size = Some(CanvasSize {
        width: 20.0,
        height: 10.0,
    });
    node.extent = Some(NodeExtent::Rect {
        rect: CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 100.0,
                height: 60.0,
            },
        },
    });

    let mut harness = InteractionHarness::new("single node per-node extent", fixture.graph);
    harness.store_mut().update_editor_config(|editor_config| {
        editor_config.interaction.node_origin = NodeGraphNodeOrigin { x: 0.5, y: 0.5 };
    });

    let plan = harness
        .store()
        .plan_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: CanvasPoint { x: 95.0, y: 55.0 },
        })
        .expect("per-node extent drag plan");

    assert_eq!(plan.to, CanvasPoint { x: 90.0, y: 55.0 });
    assert_eq!(
        plan.items(),
        &[NodeDragItem {
            node: fixture.enabled,
            from: CanvasPoint { x: 10.0, y: 20.0 },
            to: CanvasPoint { x: 90.0, y: 55.0 },
        }],
    );
}

#[test]
fn single_node_drag_uses_node_origin_override_for_extent_clamping() {
    let mut fixture = drag_fixture();
    let node = fixture.graph.nodes.get_mut(&fixture.enabled).unwrap();
    node.size = Some(CanvasSize {
        width: 20.0,
        height: 10.0,
    });
    node.origin = Some(NodeOrigin { x: 1.0, y: 1.0 });
    node.extent = Some(NodeExtent::Rect {
        rect: CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 100.0,
                height: 60.0,
            },
        },
    });

    let mut harness = InteractionHarness::new("single node per-node origin", fixture.graph);
    harness.store_mut().update_editor_config(|editor_config| {
        editor_config.interaction.node_origin = NodeGraphNodeOrigin { x: 0.5, y: 0.5 };
    });

    let plan = harness
        .store()
        .plan_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: CanvasPoint { x: 95.0, y: 55.0 },
        })
        .expect("per-node origin drag plan");

    assert_eq!(plan.to, CanvasPoint { x: 95.0, y: 55.0 });
    assert_eq!(
        plan.items(),
        &[NodeDragItem {
            node: fixture.enabled,
            from: CanvasPoint { x: 10.0, y: 20.0 },
            to: CanvasPoint { x: 95.0, y: 55.0 },
        }],
    );
}
