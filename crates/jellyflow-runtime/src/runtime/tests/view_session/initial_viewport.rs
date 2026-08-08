use jellyflow_core::core::CanvasPoint;

use crate::io::{ViewSessionSeed, ViewportSeed};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::view_session::ViewSessionOptions;
use crate::runtime::viewport::ViewportTransform;

use super::fixtures::graph_with_nodes;

#[test]
fn unseeded_view_accepts_exactly_one_initial_viewport_claim() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("view");
    let expected = view.viewport_snapshot().expect("initial viewport");
    let fitted = ViewportTransform::new(CanvasPoint { x: 32.0, y: -18.0 }, 1.4)
        .expect("valid fitted viewport");

    let applied = view
        .claim_initial_viewport(expected, fitted)
        .expect("initial claim")
        .expect("unseeded view accepts the claim");
    assert_eq!(applied.transform(), fitted);
    assert_eq!(view.viewport_snapshot().unwrap(), applied);

    let ignored = view
        .claim_initial_viewport(
            applied,
            ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap(),
        )
        .expect("second claim is a deterministic no-op");
    assert_eq!(ignored, None);
    assert_eq!(view.viewport_snapshot().unwrap(), applied);
}

#[test]
fn persisted_or_user_owned_viewport_always_wins_over_initial_fit() {
    let (graph, _) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let persisted = ViewportTransform::new(CanvasPoint { x: 90.0, y: 45.0 }, 0.75).unwrap();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let seeded = runtime
        .attach_view(ViewSessionOptions::with_seed(
            ViewSessionSeed::new(graph_id)
                .with_viewport(ViewportSeed::new(persisted.pan, persisted.zoom)),
        ))
        .expect("seeded view");
    let seeded_snapshot = seeded.viewport_snapshot().unwrap();
    assert_eq!(seeded_snapshot.transform(), persisted);
    assert_eq!(
        seeded
            .claim_initial_viewport(
                seeded_snapshot,
                ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap(),
            )
            .unwrap(),
        None
    );
    assert_eq!(seeded.viewport_transform().unwrap(), persisted);

    let unseeded = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("unseeded view");
    let before_user_input = unseeded.viewport_snapshot().unwrap();
    let user_viewport = ViewportTransform::new(CanvasPoint { x: -12.0, y: 8.0 }, 1.1).unwrap();
    unseeded.set_viewport(user_viewport).unwrap();
    assert_eq!(
        unseeded
            .claim_initial_viewport(before_user_input, persisted)
            .unwrap(),
        None
    );
    assert_eq!(unseeded.viewport_transform().unwrap(), user_viewport);
}

#[test]
fn ordinary_noop_viewport_operation_consumes_initial_claim() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("view");
    let initial = view.viewport_snapshot().unwrap();

    assert_eq!(view.set_viewport(initial.transform()).unwrap(), initial);
    assert_eq!(
        view.claim_initial_viewport(
            initial,
            ViewportTransform::new(CanvasPoint { x: 1.0, y: 2.0 }, 2.0).unwrap(),
        )
        .unwrap(),
        None
    );
    assert_eq!(view.viewport_snapshot().unwrap(), initial);
}
