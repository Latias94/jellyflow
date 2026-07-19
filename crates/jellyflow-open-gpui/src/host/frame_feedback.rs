use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;

use jellyflow::runtime::{
    CommandSettlement, FeedbackCollector, FeedbackCollectorError, FeedbackOperationReceipt,
    FeedbackReceipt, FeedbackSubmissionError, GraphCommand, GraphCommandOutcome, LayoutNodeStatus,
    NodeGeometryEdit, ViewSession,
};

use crate::command_journal::{
    OpenGpuiCommandBatch, OpenGpuiCommandBatchStateError, OpenGpuiCommandJournal,
    OpenGpuiCommandOrigin, OpenGpuiCommandTicketError, OpenGpuiCompletedCommand,
    OpenGpuiPendingCommandResolution,
};
use crate::port_handle::{OpenGpuiMeasurementError, OpenGpuiPendingMeasurements};
use crate::size_policy::{
    OpenGpuiNodeSizeEpoch, OpenGpuiNodeSizeSettlement, OpenGpuiNodeSizeTicket,
    OpenGpuiNodeSizeUpdate,
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
    ) -> bool {
        let environment_epoch = measurements.environment_epoch();
        if environment_epoch != self.adapter_environment_epoch {
            return false;
        }
        let mut updates = Vec::new();
        for (instance, content_epoch, measured_size) in measurements.content_sizes() {
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

        self.dispatch_node_size_updates(updates)
    }

    fn dispatch_node_size_updates(
        &mut self,
        mut updates: Vec<(jellyflow::runtime::NodeInstanceKey, OpenGpuiNodeSizeUpdate)>,
    ) -> bool {
        loop {
            let snapshot = self.view.graph_snapshot().ok();
            let mut commands = Vec::new();
            let mut retry_updates = Vec::new();
            let mut pending_updates = VecDeque::from(updates);
            while let Some((instance, update)) = pending_updates.pop_front() {
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
                let Some(size_command) = update.command() else {
                    continue;
                };

                let current_instance = self.view.node_incarnation(instance.node()).ok().flatten();
                let valid = snapshot.as_ref().is_some_and(|snapshot| {
                    current_instance == Some(instance.incarnation())
                        && snapshot.graph().node(&instance.node()).is_some()
                });
                if valid {
                    commands.push((instance, size_command));
                    continue;
                }

                let Some(mounted) = self.mounts.get_mut(&instance) else {
                    continue;
                };
                let update = mounted.size_state.settle(
                    mounted.registration_policy.size_policy(),
                    size_command.ticket(),
                    if current_instance == Some(instance.incarnation()) {
                        OpenGpuiNodeSizeSettlement::Rejected
                    } else {
                        OpenGpuiNodeSizeSettlement::Retired
                    },
                );
                if !update.is_idle() {
                    pending_updates.push_back((instance, update));
                }
            }

            if commands.is_empty() {
                return false;
            }

            let snapshot = snapshot.expect("validated size commands require a graph snapshot");
            let command = GraphCommand::set_node_geometry(
                snapshot.revision(),
                commands.iter().map(|(instance, size_command)| {
                    NodeGeometryEdit::new(instance.node()).with_size(Some(size_command.size()))
                }),
            )
            .without_history();
            let tickets = commands
                .iter()
                .map(|(instance, size_command)| (instance.clone(), size_command.ticket()))
                .collect();
            match self
                .commands
                .enqueue_detached(command, OpenGpuiCommandOrigin::NodeSizes { tickets })
            {
                Ok(()) => return true,
                Err(error) => {
                    self.record_command_diagnostic(format!(
                        "failed to enqueue content sizes for {} nodes: {error}",
                        commands.len()
                    ));
                }
            }

            for (instance, size_command) in commands {
                let current_instance = self.view.node_incarnation(instance.node()).ok().flatten();
                let Some(mounted) = self.mounts.get_mut(&instance) else {
                    continue;
                };
                let update = mounted.size_state.settle(
                    mounted.registration_policy.size_policy(),
                    size_command.ticket(),
                    if current_instance == Some(instance.incarnation()) {
                        OpenGpuiNodeSizeSettlement::Rejected
                    } else {
                        OpenGpuiNodeSizeSettlement::Retired
                    },
                );
                if !update.is_idle() {
                    retry_updates.push((instance, update));
                }
            }
            if retry_updates.is_empty() {
                return false;
            };
            updates = retry_updates;
        }
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

    pub(super) fn handle_feedback_receipt(&mut self, receipt: &FeedbackReceipt) -> bool {
        let mut refresh = receipt.request_next_frame();
        for operation in receipt.operations() {
            let FeedbackOperationReceipt::Layout { node, status } = operation else {
                continue;
            };
            match layout_receipt_action(*status) {
                LayoutReceiptAction::Benign => {}
                LayoutReceiptAction::Refresh => refresh = true,
                LayoutReceiptAction::Reject => self.record_layout_rejection(*node, *status),
            }
        }
        refresh
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
                    self.dispatch_node_size_updates(updates);
                }
            }
        }
        resolutions
    }
}
