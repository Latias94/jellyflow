use std::cell::Cell;
use std::collections::BTreeSet;
use std::rc::Rc;

use jellyflow::runtime::{
    CommandSettlement, FeedbackCollector, FeedbackCollectorError, FeedbackOperationReceipt,
    FeedbackReceipt, FeedbackSubmissionError, GraphCommand, GraphCommandOutcome, LayoutNodeStatus,
    NodeGeometryEdit, ViewSession,
};
use open_gpui::{Context, Task};

use crate::command_journal::{
    OpenGpuiCommandBatch, OpenGpuiCommandBatchStateError, OpenGpuiCommandJournal,
    OpenGpuiCommandOrigin, OpenGpuiCommandPrefixBarrierCancellation, OpenGpuiCommandPrefixPermit,
    OpenGpuiCommandTicketError, OpenGpuiCompletedCommand, OpenGpuiPendingCommandResolution,
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
    pub(super) accepted_content_nodes: BTreeSet<jellyflow::core::NodeId>,
}

pub(super) struct PendingNodeSizeSubmission {
    commands: Vec<(jellyflow::runtime::NodeInstanceKey, OpenGpuiNodeSizeCommand)>,
    follow_up_commands: Vec<(jellyflow::runtime::NodeInstanceKey, OpenGpuiNodeSizeCommand)>,
    prefix_task: Option<Task<()>>,
    prefix_cancellation: Option<OpenGpuiCommandPrefixBarrierCancellation>,
}

impl PendingNodeSizeSubmission {
    fn new(commands: Vec<(jellyflow::runtime::NodeInstanceKey, OpenGpuiNodeSizeCommand)>) -> Self {
        Self {
            commands,
            follow_up_commands: Vec::new(),
            prefix_task: None,
            prefix_cancellation: None,
        }
    }
}

pub(super) struct DeferredFrameFeedback {
    collector: FeedbackCollector,
    pub(super) measurements: Rc<OpenGpuiPendingMeasurements>,
    commands: Rc<OpenGpuiCommandJournal>,
    command_batch: Option<OpenGpuiCommandBatch>,
    scheduled: Cell<bool>,
}

impl DeferredFrameFeedback {
    pub(super) fn new(
        collector: FeedbackCollector,
        measurements: Rc<OpenGpuiPendingMeasurements>,
        commands: Rc<OpenGpuiCommandJournal>,
        command_batch: Option<OpenGpuiCommandBatch>,
    ) -> Self {
        Self {
            collector,
            measurements,
            commands,
            command_batch,
            scheduled: Cell::new(false),
        }
    }

    pub(super) fn schedule_once(&self) -> bool {
        !self.scheduled.replace(true)
    }

    pub(super) fn submit(&self, view: &ViewSession) -> DeferredFeedbackSubmission {
        let feedback: Result<_, DeferredFeedbackError> = (|| {
            for layout in self.measurements.layouts()? {
                self.collector.layout(layout)?;
            }

            let feedback = self.collector.finish()?;
            if let Some(batch) = self.command_batch {
                self.commands.mark_submitted(batch)?;
            }
            Ok(feedback)
        })();
        let feedback = match feedback {
            Ok(feedback) => feedback,
            Err(error) => return DeferredFeedbackSubmission::Failed(error),
        };

        // GPUI entity and window leases cannot safely restore themselves across this unwind.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            view.submit_feedback(feedback)
        })) {
            Ok(Ok(receipt)) => DeferredFeedbackSubmission::Submitted(receipt),
            Ok(Err(error)) => DeferredFeedbackSubmission::Failed(error.into()),
            Err(payload) => DeferredFeedbackSubmission::Panicked(payload),
        }
    }

    pub(super) fn requires_follow_up_frame(&self) -> bool {
        self.command_batch.is_some()
    }

    pub(super) fn complete_commands(
        &self,
        receipt: FeedbackReceipt,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError> {
        self.command_batch.map_or_else(
            || Ok(Vec::new()),
            |batch| self.commands.complete_submitted(batch, receipt),
        )
    }

    pub(super) fn fail_commands(
        &self,
        error: impl FnMut() -> OpenGpuiCommandTicketError,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError> {
        self.command_batch.map_or_else(
            || Ok(Vec::new()),
            |batch| self.commands.fail_batch(batch, error),
        )
    }
}

pub(super) enum DeferredFeedbackSubmission {
    Submitted(FeedbackReceipt),
    Failed(DeferredFeedbackError),
    Panicked(Box<dyn std::any::Any + Send + 'static>),
}

#[derive(Debug, thiserror::Error)]
pub(super) enum DeferredFeedbackError {
    #[error(transparent)]
    Measurement(#[from] OpenGpuiMeasurementError),
    #[error(transparent)]
    Collector(#[from] FeedbackCollectorError),
    #[error(transparent)]
    CommandBatch(#[from] OpenGpuiCommandBatchStateError),
    #[error(transparent)]
    Submission(#[from] FeedbackSubmissionError),
}

impl OpenGpuiSceneHost {
    pub(super) fn observe_content_measurements(
        &mut self,
        measurements: &OpenGpuiPendingMeasurements,
        accepted_nodes: &BTreeSet<jellyflow::core::NodeId>,
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
        updates: Vec<(jellyflow::runtime::NodeInstanceKey, OpenGpuiNodeSizeUpdate)>,
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
        commands: Vec<(jellyflow::runtime::NodeInstanceKey, OpenGpuiNodeSizeCommand)>,
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
            let current_instance = self.view.node_incarnation(instance.node()).ok().flatten();
            if current_epoch == Some(command.epoch())
                && current_instance == Some(instance.incarnation())
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
            snapshot.revision(),
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
        commands: Vec<(jellyflow::runtime::NodeInstanceKey, OpenGpuiNodeSizeCommand)>,
        message: String,
    ) {
        self.record_command_diagnostic(message);
        let updates = commands
            .into_iter()
            .filter_map(|(instance, command)| {
                let current_instance = self.view.node_incarnation(instance.node()).ok().flatten();
                self.settle_node_size_command(
                    instance.clone(),
                    command.ticket(),
                    if current_instance == Some(instance.incarnation()) {
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
        instance: jellyflow::runtime::NodeInstanceKey,
        ticket: OpenGpuiNodeSizeTicket,
        settlement: OpenGpuiNodeSizeSettlement,
    ) -> Option<(jellyflow::runtime::NodeInstanceKey, OpenGpuiNodeSizeUpdate)> {
        let mounted = self.mounts.get_mut(&instance)?;
        let update = mounted.size_state.settle(
            mounted.registration_policy.size_policy(),
            ticket,
            settlement,
        );
        (!update.is_idle()).then_some((instance, update))
    }

    pub(super) fn handle_feedback_receipt(
        &mut self,
        receipt: &FeedbackReceipt,
    ) -> OpenGpuiFrameReceiptDisposition {
        let mut disposition = OpenGpuiFrameReceiptDisposition {
            request_next_frame: receipt.request_next_frame(),
            accepted_content_nodes: BTreeSet::new(),
        };
        for operation in receipt.operations() {
            let FeedbackOperationReceipt::Layout { node, status } = operation else {
                continue;
            };
            match layout_receipt_action(*status) {
                LayoutReceiptAction::Benign => {
                    disposition.accepted_content_nodes.insert(*node);
                }
                LayoutReceiptAction::Refresh => {}
                LayoutReceiptAction::Reject => self.record_layout_rejection(*node, *status),
            }
        }
        disposition
    }

    pub(super) fn settle_command_batch(
        &mut self,
        result: Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError>,
    ) -> Vec<OpenGpuiPendingCommandResolution> {
        match result {
            Ok(completed) => self.handle_completed_commands(completed),
            Err(error) => {
                self.canvas_interaction.cancel();
                self.discard_pending_interaction_submission();
                self.record_feedback_failure(
                    None,
                    format!("failed to settle native command batch: {error}"),
                );
                Vec::new()
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
        for command in completed {
            match command.origin.clone() {
                OpenGpuiCommandOrigin::Binding => {
                    if let Some(resolution) = command.into_resolution() {
                        resolutions.push(resolution);
                    }
                }
                OpenGpuiCommandOrigin::Interaction { ticket } => {
                    if let Ok(result) = command.into_detached_result() {
                        let settlement = match &result {
                            Ok(GraphCommandOutcome::Applied(_)) => CommandSettlement::Applied,
                            Ok(GraphCommandOutcome::Noop(_)) => CommandSettlement::Noop,
                            Ok(GraphCommandOutcome::Rejected(rejection)) => {
                                self.record_command_diagnostic(rejection.message().to_owned());
                                CommandSettlement::Rejected
                            }
                            Ok(GraphCommandOutcome::Conflict(conflict)) => {
                                self.record_command_diagnostic(format!(
                                    "interaction expected graph revision {} but current revision is {}",
                                    conflict.expected_revision(),
                                    conflict.current().revision()
                                ));
                                CommandSettlement::Conflict
                            }
                            Err(error) => {
                                self.record_command_diagnostic(error.to_string());
                                CommandSettlement::TransportFailure
                            }
                        };
                        self.settle_interaction_command(ticket, settlement);
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
                        Ok(GraphCommandOutcome::Applied(_)) | Ok(GraphCommandOutcome::Noop(_)) => {
                            OpenGpuiNodeSizeSettlement::Applied
                        }
                        Ok(GraphCommandOutcome::Rejected(rejection)) => {
                            self.record_command_diagnostic(rejection.message().to_owned());
                            OpenGpuiNodeSizeSettlement::Rejected
                        }
                        Ok(GraphCommandOutcome::Conflict(conflict)) => {
                            self.record_command_diagnostic(format!(
                                "content size expected graph revision {} but current revision is {}",
                                conflict.expected_revision(),
                                conflict.current().revision()
                            ));
                            OpenGpuiNodeSizeSettlement::Conflict
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
