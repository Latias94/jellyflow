use std::cell::Cell;
use std::rc::Rc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

use crate::io::ViewSessionSeed;
use crate::runtime::commands::GraphCommand;
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::view_session::ViewSessionEvent;

use super::fixtures::{graph_with_nodes, publish_node_layouts, view_options};

#[test]
fn graph_commit_fans_out_one_shared_patch_while_view_facts_stay_isolated() {
    let (graph, ids) = graph_with_nodes(2);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view_a = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view A");
    let view_b = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view B");
    publish_node_layouts(
        &view_a,
        [(
            ids[0],
            CanvasRect {
                origin: CanvasPoint { x: 1.0, y: 2.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 80.0,
                },
            },
        )],
    );
    assert!(view_b.geometry_snapshot().unwrap().node(ids[0]).is_none());
    let stats_before = view_a.stats().unwrap();

    let a_events = Rc::new(Cell::new(0));
    let a_events_observer = a_events.clone();
    view_a
        .subscribe(move |event| {
            if matches!(event, ViewSessionEvent::RuntimePatched { .. }) {
                a_events_observer.set(a_events_observer.get() + 1);
            }
        })
        .unwrap();
    let b_events = Rc::new(Cell::new(0));
    let b_events_observer = b_events.clone();
    view_b
        .subscribe(move |event| {
            if matches!(event, ViewSessionEvent::RuntimePatched { .. }) {
                b_events_observer.set(b_events_observer.get() + 1);
            }
        })
        .unwrap();

    view_b
        .execute(GraphCommand::set_node_position(
            0,
            ids[0],
            CanvasPoint { x: 32.0, y: 48.0 },
        ))
        .expect("shared graph commit");

    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(view_a.graph_snapshot().unwrap().revision(), 1);
    assert_eq!(view_b.graph_snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(a_events.get(), 1);
    assert_eq!(b_events.get(), 1);
    assert_eq!(
        view_a
            .geometry_snapshot()
            .unwrap()
            .node(ids[0])
            .unwrap()
            .bounds()
            .origin,
        CanvasPoint { x: 33.0, y: 50.0 }
    );
    assert!(view_b.geometry_snapshot().unwrap().node(ids[0]).is_none());
    let stats_after = view_a.stats().unwrap();
    assert_eq!(
        stats_after.spatial_updates - stats_before.spatial_updates,
        1
    );
    assert_eq!(
        stats_after.route_invalidations - stats_before.route_invalidations,
        0
    );
}
