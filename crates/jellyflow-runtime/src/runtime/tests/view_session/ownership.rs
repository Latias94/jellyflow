use jellyflow_core::core::CanvasPoint;

use crate::io::{ViewSelectionSeed, ViewSessionSeed, ViewportSeed};
use crate::runtime::commands::GraphCommand;
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::tests::fixtures::set_selection_by_id;
use crate::runtime::viewport::ViewportTransform;

use super::fixtures::{graph_with_nodes, view_options};

#[test]
fn view_only_mutations_do_not_change_shared_graph_or_history() {
    let (graph, ids) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let graph_id = runtime.snapshot().unwrap().graph().graph_id();
    let view_a = runtime
        .attach_view(view_options(
            ViewSessionSeed::new(graph_id)
                .with_viewport(ViewportSeed::new(CanvasPoint { x: 10.0, y: 20.0 }, 1.0))
                .with_selection(ViewSelectionSeed::new(vec![ids[0]], vec![], vec![])),
        ))
        .expect("view A");
    let view_b = runtime
        .attach_view(view_options(
            ViewSessionSeed::new(graph_id)
                .with_viewport(ViewportSeed::new(CanvasPoint { x: -5.0, y: 8.0 }, 1.0))
                .with_selection(ViewSelectionSeed::new(vec![ids[1]], vec![], vec![])),
        ))
        .expect("view B");

    let before = runtime.snapshot().expect("runtime snapshot");
    let history_before = runtime.history_state().expect("history state");
    view_a
        .set_viewport(ViewportTransform::new(CanvasPoint { x: 44.0, y: 55.0 }, 1.75).unwrap())
        .expect("view mutation");
    set_selection_by_id(&view_a, [ids[1]], [], Vec::new()).expect("selection mutation");

    let after = runtime.snapshot().expect("runtime snapshot");
    assert_eq!(after.revision(), before.revision());
    assert!(after.ptr_eq(&before));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(view_a.graph_snapshot().unwrap().revision(), 0);
    assert_eq!(view_b.graph_snapshot().unwrap().revision(), 0);
    assert_eq!(view_a.export_seed().unwrap().selection().nodes(), [ids[1]]);
    assert_eq!(view_b.export_seed().unwrap().selection().nodes(), [ids[1]]);
    assert_eq!(
        view_b.export_seed().unwrap().viewport().pan(),
        CanvasPoint { x: -5.0, y: 8.0 }
    );
}

#[test]
fn runtime_snapshot_contains_owned_graph_facts_without_view_state() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let before = runtime.snapshot().expect("snapshot");
    runtime
        .execute(GraphCommand::set_node_data(
            before.version(),
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
        .into_result()
        .unwrap();
    let after = runtime.snapshot().expect("updated snapshot");

    drop(runtime);

    assert_eq!(
        before.graph().node(&ids[0]).unwrap().data,
        serde_json::Value::Null
    );
    assert_eq!(before.revision(), 0);
    assert_eq!(
        after.graph().node(&ids[0]).unwrap().data,
        serde_json::json!({ "value": 1 })
    );
    assert_eq!(after.revision(), 1);
    assert!(!before.ptr_eq(&after));
}
