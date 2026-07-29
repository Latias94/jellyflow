use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::activity::{OpenGpuiActivityCoordinator, OpenGpuiNodeActivityRegistry};
use crate::authoring::{
    MutationDiagnosticSource, MutationDispatchError, MutationLayoutEffect,
    MutationLayoutSettlement, MutationRequest, MutationRetryReason, MutationSettlement,
    OpenGpuiManagedMutation,
};
use crate::command_journal::{
    OpenGpuiCommandJournal, OpenGpuiCommandOrigin, OpenGpuiCompletedCommand,
    resolve_command_tickets,
};
use crate::managed_mutation::ManagedMutationPostAdmissionFault;
use crate::port_handle::OpenGpuiPendingMeasurements;

use super::super::frame_feedback::{
    OpenGpuiFrameTransaction, OpenGpuiFrameTransactionError, OpenGpuiFrameTransactionFault,
    OpenGpuiFrameTransactionResult, OpenGpuiFrameTransactionSubmission,
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
        Err(crate::command_journal::OpenGpuiCommandTicketError::FramePreparation { .. })
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
        Err(crate::command_journal::OpenGpuiCommandTicketError::HostDropped)
    ));
    assert_command_ticket_pending_at(&mut ticket, "after drop settled it once");
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
        Err(crate::command_journal::OpenGpuiCommandTicketError::FramePreparation { .. })
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
        Err(crate::command_journal::OpenGpuiCommandTicketError::FeedbackOutcomeUnknown)
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
        Err(crate::command_journal::OpenGpuiCommandTicketError::FramePreparation { .. })
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

#[open_gpui::test]
fn managed_mutation_maps_pre_submission_unwind_to_explicit_retry(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let binding = mounted_binding(cx, &host, NodeId::from_u128(10));
    let mut mutation = cx.read_entity(&binding, |binding, _| {
        binding.managed_mutation::<serde_json::Value>()
    });
    let intent = serde_json::json!({ "managed": "retry-after-frame-preparation" });

    let panic = dispatch_and_submit_managed_data_mutation(
        cx,
        &host,
        &binding,
        &mut mutation,
        MutationRequest::new(intent.clone()),
        intent.clone(),
        MutationLayoutEffect::Preserve,
        Some(OpenGpuiFrameTransactionFault::AfterStateTake),
    );
    assert!(panic.is_err());
    assert_eq!(runtime.snapshot().expect("unsubmitted graph").revision(), 0);
    cx.run_until_parked();

    let settlement = observe_managed_data_mutation(cx, &binding, &mut mutation);
    match settlement {
        MutationSettlement::Retryable { reason, diagnostic } => {
            assert_eq!(reason, MutationRetryReason::NativeDispatchFailed);
            assert_eq!(
                diagnostic.source(),
                MutationDiagnosticSource::NativeDispatch
            );
        }
        settlement => panic!("unexpected frame-preparation settlement: {settlement:?}"),
    }
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        assert_managed_activity(host, NodeId::from_u128(10), 1, 1);
    });

    let retry = mutation.retry().expect("definitive failure permits retry");
    let mut result = dispatch_and_submit_managed_data_mutation(
        cx,
        &host,
        &binding,
        &mut mutation,
        MutationRequest::retry(retry),
        intent,
        MutationLayoutEffect::Preserve,
        None,
    )
    .expect("retry transaction does not unwind");
    assert!(matches!(
        result.submission,
        OpenGpuiFrameTransactionSubmission::Submitted(_)
    ));
    result
        .take_command_batch_result()
        .expect("retry transaction settles its command batch");
    let completed = cx.read_entity(&host, |host, _| host.commands.take_deferred_completions());
    resolve_binding_commands(completed);
    let advanced = cx.update_entity(&binding, |binding, cx| {
        binding.advance(1, cx).expect("advance retry binding")
    });
    assert_eq!(advanced, crate::binding::OpenGpuiBindingAdvance::Applied);
    cx.run_until_parked();

    assert_eq!(
        observe_managed_data_mutation(cx, &binding, &mut mutation),
        MutationSettlement::Satisfied {
            layout: MutationLayoutSettlement::NotRequested,
        }
    );
    assert_eq!(runtime.snapshot().expect("retried graph").revision(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        assert_managed_activity(host, NodeId::from_u128(10), 0, 0);
    });
}

#[open_gpui::test]
fn managed_mutation_maps_task_installation_unwind_to_dispatch_uncertainty(
    cx: &mut open_gpui::TestAppContext,
) {
    assert_managed_post_admission_unwind(
        cx,
        ManagedMutationPostAdmissionFault::TaskInstallationUnwound,
        "task-installation-unwind",
    );
}

#[open_gpui::test]
fn managed_mutation_maps_host_notification_unwind_to_dispatch_uncertainty(
    cx: &mut open_gpui::TestAppContext,
) {
    assert_managed_post_admission_unwind(
        cx,
        ManagedMutationPostAdmissionFault::HostNotificationUnwound,
        "host-notification-unwind",
    );
}

#[open_gpui::test]
fn managed_mount_set_retires_when_its_primary_member_retires(cx: &mut open_gpui::TestAppContext) {
    assert_managed_mount_set_terminal(cx, ManagedMountSetTerminalScenario::Retire(0));
}

#[open_gpui::test]
fn managed_mount_set_retires_when_a_non_primary_member_retires(cx: &mut open_gpui::TestAppContext) {
    assert_managed_mount_set_terminal(cx, ManagedMountSetTerminalScenario::Retire(1));
}

#[open_gpui::test]
fn managed_mount_set_close_wins_after_another_member_retires(cx: &mut open_gpui::TestAppContext) {
    assert_managed_mount_set_terminal(cx, ManagedMountSetTerminalScenario::RetireThenClose);
}

#[open_gpui::test]
fn managed_mount_set_close_wins_before_another_member_retires(cx: &mut open_gpui::TestAppContext) {
    assert_managed_mount_set_terminal(cx, ManagedMountSetTerminalScenario::CloseThenRetire);
}

#[open_gpui::test]
fn managed_mutation_satisfies_late_unknown_outcome_without_reinvalidating_layout(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let node = NodeId::from_u128(10);
    let binding = mounted_binding(cx, &host, node);
    let mut mutation = cx.read_entity(&binding, |binding, _| {
        binding.managed_mutation::<serde_json::Value>()
    });
    let intent = serde_json::json!({ "managed": "authoritative-before-ticket" });

    let panic = dispatch_and_submit_managed_data_mutation(
        cx,
        &host,
        &binding,
        &mut mutation,
        MutationRequest::new(intent.clone()),
        intent,
        MutationLayoutEffect::InvalidateWhenSatisfied,
        Some(OpenGpuiFrameTransactionFault::AfterFeedbackSubmission),
    );
    assert!(panic.is_err());
    assert_eq!(runtime.snapshot().expect("submitted graph").revision(), 1);

    let advanced = cx.update_entity(&binding, |binding, cx| {
        binding
            .advance(1, cx)
            .expect("advance binding before ticket completion")
    });
    assert_eq!(advanced, crate::binding::OpenGpuiBindingAdvance::Applied);
    assert_eq!(
        observe_managed_data_mutation(cx, &binding, &mut mutation),
        MutationSettlement::Pending {
            awaiting_authority: false,
        }
    );
    let generation_before = node_layout_generation(cx, &host, node);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        assert_managed_activity(host, node, 2, 1);
    });

    cx.run_until_parked();

    assert_eq!(
        observe_managed_data_mutation(cx, &binding, &mut mutation),
        MutationSettlement::Satisfied {
            layout: MutationLayoutSettlement::Invalidated,
        }
    );
    let generation_after = node_layout_generation(cx, &host, node);
    assert_eq!(generation_after, generation_before + 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        assert_managed_activity(host, node, 0, 0);
    });

    assert_eq!(
        observe_managed_data_mutation(cx, &binding, &mut mutation),
        MutationSettlement::Ready
    );
    assert_eq!(node_layout_generation(cx, &host, node), generation_after);
}

#[open_gpui::test]
fn managed_mutation_drains_unavailable_completion_before_explicit_abandonment(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let node = NodeId::from_u128(10);
    let binding = mounted_binding(cx, &host, node);
    let mut mutation = cx.read_entity(&binding, |binding, _| {
        binding.managed_mutation::<serde_json::Value>()
    });
    let intent = serde_json::json!({ "managed": "unavailable-observation" });

    let mut result = dispatch_and_submit_managed_data_mutation(
        cx,
        &host,
        &binding,
        &mut mutation,
        MutationRequest::new(intent.clone()),
        intent,
        MutationLayoutEffect::InvalidateWhenSatisfied,
        None,
    )
    .expect("unavailable-observation command submission does not unwind");
    assert_eq!(runtime.snapshot().expect("submitted graph").revision(), 1);
    assert!(mutation.unobservable_abandonment().is_none());
    assert!(!cx.update_entity(&binding, |binding, cx| {
        let authoring = binding.authoring().expect("pending unavailable authoring");
        assert_eq!(
            mutation.observe_unavailable(&authoring, cx),
            MutationSettlement::Pending {
                awaiting_authority: false,
            }
        );
        mutation.unobservable_abandonment().is_some()
    }));

    result
        .take_command_batch_result()
        .expect("unavailable-observation command batch settles");
    let completed = cx.read_entity(&host, |host, _| host.commands.take_deferred_completions());
    resolve_binding_commands(completed);
    cx.run_until_parked();
    let generation_before_abandon = node_layout_generation(cx, &host, node);

    let settlement = cx.update_entity(&binding, |binding, cx| {
        let authoring = binding
            .authoring()
            .expect("completed unavailable authoring");
        mutation.observe_unavailable(&authoring, cx)
    });
    assert_eq!(
        settlement,
        MutationSettlement::Pending {
            awaiting_authority: true,
        }
    );
    let abandonment = mutation
        .unobservable_abandonment()
        .expect("definitive completion exposes one exact abandonment");
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        assert_managed_activity(host, node, 1, 1);
    });

    assert!(cx.update_entity(&binding, |_, _| mutation.abandon_unobservable(abandonment)));
    assert_eq!(mutation.settlement(), MutationSettlement::Ready);
    assert_eq!(
        node_layout_generation(cx, &host, node),
        generation_before_abandon,
        "discarding unavailable product facts consumes the pending layout effect"
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        assert_managed_activity(host, node, 0, 0);
    });
}

#[open_gpui::test]
fn managed_mutation_preserves_intent_when_activity_admission_rejects(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let node = NodeId::from_u128(10);
    let binding = mounted_binding(cx, &host, node);
    let mut mutation = cx.read_entity(&binding, |binding, _| {
        binding.managed_mutation::<serde_json::Value>()
    });
    let instance = cx.read_entity(&binding, |binding, _| {
        binding
            .authoring()
            .expect("activity-failure authoring")
            .instance()
            .clone()
    });
    let blocked_activity =
        OpenGpuiNodeActivityRegistry::for_host(instance, OpenGpuiActivityCoordinator::new(0));
    let live_activity = mutation.replace_activity_for_test(blocked_activity.clone());
    let intent = serde_json::json!({ "managed": "activity-capacity-retry" });

    let dispatch = cx.update_entity(&binding, |binding, cx| {
        let authoring = binding
            .authoring()
            .expect("live activity-failure authoring");
        let command =
            GraphCommand::set_node_data(authoring.version(), authoring.node_id(), intent.clone());
        mutation.dispatch(
            authoring,
            command,
            MutationRequest::new(intent.clone()),
            MutationLayoutEffect::Preserve,
            cx,
        )
    });
    match dispatch {
        Err(MutationDispatchError::NotDispatched { diagnostic }) => {
            assert_eq!(
                diagnostic.source(),
                MutationDiagnosticSource::NativeActivity
            );
        }
        result => panic!("unexpected activity admission result: {result:?}"),
    }
    assert_eq!(mutation.active_intent(), Some(&intent));
    match mutation.settlement() {
        MutationSettlement::Retryable { reason, diagnostic } => {
            assert_eq!(reason, MutationRetryReason::NotDispatched);
            assert_eq!(
                diagnostic.source(),
                MutationDiagnosticSource::NativeActivity
            );
        }
        settlement => panic!("unexpected activity-failure settlement: {settlement:?}"),
    }
    assert!(
        mutation.retry().is_none(),
        "retry waits for authoritative unsatisfied evidence"
    );
    assert_eq!(blocked_activity.active_count(), 0);
    assert_eq!(blocked_activity.protected_count(), 0);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
    });

    let settlement = cx.update_entity(&binding, |binding, cx| {
        let authoring = binding
            .authoring()
            .expect("authoritative retry observation");
        mutation.observe(&authoring, |_| false, cx)
    });
    assert!(matches!(
        settlement,
        MutationSettlement::Retryable {
            reason: MutationRetryReason::NotDispatched,
            ..
        }
    ));
    let retry = mutation
        .retry()
        .expect("authoritative mismatch enables explicit retry");
    let replaced = mutation.replace_activity_for_test(live_activity);
    drop(replaced);

    let mut result = dispatch_and_submit_managed_data_mutation(
        cx,
        &host,
        &binding,
        &mut mutation,
        MutationRequest::retry(retry),
        intent,
        MutationLayoutEffect::Preserve,
        None,
    )
    .expect("activity-capacity retry does not unwind");
    assert!(matches!(
        result.submission,
        OpenGpuiFrameTransactionSubmission::Submitted(_)
    ));
    result
        .take_command_batch_result()
        .expect("activity-capacity retry batch settles");
    let completed = cx.read_entity(&host, |host, _| host.commands.take_deferred_completions());
    resolve_binding_commands(completed);
    let advanced = cx.update_entity(&binding, |binding, cx| {
        binding
            .advance(1, cx)
            .expect("advance activity-capacity retry binding")
    });
    assert_eq!(advanced, crate::binding::OpenGpuiBindingAdvance::Applied);
    cx.run_until_parked();
    assert_eq!(
        observe_managed_data_mutation(cx, &binding, &mut mutation),
        MutationSettlement::Satisfied {
            layout: MutationLayoutSettlement::NotRequested,
        }
    );
    assert_eq!(
        runtime.snapshot().expect("activity retry graph").revision(),
        1
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        assert_managed_activity(host, node, 0, 0);
    });
}

#[open_gpui::test]
fn managed_mutation_persists_panicking_layout_diagnostic_without_replaying(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let node = NodeId::from_u128(10);
    let binding = mounted_binding(cx, &host, node);
    let mut mutation = cx.read_entity(&binding, |binding, _| {
        binding.managed_mutation::<serde_json::Value>()
    });
    let intent = serde_json::json!({ "managed": "layout-panic" });

    let mut result = dispatch_and_submit_managed_data_mutation(
        cx,
        &host,
        &binding,
        &mut mutation,
        MutationRequest::new(intent.clone()),
        intent,
        MutationLayoutEffect::InvalidateWhenSatisfied,
        None,
    )
    .expect("layout-panic command submission does not unwind");
    assert!(matches!(
        result.submission,
        OpenGpuiFrameTransactionSubmission::Submitted(_)
    ));
    result
        .take_command_batch_result()
        .expect("layout-panic command batch settles");
    let completed = cx.read_entity(&host, |host, _| host.commands.take_deferred_completions());
    resolve_binding_commands(completed);
    let advanced = cx.update_entity(&binding, |binding, cx| {
        binding
            .advance(1, cx)
            .expect("advance layout-panic binding")
    });
    assert_eq!(advanced, crate::binding::OpenGpuiBindingAdvance::Applied);
    cx.run_until_parked();
    assert_eq!(
        runtime.snapshot().expect("layout-panic graph").revision(),
        1
    );

    let calls = Rc::new(Cell::new(0_u32));
    let invalidator_calls = Rc::clone(&calls);
    mutation.replace_layout_invalidator_for_test(Rc::new(move |_, _, _| {
        invalidator_calls.set(invalidator_calls.get().saturating_add(1));
        panic!("injected layout invalidation panic")
    }));

    let settlement = observe_managed_data_mutation(cx, &binding, &mut mutation);
    match settlement {
        MutationSettlement::Satisfied {
            layout: MutationLayoutSettlement::Failed { diagnostic },
        } => assert_eq!(diagnostic.source(), MutationDiagnosticSource::NativeLayout),
        settlement => panic!("unexpected layout-panic settlement: {settlement:?}"),
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(
        observe_managed_data_mutation(cx, &binding, &mut mutation),
        MutationSettlement::Ready
    );
    assert_eq!(
        mutation
            .layout_failure()
            .expect("layout failure remains observable")
            .source(),
        MutationDiagnosticSource::NativeLayout
    );
    assert_eq!(calls.get(), 1, "layout invalidation is never replayed");
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        assert_managed_activity(host, node, 0, 0);
    });
}

fn command_transaction(
    label: &str,
) -> (
    GraphRuntime,
    ViewSession,
    Rc<OpenGpuiCommandJournal>,
    crate::command_journal::OpenGpuiCommandTicket,
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
) -> crate::command_journal::OpenGpuiCommandTicket {
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
    crate::command_journal::OpenGpuiCommandTicket,
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
        host.content_sizes.environment_epoch(),
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

fn mounted_binding(
    cx: &mut open_gpui::VisualTestContext,
    host: &open_gpui::Entity<OpenGpuiSceneHost>,
    node: NodeId,
) -> open_gpui::Entity<OpenGpuiNodeBinding> {
    cx.read_entity(host, |host, _| {
        host.retained_nodes
            .active_in_order()
            .find(|(instance, _)| instance.node() == node)
            .and_then(|(_, mounted)| mounted.binding.clone())
            .expect("mounted node binding")
    })
}

fn dispatch_and_submit_managed_data_mutation(
    cx: &mut open_gpui::VisualTestContext,
    host: &open_gpui::Entity<OpenGpuiSceneHost>,
    binding: &open_gpui::Entity<OpenGpuiNodeBinding>,
    mutation: &mut OpenGpuiManagedMutation<serde_json::Value>,
    request: MutationRequest<serde_json::Value>,
    value: serde_json::Value,
    layout_effect: MutationLayoutEffect,
    fault: Option<OpenGpuiFrameTransactionFault>,
) -> Result<OpenGpuiFrameTransactionResult, Box<dyn std::any::Any + Send + 'static>> {
    let (authoring, command) = cx.read_entity(binding, |binding, _| {
        let authoring = binding.authoring().expect("live managed authoring");
        let command = GraphCommand::set_node_data(authoring.version(), authoring.node_id(), value);
        (authoring, command)
    });
    let node = authoring.node_id();
    cx.update_entity(host, |host, cx| {
        let settlement = mutation
            .dispatch(authoring, command, request, layout_effect, cx)
            .expect("managed mutation dispatch");
        assert_eq!(
            settlement,
            MutationSettlement::Pending {
                awaiting_authority: false,
            }
        );
        assert_eq!(host.commands.capacity_usage(), (1, 0, 0));
        assert_managed_activity(host, node, 2, 1);
        let transaction = collect_managed_transaction(host);
        assert_eq!(host.commands.capacity_usage(), (0, 1, 0));
        if let Some(fault) = fault {
            transaction.inject_fault(fault);
        }
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            transaction.submit(&host.view)
        }))
    })
}

fn observe_managed_data_mutation(
    cx: &mut open_gpui::VisualTestContext,
    binding: &open_gpui::Entity<OpenGpuiNodeBinding>,
    mutation: &mut OpenGpuiManagedMutation<serde_json::Value>,
) -> MutationSettlement {
    cx.update_entity(binding, |binding, cx| {
        let authoring = binding.authoring().expect("live managed observation");
        mutation.observe(&authoring, |intent| authoring.node().data == *intent, cx)
    })
}

#[derive(Clone, Copy)]
enum ManagedMountSetTerminalScenario {
    Retire(usize),
    RetireThenClose,
    CloseThenRetire,
}

fn assert_managed_mount_set_terminal(
    cx: &mut open_gpui::TestAppContext,
    scenario: ManagedMountSetTerminalScenario,
) {
    let (_runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let primary_node = NodeId::from_u128(10);
    let secondary_node = NodeId::from_u128(11);
    let primary_binding = mounted_binding(cx, &host, primary_node);
    let secondary_binding = mounted_binding(cx, &host, secondary_node);
    let owners = vec![
        cx.read_entity(&primary_binding, |binding, _| {
            binding.managed_mutation_owner()
        }),
        cx.read_entity(&secondary_binding, |binding, _| {
            binding.managed_mutation_owner()
        }),
    ];
    let primary_owner = owners[0].clone();
    let secondary_owner = owners[1].clone();
    let mut mutation = OpenGpuiManagedMutation::mount_set(owners)
        .expect("two exact mounts from one host form one mutation owner set");
    let intent = serde_json::json!({ "managed": "mount-set-retirement" });

    let settlement = cx.update_entity(&primary_binding, |binding, cx| {
        let authoring = binding.authoring().expect("live mount-set authoring");
        let command =
            GraphCommand::set_node_data(authoring.version(), authoring.node_id(), intent.clone());
        mutation
            .dispatch(
                authoring,
                command,
                MutationRequest::new(intent.clone()),
                MutationLayoutEffect::Preserve,
                cx,
            )
            .expect("mount-set mutation dispatch")
    });
    assert_eq!(
        settlement,
        MutationSettlement::Pending {
            awaiting_authority: false,
        }
    );
    cx.read_entity(&host, |host, _| {
        let activity = |node| {
            host.retained_nodes
                .active_in_order()
                .find(|(instance, _)| instance.node() == node)
                .map(|(_, mounted)| {
                    (
                        mounted.activity.active_count(),
                        mounted.activity.protected_count(),
                    )
                })
                .expect("mount-set member remains mounted")
        };
        assert_eq!(activity(primary_node), (2, 1));
        assert_eq!(activity(secondary_node), (1, 1));
        let resources = host.resource_snapshot();
        assert_eq!(resources.protected_instances(), 2);
        assert_eq!(resources.protected_leases(), 2);
    });

    match scenario {
        ManagedMountSetTerminalScenario::Retire(owner) => match owner {
            0 => primary_owner.retire_mount_for_test(),
            1 => secondary_owner.retire_mount_for_test(),
            _ => panic!("mount-set retirement owner is out of range"),
        },
        ManagedMountSetTerminalScenario::RetireThenClose => {
            primary_owner.retire_mount_for_test();
            secondary_owner.close_mount_for_test(ViewCloseReason::HostClosed);
        }
        ManagedMountSetTerminalScenario::CloseThenRetire => {
            secondary_owner.close_mount_for_test(ViewCloseReason::HostClosed);
            primary_owner.retire_mount_for_test();
        }
    }
    let settlement = cx.update_entity(&primary_binding, |binding, cx| match binding.authoring() {
        Ok(authoring) => mutation.observe(&authoring, |_| false, cx),
        Err(error) => mutation.observe_authoring_error(&error),
    });
    match scenario {
        ManagedMountSetTerminalScenario::Retire(_) => {
            assert!(matches!(settlement, MutationSettlement::Retired { .. }));
        }
        ManagedMountSetTerminalScenario::RetireThenClose
        | ManagedMountSetTerminalScenario::CloseThenRetire => {
            assert!(matches!(
                settlement,
                MutationSettlement::Closed {
                    reason: ViewCloseReason::HostClosed,
                    ..
                }
            ));
        }
    }
    assert_eq!(mutation.active_intent(), Some(&intent));
    cx.read_entity(&host, |host, _| {
        for node in [primary_node, secondary_node] {
            let mounted = host
                .retained_nodes
                .active_in_order()
                .find(|(instance, _)| instance.node() == node)
                .map(|(_, mounted)| mounted)
                .expect("retired mutation member remains retained until host reconciliation");
            assert_eq!(mounted.activity.active_count(), 0);
            assert_eq!(mounted.activity.protected_count(), 0);
        }
        let resources = host.resource_snapshot();
        assert_eq!(resources.protected_instances(), 0);
        assert_eq!(resources.protected_leases(), 0);
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
    });
}

fn assert_managed_post_admission_unwind(
    cx: &mut open_gpui::TestAppContext,
    fault: ManagedMutationPostAdmissionFault,
    label: &str,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let node = NodeId::from_u128(10);
    let binding = mounted_binding(cx, &host, node);
    let mut mutation = cx.read_entity(&binding, |binding, _| {
        binding.managed_mutation::<serde_json::Value>()
    });
    let intent = serde_json::json!({ "managed": label });
    mutation.inject_post_admission_fault_for_test(fault);

    let panic = cx.update_entity(&binding, |binding, cx| {
        let authoring = binding
            .authoring()
            .expect("post-admission unwind authoring");
        let command =
            GraphCommand::set_node_data(authoring.version(), authoring.node_id(), intent.clone());
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mutation.dispatch(
                authoring,
                command,
                MutationRequest::new(intent.clone()),
                MutationLayoutEffect::InvalidateWhenSatisfied,
                cx,
            )
        }))
    });

    assert!(panic.is_err());
    assert_eq!(runtime.snapshot().expect("unsubmitted graph").revision(), 0);
    assert_eq!(mutation.active_intent(), Some(&intent));
    match mutation.settlement() {
        MutationSettlement::Uncertain {
            diagnostic,
            retry_confirmable,
        } => {
            assert_eq!(
                diagnostic.source(),
                MutationDiagnosticSource::NativeDispatch
            );
            let expected_message = match fault {
                ManagedMutationPostAdmissionFault::TaskInstallationUnwound => {
                    "native task installation unwound"
                }
                ManagedMutationPostAdmissionFault::HostNotificationUnwound => {
                    "native host notification unwound"
                }
            };
            assert!(diagnostic.message().contains(expected_message));
            assert!(!retry_confirmable);
        }
        settlement => panic!("unexpected post-admission unwind settlement: {settlement:?}"),
    }
    assert!(mutation.confirm_uncertain_retry().is_none());
    assert!(mutation.unobservable_abandonment().is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (1, 0, 0));
        assert_managed_activity(host, node, 1, 1);
    });

    let mut result = cx.update_entity(&host, |host, _| {
        let transaction = collect_managed_transaction(host);
        transaction.submit(&host.view)
    });
    assert!(matches!(
        result.submission,
        OpenGpuiFrameTransactionSubmission::Submitted(_)
    ));
    result
        .take_command_batch_result()
        .expect("post-admission unwind command batch settles");
    let completed = cx.read_entity(&host, |host, _| host.commands.take_deferred_completions());
    resolve_binding_commands(completed);
    assert_eq!(runtime.snapshot().expect("submitted graph").revision(), 1);
    let advanced = cx.update_entity(&binding, |binding, cx| {
        binding
            .advance(1, cx)
            .expect("advance post-admission unwind binding")
    });
    assert_eq!(advanced, crate::binding::OpenGpuiBindingAdvance::Applied);
    let generation_before = node_layout_generation(cx, &host, node);
    cx.run_until_parked();

    assert_eq!(
        observe_managed_data_mutation(cx, &binding, &mut mutation),
        MutationSettlement::Satisfied {
            layout: MutationLayoutSettlement::Invalidated,
        }
    );
    let generation_after = node_layout_generation(cx, &host, node);
    assert_eq!(generation_after, generation_before + 1);
    assert_eq!(
        observe_managed_data_mutation(cx, &binding, &mut mutation),
        MutationSettlement::Ready
    );
    assert_eq!(node_layout_generation(cx, &host, node), generation_after);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        assert_managed_activity(host, node, 0, 0);
    });
}

fn collect_managed_transaction(host: &OpenGpuiSceneHost) -> OpenGpuiFrameTransaction {
    assert!(host.frame_transaction.is_none());
    let frame = host
        .view
        .frame(host.frame_request())
        .expect("managed transaction frame");
    let collector = frame.collector();
    let batch = host
        .commands
        .collect_next_batch(&collector)
        .expect("collect managed transaction batch")
        .expect("managed transaction has one queued command");
    let measurements = Rc::new(OpenGpuiPendingMeasurements::new(
        frame.nodes(),
        frame.transform(),
        BTreeMap::new(),
        host.content_sizes.environment_epoch(),
    ));
    OpenGpuiFrameTransaction::new(
        collector,
        measurements,
        Rc::clone(&host.commands),
        Some(batch),
    )
}

fn assert_managed_activity(
    host: &OpenGpuiSceneHost,
    node: NodeId,
    active_leases: usize,
    protected_leases: usize,
) {
    let mounted = host
        .retained_nodes
        .active_in_order()
        .find(|(instance, _)| instance.node() == node)
        .map(|(_, mounted)| mounted)
        .expect("managed node remains mounted");
    assert_eq!(mounted.activity.active_count(), active_leases);
    assert_eq!(mounted.activity.protected_count(), protected_leases);
    let resources = host.resource_snapshot();
    assert_eq!(
        resources.protected_instances(),
        usize::from(protected_leases > 0)
    );
    assert_eq!(resources.protected_leases(), protected_leases);
}

fn node_layout_generation(
    cx: &mut open_gpui::VisualTestContext,
    host: &open_gpui::Entity<OpenGpuiSceneHost>,
    node: NodeId,
) -> u64 {
    cx.read_entity(host, |host, _| {
        host.view
            .frame(host.frame_request())
            .expect("layout generation frame")
            .nodes()
            .iter()
            .find(|frame| frame.id() == node)
            .expect("managed node frame")
            .layout_generation()
    })
}

fn resolve_binding_commands(completed: Vec<OpenGpuiCompletedCommand>) {
    resolve_command_tickets(
        completed
            .into_iter()
            .filter_map(OpenGpuiCompletedCommand::into_resolution),
    );
}
