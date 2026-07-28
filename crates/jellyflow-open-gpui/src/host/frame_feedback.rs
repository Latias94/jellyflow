use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use jellyflow_core::NodeId;
use jellyflow_runtime::{
    commands::CommandExecutionKind, commands::GraphCommand, commands::NodeGeometryEdit,
    view::FeedbackCollector, view::FeedbackCollectorError, view::FeedbackReceipt,
    view::FeedbackSubmissionError, view::LayoutNodeStatus, view::ViewSession,
};
use open_gpui::{Context, Task};

use crate::command_journal::{
    OpenGpuiCommandBatch, OpenGpuiCommandBatchStateError, OpenGpuiCommandJournal,
    OpenGpuiCommandOrigin, OpenGpuiCommandPrefixBarrierCancellation, OpenGpuiCommandPrefixPermit,
    OpenGpuiCommandTicketError, OpenGpuiCompletedCommand, OpenGpuiPendingCommandResolution,
    resolve_command_tickets,
};
use crate::port_handle::{OpenGpuiMeasurementError, OpenGpuiPendingMeasurements};
use crate::size_policy::{
    OpenGpuiNodeSizeCommand, OpenGpuiNodeSizeEpoch, OpenGpuiNodeSizeSettlement,
    OpenGpuiNodeSizeTicket, OpenGpuiNodeSizeUpdate,
};

use super::OpenGpuiSceneHost;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LayoutReceiptAction {
    Benign,
    Refresh,
    Reject,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenGpuiFrameTransactionFault {
    AfterStateTake,
    AfterFeedbackSubmission,
}

pub(super) fn layout_receipt_action(status: LayoutNodeStatus) -> LayoutReceiptAction {
    match status {
        LayoutNodeStatus::Published | LayoutNodeStatus::Unchanged => LayoutReceiptAction::Benign,
        LayoutNodeStatus::Stale => LayoutReceiptAction::Refresh,
        LayoutNodeStatus::MissingNode
        | LayoutNodeStatus::Hidden
        | LayoutNodeStatus::Culled
        | LayoutNodeStatus::InvalidBounds
        | LayoutNodeStatus::IncompletePorts
        | LayoutNodeStatus::DuplicatePorts
        | LayoutNodeStatus::ForeignPort
        | LayoutNodeStatus::DuplicateNode => LayoutReceiptAction::Reject,
    }
}

pub(super) struct OpenGpuiFrameReceiptDisposition {
    pub(super) request_next_frame: bool,
    pub(super) accepted_content_nodes: BTreeSet<jellyflow_core::NodeId>,
    pub(super) rejected_layouts: Vec<(NodeId, LayoutNodeStatus)>,
}

pub(super) struct PendingNodeSizeSubmission {
    commands: Vec<(
        jellyflow_runtime::view::NodeInstanceKey,
        OpenGpuiNodeSizeCommand,
    )>,
    follow_up_commands: Vec<(
        jellyflow_runtime::view::NodeInstanceKey,
        OpenGpuiNodeSizeCommand,
    )>,
    prefix_task: Option<Task<()>>,
    prefix_cancellation: Option<OpenGpuiCommandPrefixBarrierCancellation>,
}

impl PendingNodeSizeSubmission {
    fn new(
        commands: Vec<(
            jellyflow_runtime::view::NodeInstanceKey,
            OpenGpuiNodeSizeCommand,
        )>,
    ) -> Self {
        Self {
            commands,
            follow_up_commands: Vec::new(),
            prefix_task: None,
            prefix_cancellation: None,
        }
    }
}

pub(super) struct OpenGpuiFrameTransaction {
    state: RefCell<Option<OpenGpuiFrameTransactionState>>,
    measurements: Rc<OpenGpuiPendingMeasurements>,
    commands: Rc<OpenGpuiCommandJournal>,
    scheduled: Cell<bool>,
    #[cfg(test)]
    fault: Cell<Option<OpenGpuiFrameTransactionFault>>,
}

struct OpenGpuiFrameTransactionState {
    collector: FeedbackCollector,
    command_batch: Option<OpenGpuiCommandBatch>,
}

impl OpenGpuiFrameTransaction {
    pub(super) fn new(
        collector: FeedbackCollector,
        measurements: Rc<OpenGpuiPendingMeasurements>,
        commands: Rc<OpenGpuiCommandJournal>,
        command_batch: Option<OpenGpuiCommandBatch>,
    ) -> Self {
        Self {
            state: RefCell::new(Some(OpenGpuiFrameTransactionState {
                collector,
                command_batch,
            })),
            measurements,
            commands,
            scheduled: Cell::new(false),
            #[cfg(test)]
            fault: Cell::new(None),
        }
    }

    pub(super) fn measurements(&self) -> &OpenGpuiPendingMeasurements {
        &self.measurements
    }

    pub(super) fn measurement_writer(&self) -> Rc<OpenGpuiPendingMeasurements> {
        Rc::clone(&self.measurements)
    }

    pub(super) fn schedule_once(&self) -> bool {
        !self.scheduled.replace(true)
    }

    pub(super) fn requires_follow_up_frame(&self) -> bool {
        self.state
            .borrow()
            .as_ref()
            .is_some_and(|state| state.command_batch.is_some())
    }

    pub(super) fn submit(&self, view: &ViewSession) -> OpenGpuiFrameTransactionResult {
        let Some(mut state) = self.state.borrow_mut().take() else {
            return OpenGpuiFrameTransactionResult {
                submission: OpenGpuiFrameTransactionSubmission::Failed(
                    OpenGpuiFrameTransactionError::AlreadyConsumed,
                ),
                command_batch_result: Ok(()),
                measurement_errors: Vec::new(),
                command_pairing_error: None,
            };
        };
        #[cfg(test)]
        self.panic_at_fault(OpenGpuiFrameTransactionFault::AfterStateTake);
        let (layouts, measurement_errors) = self.measurements.seal_layouts().into_parts();
        let feedback: Result<_, OpenGpuiFrameTransactionError> = (|| {
            for layout in layouts {
                state.collector.layout(layout)?;
            }

            let feedback = state.collector.finish()?;
            if let Some(batch) = state.command_batch.as_ref() {
                self.commands.mark_submitted(batch)?;
            }
            Ok(feedback)
        })();
        let feedback = match feedback {
            Ok(feedback) => feedback,
            Err(error) => {
                let message = error.to_string();
                let commands = state.fail_commands(&self.commands, || {
                    OpenGpuiCommandTicketError::FramePreparation {
                        message: message.clone(),
                    }
                });
                return OpenGpuiFrameTransactionResult {
                    submission: OpenGpuiFrameTransactionSubmission::Failed(error),
                    command_batch_result: retain_command_completions(&self.commands, commands),
                    measurement_errors,
                    command_pairing_error: None,
                };
            }
        };

        // GPUI entity and window leases cannot safely restore themselves across this unwind.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            view.submit_feedback(feedback)
        })) {
            Ok(Ok(receipt)) => {
                #[cfg(test)]
                self.panic_at_fault(OpenGpuiFrameTransactionFault::AfterFeedbackSubmission);
                let disposition = frame_receipt_disposition(&receipt);
                let (commands, command_pairing_error) =
                    state.complete_commands(&self.commands, receipt);
                OpenGpuiFrameTransactionResult {
                    submission: OpenGpuiFrameTransactionSubmission::Submitted(disposition),
                    command_batch_result: retain_command_completions(&self.commands, commands),
                    measurement_errors,
                    command_pairing_error,
                }
            }
            Ok(Err(mut error)) => {
                let post_admission = error.take_post_admission_receipt();
                let error = Rc::new(error);
                let (commands, command_pairing_error) = match post_admission {
                    Some(receipt) => state.complete_commands(&self.commands, receipt),
                    None => (
                        state.fail_commands(&self.commands, || {
                            OpenGpuiCommandTicketError::FeedbackSubmission(Rc::clone(&error))
                        }),
                        None,
                    ),
                };
                OpenGpuiFrameTransactionResult {
                    submission: OpenGpuiFrameTransactionSubmission::Failed(
                        OpenGpuiFrameTransactionError::Submission(error),
                    ),
                    command_batch_result: retain_command_completions(&self.commands, commands),
                    measurement_errors,
                    command_pairing_error,
                }
            }
            Err(payload) => OpenGpuiFrameTransactionResult {
                submission: OpenGpuiFrameTransactionSubmission::Panicked(payload),
                command_batch_result: retain_command_completions(
                    &self.commands,
                    state.fail_commands(&self.commands, || {
                        OpenGpuiCommandTicketError::FeedbackOutcomeUnknown
                    }),
                ),
                measurement_errors,
                command_pairing_error: None,
            },
        }
    }

    #[cfg(test)]
    pub(super) fn inject_fault(&self, fault: OpenGpuiFrameTransactionFault) {
        self.fault.set(Some(fault));
    }

    #[cfg(test)]
    fn panic_at_fault(&self, fault: OpenGpuiFrameTransactionFault) {
        if self.fault.get() == Some(fault) {
            self.fault.set(None);
            panic!("injected native frame transaction fault at {fault:?}");
        }
    }

    pub(super) fn abort_frame_preparation(
        &self,
        message: String,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError> {
        let Some(mut state) = self.state.borrow_mut().take() else {
            return Ok(Vec::new());
        };
        state.fail_commands(&self.commands, || {
            OpenGpuiCommandTicketError::FramePreparation {
                message: message.clone(),
            }
        })
    }

    pub(super) fn discard_after_journal_terminal(&self) {
        self.state.borrow_mut().take();
    }
}

impl OpenGpuiFrameTransactionState {
    fn complete_commands(
        &mut self,
        commands: &OpenGpuiCommandJournal,
        receipt: FeedbackReceipt,
    ) -> (
        Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError>,
        Option<String>,
    ) {
        let Some(batch) = self.command_batch.take() else {
            return (Ok(Vec::new()), None);
        };
        match commands.complete_submitted(&batch, receipt) {
            Ok(completed) => (Ok(completed), None),
            Err(error) => {
                let message = format!("failed to pair native command receipts: {error}");
                let completed = commands
                    .fail_batch(batch, || OpenGpuiCommandTicketError::FeedbackOutcomeUnknown);
                (completed, Some(message))
            }
        }
    }

    fn fail_commands(
        &mut self,
        commands: &OpenGpuiCommandJournal,
        error: impl FnMut() -> OpenGpuiCommandTicketError,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError> {
        self.command_batch
            .take()
            .map_or_else(|| Ok(Vec::new()), |batch| commands.fail_batch(batch, error))
    }
}

impl Drop for OpenGpuiFrameTransaction {
    fn drop(&mut self) {
        let Some(mut state) = self.state.get_mut().take() else {
            return;
        };
        let Ok(completed) =
            state.fail_commands(&self.commands, || OpenGpuiCommandTicketError::HostDropped)
        else {
            return;
        };
        resolve_command_tickets(self.commands.defer_completions(completed));
    }
}

pub(super) struct OpenGpuiFrameTransactionResult {
    pub(super) submission: OpenGpuiFrameTransactionSubmission,
    command_batch_result: Result<(), OpenGpuiCommandBatchStateError>,
    pub(super) measurement_errors: Vec<OpenGpuiMeasurementError>,
    pub(super) command_pairing_error: Option<String>,
}

impl OpenGpuiFrameTransactionResult {
    pub(super) fn take_command_batch_result(
        &mut self,
    ) -> Result<(), OpenGpuiCommandBatchStateError> {
        std::mem::replace(&mut self.command_batch_result, Ok(()))
    }
}

fn retain_command_completions(
    journal: &OpenGpuiCommandJournal,
    completed: Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError>,
) -> Result<(), OpenGpuiCommandBatchStateError> {
    completed.map(|completed| journal.retain_completions_for_host(completed))
}

pub(super) enum OpenGpuiFrameTransactionSubmission {
    Submitted(OpenGpuiFrameReceiptDisposition),
    Failed(OpenGpuiFrameTransactionError),
    Panicked(Box<dyn std::any::Any + Send + 'static>),
}

#[derive(Debug, thiserror::Error)]
pub(super) enum OpenGpuiFrameTransactionError {
    #[error("native frame transaction was already consumed")]
    AlreadyConsumed,
    #[error(transparent)]
    Collector(#[from] FeedbackCollectorError),
    #[error(transparent)]
    CommandBatch(#[from] OpenGpuiCommandBatchStateError),
    #[error("native feedback submission failed: {0}")]
    Submission(Rc<FeedbackSubmissionError>),
}

fn frame_receipt_disposition(receipt: &FeedbackReceipt) -> OpenGpuiFrameReceiptDisposition {
    let mut disposition = OpenGpuiFrameReceiptDisposition {
        request_next_frame: receipt.request_next_frame(),
        accepted_content_nodes: BTreeSet::new(),
        rejected_layouts: Vec::new(),
    };
    for layout in receipt.layout_receipts() {
        let Some(status) = layout.status() else {
            continue;
        };
        match layout_receipt_action(status) {
            LayoutReceiptAction::Benign => {
                disposition.accepted_content_nodes.insert(layout.node());
            }
            LayoutReceiptAction::Refresh => {}
            LayoutReceiptAction::Reject => {
                disposition.rejected_layouts.push((layout.node(), status));
            }
        }
    }
    disposition
}

impl OpenGpuiSceneHost {
    pub(super) fn abort_frame_transaction(
        &mut self,
        message: String,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError> {
        let Some(transaction) = self.frame_transaction.take() else {
            return Ok(Vec::new());
        };
        transaction.abort_frame_preparation(message)
    }

    pub(super) fn take_frame_transaction(
        &mut self,
        candidate: &Rc<OpenGpuiFrameTransaction>,
    ) -> Option<Rc<OpenGpuiFrameTransaction>> {
        let current = self.frame_transaction.take()?;
        if Rc::ptr_eq(&current, candidate) {
            Some(current)
        } else {
            self.frame_transaction = Some(current);
            None
        }
    }

    pub(super) fn discard_frame_transaction(&mut self) {
        if let Some(transaction) = self.frame_transaction.take() {
            transaction.discard_after_journal_terminal();
        }
    }

    pub(super) fn observe_content_measurements(
        &mut self,
        measurements: &OpenGpuiPendingMeasurements,
        accepted_nodes: &BTreeSet<jellyflow_core::NodeId>,
    ) -> bool {
        let environment_epoch = measurements.environment_epoch();
        if environment_epoch != self.adapter_environment_epoch {
            return false;
        }
        let mut updates = Vec::new();
        for (instance, content_epoch, measured_size) in measurements.content_sizes() {
            if !accepted_nodes.contains(&instance.node()) {
                continue;
            }
            let Some(mounted) = self.mounts.get_mut(&instance) else {
                continue;
            };
            if content_epoch != mounted.content_epoch {
                continue;
            }
            let policy = mounted.registration_policy.size_policy();
            let epoch = OpenGpuiNodeSizeEpoch::new(mounted.content_epoch, environment_epoch);
            let update = mounted.size_state.observe(
                policy,
                epoch,
                mounted.semantic_zoom.canonical_size(),
                measured_size,
            );
            if !update.is_idle() {
                updates.push((instance, update));
            }
        }

        self.queue_node_size_updates(updates)
    }

    pub(super) fn queue_node_size_updates(
        &mut self,
        updates: Vec<(
            jellyflow_runtime::view::NodeInstanceKey,
            OpenGpuiNodeSizeUpdate,
        )>,
    ) -> bool {
        let mut commands = Vec::new();
        for (instance, update) in updates {
            if let Some(diagnostic) = update.diagnostic() {
                self.record_command_diagnostic(format!(
                        "content size convergence froze for node {:?}: reason={:?}, attempts={}, last_applied={:?}, suppressed={:?}",
                        instance.node(),
                        diagnostic.reason(),
                        diagnostic.attempts(),
                        diagnostic.last_applied_size(),
                        diagnostic.suppressed_size(),
                    ));
            }
            if let Some(command) = update.command() {
                commands.push((instance, command));
            }
        }

        if commands.is_empty() {
            return false;
        }
        if let Some(pending) = self.pending_node_size_submission.as_mut() {
            if pending.prefix_task.is_some() {
                pending.follow_up_commands.extend(commands);
            } else {
                pending.commands.extend(commands);
            }
        } else {
            self.pending_node_size_submission = Some(PendingNodeSizeSubmission::new(commands));
        }
        true
    }

    pub(super) fn schedule_pending_node_size_prefix(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_node_size_submission.as_ref() else {
            return;
        };
        if pending.prefix_task.is_some() {
            return;
        }

        let barrier = match self.commands.capture_prefix_barrier() {
            Ok(barrier) => barrier,
            Err(error) => {
                self.fail_pending_node_size_submission(error.to_string());
                return;
            }
        };
        let cancellation = barrier.cancellation();
        let host = cx.weak_entity();
        let task = cx.spawn(async move |_, cx| {
            let result = barrier.await;
            let _ = host.update(cx, |host, cx| {
                host.ready_node_size_prefixes.push_back(result);
                cx.notify();
            });
        });
        let pending = self
            .pending_node_size_submission
            .as_mut()
            .expect("node-size prefix state remains installed while its task is created");
        pending.prefix_cancellation = Some(cancellation);
        pending.prefix_task = Some(task);
    }

    pub(super) fn process_one_ready_node_size_prefix(&mut self, cx: &mut Context<Self>) {
        let Some(result) = self.ready_node_size_prefixes.pop_front() else {
            return;
        };
        let Some(mut pending) = self.pending_node_size_submission.take() else {
            drop(result);
            return;
        };
        pending.prefix_task = None;
        pending.prefix_cancellation = None;
        if !pending.follow_up_commands.is_empty() {
            self.pending_node_size_submission = Some(PendingNodeSizeSubmission::new(
                std::mem::take(&mut pending.follow_up_commands),
            ));
        }
        match result {
            Ok(permit) => self.bind_and_enqueue_node_sizes(pending.commands, permit),
            Err(error) => self.fail_node_size_commands(
                pending.commands,
                format!("content size command prefix failed: {error}"),
            ),
        }
        if self.pending_node_size_submission.is_some() || self.commands.has_pending() {
            cx.notify();
        }
    }

    fn bind_and_enqueue_node_sizes(
        &mut self,
        commands: Vec<(
            jellyflow_runtime::view::NodeInstanceKey,
            OpenGpuiNodeSizeCommand,
        )>,
        permit: OpenGpuiCommandPrefixPermit,
    ) {
        let snapshot = match self.view.graph_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                permit.abandon();
                self.fail_node_size_commands(
                    commands,
                    format!("failed to bind content sizes to a fresh graph snapshot: {error}"),
                );
                return;
            }
        };
        let mut valid = Vec::new();
        let mut retired = Vec::new();
        for (instance, command) in commands {
            let current_epoch = self.mounts.get(&instance).map(|mounted| {
                OpenGpuiNodeSizeEpoch::new(mounted.content_epoch, self.adapter_environment_epoch)
            });
            let current_lifetime = self.view.node_lifetime_key(instance.node()).ok().flatten();
            if current_epoch == Some(command.epoch())
                && current_lifetime == Some(instance.lifetime())
                && snapshot.graph().node(&instance.node()).is_some()
            {
                valid.push((instance, command));
            } else {
                retired.push((instance, command));
            }
        }
        let retired_updates = retired
            .into_iter()
            .filter_map(|(instance, command)| {
                self.settle_node_size_command(
                    instance,
                    command.ticket(),
                    OpenGpuiNodeSizeSettlement::Retired,
                )
            })
            .collect();
        self.queue_node_size_updates(retired_updates);

        if valid.is_empty() {
            permit.abandon();
            return;
        }
        let command = GraphCommand::set_node_geometry(
            snapshot.version(),
            valid.iter().map(|(instance, size_command)| {
                NodeGeometryEdit::new(instance.node()).with_size(Some(size_command.size()))
            }),
        )
        .without_history();
        let tickets = valid
            .iter()
            .map(|(instance, size_command)| (instance.clone(), size_command.ticket()))
            .collect();
        if let Err(error) =
            permit.enqueue_detached(command, OpenGpuiCommandOrigin::NodeSizes { tickets })
        {
            self.fail_node_size_commands(
                valid,
                format!("failed to enqueue freshly bound content sizes: {error}"),
            );
        }
    }

    fn fail_pending_node_size_submission(&mut self, message: String) {
        let Some(pending) = self.pending_node_size_submission.take() else {
            return;
        };
        let mut commands = pending.commands;
        commands.extend(pending.follow_up_commands);
        self.fail_node_size_commands(commands, message);
    }

    fn fail_node_size_commands(
        &mut self,
        commands: Vec<(
            jellyflow_runtime::view::NodeInstanceKey,
            OpenGpuiNodeSizeCommand,
        )>,
        message: String,
    ) {
        self.record_command_diagnostic(message);
        let updates = commands
            .into_iter()
            .filter_map(|(instance, command)| {
                let current_lifetime = self.view.node_lifetime_key(instance.node()).ok().flatten();
                self.settle_node_size_command(
                    instance.clone(),
                    command.ticket(),
                    if current_lifetime == Some(instance.lifetime()) {
                        OpenGpuiNodeSizeSettlement::Rejected
                    } else {
                        OpenGpuiNodeSizeSettlement::Retired
                    },
                )
            })
            .collect();
        self.queue_node_size_updates(updates);
    }

    pub(super) fn node_size_submission_needs_frame(&self) -> bool {
        self.pending_node_size_submission
            .as_ref()
            .is_some_and(|pending| pending.prefix_task.is_none())
    }

    pub(super) fn discard_pending_node_size_submission(&mut self) {
        self.pending_node_size_submission = None;
        self.ready_node_size_prefixes.clear();
    }

    #[cfg(test)]
    pub(super) fn pending_node_size_command_counts(&self) -> Option<(usize, usize)> {
        self.pending_node_size_submission
            .as_ref()
            .map(|pending| (pending.commands.len(), pending.follow_up_commands.len()))
    }

    #[cfg(test)]
    pub(super) fn has_pending_node_size_prefix_task(&self) -> bool {
        self.pending_node_size_submission
            .as_ref()
            .is_some_and(|pending| pending.prefix_task.is_some())
    }

    fn settle_node_size_command(
        &mut self,
        instance: jellyflow_runtime::view::NodeInstanceKey,
        ticket: OpenGpuiNodeSizeTicket,
        settlement: OpenGpuiNodeSizeSettlement,
    ) -> Option<(
        jellyflow_runtime::view::NodeInstanceKey,
        OpenGpuiNodeSizeUpdate,
    )> {
        let mounted = self.mounts.get_mut(&instance)?;
        let update = mounted.size_state.settle(
            mounted.registration_policy.size_policy(),
            ticket,
            settlement,
        );
        (!update.is_idle()).then_some((instance, update))
    }

    pub(super) fn apply_frame_receipt_disposition(
        &mut self,
        mut disposition: OpenGpuiFrameReceiptDisposition,
    ) -> OpenGpuiFrameReceiptDisposition {
        for (node, status) in &disposition.rejected_layouts {
            self.record_layout_rejection(*node, *status);
        }
        match self.apply_initial_viewport_policy() {
            Ok(changed) => disposition.request_next_frame |= changed,
            Err(error) => self.record_command_diagnostic(error.to_string()),
        }
        disposition
    }

    pub(super) fn settle_command_batch(
        &mut self,
        result: Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError>,
    ) -> Vec<OpenGpuiPendingCommandResolution> {
        let result = retain_command_completions(&self.commands, result);
        self.settle_retained_command_batch(result)
    }

    pub(super) fn settle_retained_command_batch(
        &mut self,
        result: Result<(), OpenGpuiCommandBatchStateError>,
    ) -> Vec<OpenGpuiPendingCommandResolution> {
        let resolutions = self.handle_completed_commands(Vec::new());
        match result {
            Ok(()) => resolutions,
            Err(error) => {
                if let Err(cancel_error) = self.interaction_owner.cancel() {
                    self.record_feedback_failure(
                        None,
                        format!("failed to cancel native interaction after batch failure: {cancel_error}"),
                    );
                }
                self.discard_pending_interaction_submission();
                self.record_feedback_failure(
                    None,
                    format!("failed to settle native command batch: {error}"),
                );
                resolutions
            }
        }
    }

    pub(super) fn take_prepaint_measurement_errors(
        &self,
        measurements: &OpenGpuiPendingMeasurements,
    ) -> Vec<OpenGpuiMeasurementError> {
        let mut errors = measurements.take_errors();
        errors.extend(
            self.mounts
                .active_values()
                .flat_map(|mounted| mounted.measurement.take_errors()),
        );
        errors
    }

    pub(super) fn handle_completed_commands(
        &mut self,
        completed: Vec<OpenGpuiCompletedCommand>,
    ) -> Vec<OpenGpuiPendingCommandResolution> {
        let mut resolutions = Vec::new();
        let deferred = self.commands.take_deferred_completions();
        for command in deferred.into_iter().chain(completed) {
            match command.origin.clone() {
                OpenGpuiCommandOrigin::Binding => {
                    if let Some(resolution) = command.into_resolution() {
                        resolutions.push(resolution);
                    }
                }
                OpenGpuiCommandOrigin::Interaction { ticket } => {
                    if let Ok(result) = command.into_detached_result() {
                        match result {
                            Ok(completion) => {
                                if let Some(message) = completion.diagnostic_message() {
                                    self.record_command_diagnostic(message);
                                }
                                self.settle_interaction_command(ticket, completion);
                            }
                            Err(error) => {
                                self.record_command_diagnostic(error.to_string());
                                if error.outcome_unknown() {
                                    self.interaction_submission_outcome_unknown(ticket);
                                } else {
                                    self.interaction_submission_not_dispatched(ticket);
                                }
                            }
                        }
                    }
                }
                OpenGpuiCommandOrigin::NodeSizes { tickets } => {
                    let Ok(result) = command.into_detached_result() else {
                        self.record_command_diagnostic(format!(
                            "content size command for {} nodes unexpectedly owned a public ticket",
                            tickets.len()
                        ));
                        continue;
                    };
                    let settlement = match result {
                        Ok(completion) => {
                            if let Some(message) = completion.diagnostic_message() {
                                self.record_command_diagnostic(message);
                            }
                            match completion.kind() {
                                CommandExecutionKind::Applied | CommandExecutionKind::Noop => {
                                    OpenGpuiNodeSizeSettlement::Applied
                                }
                                CommandExecutionKind::Conflict => {
                                    OpenGpuiNodeSizeSettlement::Conflict
                                }
                                CommandExecutionKind::Rejected | CommandExecutionKind::Failed => {
                                    OpenGpuiNodeSizeSettlement::Rejected
                                }
                            }
                        }
                        Err(error) => {
                            self.record_command_diagnostic(error.to_string());
                            OpenGpuiNodeSizeSettlement::Rejected
                        }
                    };
                    let updates = tickets
                        .into_iter()
                        .filter_map(|(instance, ticket)| {
                            self.settle_node_size_command(instance, ticket, settlement)
                        })
                        .collect();
                    self.queue_node_size_updates(updates);
                }
            }
        }
        resolutions
    }
}
