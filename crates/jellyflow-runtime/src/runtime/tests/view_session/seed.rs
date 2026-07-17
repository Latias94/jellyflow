use jellyflow_core::core::{CanvasPoint, GraphId, NodeId};

use crate::io::{ViewSelectionSeed, ViewSessionSeed, ViewStackingSeed, ViewportSeed};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::view_session::{AttachViewError, ViewSessionOptions};

use super::fixtures::{graph_with_nodes, view_options};

#[test]
fn attach_rejects_cross_graph_seed_without_allocating_a_session() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let options = ViewSessionOptions::with_seed(ViewSessionSeed::new(GraphId::from_u128(0xdead)));

    assert!(matches!(
        runtime.attach_view(options),
        Err(AttachViewError::SeedGraphMismatch(_))
    ));
    assert_eq!(runtime.live_view_count(), 0);
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("first valid view");
    assert_eq!(view.id().get(), 1);
}

#[test]
fn attach_sanitizes_stale_and_duplicate_seed_facts_in_stable_order() {
    let (graph, ids) = graph_with_nodes(2);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let stale = NodeId::from_u128(0xffff);
    let view = runtime
        .attach_view(view_options(
            ViewSessionSeed::new(graph_id)
                .with_viewport(ViewportSeed::new(
                    CanvasPoint {
                        x: f32::INFINITY,
                        y: 4.0,
                    },
                    -2.0,
                ))
                .with_selection(ViewSelectionSeed::new(
                    vec![ids[1], ids[1], stale, ids[0]],
                    vec![],
                    vec![],
                ))
                .with_stacking(ViewStackingSeed::new(
                    vec![ids[0], ids[0], stale, ids[1]],
                    vec![],
                    vec![],
                )),
        ))
        .expect("sanitized view");

    let seed = view.export_seed().unwrap();
    assert_eq!(seed.viewport().pan(), CanvasPoint::default());
    assert_eq!(seed.viewport().zoom(), 1.0);
    assert_eq!(seed.selection().nodes(), [ids[1], ids[0]]);
    assert_eq!(seed.stacking().nodes(), [ids[0], ids[1]]);
}

#[test]
fn exporting_a_seed_never_changes_graph_revision_or_history() {
    let (graph, ids) = graph_with_nodes(2);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    view.set_viewport(CanvasPoint { x: 4.0, y: 8.0 }, 1.5)
        .unwrap();
    view.set_selection(vec![ids[1]], Vec::new(), Vec::new())
        .unwrap();

    let before = runtime.snapshot().unwrap();
    let history = runtime.history_state().unwrap();
    let seed = view.export_seed().unwrap();

    assert_eq!(seed.graph_id(), graph_id);
    assert_eq!(seed.viewport().pan(), CanvasPoint { x: 4.0, y: 8.0 });
    assert_eq!(seed.selection().nodes(), &[ids[1]]);
    assert!(runtime.snapshot().unwrap().ptr_eq(&before));
    assert_eq!(runtime.history_state().unwrap(), history);
}
