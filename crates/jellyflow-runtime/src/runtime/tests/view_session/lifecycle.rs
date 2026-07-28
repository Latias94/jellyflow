use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::io::ViewSessionSeed;
use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::{GraphRuntime, GraphRuntimeError};
use crate::runtime::view_session::{
    ViewCloseReason, ViewCloseRequest, ViewSessionError, ViewSessionEvent,
};
use crate::runtime::viewport::ViewportTransform;

use super::fixtures::{graph_with_nodes, publish_graph_layouts, view_options};

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
    assert!(outcome.command_completions().next().is_none());
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
        view.set_viewport(ViewportTransform::new(Default::default(), 1.0).unwrap()),
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
    publish_graph_layouts(&view);

    let terminal_reasons = Rc::new(RefCell::new(Vec::new()));
    let callback_reasons = terminal_reasons.clone();
    let callback_view = view.clone();
    view.subscribe(move |event| {
        if let ViewSessionEvent::Closed { reason } = event {
            assert_eq!(callback_view.close_reason(), Some(reason));
            assert!(callback_view.retired_geometry_is_empty_for_test());
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
    assert!(view.retired_geometry_is_empty_for_test());
}

#[test]
fn runtime_drop_publishes_one_terminal_event_after_retiring_view_facts() {
    let (graph, _) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = Rc::new(
        runtime
            .attach_view(view_options(ViewSessionSeed::new(graph_id)))
            .expect("view"),
    );
    publish_graph_layouts(&view);

    let terminal_reasons = Rc::new(RefCell::new(Vec::new()));
    let callback_reasons = terminal_reasons.clone();
    let callback_view = view.clone();
    view.subscribe(move |event| {
        if let ViewSessionEvent::Closed { reason } = event {
            assert_eq!(callback_view.close_reason(), Some(reason));
            assert!(callback_view.retired_geometry_is_empty_for_test());
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
    assert!(view.retired_geometry_is_empty_for_test());
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
        publish_graph_layouts(&view);
        assert!(view.geometry_snapshot().unwrap().node(ids[0]).is_some());
        view
    };

    assert_eq!(view.close_reason(), Some(ViewCloseReason::RuntimeDropped));
    assert!(view.retired_geometry_is_empty_for_test());
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
    let initial_version = weak.graph_snapshot().expect("initial snapshot").version();

    let outcome = weak
        .execute(GraphCommand::set_node_data(
            initial_version,
            ids[0],
            serde_json::json!({ "source": "weak view" }),
        ))
        .expect("open weak session executes commands")
        .into_result()
        .expect("open weak session command execution");
    let committed_version = outcome.applied().expect("command applies").version();
    assert_eq!(committed_version.revision(), 1);

    view.close().expect("close view");
    assert!(matches!(
        weak.execute(GraphCommand::set_node_data(
            committed_version,
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
    let initial_version = view.graph_snapshot().expect("initial snapshot").version();
    let command = GraphCommand::set_node_data(
        initial_version,
        ids[0],
        serde_json::json!({ "saved": true }),
    );
    let command_identity = command.identity().clone();
    let outcome = view
        .close_with_request(ViewCloseRequest::new().with_command(command))
        .expect("close with final log");

    assert_eq!(outcome.command_completions().count(), 1);
    let completion = outcome
        .command_completion(&command_identity)
        .expect("final command completion");
    assert_eq!(
        completion
            .result()
            .expect("final command executes")
            .applied()
            .expect("final command applies")
            .revision(),
        1
    );
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
fn close_rejects_duplicate_command_identity_before_entering_closing_state() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let installed_impact_before = runtime.installed_impact_state_for_test();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();
    let command = GraphCommand::set_node_data(
        before.version(),
        ids[0],
        serde_json::json!({ "saved": true }),
    );
    let identity = command.identity().clone();

    let error = view
        .close_with_request(
            ViewCloseRequest::new()
                .with_command(command.clone())
                .with_command(command.clone()),
        )
        .expect_err("duplicate identity must reject the whole close request");
    assert!(matches!(
        error,
        ViewSessionError::DuplicateCommandIdentity {
            identity: actual,
        } if actual == identity
    ));
    assert_eq!(view.close_reason(), None);
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(
        runtime.installed_impact_state_for_test(),
        installed_impact_before,
        "invalid close requests must not update indexes or node identities"
    );
    assert_eq!(event_count.get(), 0);

    let outcome = view
        .close_with_request(ViewCloseRequest::new().with_command(command))
        .expect("the unconsumed command remains valid for a unique close request");
    assert_eq!(outcome.command_completions().count(), 1);
    assert!(outcome.command_completion(&identity).is_ok());
}

#[test]
fn close_applies_at_most_one_command_planned_from_the_same_current_version() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    let current_version = view.graph_snapshot().expect("initial snapshot").version();
    let first_command =
        GraphCommand::set_node_data(current_version, ids[0], serde_json::json!({ "step": 1 }));
    let first_identity = first_command.identity().clone();
    let second_command =
        GraphCommand::set_node_data(current_version, ids[0], serde_json::json!({ "step": 2 }));
    let second_identity = second_command.identity().clone();

    let outcome = view
        .close_with_request(
            ViewCloseRequest::new()
                .with_command(first_command)
                .with_command(second_command),
        )
        .expect("close with ordered final commands");

    assert_eq!(outcome.command_completions().count(), 2);
    assert_eq!(
        outcome
            .command_completion(&first_identity)
            .expect("first command completion")
            .result()
            .expect("first command executes")
            .applied()
            .expect("first current command applies")
            .revision(),
        1
    );
    assert!(
        outcome
            .command_completion(&second_identity)
            .expect("second command completion")
            .result()
            .expect("second command has an outcome")
            .conflict()
            .is_some()
    );
    let snapshot = runtime.snapshot().expect("final runtime snapshot");
    assert_eq!(snapshot.revision(), 1);
    assert_eq!(
        snapshot.graph().node(&ids[0]).expect("node remains").data,
        serde_json::json!({ "step": 1 })
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
}

#[test]
fn close_records_conflict_and_continues_final_command_log() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    let stale_version = view.graph_snapshot().expect("initial snapshot").version();
    let current_version = runtime
        .execute(GraphCommand::set_node_data(
            stale_version,
            ids[0],
            serde_json::json!({ "seed": true }),
        ))
        .into_result()
        .expect("seed command")
        .applied()
        .expect("seed applies")
        .version();
    let stale_command =
        GraphCommand::set_node_data(stale_version, ids[0], serde_json::json!({ "stale": true }));
    let stale_identity = stale_command.identity().clone();
    let current_command = GraphCommand::set_node_data(
        current_version,
        ids[0],
        serde_json::json!({ "saved": true }),
    );
    let current_identity = current_command.identity().clone();

    let outcome = view
        .close_with_request(
            ViewCloseRequest::new()
                .with_command(stale_command)
                .with_command(current_command),
        )
        .expect("close with a conflicting final command");

    assert_eq!(outcome.command_completions().count(), 2);
    assert!(
        outcome
            .command_completion(&stale_identity)
            .expect("stale command completion")
            .result()
            .expect("conflict is a command outcome")
            .conflict()
            .is_some()
    );
    assert_eq!(
        outcome
            .command_completion(&current_identity)
            .expect("current command completion")
            .result()
            .expect("current command executes")
            .applied()
            .expect("later valid command applies")
            .revision(),
        2
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
fn close_records_a_foreign_command_failure_without_mutating_the_graph() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    let (foreign_graph, _) = graph_with_nodes(1);
    let foreign_runtime = GraphRuntime::builder(Default::default())
        .build(foreign_graph)
        .expect("foreign runtime");
    let foreign_version = foreign_runtime
        .snapshot()
        .expect("foreign snapshot")
        .version();
    let command = GraphCommand::set_node_data(
        foreign_version,
        ids[0],
        serde_json::json!({ "must_not_apply": true }),
    );
    let command_identity = command.identity().clone();

    let outcome = view
        .close_with_request(ViewCloseRequest::new().with_command(command))
        .expect("foreign command is recorded while closing");

    assert!(matches!(
        outcome
            .command_completion(&command_identity)
            .expect("foreign command completion")
            .result(),
        Err(GraphRuntimeError::ForeignCommandRuntime { .. })
    ));
    let snapshot = runtime.snapshot().expect("unchanged runtime snapshot");
    assert_eq!(snapshot.revision(), 0);
    assert_eq!(
        snapshot.graph().node(&ids[0]).expect("node remains").data,
        serde_json::Value::Null
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
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
    let initial_version = runtime.snapshot().expect("initial snapshot").version();
    runtime
        .subscribe(move |_| {
            callback_close_was_rejected.set(matches!(
                callback_view.close_with_request(ViewCloseRequest::new().with_command(
                    GraphCommand::set_node_data(
                        initial_version,
                        node,
                        serde_json::json!({ "value": 2 }),
                    ),
                )),
                Err(ViewSessionError::Runtime(
                    GraphRuntimeError::ReentrantMutation
                ))
            ));
        })
        .expect("subscription");

    runtime
        .execute(GraphCommand::set_node_data(
            initial_version,
            node,
            serde_json::json!({ "value": 1 }),
        ))
        .into_result()
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
    let initial_version = runtime.snapshot().expect("initial snapshot").version();

    runtime
        .execute(GraphCommand::set_node_data(
            initial_version,
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
        .into_result()
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
    let initial = runtime.node_lifetime_key(ids[0]).unwrap().unwrap();
    let initial_version = runtime.snapshot().expect("initial snapshot").version();

    runtime
        .execute(GraphCommand::topology(
            initial_version,
            TopologyEdit::new().remove_node(ids[0], IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .expect("remove command")
        .applied()
        .expect("remove applies");
    assert!(runtime.node_lifetime_key(ids[0]).unwrap().is_none());

    let removed_version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::undo(removed_version))
        .into_result()
        .unwrap()
        .applied()
        .expect("undo applies");
    let restored = runtime.node_lifetime_key(ids[0]).unwrap().unwrap();
    assert_ne!(restored, initial);

    let document = runtime.catalog().canonicalize_graph(&graph).unwrap();
    runtime
        .replace_document(document)
        .expect("replace document");
    let replaced = runtime.node_lifetime_key(ids[0]).unwrap().unwrap();
    assert_ne!(replaced, restored);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
}
