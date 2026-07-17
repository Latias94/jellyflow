use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::io::ViewSessionSeed;
use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::{GraphRuntime, GraphRuntimeError};
use crate::runtime::view_session::{
    ViewCloseReason, ViewCloseRequest, ViewSessionError, ViewSessionEvent,
};

use super::fixtures::{graph_with_nodes, view_options};

#[test]
fn close_is_terminal_and_idempotent() {
    let (graph, _) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");

    let outcome = view.close().expect("first close");
    assert_eq!(outcome.reason(), ViewCloseReason::HostClosed);
    assert!(outcome.receipts().is_empty());
    assert!(matches!(
        view.close(),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed
        })
    ));
    assert_eq!(view.close_reason(), Some(ViewCloseReason::HostClosed));
    assert!(matches!(
        view.graph_snapshot(),
        Err(ViewSessionError::Closed { .. })
    ));
    assert!(matches!(
        view.set_viewport(Default::default(), 1.0),
        Err(ViewSessionError::Closed { .. })
    ));
    assert_eq!(runtime.live_view_count(), 0);
}

#[test]
fn explicit_close_publishes_one_terminal_event_after_retiring_view_facts() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = Rc::new(
        runtime
            .attach_view(view_options(ViewSessionSeed::new(graph_id)))
            .expect("view"),
    );
    view.seed_all_node_facts_for_test();

    let terminal_reasons = Rc::new(RefCell::new(Vec::new()));
    let callback_reasons = terminal_reasons.clone();
    let callback_view = view.clone();
    view.subscribe(move |event| {
        if let ViewSessionEvent::Closed { reason } = event {
            assert_eq!(callback_view.close_reason(), Some(reason));
            let facts = callback_view.derived_fact_counts_for_test();
            assert_eq!((facts.nodes, facts.routes), (0, 0));
            callback_reasons.borrow_mut().push(reason);
        }
    })
    .expect("terminal subscription");
    let node_subscription_owner = Rc::new(());
    let callback_owner = node_subscription_owner.clone();
    view.subscribe_node(ids[0], move |_| {
        let _ = &callback_owner;
    })
    .expect("node subscription");
    assert_eq!(Rc::strong_count(&node_subscription_owner), 2);

    view.close().expect("first close");
    assert!(matches!(
        view.close(),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed
        })
    ));
    assert_eq!(&*terminal_reasons.borrow(), &[ViewCloseReason::HostClosed]);
    assert_eq!(Rc::strong_count(&view), 1);
    assert_eq!(Rc::strong_count(&node_subscription_owner), 1);
    assert!(!view.has_node_geometry_for_test(ids[0]));
}

#[test]
fn runtime_drop_publishes_one_terminal_event_after_retiring_view_facts() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = Rc::new(
        runtime
            .attach_view(view_options(ViewSessionSeed::new(graph_id)))
            .expect("view"),
    );
    view.seed_all_node_facts_for_test();

    let terminal_reasons = Rc::new(RefCell::new(Vec::new()));
    let callback_reasons = terminal_reasons.clone();
    let callback_view = view.clone();
    view.subscribe(move |event| {
        if let ViewSessionEvent::Closed { reason } = event {
            assert_eq!(callback_view.close_reason(), Some(reason));
            let facts = callback_view.derived_fact_counts_for_test();
            assert_eq!((facts.nodes, facts.routes), (0, 0));
            callback_reasons.borrow_mut().push(reason);
        }
    })
    .expect("terminal subscription");

    drop(runtime);

    assert_eq!(
        &*terminal_reasons.borrow(),
        &[ViewCloseReason::RuntimeDropped]
    );
    assert_eq!(Rc::strong_count(&view), 1);
    assert!(!view.has_node_geometry_for_test(ids[0]));
}

#[test]
fn dropping_runtime_revokes_live_sessions_and_retires_facts() {
    let view = {
        let (graph, ids) = graph_with_nodes(1);
        let graph_id = graph.graph_id();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("runtime");
        let view = runtime
            .attach_view(view_options(ViewSessionSeed::new(graph_id)))
            .expect("view");
        view.seed_all_node_facts_for_test();
        assert!(view.has_node_geometry_for_test(ids[0]));
        view
    };

    assert_eq!(view.close_reason(), Some(ViewCloseReason::RuntimeDropped));
    assert_eq!(view.derived_fact_counts_for_test().nodes, 0);
    assert!(matches!(
        view.graph_snapshot(),
        Err(ViewSessionError::Closed { .. })
    ));
}

#[test]
fn dropped_runtime_revokes_a_view_even_while_internal_shared_ownership_remains() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(Default::default())
        .expect("view session");
    let internal_owner = runtime.retain_shared_for_test();

    drop(runtime);

    assert!(matches!(
        view.graph_snapshot(),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::RuntimeDropped
        })
    ));
    assert_eq!(view.close_reason(), Some(ViewCloseReason::RuntimeDropped));
    drop(internal_owner);
}

#[test]
fn dropping_or_closing_a_session_revokes_weak_callbacks_and_unregisters_it() {
    let (graph, _) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let weak = view.downgrade();
    assert!(weak.is_open());
    assert_eq!(runtime.live_view_count(), 1);

    view.close().expect("close");
    assert!(!weak.is_open());
    assert!(matches!(
        weak.graph_snapshot(),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed
        })
    ));
    assert_eq!(runtime.live_view_count(), 0);

    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("replacement view");
    assert_eq!(runtime.live_view_count(), 1);
    drop(view);
    assert_eq!(runtime.live_view_count(), 0);
}

#[test]
fn weak_session_executes_commands_only_while_its_view_capability_is_open() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let weak = view.downgrade();

    let outcome = weak
        .execute(GraphCommand::set_node_data(
            0,
            ids[0],
            serde_json::json!({ "source": "weak view" }),
        ))
        .expect("open weak session executes commands");
    assert_eq!(outcome.applied().expect("command applies").revision(), 1);

    view.close().expect("close view");
    assert!(matches!(
        weak.execute(GraphCommand::set_node_data(
            1,
            ids[0],
            serde_json::json!({ "source": "closed weak view" }),
        )),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed,
        })
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
}

#[test]
fn close_drains_final_command_once_and_preserves_its_receipt() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let notifications = Rc::new(Cell::new(0));
    let callback_notifications = notifications.clone();
    view.subscribe(move |event| {
        if matches!(event, ViewSessionEvent::RuntimePatched { .. }) {
            callback_notifications.set(callback_notifications.get() + 1);
        }
    })
    .unwrap();
    let outcome = view
        .close_with_request(
            ViewCloseRequest::new().with_command(GraphCommand::set_node_data(
                0,
                ids[0],
                serde_json::json!({ "saved": true }),
            )),
        )
        .expect("close with final log");

    assert_eq!(outcome.receipts().len(), 1);
    assert!(outcome.receipts()[0].patch().is_some());
    assert_eq!(
        outcome.receipts()[0]
            .command_outcome()
            .and_then(|outcome| outcome.applied())
            .expect("final command applies")
            .revision(),
        1
    );
    assert!(outcome.receipts()[0].command_error().is_none());
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&ids[0])
            .expect("node remains")
            .data,
        serde_json::json!({ "saved": true })
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(notifications.get(), 0);
    assert_eq!(view.close_reason(), Some(ViewCloseReason::HostClosed));
    assert!(matches!(
        view.close_with_request(ViewCloseRequest::new()),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed
        })
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
}

#[test]
fn close_executes_final_commands_in_order_and_preserves_each_receipt() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");

    let outcome = view
        .close_with_request(
            ViewCloseRequest::new()
                .with_command(GraphCommand::set_node_data(
                    0,
                    ids[0],
                    serde_json::json!({ "step": 1 }),
                ))
                .with_command(GraphCommand::set_node_data(
                    1,
                    ids[0],
                    serde_json::json!({ "step": 2 }),
                )),
        )
        .expect("close with ordered final commands");

    assert_eq!(outcome.receipts().len(), 2);
    for (receipt, expected_revision) in outcome.receipts().iter().zip([1, 2]) {
        assert_eq!(
            receipt
                .command_outcome()
                .and_then(|outcome| outcome.applied())
                .expect("final command applies")
                .revision(),
            expected_revision
        );
    }
    let snapshot = runtime.snapshot().expect("final runtime snapshot");
    assert_eq!(snapshot.revision(), 2);
    assert_eq!(
        snapshot.graph().node(&ids[0]).expect("node remains").data,
        serde_json::json!({ "step": 2 })
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
}

#[test]
fn close_records_conflict_and_continues_final_command_log() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");

    let outcome = view
        .close_with_request(
            ViewCloseRequest::new()
                .with_command(GraphCommand::set_node_data(
                    1,
                    ids[0],
                    serde_json::json!({ "stale": true }),
                ))
                .with_command(GraphCommand::set_node_data(
                    0,
                    ids[0],
                    serde_json::json!({ "saved": true }),
                )),
        )
        .expect("close with a conflicting final command");

    assert_eq!(outcome.receipts().len(), 2);
    assert!(
        outcome.receipts()[0]
            .command_outcome()
            .expect("conflict is a command outcome")
            .conflict()
            .is_some()
    );
    assert!(outcome.receipts()[0].command_error().is_none());
    assert_eq!(
        outcome.receipts()[1]
            .command_outcome()
            .and_then(|outcome| outcome.applied())
            .expect("later valid command applies")
            .revision(),
        1
    );
    assert_eq!(
        runtime
            .snapshot()
            .expect("final runtime snapshot")
            .graph()
            .node(&ids[0])
            .expect("node remains")
            .data,
        serde_json::json!({ "saved": true })
    );
    assert_eq!(view.close_reason(), Some(ViewCloseReason::HostClosed));
}

#[test]
fn reentrant_close_with_command_is_rejected_and_leaves_view_open() {
    let (graph, ids) = graph_with_nodes(1);
    let node = ids[0];
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = Rc::new(
        runtime
            .attach_view(view_options(ViewSessionSeed::new(graph_id)))
            .expect("view"),
    );
    let close_was_rejected = Rc::new(Cell::new(false));
    let callback_view = view.clone();
    let callback_close_was_rejected = close_was_rejected.clone();
    runtime
        .subscribe(move |_| {
            callback_close_was_rejected.set(matches!(
                callback_view.close_with_request(ViewCloseRequest::new().with_command(
                    GraphCommand::set_node_data(1, node, serde_json::json!({ "value": 2 }),),
                )),
                Err(ViewSessionError::Runtime(
                    GraphRuntimeError::ReentrantMutation
                ))
            ));
        })
        .expect("subscription");

    runtime
        .execute(GraphCommand::set_node_data(
            0,
            node,
            serde_json::json!({ "value": 1 }),
        ))
        .expect("commit");

    assert!(close_was_rejected.get());
    assert!(view.generation().is_ok());
    assert_eq!(runtime.live_view_count(), 1);
}

#[test]
fn close_reentered_from_patch_callback_preserves_the_terminal_event() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = Rc::new(
        runtime
            .attach_view(view_options(ViewSessionSeed::new(graph_id)))
            .expect("view"),
    );
    let events = Rc::new(RefCell::new(Vec::new()));
    let callback_events = events.clone();
    let callback_view = view.clone();
    view.subscribe(move |event| match event {
        ViewSessionEvent::RuntimePatched { .. } => {
            callback_events.borrow_mut().push("runtime-patched");
            callback_view.close().expect("reentrant close");
        }
        ViewSessionEvent::Closed { reason } => {
            assert_eq!(reason, ViewCloseReason::HostClosed);
            callback_events.borrow_mut().push("closed");
        }
        ViewSessionEvent::LayoutPublished { .. }
        | ViewSessionEvent::ViewportChanged { .. }
        | ViewSessionEvent::SelectionChanged { .. } => {}
    })
    .expect("view subscription");

    runtime
        .execute(GraphCommand::set_node_data(
            0,
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
        .expect("commit");

    assert_eq!(&*events.borrow(), &["runtime-patched", "closed"]);
    assert_eq!(view.close_reason(), Some(ViewCloseReason::HostClosed));
    assert_eq!(runtime.live_view_count(), 0);
    assert_eq!(Rc::strong_count(&view), 1);
}

#[test]
fn node_incarnation_advances_across_hard_retirement_and_document_replacement() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph.clone())
        .expect("runtime");
    let initial = runtime.node_incarnation(ids[0]).unwrap().unwrap();

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(ids[0], IncidentEdgePolicy::Reject),
        ))
        .expect("remove command")
        .applied()
        .expect("remove applies");
    assert!(runtime.node_incarnation(ids[0]).unwrap().is_none());

    let revision = runtime.snapshot().unwrap().revision();
    runtime
        .execute(GraphCommand::undo(revision))
        .expect("undo command")
        .applied()
        .expect("undo applies");
    let restored = runtime.node_incarnation(ids[0]).unwrap().unwrap();
    assert!(restored > initial);

    let document = runtime.catalog().canonicalize_graph(&graph).unwrap();
    runtime
        .replace_document(document)
        .expect("replace document");
    let replaced = runtime.node_incarnation(ids[0]).unwrap().unwrap();
    assert!(replaced > restored);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
}
