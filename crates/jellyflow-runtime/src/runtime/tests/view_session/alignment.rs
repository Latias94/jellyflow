use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

use crate::runtime::adapter_protocol::ViewPoint;
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::interaction::{
    AlignmentSnapConfig, CanvasInteractionConfig, CanvasInteractionEffect,
    CanvasInteractionQueryPageReceipt, CanvasInteractionSession, CanvasPressTarget, SelectionMode,
};
use crate::runtime::tests::fixtures::node_drag_start_by_id;
use crate::runtime::view_session::{ViewSession, ViewSessionOptions};
use crate::runtime::viewport::{ViewportPointerSample, ViewportTransform};

use super::fixtures::{graph_with_nodes, publish_node_layouts};

#[test]
fn alignment_tolerance_is_screen_pixel_invariant_across_zoom() {
    for (zoom, canvas_gap) in [(0.5, 12.0), (1.0, 6.0), (2.0, 3.0)] {
        let (_runtime, view, ids) = alignment_view(2, zoom, &[0.0, 200.0 + canvas_gap]);
        let mut interaction = interaction_session(&view, 64);
        move_and_resolve(&view, &mut interaction, ids[0], 100.0, zoom);

        let presentation = interaction.presentation();
        let drag = presentation.node_drag().unwrap();
        let screen_adjustment = (drag.delta().x - 100.0) * zoom;
        assert!((screen_adjustment - 6.0).abs() < 0.001);
        assert!(
            drag.guides()
                .iter()
                .all(|guide| guide.canvas_coordinate().is_finite())
        );
    }
}

#[test]
fn alignment_uses_stable_node_order_for_equal_distance_candidates() {
    let (_runtime, view, ids) = alignment_view(3, 1.0, &[0.0, 205.0, 195.0]);
    let mut interaction = interaction_session(&view, 64);
    move_and_resolve(&view, &mut interaction, ids[0], 100.0, 1.0);

    assert_eq!(
        interaction.presentation().node_drag().unwrap().delta().x,
        105.0
    );
}

#[test]
fn alignment_feature_index_finds_columns_across_large_perpendicular_distance() {
    let (_runtime, view, ids) = alignment_view(2, 1.0, &[0.0, 205.0]);
    publish_node_layouts(
        &view,
        [(
            ids[1],
            CanvasRect {
                origin: CanvasPoint {
                    x: 205.0,
                    y: 5_000.0,
                },
                size: CanvasSize {
                    width: 100.0,
                    height: 80.0,
                },
            },
        )],
    );
    let mut interaction = interaction_session(&view, 64);
    move_and_resolve(&view, &mut interaction, ids[0], 100.0, 1.0);

    assert_eq!(
        interaction.presentation().node_drag().unwrap().delta(),
        CanvasPoint { x: 105.0, y: 0.0 }
    );
}

#[test]
fn alignment_feature_index_removes_old_axis_membership_after_geometry_moves() {
    let (_runtime, view, ids) = alignment_view(2, 1.0, &[0.0, 205.0]);
    publish_node_layouts(
        &view,
        [(
            ids[1],
            CanvasRect {
                origin: CanvasPoint {
                    x: 1_000.0,
                    y: 5_000.0,
                },
                size: CanvasSize {
                    width: 100.0,
                    height: 80.0,
                },
            },
        )],
    );
    let mut interaction = interaction_session(&view, 64);
    move_and_resolve(&view, &mut interaction, ids[0], 100.0, 1.0);

    let presentation = interaction.presentation();
    let drag = presentation.node_drag().unwrap();
    assert_eq!(drag.delta(), CanvasPoint { x: 100.0, y: 0.0 });
    assert!(drag.guides().is_empty());
}

#[test]
fn alignment_budget_exhaustion_is_terminal_and_falls_back_to_raw_delta() {
    let (_runtime, view, ids) = alignment_view(2, 1.0, &[0.0, 206.0]);
    let mut interaction = interaction_session(&view, 1);
    move_and_resolve(&view, &mut interaction, ids[0], 100.0, 1.0);

    let presentation = interaction.presentation();
    let drag = presentation.node_drag().unwrap();
    assert_eq!(drag.delta(), CanvasPoint { x: 100.0, y: 0.0 });
    assert!(drag.guides().is_empty());
    assert!(!interaction.query_work_pending());
}

#[test]
fn alignment_lock_holds_inside_release_band_after_leaving_acquire_band() {
    let (_runtime, view, ids) = alignment_view(2, 1.0, &[0.0, 205.0]);
    let mut interaction = interaction_session(&view, 64);
    let start = node_drag_start_by_id(&view, ids[0], SelectionMode::Replace)
        .unwrap()
        .unwrap();
    let start_position = canvas_pointer_sample(&view, 0.0, 1.0);
    let first_position = canvas_pointer_sample(&view, 100.0, 1.0);
    let outside_position = canvas_pointer_sample(&view, 112.0, 1.0);
    interaction
        .press(CanvasPressTarget::NodeDrag(start), start_position)
        .unwrap();

    let first = interaction.move_to(first_position).unwrap();
    resolve_update(&mut interaction, first);
    assert_eq!(
        interaction.presentation().node_drag().unwrap().delta().x,
        105.0
    );

    let outside_acquire = interaction.move_to(outside_position).unwrap();
    resolve_update(&mut interaction, outside_acquire);
    assert_eq!(
        interaction.presentation().node_drag().unwrap().delta().x,
        105.0,
        "the 7px gap exceeds acquire=6 but remains inside release=9"
    );
}

#[test]
fn released_node_drag_waits_for_runtime_alignment_before_submitting() {
    let (_runtime, view, ids) = alignment_view(2, 1.0, &[0.0, 206.0]);
    let mut interaction = interaction_session(&view, 64);
    let start = node_drag_start_by_id(&view, ids[0], SelectionMode::Replace)
        .unwrap()
        .unwrap();
    let start_position = canvas_pointer_sample(&view, 0.0, 1.0);
    let moved_position = canvas_pointer_sample(&view, 80.0, 1.0);
    let released_position = canvas_pointer_sample(&view, 100.0, 1.0);
    interaction
        .press(CanvasPressTarget::NodeDrag(start), start_position)
        .unwrap();
    interaction.move_to(moved_position).unwrap();

    let released = interaction.release(released_position).unwrap();
    assert!(released.effects().is_empty());
    let committed = advance_alignment_query(&mut interaction, released);
    assert!(matches!(
        committed.effects(),
        [CanvasInteractionEffect::SubmitGraph { .. }]
    ));
    assert_eq!(
        interaction.presentation().node_drag().unwrap().delta().x,
        106.0
    );
}

#[test]
fn runtime_owned_alignment_query_resolves_against_latest_node_spatial_index() {
    let (_runtime, view, ids) = alignment_view(2, 1.0, &[0.0, 206.0]);
    let mut interaction = interaction_session(&view, 64);
    let start = node_drag_start_by_id(&view, ids[0], SelectionMode::Replace)
        .unwrap()
        .unwrap();
    let start_position = canvas_pointer_sample(&view, 0.0, 1.0);
    let moved_position = canvas_pointer_sample(&view, 100.0, 1.0);
    interaction
        .press(CanvasPressTarget::NodeDrag(start), start_position)
        .unwrap();
    let moved = interaction.move_to(moved_position).unwrap();
    assert!(moved.query_work_pending());

    publish_node_layouts(
        &view,
        [(
            ids[1],
            CanvasRect {
                origin: CanvasPoint { x: 240.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 80.0,
                },
            },
        )],
    );
    let resolved = advance_alignment_query(&mut interaction, moved);
    assert!(!resolved.query_work_pending());
    assert_eq!(
        interaction.presentation().node_drag().unwrap().delta().x,
        100.0
    );
}

fn alignment_view(
    count: usize,
    zoom: f32,
    x_positions: &[f32],
) -> (GraphRuntime, ViewSession, Vec<jellyflow_core::core::NodeId>) {
    let (graph, ids) = graph_with_nodes(count);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    view.set_viewport(ViewportTransform::new(CanvasPoint::default(), zoom).unwrap())
        .unwrap();
    publish_node_layouts(
        &view,
        ids.iter()
            .copied()
            .zip(x_positions.iter().copied())
            .map(|(node, x)| {
                (
                    node,
                    CanvasRect {
                        origin: CanvasPoint { x, y: 0.0 },
                        size: CanvasSize {
                            width: 100.0,
                            height: 80.0,
                        },
                    },
                )
            }),
    );
    assert_eq!(view.geometry_snapshot().unwrap().nodes().len(), count);
    (runtime, view, ids)
}

fn interaction_session(view: &ViewSession, query_entry_budget: usize) -> CanvasInteractionSession {
    CanvasInteractionSession::attach(
        view,
        CanvasInteractionConfig {
            alignment_snap: AlignmentSnapConfig::new(6.0, 9.0),
            query_entry_budget,
            ..CanvasInteractionConfig::default()
        },
    )
    .unwrap()
}

fn move_and_resolve(
    view: &ViewSession,
    interaction: &mut CanvasInteractionSession,
    node: jellyflow_core::core::NodeId,
    delta_in_canvas: f32,
    zoom: f32,
) {
    let start = node_drag_start_by_id(view, node, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    let start_position = canvas_pointer_sample(view, 0.0, zoom);
    let moved_position = canvas_pointer_sample(view, delta_in_canvas, zoom);
    interaction
        .press(CanvasPressTarget::NodeDrag(start), start_position)
        .unwrap();
    let update = interaction.move_to(moved_position).unwrap();
    assert!(update.effects().is_empty());
    advance_alignment_query(interaction, update);
    let presentation = interaction.presentation();
    let drag = presentation.node_drag().unwrap();
    assert_eq!(drag.nodes(), &[node]);
    assert_eq!(
        drag.delta().y,
        0.0,
        "alignment must preserve an exclusively horizontal drag"
    );
    assert_eq!(view.viewport_transform().unwrap().zoom, zoom);
}

fn resolve_update(
    interaction: &mut CanvasInteractionSession,
    update: crate::runtime::interaction::CanvasInteractionUpdate,
) {
    advance_alignment_query(interaction, update);
}

fn advance_alignment_query(
    interaction: &mut CanvasInteractionSession,
    update: crate::runtime::interaction::CanvasInteractionUpdate,
) -> crate::runtime::interaction::CanvasInteractionUpdate {
    assert!(update.query_work_pending());
    assert!(interaction.query_work_pending());
    let (resolved, needs_tick, receipt, terminal_error) =
        interaction.advance_query_work(1).into_parts();
    assert!(terminal_error.is_none());
    assert_eq!(receipt.pages_granted(), 1);
    assert_eq!(receipt.pages_attempted(), 1);
    assert!(matches!(
        receipt.page_receipts(),
        [CanvasInteractionQueryPageReceipt::AlignmentSnap(_)]
    ));
    assert!(!needs_tick);
    assert!(!interaction.query_work_pending());
    assert!(!resolved.query_work_pending());
    resolved
}

fn canvas_pointer_sample(view: &ViewSession, x: f32, zoom: f32) -> ViewportPointerSample {
    view.viewport_pointer_sample(ViewPoint::new(x * zoom, 0.0))
        .unwrap()
}
