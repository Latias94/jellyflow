use std::cell::RefCell;
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
    let a_events = Rc::new(RefCell::new(Vec::new()));
    let a_events_observer = a_events.clone();
    view_a
        .subscribe(move |event| {
            if let ViewSessionEvent::RuntimePatched { reconciliation, .. } = event {
                a_events_observer.borrow_mut().push(reconciliation);
            }
        })
        .unwrap();
    let b_events = Rc::new(RefCell::new(Vec::new()));
    let b_events_observer = b_events.clone();
    view_b
        .subscribe(move |event| {
            if let ViewSessionEvent::RuntimePatched { reconciliation, .. } = event {
                b_events_observer.borrow_mut().push(reconciliation);
            }
        })
        .unwrap();

    view_b
        .execute(GraphCommand::set_node_position(
            runtime.snapshot().unwrap().version(),
            ids[0],
            CanvasPoint { x: 32.0, y: 48.0 },
        ))
        .expect("shared graph commit")
        .into_result()
        .unwrap();

    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(view_a.graph_snapshot().unwrap().revision(), 1);
    assert_eq!(view_b.graph_snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(a_events.borrow().len(), 1);
    assert_eq!(b_events.borrow().len(), 1);
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
    let a_events = a_events.borrow();
    let a_geometry_work = a_events[0].geometry();
    assert_eq!(a_geometry_work.spatial_updates(), 1);
    assert_eq!(a_geometry_work.route_invalidations(), 0);
    let b_events = b_events.borrow();
    let b_geometry_work = b_events[0].geometry();
    assert_eq!(b_geometry_work.spatial_updates(), 0);
    assert_eq!(b_geometry_work.route_invalidations(), 0);
}
