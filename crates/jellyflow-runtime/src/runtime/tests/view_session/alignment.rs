use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

use crate::runtime::adapter_protocol::ViewPoint;
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::interaction::{
    AlignmentSnapConfig, AlignmentSnapQueryOutcome, CanvasInteractionConfig,
    CanvasInteractionEffect, CanvasInteractionQuery, CanvasInteractionQueryKind,
    CanvasInteractionSession, CanvasPressTarget, SelectionMode,
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
        let outcome = move_and_resolve(&view, &mut interaction, ids[0], 100.0, zoom);

        let result = match outcome {
            AlignmentSnapQueryOutcome::Complete(result) => result,
            other => panic!("small fixture must resolve alignment: {other:?}"),
        };
        let screen_adjustment = (result.delta().x - 100.0) * zoom;
        assert!((screen_adjustment - 6.0).abs() < 0.001);
        assert!(
            result
                .guides()
                .iter()
                .all(|guide| guide.canvas_coordinate().is_finite())
        );
    }
}

#[test]
fn alignment_uses_stable_node_order_for_equal_distance_candidates() {
    let (_runtime, view, ids) = alignment_view(3, 1.0, &[0.0, 205.0, 195.0]);
    let mut interaction = interaction_session(&view, 64);
    let outcome = move_and_resolve(&view, &mut interaction, ids[0], 100.0, 1.0);

    let AlignmentSnapQueryOutcome::Complete(result) = outcome else {
        panic!("small fixture must resolve alignment");
    };
    assert_eq!(result.delta().x, 105.0);
    assert_eq!(result.candidates_examined(), 2);
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
    let outcome = move_and_resolve(&view, &mut interaction, ids[0], 100.0, 1.0);

    let AlignmentSnapQueryOutcome::Complete(result) = outcome else {
        panic!("axis feature buckets must resolve the distant column");
    };
    assert_eq!(result.delta().x, 105.0);
    assert_eq!(result.delta().y, 0.0);
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
    let outcome = move_and_resolve(&view, &mut interaction, ids[0], 100.0, 1.0);

    let AlignmentSnapQueryOutcome::Complete(result) = outcome else {
        panic!("bounded query should complete without a candidate");
    };
    assert_eq!(result.delta(), CanvasPoint { x: 100.0, y: 0.0 });
    assert_eq!(result.candidates_examined(), 0);
}

#[test]
fn alignment_budget_exhaustion_is_terminal_for_the_query_and_does_not_scan_unboundedly() {
    let (_runtime, view, ids) = alignment_view(2, 1.0, &[0.0, 206.0]);
    let mut interaction = interaction_session(&view, 1);
    let outcome = move_and_resolve(&view, &mut interaction, ids[0], 100.0, 1.0);

    let AlignmentSnapQueryOutcome::BudgetExhausted(work) = outcome else {
        panic!("one entry cannot admit both the moving node and candidate");
    };
    assert!(work.entries_examined() <= 1);
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
    let query = take_only_query(released);
    let committed = interaction.resolve_query(query).unwrap();
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
    let query = take_only_query(moved);
    assert!(matches!(
        query.kind_for_test(),
        CanvasInteractionQueryKind::AlignmentSnap { .. }
    ));

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
    let resolved = interaction.resolve_query(query).unwrap();
    assert!(resolved.queries().is_empty());
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
) -> AlignmentSnapQueryOutcome {
    let start = node_drag_start_by_id(view, node, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    let start_position = canvas_pointer_sample(view, 0.0, zoom);
    let moved_position = canvas_pointer_sample(view, delta_in_canvas, zoom);
    interaction
        .press(CanvasPressTarget::NodeDrag(start), start_position)
        .unwrap();
    let update = interaction.move_to(moved_position).unwrap();
    let query = take_alignment_query(update);
    assert_eq!(query.moving_nodes(), &[node]);
    assert_eq!(
        query.delta(),
        CanvasPoint {
            x: delta_in_canvas,
            y: 0.0
        }
    );
    assert_eq!(query.view(), view.id());
    assert_eq!(query.view_generation(), view.generation().unwrap());
    assert!(query.entry_budget() > 0);
    assert_eq!(query.acquire_tolerance_in_view(), 6.0);
    assert_eq!(query.release_tolerance_in_view(), 9.0);
    assert_eq!(view.viewport_transform().unwrap().zoom, zoom);
    view.alignment_snap(query).unwrap()
}

fn resolve_update(
    interaction: &mut CanvasInteractionSession,
    update: crate::runtime::interaction::CanvasInteractionUpdate,
) {
    interaction.resolve_query(take_only_query(update)).unwrap();
}

fn take_alignment_query(
    update: crate::runtime::interaction::CanvasInteractionUpdate,
) -> crate::runtime::interaction::AlignmentSnapQuery {
    let query = take_only_query(update);
    let CanvasInteractionQueryKind::AlignmentSnap { query, .. } = query.into_kind_for_test() else {
        panic!("expected one alignment query");
    };
    query
}

fn take_only_query(
    update: crate::runtime::interaction::CanvasInteractionUpdate,
) -> CanvasInteractionQuery {
    let mut queries = update.into_dispatch().queries;
    assert_eq!(queries.len(), 1, "expected exactly one interaction query");
    queries.remove(0)
}

fn canvas_pointer_sample(view: &ViewSession, x: f32, zoom: f32) -> ViewportPointerSample {
    view.viewport_pointer_sample(ViewPoint::new(x * zoom, 0.0))
        .unwrap()
}
