use std::cell::Cell;
use std::rc::Rc;

use jellyflow::runtime::{
    CommandSettlement, FeedbackCollector, FeedbackCollectorError, FeedbackOperationReceipt,
    FeedbackReceipt, FeedbackSubmissionError, GraphCommandOutcome, LayoutNodeStatus, ViewSession,
};

use crate::command_journal::{
    OpenGpuiCommandBatch, OpenGpuiCommandBatchStateError, OpenGpuiCommandJournal,
    OpenGpuiCommandOrigin, OpenGpuiCommandTicketError, OpenGpuiCompletedCommand,
    OpenGpuiPendingCommandResolution,
};
use crate::port_handle::{OpenGpuiMeasurementError, OpenGpuiPendingMeasurements};

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
            match command.origin {
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
            }
        }
        resolutions
    }
}
