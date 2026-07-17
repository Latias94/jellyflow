use std::cell::Cell;
use std::rc::Rc;

use jellyflow_core::core::CanvasPoint;

use crate::io::ViewSessionSeed;
use crate::runtime::commands::{
    GraphCommand, GraphCommandOutcome, IncidentEdgePolicy, TopologyEdit,
};
use crate::runtime::graph_runtime::GraphRuntime;

use super::fixtures::{graph_with_nodes, view_options};

#[test]
fn revisioned_position_command_derives_and_undoes_the_authoritative_inverse() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let incarnation = runtime.node_incarnation(ids[0]).unwrap().unwrap();
    let point = CanvasPoint { x: 10.0, y: 20.0 };

    runtime
        .execute(GraphCommand::set_node_position(0, ids[0], point))
        .expect("execute")
        .applied()
        .expect("position command applies");

    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&ids[0])
            .unwrap()
            .pos,
        point
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(runtime.node_incarnation(ids[0]).unwrap(), Some(incarnation));
    let revision = runtime.snapshot().unwrap().revision();
    runtime
        .execute(GraphCommand::undo(revision))
        .unwrap()
        .applied()
        .expect("undo applies");
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&ids[0])
            .unwrap()
            .pos,
        CanvasPoint::default()
    );
    assert_eq!(runtime.node_incarnation(ids[0]).unwrap(), Some(incarnation));
}

#[test]
fn non_finite_command_is_rejected_without_runtime_session_or_history_side_effects() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let events = Rc::new(Cell::new(0));
    let callback_events = events.clone();
    view.subscribe(move |_| callback_events.set(callback_events.get() + 1))
        .unwrap();
    let before = runtime.snapshot().unwrap();
    let history = runtime.history_state().unwrap();
    let runtime_stats = runtime.stats().unwrap();
    let view_stats = view.stats().unwrap();

    let outcome = runtime
        .execute(GraphCommand::set_node_position(
            0,
            ids[0],
            CanvasPoint {
                x: f32::NAN,
                y: 0.0,
            },
        ))
        .expect("invalid command returns a structured outcome");

    assert!(outcome.rejection().is_some());
    assert!(runtime.snapshot().unwrap().ptr_eq(&before));
    assert_eq!(runtime.history_state().unwrap(), history);
    assert_eq!(runtime.stats().unwrap(), runtime_stats);
    assert_eq!(view.stats().unwrap(), view_stats);
    assert_eq!(events.get(), 0);
}

#[test]
fn net_noop_does_not_build_or_deliver_a_patch() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let before = runtime.snapshot().unwrap();
    let outcome = runtime
        .execute(GraphCommand::set_node_position(
            0,
            ids[0],
            CanvasPoint::default(),
        ))
        .unwrap();

    assert!(matches!(outcome, GraphCommandOutcome::Noop(_)));
    assert!(runtime.snapshot().unwrap().ptr_eq(&before));
    assert_eq!(runtime.stats().unwrap().patch_builds, 0);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
}

#[test]
fn node_binding_subscription_is_retired_before_reused_node_id_gets_a_new_incarnation() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let old_incarnation = runtime.node_incarnation(ids[0]).unwrap().unwrap();
    let old_calls = Rc::new(Cell::new(0));
    let callback_calls = old_calls.clone();
    view.subscribe_node(ids[0], move |_| {
        callback_calls.set(callback_calls.get() + 1)
    })
    .unwrap();

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(ids[0], IncidentEdgePolicy::Reject),
        ))
        .unwrap()
        .applied()
        .expect("remove applies");
    let revision = runtime.snapshot().unwrap().revision();
    runtime
        .execute(GraphCommand::undo(revision))
        .unwrap()
        .applied()
        .expect("undo applies");
    let new_incarnation = runtime.node_incarnation(ids[0]).unwrap().unwrap();
    assert!(new_incarnation > old_incarnation);

    let new_calls = Rc::new(Cell::new(0));
    let callback_calls = new_calls.clone();
    view.subscribe_node(ids[0], move |event| {
        assert_eq!(event.incarnation(), new_incarnation);
        callback_calls.set(callback_calls.get() + 1);
    })
    .unwrap();
    runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().revision(),
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
        .unwrap();

    assert_eq!(old_calls.get(), 0);
    assert_eq!(new_calls.get(), 1);
}
