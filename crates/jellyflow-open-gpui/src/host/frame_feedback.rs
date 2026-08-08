use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use jellyflow_core::NodeId;
use jellyflow_runtime::{
    view::FeedbackCollector, view::FeedbackCollectorError, view::FeedbackReceipt,
    view::FeedbackSubmissionError, view::LayoutNodeStatus, view::ViewSession,
};

use crate::command_journal::{
    OpenGpuiCommandBatch, OpenGpuiCommandBatchStateError, OpenGpuiCommandJournal,
    OpenGpuiCommandOrigin, OpenGpuiCommandTicketError, OpenGpuiCommandTicketFailure,
    OpenGpuiCompletedCommand, OpenGpuiPendingCommandResolution, resolve_command_tickets,
};
use crate::port_handle::{OpenGpuiMeasurementError, OpenGpuiPendingMeasurements};

use super::OpenGpuiSceneHost;
use super::interaction_session::NodeDragRouteHandoffToken;

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
    pub(super) stale_content_nodes: BTreeSet<jellyflow_core::NodeId>,
    pub(super) rejected_layouts: Vec<(NodeId, LayoutNodeStatus)>,
}

pub(super) struct OpenGpuiFrameTransaction {
    state: RefCell<Option<OpenGpuiFrameTransactionState>>,
    measurements: Rc<OpenGpuiPendingMeasurements>,
    commands: Rc<OpenGpuiCommandJournal>,
    scheduled: Cell<bool>,
    node_drag_route_handoff: Cell<Option<NodeDragRouteHandoffToken>>,
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
            node_drag_route_handoff: Cell::new(None),
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

    pub(super) fn mark_node_drag_route_handoff(&self, token: NodeDragRouteHandoffToken) {
        self.node_drag_route_handoff.set(Some(token));
    }

    pub(super) fn node_drag_route_handoff(&self) -> Option<NodeDragRouteHandoffToken> {
        self.node_drag_route_handoff.get()
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
                    OpenGpuiCommandTicketFailure::definitely_not_executed(
                        OpenGpuiCommandTicketError::FramePreparation {
                            message: message.clone(),
                        },
                    )
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
                let execution_unknown = matches!(
                    &error,
                    FeedbackSubmissionError::AfterCommandAdmission { .. }
                );
                let error = Rc::new(error);
                let (commands, command_pairing_error) = match post_admission {
                    Some(receipt) => state.complete_commands(&self.commands, receipt),
                    None => (
                        state.fail_commands(&self.commands, || {
                            let error =
                                OpenGpuiCommandTicketError::FeedbackSubmission(Rc::clone(&error));
                            if execution_unknown {
                                OpenGpuiCommandTicketFailure::execution_unknown(error)
                            } else {
                                OpenGpuiCommandTicketFailure::definitely_not_executed(error)
                            }
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
                        OpenGpuiCommandTicketFailure::execution_unknown(
                            OpenGpuiCommandTicketError::FeedbackOutcomeUnknown,
                        )
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
            OpenGpuiCommandTicketFailure::definitely_not_executed(
                OpenGpuiCommandTicketError::FramePreparation {
                    message: message.clone(),
                },
            )
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
                let completed = commands.fail_batch(batch, || {
                    OpenGpuiCommandTicketFailure::execution_unknown(
                        OpenGpuiCommandTicketError::FeedbackOutcomeUnknown,
                    )
                });
                (completed, Some(message))
            }
        }
    }

    fn fail_commands(
        &mut self,
        commands: &OpenGpuiCommandJournal,
        failure: impl FnMut() -> OpenGpuiCommandTicketFailure,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError> {
        self.command_batch.take().map_or_else(
            || Ok(Vec::new()),
            |batch| commands.fail_batch(batch, failure),
        )
    }
}

impl Drop for OpenGpuiFrameTransaction {
    fn drop(&mut self) {
        let Some(mut state) = self.state.get_mut().take() else {
            return;
        };
        let Some(batch) = state.command_batch.take() else {
            return;
        };
        let Ok(completed) = self
            .commands
            .fail_batch_from_phase(batch, || OpenGpuiCommandTicketError::HostDropped)
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
        stale_content_nodes: BTreeSet::new(),
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
            LayoutReceiptAction::Refresh => {
                disposition.stale_content_nodes.insert(layout.node());
            }
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
                self.discard_revocable_interaction_submission();
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
        errors.extend(self.retained_nodes.take_active_measurement_errors());
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
                                match error {
                                    OpenGpuiCommandTicketFailure::DefinitelyNotExecuted(_) => {
                                        self.interaction_submission_not_dispatched(ticket);
                                    }
                                    OpenGpuiCommandTicketFailure::ExecutionUnknown(_) => {
                                        self.interaction_submission_outcome_unknown(ticket);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        resolutions
    }
}
