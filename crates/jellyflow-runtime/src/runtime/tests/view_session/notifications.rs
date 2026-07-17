use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::io::ViewSessionSeed;
use crate::runtime::commands::GraphCommand;
use crate::runtime::graph_runtime::{GraphRuntime, GraphRuntimeError};
use crate::runtime::view_session::ViewSessionEvent;

use super::fixtures::{graph_with_nodes, view_options};

#[test]
fn runtime_document_subscription_is_owned_and_unsubscribable() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let revisions = Rc::new(RefCell::new(Vec::new()));
    let callback_revisions = revisions.clone();
    let token = runtime
        .subscribe(move |event| {
            if let crate::runtime::graph_runtime::GraphRuntimeEvent::Patched { patch } = event {
                callback_revisions.borrow_mut().push(patch.to_revision());
            }
        })
        .unwrap();

    for value in [1, 2] {
        let revision = runtime.snapshot().unwrap().revision();
        runtime
            .execute(GraphCommand::set_node_data(
                revision,
                ids[0],
                serde_json::json!({ "value": value }),
            ))
            .unwrap();
        if value == 1 {
            assert!(runtime.unsubscribe(token));
        }
    }

    assert_eq!(&*revisions.borrow(), &[1]);
}

#[test]
fn runtime_subscription_token_cannot_unsubscribe_another_runtime() {
    let (graph_a, _) = graph_with_nodes(1);
    let (graph_b, ids_b) = graph_with_nodes(1);
    let runtime_a = GraphRuntime::builder(Default::default())
        .build(graph_a)
        .expect("runtime A");
    let runtime_b = GraphRuntime::builder(Default::default())
        .build(graph_b)
        .expect("runtime B");
    let foreign = runtime_a.subscribe(|_| {}).expect("runtime A token");
    let calls = Rc::new(Cell::new(0));
    let callback_calls = calls.clone();
    let local = runtime_b
        .subscribe(move |event| {
            if matches!(
                event,
                crate::runtime::graph_runtime::GraphRuntimeEvent::Patched { .. }
            ) {
                callback_calls.set(callback_calls.get() + 1);
            }
        })
        .expect("runtime B token");

    assert!(!runtime_b.unsubscribe(foreign));
    runtime_b
        .execute(GraphCommand::set_node_data(
            0,
            ids_b[0],
            serde_json::json!({ "value": 1 }),
        ))
        .expect("runtime B mutation");

    assert_eq!(calls.get(), 1);
    assert!(runtime_b.unsubscribe(local));
}

#[test]
fn view_subscription_token_cannot_unsubscribe_another_view() {
    let (graph, ids) = graph_with_nodes(1);
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
    let foreign = view_a.subscribe(|_| {}).expect("view A token");
    let calls = Rc::new(Cell::new(0));
    let callback_calls = calls.clone();
    let local = view_b
        .subscribe(move |event| {
            if matches!(event, ViewSessionEvent::RuntimePatched { .. }) {
                callback_calls.set(callback_calls.get() + 1);
            }
        })
        .expect("view B token");

    assert!(!view_b.unsubscribe(foreign).expect("foreign unsubscribe"));
    runtime
        .execute(GraphCommand::set_node_data(
            0,
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
        .expect("shared mutation");

    assert_eq!(calls.get(), 1);
    assert!(view_b.unsubscribe(local).expect("local unsubscribe"));
}

#[test]
fn history_clear_is_rejected_inside_a_patch_batch_and_published_when_run_afterward() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("runtime"),
    );
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let callback_runtime = runtime.clone();
    let rejection = Rc::new(RefCell::new(None));
    let callback_rejection = rejection.clone();
    view.subscribe(move |_| {
        *callback_rejection.borrow_mut() = callback_runtime.clear_history().err();
    })
    .unwrap();
    let history_events = Rc::new(Cell::new(0));
    let callback_events = history_events.clone();
    runtime
        .subscribe(move |event| {
            if matches!(
                event,
                crate::runtime::graph_runtime::GraphRuntimeEvent::HistoryChanged { .. }
            ) {
                callback_events.set(callback_events.get() + 1);
            }
        })
        .unwrap();

    runtime
        .execute(GraphCommand::set_node_data(
            0,
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
        .unwrap();

    assert!(matches!(
        rejection.borrow().as_ref(),
        Some(GraphRuntimeError::ReentrantMutation)
    ));
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    runtime.clear_history().unwrap();
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(history_events.get(), 1);
}

#[test]
fn callbacks_can_query_and_self_unsubscribe_without_borrow_panics() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let weak = view.downgrade();
    let token_slot = Rc::new(Cell::new(None));
    let calls = Rc::new(Cell::new(0));
    let callback_token = token_slot.clone();
    let callback_calls = calls.clone();
    let callback_weak = weak.clone();
    let token = view
        .subscribe(move |_| {
            callback_calls.set(callback_calls.get() + 1);
            assert_eq!(callback_weak.graph_snapshot().unwrap().revision(), 1);
            callback_weak
                .unsubscribe(callback_token.get().expect("installed token"))
                .unwrap();
        })
        .unwrap();
    token_slot.set(Some(token));

    for value in [1, 2] {
        let revision = runtime.snapshot().unwrap().revision();
        runtime
            .execute(GraphCommand::set_node_data(
                revision,
                ids[0],
                serde_json::json!({ "value": value }),
            ))
            .unwrap();
    }

    assert_eq!(calls.get(), 1);
    assert_eq!(runtime.snapshot().unwrap().revision(), 2);
}

#[test]
fn mutation_from_a_notification_is_rejected_structurally_and_later_commits_still_work() {
    let (graph, ids) = graph_with_nodes(2);
    let graph_id = graph.graph_id();
    let outer_node = ids[0];
    let nested_node = ids[1];
    let runtime = Rc::new(
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("runtime"),
    );
    let view_a = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view A");
    let view_b = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view B");
    let rejection = Rc::new(RefCell::new(None));
    let callback_runtime = runtime.clone();
    let callback_rejection = rejection.clone();
    view_a
        .subscribe(move |event| {
            let ViewSessionEvent::RuntimePatched { patch } = event else {
                return;
            };
            if patch.to_revision() != 1 {
                return;
            }
            let result = callback_runtime.execute(GraphCommand::set_node_data(
                patch.to_revision(),
                nested_node,
                serde_json::json!({ "nested": true }),
            ));
            *callback_rejection.borrow_mut() = result.err();
        })
        .unwrap();
    let b_revisions = Rc::new(RefCell::new(Vec::new()));
    let callback_revisions = b_revisions.clone();
    view_b
        .subscribe(move |event| {
            if let ViewSessionEvent::RuntimePatched { patch } = event {
                callback_revisions.borrow_mut().push(patch.to_revision());
            }
        })
        .unwrap();

    runtime
        .execute(GraphCommand::set_node_data(
            0,
            outer_node,
            serde_json::json!({ "outer": true }),
        ))
        .unwrap();

    assert!(matches!(
        rejection.borrow().as_ref(),
        Some(GraphRuntimeError::ReentrantMutation)
    ));
    assert_eq!(&*b_revisions.borrow(), &[1]);
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);

    runtime
        .execute(GraphCommand::set_node_data(
            1,
            nested_node,
            serde_json::json!({ "later": true }),
        ))
        .unwrap();
    assert_eq!(runtime.snapshot().unwrap().revision(), 2);
}
