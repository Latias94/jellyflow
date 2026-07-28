use std::collections::BTreeMap;
use std::rc::Rc;

use crate::command_journal::{
    OpenGpuiCommandJournal, OpenGpuiCommandOrigin, OpenGpuiCompletedCommand,
    resolve_command_tickets,
};
use crate::port_handle::OpenGpuiPendingMeasurements;

use super::super::frame_feedback::{
    OpenGpuiFrameTransaction, OpenGpuiFrameTransactionError, OpenGpuiFrameTransactionFault,
    OpenGpuiFrameTransactionSubmission,
};
use super::*;

#[test]
fn frame_transaction_schedule_and_submit_are_one_shot() {
    let (runtime, view, journal, mut ticket, transaction) = command_transaction("submit-once");

    assert!(transaction.schedule_once());
    assert!(!transaction.schedule_once());

    let mut first = transaction.submit(&view);
    assert!(matches!(
        first.submission,
        OpenGpuiFrameTransactionSubmission::Submitted(_)
    ));
    first
        .take_command_batch_result()
        .expect("first submission settles its batch");
    assert_command_ticket_pending_at(&mut ticket, "while its outcome is retained by the host");
    let completed = journal.take_deferred_completions();
    assert_eq!(completed.len(), 1);
    assert_eq!(runtime.snapshot().expect("submitted graph").revision(), 1);
    assert_eq!(journal.capacity_usage(), (0, 0, 0));

    let mut repeated = transaction.submit(&view);
    assert!(matches!(
        repeated.submission,
        OpenGpuiFrameTransactionSubmission::Failed(OpenGpuiFrameTransactionError::AlreadyConsumed)
    ));
    repeated
        .take_command_batch_result()
        .expect("repeated submission has no batch failure");
    assert!(journal.take_deferred_completions().is_empty());
    assert_eq!(runtime.snapshot().expect("unchanged graph").revision(), 1);
    assert_eq!(journal.capacity_usage(), (0, 0, 0));

    resolve_binding_commands(completed);
    assert_eq!(
        poll_command_ticket(&mut ticket)
            .expect("submitted command ticket")
            .applied_revision()
            .expect("submitted command applied"),
        1
    );
    assert_command_ticket_pending_at(&mut ticket, "after its only terminal result was consumed");
}

#[test]
fn aborting_frame_transaction_settles_collected_batch_once() {
    let (runtime, _view, journal, mut ticket, transaction) = command_transaction("abort-once");

    let completed = transaction
        .abort_frame_preparation("injected frame abort".to_owned())
        .expect("first abort settles the collected batch");
    assert_eq!(completed.len(), 1);
    assert!(
        transaction
            .abort_frame_preparation("repeated abort".to_owned())
            .expect("repeated abort is benign")
            .is_empty()
    );
    assert_eq!(runtime.snapshot().expect("unsubmitted graph").revision(), 0);
    assert_eq!(journal.capacity_usage(), (0, 0, 0));

    resolve_binding_commands(completed);
    assert!(matches!(
        poll_command_ticket(&mut ticket),
        Err(crate::authoring::OpenGpuiCommandTicketError::FramePreparation { .. })
    ));
    assert_command_ticket_pending_at(&mut ticket, "after abort settled it once");
}

#[test]
fn dropping_frame_transaction_resolves_collected_ticket_once() {
    let (runtime, _view, journal, mut ticket, transaction) = command_transaction("drop-once");

    drop(transaction);

    assert_eq!(runtime.snapshot().expect("unsubmitted graph").revision(), 0);
    assert_eq!(journal.capacity_usage(), (0, 0, 0));
    assert!(matches!(
        poll_command_ticket(&mut ticket),
        Err(crate::authoring::OpenGpuiCommandTicketError::HostDropped)
    ));
    assert_command_ticket_pending_at(&mut ticket, "after drop settled it once");
}

#[test]
fn dropping_frame_transaction_defers_internal_origin_for_host_settlement() {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid transaction graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("transaction view");
    let journal = OpenGpuiCommandJournal::new();
    let version = runtime
        .snapshot()
        .expect("internal transaction graph version")
        .version();
    journal
        .enqueue_detached(
            GraphCommand::set_node_data(
                version,
                NodeId::from_u128(10),
                serde_json::json!({ "transaction": "deferred-internal" }),
            ),
            OpenGpuiCommandOrigin::NodeSizes {
                tickets: Vec::new(),
            },
        )
        .expect("queued internal transaction command");
    let frame = view
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new(RENDERER, 1),
        ))
        .expect("transaction frame");
    let collector = frame.collector();
    let batch = journal
        .collect_next_batch(&collector)
        .expect("collect internal transaction batch");
    let transaction = OpenGpuiFrameTransaction::new(
        collector,
        Rc::new(OpenGpuiPendingMeasurements::new(
            frame.nodes(),
            frame.transform(),
            BTreeMap::new(),
            0,
        )),
        Rc::clone(&journal),
        batch,
    );

    drop(transaction);

    let completed = journal.take_deferred_completions();
    assert_eq!(completed.len(), 1);
    assert!(matches!(
        completed[0].origin,
        OpenGpuiCommandOrigin::NodeSizes { .. }
    ));
    assert!(matches!(
        completed[0].result,
        Err(crate::authoring::OpenGpuiCommandTicketError::HostDropped)
    ));
    assert!(journal.take_deferred_completions().is_empty());
    assert_eq!(runtime.snapshot().expect("unsubmitted graph").revision(), 0);
}

#[test]
fn unwind_after_frame_state_take_terminalizes_batch_and_allows_retry() {
    let (runtime, view, journal, mut ticket, transaction) = command_transaction("prepare-unwind");
    transaction.inject_fault(OpenGpuiFrameTransactionFault::AfterStateTake);

    let panic =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| transaction.submit(&view)));

    assert!(panic.is_err());
    assert_eq!(runtime.snapshot().expect("unsubmitted graph").revision(), 0);
    assert_eq!(journal.capacity_usage(), (0, 0, 0));
    assert!(!journal.has_pending());
    assert!(matches!(
        poll_command_ticket(&mut ticket),
        Err(crate::authoring::OpenGpuiCommandTicketError::FramePreparation { .. })
    ));
    assert_command_ticket_pending_at(&mut ticket, "after preparation unwind settled it once");

    let mut retry = submit_command_transaction(&view, &journal, "prepare-unwind-retry");
    assert_eq!(
        poll_command_ticket(&mut retry)
            .expect("retry command ticket")
            .applied_revision()
            .expect("retry command applied"),
        1
    );
    assert_eq!(journal.capacity_usage(), (0, 0, 0));
}

#[test]
fn unwind_after_feedback_submission_terminalizes_batch_and_allows_retry() {
    let (runtime, view, journal, mut ticket, transaction) =
        command_transaction("post-submit-unwind");
    transaction.inject_fault(OpenGpuiFrameTransactionFault::AfterFeedbackSubmission);

    let panic =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| transaction.submit(&view)));

    assert!(panic.is_err());
    assert_eq!(runtime.snapshot().expect("submitted graph").revision(), 1);
    assert_eq!(journal.capacity_usage(), (0, 0, 0));
    assert!(!journal.has_pending());
    assert!(matches!(
        poll_command_ticket(&mut ticket),
        Err(crate::authoring::OpenGpuiCommandTicketError::FeedbackOutcomeUnknown)
    ));
    assert_command_ticket_pending_at(&mut ticket, "after submitted unwind settled it once");

    let mut retry = submit_command_transaction(&view, &journal, "post-submit-unwind-retry");
    assert_eq!(
        poll_command_ticket(&mut retry)
            .expect("retry command ticket")
            .applied_revision()
            .expect("retry command applied"),
        2
    );
    assert_eq!(journal.capacity_usage(), (0, 0, 0));
}

#[open_gpui::test]
fn superseding_retained_frame_transaction_settles_batch_once(cx: &mut open_gpui::TestAppContext) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let (mut ticket, transaction) = cx.update_entity(&host, |host, cx| {
        let pending = install_collected_transaction(host, "superseded");
        cx.notify();
        pending
    });

    draw(cx);

    assert!(matches!(
        poll_command_ticket(&mut ticket),
        Err(crate::authoring::OpenGpuiCommandTicketError::FramePreparation { .. })
    ));
    assert_command_ticket_pending_at(&mut ticket, "after supersession settled it once");
    assert_eq!(runtime.snapshot().expect("unmodified graph").revision(), 0);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        let mut repeated = transaction.submit(&host.view);
        assert!(matches!(
            repeated.submission,
            OpenGpuiFrameTransactionSubmission::Failed(
                OpenGpuiFrameTransactionError::AlreadyConsumed
            )
        ));
        repeated
            .take_command_batch_result()
            .expect("superseded transaction has no remaining batch");
    });
}

#[open_gpui::test]
fn closing_retained_frame_transaction_executes_and_settles_batch_once(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let (mut ticket, transaction) = cx.update_window_entity(&host, |host, window, cx| {
        let pending = install_collected_transaction(host, "closed");
        host.close_in_window(window, cx)
            .expect("window close drains the retained transaction batch");
        pending
    });

    cx.run_until_parked();

    assert_eq!(
        poll_command_ticket(&mut ticket)
            .expect("close command ticket")
            .applied_revision()
            .expect("close command applied"),
        1
    );
    assert_command_ticket_pending_at(&mut ticket, "after close settled it once");
    let snapshot = runtime.snapshot().expect("closed runtime snapshot");
    assert_eq!(snapshot.revision(), 1);
    assert_eq!(
        snapshot
            .graph()
            .node(&NodeId::from_u128(10))
            .expect("closed command target")
            .data,
        serde_json::json!({ "transaction": "closed" })
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::HostClosed));
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        let mut repeated = transaction.submit(&host.view);
        assert!(matches!(
            repeated.submission,
            OpenGpuiFrameTransactionSubmission::Failed(
                OpenGpuiFrameTransactionError::AlreadyConsumed
            )
        ));
        repeated
            .take_command_batch_result()
            .expect("closed transaction has no remaining batch");
    });
}

#[open_gpui::test]
fn unwind_after_runtime_submission_settles_ticket_and_keeps_host_usable(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let mut submitted = cx.update_entity(&host, |host, cx| {
        let version = host
            .view
            .graph_snapshot()
            .expect("pre-unwind graph version")
            .version();
        let ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(10),
                    serde_json::json!({ "transaction": "post-submit-unwind" }),
                ),
                OpenGpuiCommandOrigin::Binding,
            )
            .expect("queue command before injected host unwind");
        host.frame_submission_fault = true;
        cx.notify();
        ticket
    });

    draw(cx);

    assert_eq!(
        poll_command_ticket(&mut submitted)
            .expect("submitted command ticket")
            .applied_revision()
            .expect("command committed before the injected unwind"),
        1
    );
    assert_command_ticket_pending_at(&mut submitted, "after unwind settled it once");
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        let diagnostic = host.last_diagnostic().expect("host unwind diagnostic");
        assert!(diagnostic.message().contains("host update panicked"));
    });

    let mut retry = cx.update_entity(&host, |host, cx| {
        let version = host
            .view
            .graph_snapshot()
            .expect("post-unwind graph version")
            .version();
        let ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(10),
                    serde_json::json!({ "transaction": "post-unwind-retry" }),
                ),
                OpenGpuiCommandOrigin::Binding,
            )
            .expect("host accepts a command after recovered unwind");
        cx.notify();
        ticket
    });
    draw(cx);
    assert_eq!(
        poll_command_ticket(&mut retry)
            .expect("retry command ticket")
            .applied_revision()
            .expect("retry command applied"),
        2
    );
    assert_eq!(
        runtime.snapshot().expect("runtime after retry").revision(),
        2
    );
}

fn command_transaction(
    label: &str,
) -> (
    GraphRuntime,
    ViewSession,
    Rc<OpenGpuiCommandJournal>,
    crate::authoring::OpenGpuiCommandTicket,
    OpenGpuiFrameTransaction,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid transaction graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("transaction view");
    let journal = OpenGpuiCommandJournal::new();
    let version = runtime
        .snapshot()
        .expect("transaction graph version")
        .version();
    let ticket = journal
        .enqueue(
            GraphCommand::set_node_data(
                version,
                NodeId::from_u128(10),
                serde_json::json!({ "transaction": label }),
            ),
            OpenGpuiCommandOrigin::Binding,
        )
        .expect("queued transaction command");
    let frame = view
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new(RENDERER, 1),
        ))
        .expect("transaction frame");
    let collector = frame.collector();
    let batch = journal
        .collect_next_batch(&collector)
        .expect("collect transaction batch");
    let measurements = Rc::new(OpenGpuiPendingMeasurements::new(
        frame.nodes(),
        frame.transform(),
        BTreeMap::new(),
        0,
    ));
    let transaction =
        OpenGpuiFrameTransaction::new(collector, measurements, Rc::clone(&journal), batch);
    (runtime, view, journal, ticket, transaction)
}

fn submit_command_transaction(
    view: &ViewSession,
    journal: &Rc<OpenGpuiCommandJournal>,
    label: &str,
) -> crate::authoring::OpenGpuiCommandTicket {
    let version = view
        .graph_snapshot()
        .expect("retry transaction graph version")
        .version();
    let ticket = journal
        .enqueue(
            GraphCommand::set_node_data(
                version,
                NodeId::from_u128(10),
                serde_json::json!({ "transaction": label }),
            ),
            OpenGpuiCommandOrigin::Binding,
        )
        .expect("queued retry transaction command");
    let frame = view
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new(RENDERER, 1),
        ))
        .expect("retry transaction frame");
    let collector = frame.collector();
    let batch = journal
        .collect_next_batch(&collector)
        .expect("collect retry transaction batch");
    let measurements = Rc::new(OpenGpuiPendingMeasurements::new(
        frame.nodes(),
        frame.transform(),
        BTreeMap::new(),
        0,
    ));
    let transaction =
        OpenGpuiFrameTransaction::new(collector, measurements, Rc::clone(journal), batch);
    let mut result = transaction.submit(view);
    assert!(matches!(
        result.submission,
        OpenGpuiFrameTransactionSubmission::Submitted(_)
    ));
    result
        .take_command_batch_result()
        .expect("retry transaction settles its batch");
    resolve_binding_commands(journal.take_deferred_completions());
    ticket
}

fn install_collected_transaction(
    host: &mut OpenGpuiSceneHost,
    label: &str,
) -> (
    crate::authoring::OpenGpuiCommandTicket,
    Rc<OpenGpuiFrameTransaction>,
) {
    assert!(host.frame_transaction.is_none());
    let version = host
        .view
        .graph_snapshot()
        .expect("retained transaction graph version")
        .version();
    let ticket = host
        .commands
        .enqueue(
            GraphCommand::set_node_data(
                version,
                NodeId::from_u128(10),
                serde_json::json!({ "transaction": label }),
            ),
            OpenGpuiCommandOrigin::Binding,
        )
        .expect("queued retained transaction command");
    let frame = host
        .view
        .frame(host.frame_request())
        .expect("retained transaction frame");
    let collector = frame.collector();
    let batch = host
        .commands
        .collect_next_batch(&collector)
        .expect("collect retained transaction batch");
    let measurements = Rc::new(OpenGpuiPendingMeasurements::new(
        frame.nodes(),
        frame.transform(),
        BTreeMap::new(),
        host.adapter_environment_epoch,
    ));
    let transaction = Rc::new(OpenGpuiFrameTransaction::new(
        collector,
        measurements,
        Rc::clone(&host.commands),
        batch,
    ));
    host.frame_transaction = Some(Rc::clone(&transaction));
    (ticket, transaction)
}

fn resolve_binding_commands(completed: Vec<OpenGpuiCompletedCommand>) {
    resolve_command_tickets(
        completed
            .into_iter()
            .filter_map(OpenGpuiCompletedCommand::into_resolution),
    );
}
