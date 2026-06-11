use jellyflow_core::core::{CanvasPoint, CanvasSize};

use crate::runtime::layout::{DugongLayoutApplyOutcome, LayoutRequest};
use crate::runtime::tests::fixtures::{make_graph, make_store};

#[test]
fn dugong_layout_transaction_dispatches_through_store() {
    let (graph, a, b, _out, _in, _edge) = make_graph();
    let mut store = make_store(graph);
    let request = LayoutRequest::all().with_measured_node_sizes([
        (
            a,
            CanvasSize {
                width: 160.0,
                height: 80.0,
            },
        ),
        (
            b,
            CanvasSize {
                width: 160.0,
                height: 80.0,
            },
        ),
    ]);

    let tx = store
        .dugong_layout_transaction(&request)
        .expect("layout transaction");
    let outcome = store.dispatch_transaction(&tx).expect("dispatch");

    assert_eq!(outcome.committed().label(), Some("Layout graph"));
    assert!(!outcome.committed().is_empty());
    assert!(
        store.graph().nodes[&a].pos != (CanvasPoint { x: 0.0, y: 0.0 })
            || store.graph().nodes[&b].pos != (CanvasPoint { x: 100.0, y: 0.0 })
    );
}

#[test]
fn apply_dugong_layout_returns_layout_and_dispatch_outcome() {
    let (graph, a, _b, _out, _in, _edge) = make_graph();
    let mut store = make_store(graph);
    let request = LayoutRequest::all();

    let outcome = store
        .apply_dugong_layout(&request)
        .expect("apply dugong layout");

    let DugongLayoutApplyOutcome { layout, dispatch } = outcome;
    assert!(layout.node_position(a).is_some());
    let dispatch = dispatch.expect("layout changed node positions");
    assert_eq!(dispatch.committed().label(), Some("Layout graph"));
}
