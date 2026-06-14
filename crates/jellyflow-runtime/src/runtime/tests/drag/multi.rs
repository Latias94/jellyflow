use super::super::harness::{HarnessEvent, InteractionHarness};
use super::support::drag_fixture;

use crate::io::{NodeGraphNudgeStepMode, NodeGraphViewState};
use crate::runtime::drag::{
    NODE_DRAG_TRANSACTION_LABEL, NODE_NUDGE_TRANSACTION_LABEL, NodeDragItem, NodeDragRequest,
    NodeNudgeDirection, NodeNudgeRequest,
};
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};
use jellyflow_core::ops::GraphOp;

#[test]
fn multi_selection_drag_moves_primary_and_selected_nodes_with_sorted_ops() {
    let fixture = drag_fixture();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(
        vec![
            fixture.selected_high,
            fixture.disabled,
            fixture.child_in_selected_group,
            fixture.selected_low,
        ],
        Vec::new(),
        vec![fixture.selected_group],
    );
    let mut harness =
        InteractionHarness::with_view_state("multi selection drag", fixture.graph, view_state);
    let target = CanvasPoint { x: 30.0, y: 40.0 };

    let plan = harness
        .store()
        .plan_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: target,
        })
        .expect("multi selection drag plan");

    assert_eq!(plan.node, fixture.enabled);
    assert_eq!(plan.from, CanvasPoint { x: 10.0, y: 20.0 });
    assert_eq!(plan.to, target);
    assert_eq!(
        plan.items(),
        &[
            NodeDragItem {
                node: fixture.selected_low,
                from: CanvasPoint { x: 0.0, y: 0.0 },
                to: CanvasPoint { x: 20.0, y: 20.0 },
            },
            NodeDragItem {
                node: fixture.enabled,
                from: CanvasPoint { x: 10.0, y: 20.0 },
                to: CanvasPoint { x: 30.0, y: 40.0 },
            },
            NodeDragItem {
                node: fixture.selected_high,
                from: CanvasPoint { x: 100.0, y: 0.0 },
                to: CanvasPoint { x: 120.0, y: 20.0 },
            },
        ],
    );
    assert!(
        matches!(
            plan.transaction().ops(),
            [
                GraphOp::SetNodePos { id: low, .. },
                GraphOp::SetNodePos { id: primary, .. },
                GraphOp::SetNodePos { id: high, .. },
            ] if *low == fixture.selected_low
                && *primary == fixture.enabled
                && *high == fixture.selected_high
        ),
        "multi-drag ops should be sorted by node id: {:#?}",
        plan.transaction().ops(),
    );

    harness
        .store_mut()
        .apply_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: target,
        })
        .expect("multi drag dispatch succeeds")
        .expect("multi drag dispatch commits");

    assert_eq!(
        harness.store().graph().nodes()[&fixture.selected_low].pos,
        CanvasPoint { x: 20.0, y: 20.0 },
    );
    assert_eq!(
        harness.store().graph().nodes()[&fixture.enabled].pos,
        CanvasPoint { x: 30.0, y: 40.0 },
    );
    assert_eq!(
        harness.store().graph().nodes()[&fixture.selected_high].pos,
        CanvasPoint { x: 120.0, y: 20.0 },
    );
    assert_eq!(
        harness.store().graph().nodes()[&fixture.disabled].pos,
        CanvasPoint { x: 200.0, y: 0.0 },
    );
    assert_eq!(
        harness.store().graph().nodes()[&fixture.child_in_selected_group].pos,
        CanvasPoint { x: 300.0, y: 0.0 },
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_DRAG_TRANSACTION_LABEL),
        ["set_node_pos", "set_node_pos", "set_node_pos"],
    )]);
}

#[test]
fn multi_selection_drag_uses_shared_snap_offset() {
    let fixture = drag_fixture();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(
        vec![fixture.selected_high, fixture.selected_low],
        Vec::new(),
        Vec::new(),
    );
    let mut harness = InteractionHarness::with_view_state(
        "multi selection drag shared snap",
        fixture.graph,
        view_state,
    );
    harness.store_mut().update_editor_config(|editor_config| {
        editor_config.interaction.snap_to_grid = true;
        editor_config.interaction.snap_grid = CanvasSize {
            width: 20.0,
            height: 20.0,
        };
    });

    let plan = harness
        .store()
        .plan_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: CanvasPoint { x: 35.0, y: 41.0 },
        })
        .expect("snapped multi selection drag plan");

    assert_eq!(plan.to, CanvasPoint { x: 30.0, y: 40.0 });
    assert_eq!(
        plan.items(),
        &[
            NodeDragItem {
                node: fixture.selected_low,
                from: CanvasPoint { x: 0.0, y: 0.0 },
                to: CanvasPoint { x: 20.0, y: 20.0 },
            },
            NodeDragItem {
                node: fixture.enabled,
                from: CanvasPoint { x: 10.0, y: 20.0 },
                to: CanvasPoint { x: 30.0, y: 40.0 },
            },
            NodeDragItem {
                node: fixture.selected_high,
                from: CanvasPoint { x: 100.0, y: 0.0 },
                to: CanvasPoint { x: 120.0, y: 20.0 },
            },
        ],
    );
}

#[test]
fn keyboard_nudge_moves_selected_draggable_nodes_with_screen_step() {
    let fixture = drag_fixture();
    let mut view_state = NodeGraphViewState::default();
    view_state.zoom = 2.0;
    view_state.set_selection(
        vec![
            fixture.selected_high,
            fixture.disabled,
            fixture.child_in_selected_group,
            fixture.selected_low,
        ],
        Vec::new(),
        vec![fixture.selected_group],
    );
    let mut harness = InteractionHarness::with_view_state(
        "keyboard nudge screen step",
        fixture.graph,
        view_state,
    );
    harness.store_mut().update_editor_config(|editor_config| {
        editor_config.interaction.nudge_step_px = 5.0;
    });

    let request = NodeNudgeRequest {
        direction: NodeNudgeDirection::Right,
        fast: false,
    };
    let plan = harness
        .store()
        .plan_node_nudge(request)
        .expect("keyboard nudge plan");

    assert_eq!(plan.direction, NodeNudgeDirection::Right);
    assert_eq!(plan.delta, CanvasPoint { x: 2.5, y: 0.0 });
    assert_eq!(
        plan.items(),
        &[
            NodeDragItem {
                node: fixture.selected_low,
                from: CanvasPoint { x: 0.0, y: 0.0 },
                to: CanvasPoint { x: 2.5, y: 0.0 },
            },
            NodeDragItem {
                node: fixture.selected_high,
                from: CanvasPoint { x: 100.0, y: 0.0 },
                to: CanvasPoint { x: 102.5, y: 0.0 },
            },
        ],
    );

    harness
        .store_mut()
        .apply_node_nudge(request)
        .expect("keyboard nudge dispatch succeeds")
        .expect("keyboard nudge dispatch commits");

    assert_eq!(
        harness.store().graph().nodes()[&fixture.selected_low].pos,
        CanvasPoint { x: 2.5, y: 0.0 },
    );
    assert_eq!(
        harness.store().graph().nodes()[&fixture.selected_high].pos,
        CanvasPoint { x: 102.5, y: 0.0 },
    );
    assert_eq!(
        harness.store().graph().nodes()[&fixture.disabled].pos,
        CanvasPoint { x: 200.0, y: 0.0 },
    );
    assert_eq!(
        harness.store().graph().nodes()[&fixture.child_in_selected_group].pos,
        CanvasPoint { x: 300.0, y: 0.0 },
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_NUDGE_TRANSACTION_LABEL),
        ["set_node_pos", "set_node_pos"],
    )]);
}

#[test]
fn keyboard_nudge_uses_grid_step_and_fast_factor() {
    let fixture = drag_fixture();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![fixture.selected_low], Vec::new(), Vec::new());
    let mut harness =
        InteractionHarness::with_view_state("keyboard nudge grid step", fixture.graph, view_state);
    harness.store_mut().update_editor_config(|editor_config| {
        editor_config.interaction.nudge_step_mode = NodeGraphNudgeStepMode::Grid;
        editor_config.interaction.snap_grid = CanvasSize {
            width: 20.0,
            height: 10.0,
        };
    });

    let plan = harness
        .store()
        .plan_node_nudge(NodeNudgeRequest {
            direction: NodeNudgeDirection::Down,
            fast: true,
        })
        .expect("keyboard grid nudge plan");

    assert_eq!(plan.delta, CanvasPoint { x: 0.0, y: 40.0 });
    assert_eq!(
        plan.items(),
        &[NodeDragItem {
            node: fixture.selected_low,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 0.0, y: 40.0 },
        }],
    );
}

#[test]
fn keyboard_nudge_returns_none_without_keyboard_accessibility_or_selection() {
    let fixture = drag_fixture();
    let mut harness = InteractionHarness::new("keyboard nudge disabled", fixture.graph);
    let request = NodeNudgeRequest {
        direction: NodeNudgeDirection::Left,
        fast: false,
    };

    assert!(harness.store().plan_node_nudge(request).is_none());

    harness
        .store_mut()
        .update_view_state(|state| state.selected_nodes = vec![fixture.selected_low]);
    harness.store_mut().update_editor_config(|editor_config| {
        editor_config.interaction.disable_keyboard_a11y = true;
    });

    assert!(harness.store().plan_node_nudge(request).is_none());
}

#[test]
fn multi_selection_drag_clamps_global_extent_as_group() {
    let mut fixture = drag_fixture();
    for node in [fixture.selected_low, fixture.enabled, fixture.selected_high] {
        fixture
            .graph
            .update_node(&node, |node| {
                node.size = Some(CanvasSize {
                    width: 10.0,
                    height: 10.0,
                })
            })
            .expect("node exists");
    }
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(
        vec![fixture.selected_high, fixture.selected_low],
        Vec::new(),
        Vec::new(),
    );
    let mut harness = InteractionHarness::with_view_state(
        "multi selection global extent",
        fixture.graph,
        view_state,
    );
    harness.store_mut().update_editor_config(|editor_config| {
        editor_config.interaction.node_extent = Some(CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 130.0,
                height: 60.0,
            },
        });
    });

    let plan = harness
        .store()
        .plan_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: CanvasPoint { x: 60.0, y: 40.0 },
        })
        .expect("global extent drag plan");

    assert_eq!(plan.to, CanvasPoint { x: 30.0, y: 40.0 });
    assert_eq!(
        plan.items(),
        &[
            NodeDragItem {
                node: fixture.selected_low,
                from: CanvasPoint { x: 0.0, y: 0.0 },
                to: CanvasPoint { x: 20.0, y: 20.0 },
            },
            NodeDragItem {
                node: fixture.enabled,
                from: CanvasPoint { x: 10.0, y: 20.0 },
                to: CanvasPoint { x: 30.0, y: 40.0 },
            },
            NodeDragItem {
                node: fixture.selected_high,
                from: CanvasPoint { x: 100.0, y: 0.0 },
                to: CanvasPoint { x: 120.0, y: 20.0 },
            },
        ],
    );
}
