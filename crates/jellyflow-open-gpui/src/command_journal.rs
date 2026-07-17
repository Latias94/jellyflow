use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, Poll, Waker};

use jellyflow::core::PortId;
use jellyflow::runtime::{
    FeedbackCollector, FeedbackCollectorError, FeedbackReceipt, FeedbackSubmissionError,
    GraphCommand, GraphCommandOutcome, GraphRuntimeError, ViewCloseReason, ViewCloseReceipt,
    ViewSessionError,
};
use open_gpui::{App, EntityId};

use crate::connection_controller::OpenGpuiConnectionGeneration;

/// Maximum number of commands owned by one native host across its queue and in-flight batch.
pub const OPEN_GPUI_COMMAND_JOURNAL_CAPACITY: usize = 256;

/// Failure to enqueue a command for the next real native frame.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum OpenGpuiCommandEnqueueError {
    #[error("native command host is no longer available")]
    HostUnavailable,
    #[error("native command host is closing")]
    HostClosing,
    #[error("native command host is closed: {reason:?}")]
    HostClosed { reason: ViewCloseReason },
    #[error("native command journal reached its capacity of {capacity}")]
    QueueFull { capacity: usize },
    #[error("native command ticket identity space is exhausted")]
    IdentityExhausted,
    #[error("native command journal is busy in another callback")]
    Busy,
}

/// Terminal result transport failure for one enqueued native command.
#[derive(Debug, thiserror::Error)]
pub enum OpenGpuiCommandTicketError {
    #[error("native command host was dropped before submission")]
    HostDropped,
    #[error("native command host closed before submission: {reason:?}")]
    HostClosed { reason: ViewCloseReason },
    #[error("native command result is unknown because host close panicked: {reason:?}")]
    ClosePanicked { reason: ViewCloseReason },
    #[error("native command frame preparation failed: {message}")]
    FramePreparation { message: String },
    #[error("native command feedback submission failed")]
    FeedbackSubmission(Rc<FeedbackSubmissionError>),
    #[error(
        "native command outcome is unknown: feedback callback panicked; command may already have been committed"
    )]
    FeedbackOutcomeUnknown,
    #[error("native command session failed")]
    Session(#[from] ViewSessionError),
    #[error("native close command failed")]
    Runtime(#[from] GraphRuntimeError),
    #[error("native command receipt is missing at operation index {operation_index}")]
    MissingReceipt { operation_index: usize },
    #[error("feedback operation {operation_index} is not a command receipt")]
    WrongReceiptKind { operation_index: usize },
}

pub type OpenGpuiCommandTicketResult = Result<GraphCommandOutcome, OpenGpuiCommandTicketError>;

/// One-shot future resolved by the host after its entity borrow has ended.
#[must_use = "command tickets must be awaited or explicitly dropped"]
pub struct OpenGpuiCommandTicket {
    id: u64,
    completion: Rc<CommandCompletion>,
}

impl std::fmt::Debug for OpenGpuiCommandTicket {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenGpuiCommandTicket")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Future for OpenGpuiCommandTicket {
    type Output = OpenGpuiCommandTicketResult;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.completion.poll(context)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OpenGpuiCommandSink {
    journal: Weak<OpenGpuiCommandJournal>,
    host: EntityId,
}

impl OpenGpuiCommandSink {
    pub(crate) fn new(journal: Weak<OpenGpuiCommandJournal>, host: EntityId) -> Self {
        Self { journal, host }
    }

    pub(crate) fn enqueue(
        &self,
        command: GraphCommand,
        cx: &mut App,
    ) -> Result<OpenGpuiCommandTicket, OpenGpuiCommandEnqueueError> {
        let journal = self
            .journal
            .upgrade()
            .ok_or(OpenGpuiCommandEnqueueError::HostUnavailable)?;
        let ticket = journal.enqueue(command, OpenGpuiCommandOrigin::Binding)?;
        cx.notify(self.host);
        Ok(ticket)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum OpenGpuiCommandOrigin {
    Binding,
    NodeDrag,
    CreateFromWire {
        generation: OpenGpuiConnectionGeneration,
    },
    Connection {
        generation: OpenGpuiConnectionGeneration,
        target: PortId,
    },
}

struct CommandCompletion {
    state: RefCell<CommandCompletionState>,
}

enum CommandCompletionState {
    Pending { waker: Option<Waker> },
    Ready(Box<OpenGpuiCommandTicketResult>),
    Consumed,
}

impl CommandCompletion {
    fn new() -> Self {
        Self {
            state: RefCell::new(CommandCompletionState::Pending { waker: None }),
        }
    }

    fn poll(&self, context: &mut Context<'_>) -> Poll<OpenGpuiCommandTicketResult> {
        let mut state = self.state.borrow_mut();
        match std::mem::replace(&mut *state, CommandCompletionState::Consumed) {
            CommandCompletionState::Pending { mut waker } => {
                if waker
                    .as_ref()
                    .is_none_or(|current| !current.will_wake(context.waker()))
                {
                    waker = Some(context.waker().clone());
                }
                *state = CommandCompletionState::Pending { waker };
                Poll::Pending
            }
            CommandCompletionState::Ready(result) => Poll::Ready(*result),
            CommandCompletionState::Consumed => Poll::Pending,
        }
    }

    fn resolve(&self, result: OpenGpuiCommandTicketResult) {
        let waker = {
            let mut state = self.state.borrow_mut();
            match &mut *state {
                CommandCompletionState::Pending { waker } => {
                    let waker = waker.take();
                    *state = CommandCompletionState::Ready(Box::new(result));
                    waker
                }
                CommandCompletionState::Ready(_) | CommandCompletionState::Consumed => return,
            }
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

struct JournalEntry {
    command: GraphCommand,
    origin: OpenGpuiCommandOrigin,
    completion: Option<Rc<CommandCompletion>>,
}

struct InFlightBatch {
    id: u64,
    phase: InFlightPhase,
    entries: Vec<JournalEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InFlightPhase {
    Collected,
    Submitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JournalStatus {
    Open,
    Closing,
    Closed(ViewCloseReason),
}

struct JournalState {
    status: JournalStatus,
    queue: VecDeque<JournalEntry>,
    in_flight: Option<InFlightBatch>,
    next_ticket_id: u64,
    next_batch_id: u64,
}

pub(crate) struct OpenGpuiCommandJournal {
    state: RefCell<JournalState>,
}

impl OpenGpuiCommandJournal {
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self {
            state: RefCell::new(JournalState {
                status: JournalStatus::Open,
                queue: VecDeque::new(),
                in_flight: None,
                next_ticket_id: 0,
                next_batch_id: 0,
            }),
        })
    }

    pub(crate) fn enqueue(
        &self,
        command: GraphCommand,
        origin: OpenGpuiCommandOrigin,
    ) -> Result<OpenGpuiCommandTicket, OpenGpuiCommandEnqueueError> {
        self.enqueue_entry(command, origin, true)?
            .ok_or(OpenGpuiCommandEnqueueError::IdentityExhausted)
    }

    pub(crate) fn enqueue_detached(
        &self,
        command: GraphCommand,
        origin: OpenGpuiCommandOrigin,
    ) -> Result<(), OpenGpuiCommandEnqueueError> {
        self.enqueue_entry(command, origin, false).map(|_| ())
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.state.try_borrow().map_or(true, |state| {
            state.in_flight.is_some() || !state.queue.is_empty()
        })
    }

    fn enqueue_entry(
        &self,
        command: GraphCommand,
        origin: OpenGpuiCommandOrigin,
        with_ticket: bool,
    ) -> Result<Option<OpenGpuiCommandTicket>, OpenGpuiCommandEnqueueError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandEnqueueError::Busy)?;
        match state.status {
            JournalStatus::Open => {}
            JournalStatus::Closing => return Err(OpenGpuiCommandEnqueueError::HostClosing),
            JournalStatus::Closed(reason) => {
                return Err(OpenGpuiCommandEnqueueError::HostClosed { reason });
            }
        }
        let in_flight = state
            .in_flight
            .as_ref()
            .map_or(0, |batch| batch.entries.len());
        if state.queue.len().saturating_add(in_flight) >= OPEN_GPUI_COMMAND_JOURNAL_CAPACITY {
            return Err(OpenGpuiCommandEnqueueError::QueueFull {
                capacity: OPEN_GPUI_COMMAND_JOURNAL_CAPACITY,
            });
        }
        let (id, completion) = if with_ticket {
            let id = state.next_ticket_id;
            state.next_ticket_id = state
                .next_ticket_id
                .checked_add(1)
                .ok_or(OpenGpuiCommandEnqueueError::IdentityExhausted)?;
            (Some(id), Some(Rc::new(CommandCompletion::new())))
        } else {
            (None, None)
        };
        state.queue.push_back(JournalEntry {
            command,
            origin,
            completion: completion.clone(),
        });
        Ok(id
            .zip(completion)
            .map(|(id, completion)| OpenGpuiCommandTicket { id, completion }))
    }

    pub(crate) fn collect_next_batch(
        &self,
        collector: &FeedbackCollector,
    ) -> Result<Option<OpenGpuiCommandBatch>, OpenGpuiCommandCollectError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandCollectError::Busy)?;
        if state.status != JournalStatus::Open
            || state.in_flight.is_some()
            || state.queue.is_empty()
        {
            return Ok(None);
        }
        let next_batch_id = state
            .next_batch_id
            .checked_add(1)
            .ok_or(OpenGpuiCommandCollectError::IdentityExhausted)?;
        for entry in &state.queue {
            collector.command(entry.command.clone())?;
        }
        let id = state.next_batch_id;
        state.next_batch_id = next_batch_id;
        let entries = state.queue.drain(..).collect::<Vec<_>>();
        state.in_flight = Some(InFlightBatch {
            id,
            phase: InFlightPhase::Collected,
            entries,
        });
        Ok(Some(OpenGpuiCommandBatch { id }))
    }

    pub(crate) fn mark_submitted(
        &self,
        batch: OpenGpuiCommandBatch,
    ) -> Result<(), OpenGpuiCommandBatchStateError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandBatchStateError::Busy)?;
        let in_flight = state
            .in_flight
            .as_mut()
            .filter(|current| current.id == batch.id)
            .ok_or(OpenGpuiCommandBatchStateError::UnknownBatch)?;
        if in_flight.phase != InFlightPhase::Collected {
            return Err(OpenGpuiCommandBatchStateError::AlreadySubmitted);
        }
        in_flight.phase = InFlightPhase::Submitted;
        Ok(())
    }

    pub(crate) fn complete_submitted(
        &self,
        batch: OpenGpuiCommandBatch,
        receipt: FeedbackReceipt,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError> {
        let entries = self.take_batch(batch, InFlightPhase::Submitted)?;
        let mut operations = receipt.into_operations().into_iter();
        Ok(entries
            .into_iter()
            .enumerate()
            .map(|(operation_index, entry)| {
                let result = match operations.next() {
                    Some(operation) => operation.into_command_outcome().map_or_else(
                        || Err(OpenGpuiCommandTicketError::WrongReceiptKind { operation_index }),
                        |outcome| outcome.map_err(OpenGpuiCommandTicketError::Session),
                    ),
                    None => Err(OpenGpuiCommandTicketError::MissingReceipt { operation_index }),
                };
                OpenGpuiCompletedCommand::new(entry, result)
            })
            .collect())
    }

    pub(crate) fn fail_batch(
        &self,
        batch: OpenGpuiCommandBatch,
        mut error: impl FnMut() -> OpenGpuiCommandTicketError,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError> {
        let entries = self.take_batch_any_phase(batch)?;
        Ok(entries
            .into_iter()
            .map(|entry| OpenGpuiCompletedCommand::new(entry, Err(error())))
            .collect())
    }

    pub(crate) fn begin_close(
        &self,
    ) -> Result<OpenGpuiCommandCloseDrain, OpenGpuiCommandCloseError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandCloseError::Busy)?;
        match state.status {
            JournalStatus::Open => {}
            JournalStatus::Closing => return Err(OpenGpuiCommandCloseError::AlreadyClosing),
            JournalStatus::Closed(reason) => {
                return Err(OpenGpuiCommandCloseError::Closed { reason });
            }
        }
        if state
            .in_flight
            .as_ref()
            .is_some_and(|batch| batch.phase == InFlightPhase::Submitted)
        {
            return Err(OpenGpuiCommandCloseError::BatchSubmitting);
        }
        state.status = JournalStatus::Closing;
        Ok(OpenGpuiCommandCloseDrain {
            collected: state.in_flight.take(),
            queued: std::mem::take(&mut state.queue),
        })
    }

    pub(crate) fn abort_close(
        &self,
        drain: OpenGpuiCommandCloseDrain,
    ) -> Result<(), OpenGpuiCommandCloseError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandCloseError::Busy)?;
        if state.status != JournalStatus::Closing
            || state.in_flight.is_some()
            || !state.queue.is_empty()
        {
            return Err(OpenGpuiCommandCloseError::InvalidState);
        }
        state.status = JournalStatus::Open;
        state.in_flight = drain.collected;
        state.queue = drain.queued;
        Ok(())
    }

    pub(crate) fn finish_close(
        &self,
        drain: OpenGpuiCommandCloseDrain,
        reason: ViewCloseReason,
        receipts: Vec<ViewCloseReceipt>,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandCloseError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandCloseError::Busy)?;
        if state.status != JournalStatus::Closing {
            return Err(OpenGpuiCommandCloseError::InvalidState);
        }
        state.status = JournalStatus::Closed(reason);
        let entries = drain.into_entries();
        let mut receipts = receipts.into_iter();
        let completed = entries
            .into_iter()
            .enumerate()
            .map(|(operation_index, entry)| {
                let result = receipts.next().map_or_else(
                    || Err(OpenGpuiCommandTicketError::MissingReceipt { operation_index }),
                    |receipt| {
                        receipt
                            .into_command_result()
                            .map_err(OpenGpuiCommandTicketError::Runtime)
                    },
                );
                OpenGpuiCompletedCommand::new(entry, result)
            })
            .collect();
        Ok(completed)
    }

    pub(crate) fn finish_close_without_execution(
        &self,
        drain: OpenGpuiCommandCloseDrain,
        reason: ViewCloseReason,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandCloseError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandCloseError::Busy)?;
        if state.status != JournalStatus::Closing {
            return Err(OpenGpuiCommandCloseError::InvalidState);
        }
        state.status = JournalStatus::Closed(reason);
        Ok(drain
            .into_entries()
            .into_iter()
            .map(|entry| {
                OpenGpuiCompletedCommand::new(
                    entry,
                    Err(OpenGpuiCommandTicketError::HostClosed { reason }),
                )
            })
            .collect())
    }

    pub(crate) fn finish_close_after_panic(
        &self,
        drain: OpenGpuiCommandCloseDrain,
        reason: ViewCloseReason,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandCloseError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandCloseError::Busy)?;
        if state.status != JournalStatus::Closing {
            return Err(OpenGpuiCommandCloseError::InvalidState);
        }
        state.status = JournalStatus::Closed(reason);
        Ok(drain
            .into_entries()
            .into_iter()
            .map(|entry| {
                OpenGpuiCompletedCommand::new(
                    entry,
                    Err(OpenGpuiCommandTicketError::ClosePanicked { reason }),
                )
            })
            .collect())
    }

    pub(crate) fn terminate(&self, reason: ViewCloseReason) -> Vec<OpenGpuiCompletedCommand> {
        let Ok(mut state) = self.state.try_borrow_mut() else {
            return Vec::new();
        };
        if matches!(state.status, JournalStatus::Closed(_)) {
            return Vec::new();
        }
        state.status = JournalStatus::Closed(reason);
        let entries = state
            .in_flight
            .take()
            .into_iter()
            .flat_map(|batch| batch.entries)
            .chain(state.queue.drain(..))
            .collect::<Vec<_>>();
        entries
            .into_iter()
            .map(|entry| {
                OpenGpuiCompletedCommand::new(
                    entry,
                    Err(OpenGpuiCommandTicketError::HostClosed { reason }),
                )
            })
            .collect()
    }

    pub(crate) fn terminate_dropped(&self) -> Vec<OpenGpuiPendingCommandResolution> {
        let Ok(mut state) = self.state.try_borrow_mut() else {
            return Vec::new();
        };
        let entries = state
            .in_flight
            .take()
            .into_iter()
            .flat_map(|batch| batch.entries)
            .chain(state.queue.drain(..))
            .collect::<Vec<_>>();
        entries
            .into_iter()
            .filter_map(|entry| {
                OpenGpuiCompletedCommand::new(entry, Err(OpenGpuiCommandTicketError::HostDropped))
                    .into_resolution()
            })
            .collect()
    }

    fn take_batch(
        &self,
        batch: OpenGpuiCommandBatch,
        phase: InFlightPhase,
    ) -> Result<Vec<JournalEntry>, OpenGpuiCommandBatchStateError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandBatchStateError::Busy)?;
        if !state
            .in_flight
            .as_ref()
            .is_some_and(|current| current.id == batch.id && current.phase == phase)
        {
            return Err(OpenGpuiCommandBatchStateError::UnknownBatch);
        }
        Ok(state
            .in_flight
            .take()
            .map(|batch| batch.entries)
            .unwrap_or_default())
    }

    fn take_batch_any_phase(
        &self,
        batch: OpenGpuiCommandBatch,
    ) -> Result<Vec<JournalEntry>, OpenGpuiCommandBatchStateError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandBatchStateError::Busy)?;
        if !state
            .in_flight
            .as_ref()
            .is_some_and(|current| current.id == batch.id)
        {
            return Err(OpenGpuiCommandBatchStateError::UnknownBatch);
        }
        Ok(state
            .in_flight
            .take()
            .map(|batch| batch.entries)
            .unwrap_or_default())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OpenGpuiCommandBatch {
    id: u64,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum OpenGpuiCommandCollectError {
    #[error("native command journal is busy in another callback")]
    Busy,
    #[error(transparent)]
    Collector(#[from] FeedbackCollectorError),
    #[error("native command batch identity space is exhausted")]
    IdentityExhausted,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum OpenGpuiCommandBatchStateError {
    #[error("native command journal is busy in another callback")]
    Busy,
    #[error("native command batch is unknown or in the wrong phase")]
    UnknownBatch,
    #[error("native command batch was already submitted")]
    AlreadySubmitted,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum OpenGpuiCommandCloseError {
    #[error("native command journal is busy in another callback")]
    Busy,
    #[error("native command journal is already closing")]
    AlreadyClosing,
    #[error("native command journal is closed: {reason:?}")]
    Closed { reason: ViewCloseReason },
    #[error("a command-bearing feedback batch is currently submitting")]
    BatchSubmitting,
    #[error("native command journal close state is invalid")]
    InvalidState,
}

pub(crate) struct OpenGpuiCommandCloseDrain {
    collected: Option<InFlightBatch>,
    queued: VecDeque<JournalEntry>,
}

impl OpenGpuiCommandCloseDrain {
    pub(crate) fn len(&self) -> usize {
        self.collected
            .as_ref()
            .map_or(0, |batch| batch.entries.len())
            .saturating_add(self.queued.len())
    }

    pub(crate) fn combined_request(
        &self,
        caller: &jellyflow::runtime::ViewCloseRequest,
    ) -> jellyflow::runtime::ViewCloseRequest {
        let mut request = jellyflow::runtime::ViewCloseRequest::new();
        for entry in self
            .collected
            .iter()
            .flat_map(|batch| batch.entries.iter())
            .chain(self.queued.iter())
        {
            request = request.with_command(entry.command.clone());
        }
        for command in caller.commands() {
            request = request.with_command(command.clone());
        }
        request
    }

    fn into_entries(self) -> Vec<JournalEntry> {
        self.collected
            .into_iter()
            .flat_map(|batch| batch.entries)
            .chain(self.queued)
            .collect()
    }
}

pub(crate) struct OpenGpuiCompletedCommand {
    pub(crate) origin: OpenGpuiCommandOrigin,
    pub(crate) result: OpenGpuiCommandTicketResult,
    completion: Option<Rc<CommandCompletion>>,
}

impl OpenGpuiCompletedCommand {
    fn new(entry: JournalEntry, result: OpenGpuiCommandTicketResult) -> Self {
        Self {
            origin: entry.origin,
            result,
            completion: entry.completion,
        }
    }

    pub(crate) fn into_resolution(self) -> Option<OpenGpuiPendingCommandResolution> {
        self.completion
            .map(|completion| OpenGpuiPendingCommandResolution {
                completion,
                result: self.result,
            })
    }

    pub(crate) fn into_detached_result(
        self,
    ) -> Result<OpenGpuiCommandTicketResult, Box<OpenGpuiCompletedCommand>> {
        if self.completion.is_none() {
            Ok(self.result)
        } else {
            Err(Box::new(self))
        }
    }
}

pub(crate) struct OpenGpuiPendingCommandResolution {
    completion: Rc<CommandCompletion>,
    result: OpenGpuiCommandTicketResult,
}

impl OpenGpuiPendingCommandResolution {
    pub(crate) fn resolve(self) {
        self.completion.resolve(self.result);
    }
}

pub(crate) fn resolve_command_tickets(
    resolutions: impl IntoIterator<Item = OpenGpuiPendingCommandResolution>,
) {
    for resolution in resolutions {
        resolution.resolve();
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use jellyflow::core::{
        CanvasPoint, CanvasRect, CanvasSize, EdgeId, GraphBuilder, GraphId, GroupId, Node, NodeId,
        NodeKindKey,
    };
    use jellyflow::runtime::schema::{NodeCatalog, NodeKindSpec};
    use jellyflow::runtime::{
        FrameRequest, GraphRuntime, RendererStamp, ViewCloseRequest, ViewSession,
        ViewSessionOptions,
    };

    use super::*;

    const KIND: &str = "test.open-gpui-command-journal";
    const RENDERER: &str = "test.open-gpui.command-journal";

    #[test]
    fn rejects_the_newest_command_when_queue_capacity_is_reached() {
        let journal = OpenGpuiCommandJournal::new();
        let mut tickets = Vec::with_capacity(OPEN_GPUI_COMMAND_JOURNAL_CAPACITY);

        for index in 0..OPEN_GPUI_COMMAND_JOURNAL_CAPACITY {
            tickets.push(
                journal
                    .enqueue(command(0, index), OpenGpuiCommandOrigin::Binding)
                    .expect("command within capacity"),
            );
        }

        assert!(matches!(
            journal.enqueue(
                command(0, OPEN_GPUI_COMMAND_JOURNAL_CAPACITY),
                OpenGpuiCommandOrigin::Binding,
            ),
            Err(OpenGpuiCommandEnqueueError::QueueFull { capacity })
                if capacity == OPEN_GPUI_COMMAND_JOURNAL_CAPACITY
        ));
        assert_eq!(tickets.len(), OPEN_GPUI_COMMAND_JOURNAL_CAPACITY);
    }

    #[test]
    fn in_flight_entries_count_toward_capacity_and_only_one_batch_is_collected() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        for index in 0..OPEN_GPUI_COMMAND_JOURNAL_CAPACITY {
            journal
                .enqueue_detached(command(0, index), OpenGpuiCommandOrigin::NodeDrag)
                .expect("command within capacity");
        }
        let first_collector = collector(&view);
        let first_batch = journal
            .collect_next_batch(&first_collector)
            .expect("collect batch")
            .expect("nonempty batch");

        assert!(matches!(
            journal.enqueue_detached(command(0, usize::MAX), OpenGpuiCommandOrigin::NodeDrag),
            Err(OpenGpuiCommandEnqueueError::QueueFull { .. })
        ));
        assert!(
            journal
                .collect_next_batch(&collector(&view))
                .expect("in-flight collection is benign")
                .is_none()
        );

        let completed = journal
            .fail_batch(first_batch, || {
                OpenGpuiCommandTicketError::FramePreparation {
                    message: "test releases the batch".to_owned(),
                }
            })
            .expect("release collected batch");
        assert_eq!(completed.len(), OPEN_GPUI_COMMAND_JOURNAL_CAPACITY);
        journal
            .enqueue_detached(command(0, usize::MAX), OpenGpuiCommandOrigin::NodeDrag)
            .expect("capacity is released with the batch");
        assert!(
            journal
                .collect_next_batch(&collector(&view))
                .expect("collect replacement batch")
                .is_some()
        );
    }

    #[test]
    fn submitted_receipts_resolve_each_ticket_by_exact_operation_index() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut applied_ticket = journal
            .enqueue(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("first ticket");
        let mut conflict_ticket = journal
            .enqueue(command(0, 2), OpenGpuiCommandOrigin::Binding)
            .expect("second ticket");
        let collector = collector(&view);
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect commands")
            .expect("command batch");
        journal.mark_submitted(batch).expect("submitted batch");
        let receipt = view
            .submit_feedback(collector.finish().expect("finish feedback"))
            .expect("submit feedback");
        let completed = journal
            .complete_submitted(batch, receipt)
            .expect("pair command receipts");
        resolve_completed(completed);

        let applied = ready_ticket(&mut applied_ticket).expect("first ticket transport");
        assert!(applied.applied().is_some());
        let conflict = ready_ticket(&mut conflict_ticket).expect("second ticket transport");
        assert!(conflict.conflict().is_some());
    }

    #[test]
    fn wrong_receipt_kind_fails_only_the_ticket_at_that_operation_index() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut first_ticket = journal
            .enqueue(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("first ticket");
        let mut second_ticket = journal
            .enqueue(command(0, 2), OpenGpuiCommandOrigin::Binding)
            .expect("second ticket");
        let collector = collector(&view);
        collector
            .selection(
                std::iter::empty::<NodeId>(),
                std::iter::empty::<EdgeId>(),
                std::iter::empty::<GroupId>(),
            )
            .expect("inject malformed operation prefix");
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect commands")
            .expect("command batch");
        journal.mark_submitted(batch).expect("submitted batch");
        let receipt = view
            .submit_feedback(collector.finish().expect("finish feedback"))
            .expect("submit feedback");
        let completed = journal
            .complete_submitted(batch, receipt)
            .expect("complete malformed receipt sequence");
        resolve_completed(completed);

        assert!(matches!(
            ready_ticket(&mut first_ticket),
            Err(OpenGpuiCommandTicketError::WrongReceiptKind { operation_index: 0 })
        ));
        assert!(
            ready_ticket(&mut second_ticket)
                .expect("unaffected ticket transport")
                .applied()
                .is_some()
        );
    }

    #[test]
    fn close_drains_collected_then_queued_commands_before_caller_commands() {
        let (runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut collected_ticket = journal
            .enqueue(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("collected ticket");
        let batch_collector = collector(&view);
        journal
            .collect_next_batch(&batch_collector)
            .expect("collect first close command")
            .expect("collected close batch");
        let mut queued_ticket = journal
            .enqueue(command(1, 2), OpenGpuiCommandOrigin::Binding)
            .expect("queued ticket");
        let drain = journal.begin_close().expect("begin close drain");
        let journal_count = drain.len();
        assert_eq!(journal_count, 2);
        let request = drain.combined_request(&ViewCloseRequest::new().with_command(command(2, 3)));
        let outcome = view.close_with_request(request).expect("close view");
        let reason = outcome.reason();
        let (journal_receipts, caller_outcome) = outcome.split_receipt_prefix(journal_count);
        let completed = journal
            .finish_close(drain, reason, journal_receipts)
            .expect("finish journal close");
        resolve_completed(completed);

        assert_eq!(
            ready_ticket(&mut collected_ticket)
                .expect("collected command transport")
                .applied()
                .expect("collected command applied")
                .revision(),
            1
        );
        assert_eq!(
            ready_ticket(&mut queued_ticket)
                .expect("queued command transport")
                .applied()
                .expect("queued command applied")
                .revision(),
            2
        );
        let (_, caller_receipts) = caller_outcome.into_parts();
        assert_eq!(caller_receipts.len(), 1);
        assert_eq!(
            caller_receipts
                .into_iter()
                .next()
                .expect("caller receipt")
                .into_command_result()
                .expect("caller command transport")
                .applied()
                .expect("caller command applied")
                .revision(),
            3
        );
        assert_eq!(
            runtime
                .snapshot()
                .expect("final snapshot")
                .graph()
                .node(&node_id())
                .expect("journal node")
                .data,
            serde_json::json!({ "value": 3 })
        );
    }

    #[test]
    fn close_rejects_a_submitted_batch_without_retrying_it() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        journal
            .enqueue_detached(command(0, 1), OpenGpuiCommandOrigin::NodeDrag)
            .expect("detached command");
        let collector = collector(&view);
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect batch")
            .expect("command batch");
        journal.mark_submitted(batch).expect("submit batch");

        assert!(matches!(
            journal.begin_close(),
            Err(OpenGpuiCommandCloseError::BatchSubmitting)
        ));
        assert!(journal.has_pending());
    }

    #[test]
    fn dropping_the_host_resolution_cancels_every_pending_ticket() {
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("pending ticket");

        assert!(matches!(poll_ticket(&mut ticket), Poll::Pending));
        resolve_command_tickets(journal.terminate_dropped());
        assert!(matches!(
            ready_ticket(&mut ticket),
            Err(OpenGpuiCommandTicketError::HostDropped)
        ));
    }

    fn resolve_completed(completed: Vec<OpenGpuiCompletedCommand>) {
        resolve_command_tickets(
            completed
                .into_iter()
                .filter_map(OpenGpuiCompletedCommand::into_resolution),
        );
    }

    fn ready_ticket(ticket: &mut OpenGpuiCommandTicket) -> OpenGpuiCommandTicketResult {
        match poll_ticket(ticket) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("native command ticket is still pending"),
        }
    }

    fn poll_ticket(ticket: &mut OpenGpuiCommandTicket) -> Poll<OpenGpuiCommandTicketResult> {
        let mut context = Context::from_waker(Waker::noop());
        Future::poll(Pin::new(ticket), &mut context)
    }

    fn runtime_and_view() -> (GraphRuntime, ViewSession) {
        let mut graph = GraphBuilder::new(GraphId::from_u128(0xc001));
        graph.insert_node(
            node_id(),
            Node {
                kind: NodeKindKey::new(KIND),
                kind_version: 1,
                pos: CanvasPoint { x: 20.0, y: 20.0 },
                origin: None,
                selectable: None,
                draggable: None,
                connectable: None,
                deletable: None,
                parent: None,
                extent: None,
                expand_parent: None,
                size: Some(CanvasSize {
                    width: 180.0,
                    height: 100.0,
                }),
                hidden: false,
                collapsed: false,
                ports: Vec::new(),
                data: serde_json::Value::Null,
            },
        );
        let mut catalog = NodeCatalog::new();
        catalog
            .register(
                NodeKindSpec::builder(KIND, "Command journal node", RENDERER)
                    .default_size(CanvasSize {
                        width: 180.0,
                        height: 100.0,
                    })
                    .build(),
            )
            .expect("command journal kind");
        let runtime = GraphRuntime::builder(catalog)
            .build(graph.build().expect("command journal graph"))
            .expect("command journal runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("command journal view");
        (runtime, view)
    }

    fn collector(view: &ViewSession) -> FeedbackCollector {
        view.frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 640.0,
                    height: 480.0,
                },
            },
            RendererStamp::new(RENDERER, 1),
        ))
        .expect("command journal frame")
        .collector()
    }

    fn command(expected_revision: u64, value: usize) -> GraphCommand {
        GraphCommand::set_node_data(
            expected_revision,
            node_id(),
            serde_json::json!({ "value": value }),
        )
    }

    fn node_id() -> NodeId {
        NodeId::from_u128(0xc002)
    }
}
