use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

use crate::runtime::adapter_protocol::ViewPoint;
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::interaction::{
    AlignmentSnapConfig, AlignmentSnapQueryOutcome, CanvasInteractionConfig,
    CanvasInteractionEffect, CanvasInteractionQuery, CanvasInteractionSession, CanvasPressTarget,
    InteractionPosition, SelectionMode,
};
use crate::runtime::view_session::{ViewSession, ViewSessionOptions};
use crate::runtime::viewport::ViewportTransform;

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
    let start = view
        .node_drag_start(ids[0], SelectionMode::Replace)
        .unwrap()
        .unwrap();
    interaction
        .press(
            CanvasPressTarget::NodeDrag(start),
            canvas_position(0.0, 1.0),
        )
        .unwrap();

    let first = interaction.move_to(canvas_position(100.0, 1.0)).unwrap();
    resolve_update(&view, &mut interaction, first);
    assert_eq!(
        interaction.presentation().node_drag().unwrap().delta().x,
        105.0
    );

    let outside_acquire = interaction.move_to(canvas_position(112.0, 1.0)).unwrap();
    resolve_update(&view, &mut interaction, outside_acquire);
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
    let start = view
        .node_drag_start(ids[0], SelectionMode::Replace)
        .unwrap()
        .unwrap();
    interaction
        .press(
            CanvasPressTarget::NodeDrag(start),
            canvas_position(0.0, 1.0),
        )
        .unwrap();
    interaction.move_to(canvas_position(80.0, 1.0)).unwrap();

    let released = interaction.release(canvas_position(100.0, 1.0)).unwrap();
    assert!(released.effects().is_empty());
    let (id, query) = take_alignment_query(released);
    let outcome = view.alignment_snap(query).unwrap();
    let committed = interaction.resolve_alignment_snap(id, outcome).unwrap();
    assert!(matches!(
        committed.effects(),
        [CanvasInteractionEffect::SubmitGraph { .. }]
    ));
    assert_eq!(
        interaction.presentation().node_drag().unwrap().delta().x,
        106.0
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
    view.set_viewport(CanvasPoint::default(), zoom).unwrap();
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
    CanvasInteractionSession::new(
        view.id(),
        view.generation().unwrap(),
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
    let start = view
        .node_drag_start(node, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    interaction
        .press(
            CanvasPressTarget::NodeDrag(start),
            canvas_position(0.0, zoom),
        )
        .unwrap();
    let update = interaction
        .move_to(canvas_position(delta_in_canvas, zoom))
        .unwrap();
    let (_, query) = take_alignment_query(update);
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
    view: &ViewSession,
    interaction: &mut CanvasInteractionSession,
    update: crate::runtime::interaction::CanvasInteractionUpdate,
) {
    let (id, query) = take_alignment_query(update);
    let outcome = view.alignment_snap(query).unwrap();
    interaction.resolve_alignment_snap(id, outcome).unwrap();
}

fn take_alignment_query(
    update: crate::runtime::interaction::CanvasInteractionUpdate,
) -> (
    crate::runtime::interaction::InteractionQueryId,
    crate::runtime::interaction::AlignmentSnapQuery,
) {
    let mut queries = update.into_dispatch().queries;
    let CanvasInteractionQuery::AlignmentSnap { id, query, .. } = queries.remove(0) else {
        panic!("expected one alignment query");
    };
    assert!(queries.is_empty());
    (id, query)
}

fn canvas_position(x: f32, zoom: f32) -> InteractionPosition {
    let transform = ViewportTransform::new(CanvasPoint::default(), zoom).unwrap();
    InteractionPosition::from_view(ViewPoint::new(x * zoom, 0.0), transform).unwrap()
}
