use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, Poll, Waker};

use jellyflow::runtime::{
    FeedbackCollector, FeedbackCollectorError, FeedbackReceipt, FeedbackSubmissionError,
    GraphCommand, GraphCommandOutcome, GraphRuntimeError, ViewCloseReason, ViewCloseReceipt,
    ViewSessionError,
};
use open_gpui::{App, EntityId};

use jellyflow::runtime::InteractionTicketId;

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
    #[error("native command prefix reservation is no longer active")]
    PrefixReservationInactive,
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

/// Terminal reason why a captured command prefix cannot be followed by another mutation.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub(crate) enum OpenGpuiCommandPrefixBarrierError {
    #[error("native command journal is busy in another callback")]
    Busy,
    #[error("native command host is closing")]
    HostClosing,
    #[error("native command host closed before the command prefix settled: {reason:?}")]
    HostClosed { reason: ViewCloseReason },
    #[error("native command prefix outcome is unknown because host close panicked: {reason:?}")]
    ClosePanicked { reason: ViewCloseReason },
    #[error("native command host was dropped before the command prefix settled")]
    HostDropped,
    #[error("native command prefix reservation identity space is exhausted")]
    IdentityExhausted,
    #[error("native command journal reached its capacity of {capacity}")]
    QueueFull { capacity: usize },
    #[error("native command prefix reservation is no longer active")]
    ReservationInactive,
}

type PrefixBarrierCompletionResult = Result<(), OpenGpuiCommandPrefixBarrierError>;
pub(crate) type OpenGpuiCommandPrefixBarrierResult =
    Result<OpenGpuiCommandPrefixPermit, OpenGpuiCommandPrefixBarrierError>;

/// A frozen boundary over commands queued or in flight when the barrier was captured.
#[must_use = "command prefix barriers must be awaited or explicitly dropped"]
pub(crate) struct OpenGpuiCommandPrefixBarrier {
    target_exclusive: u64,
    reservation: Rc<PrefixReservation>,
    completion: Rc<PrefixBarrierCompletion>,
}

struct PrefixReservation {
    sequence: Cell<Option<u64>>,
    journal: Weak<OpenGpuiCommandJournal>,
}

impl PrefixReservation {
    fn take(&self) -> Option<u64> {
        self.sequence.take()
    }

    fn abandon(&self) {
        let Some(sequence) = self.take() else {
            return;
        };
        if let Some(journal) = self.journal.upgrade() {
            journal.abandon_prefix_reservation(sequence);
        }
    }
}

pub(crate) struct OpenGpuiCommandPrefixBarrierCancellation {
    reservation: Rc<PrefixReservation>,
}

impl Drop for OpenGpuiCommandPrefixBarrierCancellation {
    fn drop(&mut self) {
        self.reservation.abandon();
    }
}

impl std::fmt::Debug for OpenGpuiCommandPrefixBarrier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenGpuiCommandPrefixBarrier")
            .field("target_exclusive", &self.target_exclusive)
            .finish_non_exhaustive()
    }
}

impl OpenGpuiCommandPrefixBarrier {
    pub(crate) fn cancellation(&self) -> OpenGpuiCommandPrefixBarrierCancellation {
        OpenGpuiCommandPrefixBarrierCancellation {
            reservation: Rc::clone(&self.reservation),
        }
    }
}

impl Future for OpenGpuiCommandPrefixBarrier {
    type Output = OpenGpuiCommandPrefixBarrierResult;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let barrier = self.get_mut();
        match barrier.completion.poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => {
                let Some(sequence) = barrier.reservation.take() else {
                    return Poll::Ready(Err(
                        OpenGpuiCommandPrefixBarrierError::ReservationInactive,
                    ));
                };
                Poll::Ready(Ok(OpenGpuiCommandPrefixPermit {
                    sequence: Some(sequence),
                    journal: barrier.reservation.journal.clone(),
                }))
            }
            Poll::Ready(Err(error)) => {
                barrier.reservation.take();
                Poll::Ready(Err(error))
            }
        }
    }
}

impl Drop for OpenGpuiCommandPrefixBarrier {
    fn drop(&mut self) {
        self.reservation.abandon();
    }
}

/// A one-shot insertion right after a settled command prefix and before its suffix.
#[must_use = "command prefix permits must enqueue one command or be explicitly abandoned"]
pub(crate) struct OpenGpuiCommandPrefixPermit {
    sequence: Option<u64>,
    journal: Weak<OpenGpuiCommandJournal>,
}

impl std::fmt::Debug for OpenGpuiCommandPrefixPermit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenGpuiCommandPrefixPermit")
            .field("sequence", &self.sequence)
            .finish_non_exhaustive()
    }
}

impl OpenGpuiCommandPrefixPermit {
    #[cfg(test)]
    pub(crate) fn enqueue(
        mut self,
        command: GraphCommand,
        origin: OpenGpuiCommandOrigin,
    ) -> Result<OpenGpuiCommandTicket, OpenGpuiCommandEnqueueError> {
        let sequence = self
            .sequence
            .ok_or(OpenGpuiCommandEnqueueError::PrefixReservationInactive)?;
        let journal = self
            .journal
            .upgrade()
            .ok_or(OpenGpuiCommandEnqueueError::HostUnavailable)?;
        let ticket = journal.enqueue_reserved(sequence, command, origin, true)?;
        self.sequence = None;
        ticket.ok_or(OpenGpuiCommandEnqueueError::IdentityExhausted)
    }

    pub(crate) fn enqueue_detached(
        mut self,
        command: GraphCommand,
        origin: OpenGpuiCommandOrigin,
    ) -> Result<(), OpenGpuiCommandEnqueueError> {
        let sequence = self
            .sequence
            .ok_or(OpenGpuiCommandEnqueueError::PrefixReservationInactive)?;
        let journal = self
            .journal
            .upgrade()
            .ok_or(OpenGpuiCommandEnqueueError::HostUnavailable)?;
        journal.enqueue_reserved(sequence, command, origin, false)?;
        self.sequence = None;
        Ok(())
    }

    pub(crate) fn abandon(self) {
        drop(self);
    }
}

impl Drop for OpenGpuiCommandPrefixPermit {
    fn drop(&mut self) {
        let Some(sequence) = self.sequence.take() else {
            return;
        };
        if let Some(journal) = self.journal.upgrade() {
            journal.abandon_prefix_reservation(sequence);
        }
    }
}

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
    Interaction { ticket: InteractionTicketId },
}

struct OneShotCompletion<T> {
    state: RefCell<OneShotCompletionState<T>>,
}

enum OneShotCompletionState<T> {
    Pending { waker: Option<Waker> },
    Ready(Box<T>),
    Consumed,
}

impl<T> OneShotCompletion<T> {
    fn new() -> Self {
        Self {
            state: RefCell::new(OneShotCompletionState::Pending { waker: None }),
        }
    }

    fn poll(&self, context: &mut Context<'_>) -> Poll<T> {
        let mut state = self.state.borrow_mut();
        match std::mem::replace(&mut *state, OneShotCompletionState::Consumed) {
            OneShotCompletionState::Pending { mut waker } => {
                if waker
                    .as_ref()
                    .is_none_or(|current| !current.will_wake(context.waker()))
                {
                    waker = Some(context.waker().clone());
                }
                *state = OneShotCompletionState::Pending { waker };
                Poll::Pending
            }
            OneShotCompletionState::Ready(result) => Poll::Ready(*result),
            OneShotCompletionState::Consumed => Poll::Pending,
        }
    }

    fn resolve(&self, result: T) {
        let waker = {
            let mut state = self.state.borrow_mut();
            match &mut *state {
                OneShotCompletionState::Pending { waker } => {
                    let waker = waker.take();
                    *state = OneShotCompletionState::Ready(Box::new(result));
                    waker
                }
                OneShotCompletionState::Ready(_) | OneShotCompletionState::Consumed => return,
            }
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

type CommandCompletion = OneShotCompletion<OpenGpuiCommandTicketResult>;
type PrefixBarrierCompletion = OneShotCompletion<PrefixBarrierCompletionResult>;

struct JournalEntry {
    sequence: u64,
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
    Dropped,
}

struct PrefixBarrierRegistration {
    target_exclusive: u64,
    reservation_sequence: u64,
    completion: Weak<PrefixBarrierCompletion>,
}

struct JournalState {
    status: JournalStatus,
    queue: VecDeque<JournalEntry>,
    in_flight: Option<InFlightBatch>,
    next_ticket_id: u64,
    next_batch_id: u64,
    next_entry_sequence: u64,
    terminal_prefix_exclusive: u64,
    terminal_sequences: BTreeSet<u64>,
    prefix_reservations: BTreeSet<u64>,
    prefix_barriers: Vec<PrefixBarrierRegistration>,
}

impl JournalState {
    fn advance_terminal_prefix(
        &mut self,
        entries: &[JournalEntry],
    ) -> Vec<Rc<PrefixBarrierCompletion>> {
        for entry in entries {
            self.mark_sequence_terminal(entry.sequence);
        }
        self.take_ready_prefix_barriers()
    }

    fn mark_sequence_terminal(&mut self, sequence: u64) {
        if sequence >= self.terminal_prefix_exclusive {
            self.terminal_sequences.insert(sequence);
        }
        while self
            .terminal_sequences
            .remove(&self.terminal_prefix_exclusive)
        {
            self.terminal_prefix_exclusive = self.terminal_prefix_exclusive.saturating_add(1);
        }
    }

    fn take_ready_prefix_barriers(&mut self) -> Vec<Rc<PrefixBarrierCompletion>> {
        let terminal_prefix_exclusive = self.terminal_prefix_exclusive;
        self.take_prefix_barriers(|target_exclusive| target_exclusive <= terminal_prefix_exclusive)
    }

    fn take_all_prefix_barriers(&mut self) -> Vec<Rc<PrefixBarrierCompletion>> {
        self.take_prefix_barriers(|_| true)
    }

    fn terminate_prefixes(&mut self) -> Vec<Rc<PrefixBarrierCompletion>> {
        self.terminal_prefix_exclusive = self.next_entry_sequence;
        self.terminal_sequences.clear();
        self.prefix_reservations.clear();
        self.take_all_prefix_barriers()
    }

    fn take_prefix_barriers(
        &mut self,
        mut should_take: impl FnMut(u64) -> bool,
    ) -> Vec<Rc<PrefixBarrierCompletion>> {
        let registrations = std::mem::take(&mut self.prefix_barriers);
        let mut retained = Vec::with_capacity(registrations.len());
        let mut taken = Vec::new();
        for registration in registrations {
            let Some(completion) = registration.completion.upgrade() else {
                continue;
            };
            if should_take(registration.target_exclusive) {
                taken.push(completion);
            } else {
                retained.push(registration);
            }
        }
        self.prefix_barriers = retained;
        taken
    }

    fn remove_prefix_barrier(&mut self, reservation_sequence: u64) {
        self.prefix_barriers
            .retain(|barrier| barrier.reservation_sequence != reservation_sequence);
    }
}

fn resolve_prefix_barriers(
    barriers: Vec<Rc<PrefixBarrierCompletion>>,
    result: PrefixBarrierCompletionResult,
) {
    for barrier in barriers {
        barrier.resolve(result.clone());
    }
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
                next_entry_sequence: 0,
                terminal_prefix_exclusive: 0,
                terminal_sequences: BTreeSet::new(),
                prefix_reservations: BTreeSet::new(),
                prefix_barriers: Vec::new(),
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

    #[cfg(test)]
    pub(crate) fn enqueue_detached(
        &self,
        command: GraphCommand,
        origin: OpenGpuiCommandOrigin,
    ) -> Result<(), OpenGpuiCommandEnqueueError> {
        self.enqueue_entry(command, origin, false).map(|_| ())
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.state.try_borrow().map_or(true, |state| {
            state.in_flight.is_some()
                || !state.queue.is_empty()
                || !state.prefix_reservations.is_empty()
        })
    }

    #[cfg(test)]
    pub(crate) fn capacity_usage(&self) -> (usize, usize, usize) {
        self.state
            .try_borrow()
            .map(|state| {
                (
                    state.queue.len(),
                    state
                        .in_flight
                        .as_ref()
                        .map_or(0, |batch| batch.entries.len()),
                    state.prefix_reservations.len(),
                )
            })
            .expect("test capacity inspection cannot overlap a journal mutation")
    }

    /// Captures exactly the commands that are queued or in flight at this instant.
    ///
    /// Commands enqueued after this call are kept behind a batch boundary and therefore cannot
    /// delay this barrier. A successful completion means every captured command reached a terminal
    /// journal result; callers must then take one fresh snapshot, re-probe geometry-sensitive
    /// targets, bind once, and enqueue at most one command.
    pub(crate) fn capture_prefix_barrier(
        self: &Rc<Self>,
    ) -> Result<OpenGpuiCommandPrefixBarrier, OpenGpuiCommandPrefixBarrierError> {
        let completion = Rc::new(PrefixBarrierCompletion::new());
        let (target_exclusive, reservation_sequence, ready) = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| OpenGpuiCommandPrefixBarrierError::Busy)?;
            match state.status {
                JournalStatus::Open => {}
                JournalStatus::Closing => {
                    return Err(OpenGpuiCommandPrefixBarrierError::HostClosing);
                }
                JournalStatus::Closed(reason) => {
                    return Err(OpenGpuiCommandPrefixBarrierError::HostClosed { reason });
                }
                JournalStatus::Dropped => {
                    return Err(OpenGpuiCommandPrefixBarrierError::HostDropped);
                }
            }
            let in_flight = state
                .in_flight
                .as_ref()
                .map_or(0, |batch| batch.entries.len());
            if state
                .queue
                .len()
                .saturating_add(in_flight)
                .saturating_add(state.prefix_reservations.len())
                >= OPEN_GPUI_COMMAND_JOURNAL_CAPACITY
            {
                return Err(OpenGpuiCommandPrefixBarrierError::QueueFull {
                    capacity: OPEN_GPUI_COMMAND_JOURNAL_CAPACITY,
                });
            }
            let reservation_sequence = state.next_entry_sequence;
            state.next_entry_sequence = reservation_sequence
                .checked_add(1)
                .ok_or(OpenGpuiCommandPrefixBarrierError::IdentityExhausted)?;
            state.prefix_reservations.insert(reservation_sequence);
            let target_exclusive = reservation_sequence;
            let ready = state.terminal_prefix_exclusive >= target_exclusive;
            if !ready {
                state.prefix_barriers.push(PrefixBarrierRegistration {
                    target_exclusive,
                    reservation_sequence,
                    completion: Rc::downgrade(&completion),
                });
            }
            (target_exclusive, reservation_sequence, ready)
        };
        if ready {
            completion.resolve(Ok(()));
        }
        Ok(OpenGpuiCommandPrefixBarrier {
            target_exclusive,
            reservation: Rc::new(PrefixReservation {
                sequence: Cell::new(Some(reservation_sequence)),
                journal: Rc::downgrade(self),
            }),
            completion,
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
            JournalStatus::Dropped => return Err(OpenGpuiCommandEnqueueError::HostUnavailable),
        }
        let in_flight = state
            .in_flight
            .as_ref()
            .map_or(0, |batch| batch.entries.len());
        if state
            .queue
            .len()
            .saturating_add(in_flight)
            .saturating_add(state.prefix_reservations.len())
            >= OPEN_GPUI_COMMAND_JOURNAL_CAPACITY
        {
            return Err(OpenGpuiCommandEnqueueError::QueueFull {
                capacity: OPEN_GPUI_COMMAND_JOURNAL_CAPACITY,
            });
        }
        let sequence = state.next_entry_sequence;
        let next_entry_sequence = sequence
            .checked_add(1)
            .ok_or(OpenGpuiCommandEnqueueError::IdentityExhausted)?;
        let (id, next_ticket_id, completion) = if with_ticket {
            let id = state.next_ticket_id;
            let next_ticket_id = state
                .next_ticket_id
                .checked_add(1)
                .ok_or(OpenGpuiCommandEnqueueError::IdentityExhausted)?;
            (
                Some(id),
                Some(next_ticket_id),
                Some(Rc::new(CommandCompletion::new())),
            )
        } else {
            (None, None, None)
        };
        state.next_entry_sequence = next_entry_sequence;
        if let Some(next_ticket_id) = next_ticket_id {
            state.next_ticket_id = next_ticket_id;
        }
        state.queue.push_back(JournalEntry {
            sequence,
            command,
            origin,
            completion: completion.clone(),
        });
        Ok(id
            .zip(completion)
            .map(|(id, completion)| OpenGpuiCommandTicket { id, completion }))
    }

    fn enqueue_reserved(
        &self,
        sequence: u64,
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
            JournalStatus::Dropped => return Err(OpenGpuiCommandEnqueueError::HostUnavailable),
        }
        let (id, next_ticket_id, completion) = if with_ticket {
            let id = state.next_ticket_id;
            let next_ticket_id = state
                .next_ticket_id
                .checked_add(1)
                .ok_or(OpenGpuiCommandEnqueueError::IdentityExhausted)?;
            (
                Some(id),
                Some(next_ticket_id),
                Some(Rc::new(CommandCompletion::new())),
            )
        } else {
            (None, None, None)
        };
        if !state.prefix_reservations.remove(&sequence) {
            return Err(OpenGpuiCommandEnqueueError::PrefixReservationInactive);
        }
        if let Some(next_ticket_id) = next_ticket_id {
            state.next_ticket_id = next_ticket_id;
        }
        let insertion_index = state
            .queue
            .iter()
            .position(|entry| entry.sequence > sequence)
            .unwrap_or(state.queue.len());
        state.queue.insert(
            insertion_index,
            JournalEntry {
                sequence,
                command,
                origin,
                completion: completion.clone(),
            },
        );
        Ok(id
            .zip(completion)
            .map(|(id, completion)| OpenGpuiCommandTicket { id, completion }))
    }

    fn abandon_prefix_reservation(&self, sequence: u64) {
        let barriers = {
            let Ok(mut state) = self.state.try_borrow_mut() else {
                return;
            };
            if !state.prefix_reservations.remove(&sequence) {
                return;
            }
            state.remove_prefix_barrier(sequence);
            state.mark_sequence_terminal(sequence);
            state.take_ready_prefix_barriers()
        };
        resolve_prefix_barriers(barriers, Ok(()));
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
        let mut expected_sequence = state.terminal_prefix_exclusive;
        let batch_len = state
            .queue
            .iter()
            .take_while(|entry| {
                let contiguous = entry.sequence == expected_sequence;
                if contiguous {
                    expected_sequence = expected_sequence.saturating_add(1);
                }
                contiguous
            })
            .count();
        if batch_len == 0 {
            return Ok(None);
        }
        for entry in state.queue.iter().take(batch_len) {
            collector.command(entry.command.clone())?;
        }
        let id = state.next_batch_id;
        state.next_batch_id = next_batch_id;
        let entries = state.queue.drain(..batch_len).collect::<Vec<_>>();
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
        let entries = self.take_batch(batch, Some(InFlightPhase::Submitted))?;
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
        let entries = self.take_batch(batch, None)?;
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
            JournalStatus::Dropped => return Err(OpenGpuiCommandCloseError::HostDropped),
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
        let (entries, barriers) = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| OpenGpuiCommandCloseError::Busy)?;
            if state.status != JournalStatus::Closing {
                return Err(OpenGpuiCommandCloseError::InvalidState);
            }
            state.status = JournalStatus::Closed(reason);
            let barriers = state.terminate_prefixes();
            (drain.into_entries(), barriers)
        };
        resolve_prefix_barriers(
            barriers,
            Err(OpenGpuiCommandPrefixBarrierError::HostClosed { reason }),
        );
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
        let (entries, barriers) = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| OpenGpuiCommandCloseError::Busy)?;
            if state.status != JournalStatus::Closing {
                return Err(OpenGpuiCommandCloseError::InvalidState);
            }
            state.status = JournalStatus::Closed(reason);
            let barriers = state.terminate_prefixes();
            (drain.into_entries(), barriers)
        };
        resolve_prefix_barriers(
            barriers,
            Err(OpenGpuiCommandPrefixBarrierError::HostClosed { reason }),
        );
        Ok(entries
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
        let (entries, barriers) = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| OpenGpuiCommandCloseError::Busy)?;
            if state.status != JournalStatus::Closing {
                return Err(OpenGpuiCommandCloseError::InvalidState);
            }
            state.status = JournalStatus::Closed(reason);
            let barriers = state.terminate_prefixes();
            (drain.into_entries(), barriers)
        };
        resolve_prefix_barriers(
            barriers,
            Err(OpenGpuiCommandPrefixBarrierError::ClosePanicked { reason }),
        );
        Ok(entries
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
        let (entries, barriers) = {
            let Ok(mut state) = self.state.try_borrow_mut() else {
                return Vec::new();
            };
            if matches!(
                state.status,
                JournalStatus::Closed(_) | JournalStatus::Dropped
            ) {
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
            let barriers = state.terminate_prefixes();
            (entries, barriers)
        };
        resolve_prefix_barriers(
            barriers,
            Err(OpenGpuiCommandPrefixBarrierError::HostClosed { reason }),
        );
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
        let (entries, barriers) = {
            let Ok(mut state) = self.state.try_borrow_mut() else {
                return Vec::new();
            };
            if state.status == JournalStatus::Dropped {
                return Vec::new();
            }
            state.status = JournalStatus::Dropped;
            let entries = state
                .in_flight
                .take()
                .into_iter()
                .flat_map(|batch| batch.entries)
                .chain(state.queue.drain(..))
                .collect::<Vec<_>>();
            let barriers = state.terminate_prefixes();
            (entries, barriers)
        };
        resolve_prefix_barriers(
            barriers,
            Err(OpenGpuiCommandPrefixBarrierError::HostDropped),
        );
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
        required_phase: Option<InFlightPhase>,
    ) -> Result<Vec<JournalEntry>, OpenGpuiCommandBatchStateError> {
        let (entries, barriers) = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| OpenGpuiCommandBatchStateError::Busy)?;
            if !state.in_flight.as_ref().is_some_and(|current| {
                current.id == batch.id && required_phase.is_none_or(|phase| current.phase == phase)
            }) {
                return Err(OpenGpuiCommandBatchStateError::UnknownBatch);
            }
            let entries = state
                .in_flight
                .take()
                .map(|batch| batch.entries)
                .unwrap_or_default();
            let barriers = state.advance_terminal_prefix(&entries);
            (entries, barriers)
        };
        resolve_prefix_barriers(barriers, Ok(()));
        Ok(entries)
    }
}

impl Drop for OpenGpuiCommandJournal {
    fn drop(&mut self) {
        let (barriers, ticket_completions) = {
            let state = self.state.get_mut();
            state.status = JournalStatus::Dropped;
            let entries = state
                .in_flight
                .take()
                .into_iter()
                .flat_map(|batch| batch.entries)
                .chain(state.queue.drain(..))
                .collect::<Vec<_>>();
            let barriers = state.terminate_prefixes();
            let ticket_completions = entries
                .into_iter()
                .filter_map(|entry| entry.completion)
                .collect::<Vec<_>>();
            (barriers, ticket_completions)
        };
        resolve_prefix_barriers(
            barriers,
            Err(OpenGpuiCommandPrefixBarrierError::HostDropped),
        );
        for completion in ticket_completions {
            completion.resolve(Err(OpenGpuiCommandTicketError::HostDropped));
        }
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
    #[error("native command host was dropped")]
    HostDropped,
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
    fn full_journal_rejects_an_additional_prefix_reservation() {
        let journal = OpenGpuiCommandJournal::new();
        for index in 0..OPEN_GPUI_COMMAND_JOURNAL_CAPACITY {
            journal
                .enqueue_detached(command(0, index), OpenGpuiCommandOrigin::Binding)
                .expect("command within capacity");
        }

        assert!(matches!(
            journal.capture_prefix_barrier(),
            Err(OpenGpuiCommandPrefixBarrierError::QueueFull { capacity })
                if capacity == OPEN_GPUI_COMMAND_JOURNAL_CAPACITY
        ));
    }

    #[test]
    fn in_flight_entries_count_toward_capacity_and_only_one_batch_is_collected() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        for index in 0..OPEN_GPUI_COMMAND_JOURNAL_CAPACITY {
            journal
                .enqueue_detached(command(0, index), OpenGpuiCommandOrigin::Binding)
                .expect("command within capacity");
        }
        let first_collector = collector(&view);
        let first_batch = journal
            .collect_next_batch(&first_collector)
            .expect("collect batch")
            .expect("nonempty batch");

        assert!(matches!(
            journal.enqueue_detached(command(0, usize::MAX), OpenGpuiCommandOrigin::Binding),
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
            .enqueue_detached(command(0, usize::MAX), OpenGpuiCommandOrigin::Binding)
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
    fn empty_prefix_barrier_is_immediately_ready() {
        let journal = OpenGpuiCommandJournal::new();
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("capture empty prefix");

        let permit = ready_prefix_permit(&mut barrier);
        assert!(journal.has_pending());
        permit.abandon();
        assert!(!journal.has_pending());
    }

    #[test]
    fn prefix_barrier_capture_reports_busy_closing_and_closed_states() {
        let journal = OpenGpuiCommandJournal::new();
        let state_borrow = journal.state.borrow_mut();
        assert!(matches!(
            journal.capture_prefix_barrier(),
            Err(OpenGpuiCommandPrefixBarrierError::Busy)
        ));
        drop(state_borrow);

        let drain = journal.begin_close().expect("begin close");
        assert!(matches!(
            journal.capture_prefix_barrier(),
            Err(OpenGpuiCommandPrefixBarrierError::HostClosing)
        ));
        let reason = ViewCloseReason::HostClosed;
        let _ = journal
            .finish_close_without_execution(drain, reason)
            .expect("finish close");
        assert!(matches!(
            journal.capture_prefix_barrier(),
            Err(OpenGpuiCommandPrefixBarrierError::HostClosed {
                reason: ViewCloseReason::HostClosed
            })
        ));
    }

    #[test]
    fn prefix_permit_holds_suffix_across_collect_attempts_and_inserts_before_it() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        journal
            .enqueue_detached(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("captured detached command");
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("capture queued prefix");
        let mut suffix_ticket = journal
            .enqueue(command(2, 3), OpenGpuiCommandOrigin::Binding)
            .expect("command after barrier");

        assert!(matches!(poll_prefix_barrier(&mut barrier), Poll::Pending));
        assert!(matches!(poll_ticket(&mut suffix_ticket), Poll::Pending));
        resolve_completed(submit_next_batch(&journal, &view));

        let permit = ready_prefix_permit(&mut barrier);
        assert!(matches!(poll_ticket(&mut suffix_ticket), Poll::Pending));
        assert_eq!(
            view.graph_snapshot()
                .expect("snapshot after prefix")
                .graph()
                .node(&node_id())
                .expect("journal node")
                .data,
            serde_json::json!({ "value": 1 })
        );

        for _ in 0..3 {
            assert!(
                journal
                    .collect_next_batch(&collector(&view))
                    .expect("held permit blocks suffix collection")
                    .is_none()
            );
        }

        permit
            .enqueue_detached(command(1, 2), OpenGpuiCommandOrigin::Binding)
            .expect("fill reserved prefix slot");

        resolve_completed(submit_next_batch(&journal, &view));
        assert!(
            ready_ticket(&mut suffix_ticket)
                .expect("suffix command transport")
                .applied()
                .is_some()
        );
        assert_eq!(
            view.graph_snapshot()
                .expect("snapshot after priority and suffix")
                .revision(),
            3
        );
    }

    #[test]
    fn prefix_barrier_waits_for_in_flight_and_queued_prefix_but_not_suffix() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut first_ticket = journal
            .enqueue(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("first prefix command");
        let first_collector = collector(&view);
        let first_batch = journal
            .collect_next_batch(&first_collector)
            .expect("collect first prefix")
            .expect("first prefix batch");
        journal
            .mark_submitted(first_batch)
            .expect("submit first prefix");
        let mut second_ticket = journal
            .enqueue(command(1, 2), OpenGpuiCommandOrigin::Binding)
            .expect("second prefix command");
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("capture in-flight and queued prefix");
        let mut suffix_ticket = journal
            .enqueue(command(3, 4), OpenGpuiCommandOrigin::Binding)
            .expect("suffix command");

        let first_receipt = view
            .submit_feedback(
                first_collector
                    .finish()
                    .expect("finish first prefix feedback"),
            )
            .expect("submit first prefix feedback");
        resolve_completed(
            journal
                .complete_submitted(first_batch, first_receipt)
                .expect("complete first prefix"),
        );
        assert!(
            ready_ticket(&mut first_ticket)
                .expect("first prefix transport")
                .applied()
                .is_some()
        );
        assert!(matches!(poll_prefix_barrier(&mut barrier), Poll::Pending));

        resolve_completed(submit_next_batch(&journal, &view));
        assert!(
            ready_ticket(&mut second_ticket)
                .expect("second prefix transport")
                .applied()
                .is_some()
        );
        let permit = ready_prefix_permit(&mut barrier);
        assert!(matches!(poll_ticket(&mut suffix_ticket), Poll::Pending));

        let mut priority_ticket = permit
            .enqueue(command(2, 3), OpenGpuiCommandOrigin::Binding)
            .expect("priority command ticket");
        resolve_completed(submit_next_batch(&journal, &view));
        assert!(
            ready_ticket(&mut priority_ticket)
                .expect("priority transport")
                .applied()
                .is_some()
        );
        assert!(
            ready_ticket(&mut suffix_ticket)
                .expect("suffix transport")
                .applied()
                .is_some()
        );
    }

    #[test]
    fn later_barrier_waits_for_an_earlier_permit_and_its_suffix_in_sequence_order() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        journal
            .enqueue_detached(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("initial prefix");
        let mut first_barrier = journal.capture_prefix_barrier().expect("first barrier");
        let mut suffix_ticket = journal
            .enqueue(command(2, 3), OpenGpuiCommandOrigin::Binding)
            .expect("first suffix");
        let mut second_barrier = journal.capture_prefix_barrier().expect("second barrier");
        let mut final_ticket = journal
            .enqueue(command(3, 4), OpenGpuiCommandOrigin::Binding)
            .expect("second suffix");

        resolve_completed(submit_next_batch(&journal, &view));
        let first_permit = ready_prefix_permit(&mut first_barrier);
        assert!(matches!(
            poll_prefix_barrier(&mut second_barrier),
            Poll::Pending
        ));
        assert!(
            journal
                .collect_next_batch(&collector(&view))
                .expect("first permit blocks both suffixes")
                .is_none()
        );

        first_permit
            .enqueue_detached(command(1, 2), OpenGpuiCommandOrigin::Binding)
            .expect("fill first reservation");
        resolve_completed(submit_next_batch(&journal, &view));
        assert!(
            ready_ticket(&mut suffix_ticket)
                .expect("first suffix transport")
                .applied()
                .is_some()
        );
        let second_permit = ready_prefix_permit(&mut second_barrier);
        assert!(matches!(poll_ticket(&mut final_ticket), Poll::Pending));
        assert!(
            journal
                .collect_next_batch(&collector(&view))
                .expect("second permit blocks final suffix")
                .is_none()
        );

        second_permit.abandon();
        resolve_completed(submit_next_batch(&journal, &view));
        assert!(
            ready_ticket(&mut final_ticket)
                .expect("final suffix transport")
                .applied()
                .is_some()
        );
        assert!(!journal.has_pending());
    }

    #[test]
    fn dropping_a_settled_permit_releases_its_suffix() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("empty prefix barrier");
        let mut suffix_ticket = journal
            .enqueue(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("suffix behind empty prefix");
        let permit = ready_prefix_permit(&mut barrier);

        assert!(
            journal
                .collect_next_batch(&collector(&view))
                .expect("held permit blocks suffix")
                .is_none()
        );
        drop(permit);
        resolve_completed(submit_next_batch(&journal, &view));

        assert!(
            ready_ticket(&mut suffix_ticket)
                .expect("suffix transport")
                .applied()
                .is_some()
        );
        assert!(!journal.has_pending());
    }

    #[test]
    fn a_forged_duplicate_permit_cannot_fill_the_same_reservation_twice() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("empty prefix barrier");
        let permit = ready_prefix_permit(&mut barrier);
        let duplicate = OpenGpuiCommandPrefixPermit {
            sequence: permit.sequence,
            journal: permit.journal.clone(),
        };

        permit
            .enqueue_detached(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("first permit use");
        assert!(matches!(
            duplicate.enqueue_detached(command(0, 2), OpenGpuiCommandOrigin::Binding),
            Err(OpenGpuiCommandEnqueueError::PrefixReservationInactive)
        ));
        resolve_completed(submit_next_batch(&journal, &view));
        assert_eq!(
            view.graph_snapshot()
                .expect("snapshot after one permit use")
                .revision(),
            1
        );
    }

    #[test]
    fn aborting_close_restores_the_prefix_without_completing_its_barrier() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        journal
            .enqueue_detached(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("pending command");
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("capture pending prefix");
        let drain = journal.begin_close().expect("begin close");

        assert!(matches!(poll_prefix_barrier(&mut barrier), Poll::Pending));
        journal.abort_close(drain).expect("abort close");
        assert!(matches!(poll_prefix_barrier(&mut barrier), Poll::Pending));

        resolve_completed(submit_next_batch(&journal, &view));
        ready_prefix_permit(&mut barrier).abandon();
    }

    #[test]
    fn close_drop_and_close_panic_terminate_prefix_barriers() {
        let reason = ViewCloseReason::HostClosed;

        let close_journal = OpenGpuiCommandJournal::new();
        close_journal
            .enqueue_detached(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("close command");
        let mut close_barrier = close_journal
            .capture_prefix_barrier()
            .expect("close barrier");
        let close_drain = close_journal.begin_close().expect("begin normal close");
        let _ = close_journal
            .finish_close_without_execution(close_drain, reason)
            .expect("finish normal close");
        assert!(matches!(
            poll_prefix_barrier(&mut close_barrier),
            Poll::Ready(Err(OpenGpuiCommandPrefixBarrierError::HostClosed {
                reason: ViewCloseReason::HostClosed
            }))
        ));

        let panic_journal = OpenGpuiCommandJournal::new();
        panic_journal
            .enqueue_detached(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("panic command");
        let mut panic_barrier = panic_journal
            .capture_prefix_barrier()
            .expect("panic barrier");
        let panic_drain = panic_journal.begin_close().expect("begin panic close");
        let _ = panic_journal
            .finish_close_after_panic(panic_drain, reason)
            .expect("finish panic close");
        assert!(matches!(
            poll_prefix_barrier(&mut panic_barrier),
            Poll::Ready(Err(OpenGpuiCommandPrefixBarrierError::ClosePanicked {
                reason: ViewCloseReason::HostClosed
            }))
        ));

        let mut drop_barrier = {
            let drop_journal = OpenGpuiCommandJournal::new();
            drop_journal
                .enqueue_detached(command(0, 1), OpenGpuiCommandOrigin::Binding)
                .expect("drop command");
            drop_journal.capture_prefix_barrier().expect("drop barrier")
        };
        assert!(matches!(
            poll_prefix_barrier(&mut drop_barrier),
            Poll::Ready(Err(OpenGpuiCommandPrefixBarrierError::HostDropped))
        ));
    }

    #[test]
    fn prefix_barrier_does_not_rewrite_or_retry_a_stale_follow_up_command() {
        let (runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        journal
            .enqueue_detached(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("prefix command");
        let mut barrier = journal.capture_prefix_barrier().expect("capture prefix");
        resolve_completed(submit_next_batch(&journal, &view));
        let permit = ready_prefix_permit(&mut barrier);
        assert_eq!(runtime.snapshot().expect("fresh snapshot").revision(), 1);

        let mut stale_ticket = permit
            .enqueue(command(0, 2), OpenGpuiCommandOrigin::Binding)
            .expect("stale follow-up command");
        resolve_completed(submit_next_batch(&journal, &view));

        let outcome = ready_ticket(&mut stale_ticket).expect("stale command transport");
        assert!(outcome.conflict().is_some());
        assert!(!journal.has_pending());
        let snapshot = runtime.snapshot().expect("snapshot after conflict");
        assert_eq!(snapshot.revision(), 1);
        assert_eq!(
            snapshot
                .graph()
                .node(&node_id())
                .expect("journal node")
                .data,
            serde_json::json!({ "value": 1 })
        );
    }

    #[test]
    fn rejected_priority_command_is_terminal_and_does_not_block_its_suffix() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("empty prefix barrier");
        let mut suffix_ticket = journal
            .enqueue(command(0, 1), OpenGpuiCommandOrigin::Binding)
            .expect("suffix command");
        let permit = ready_prefix_permit(&mut barrier);
        let mut rejected_ticket = permit
            .enqueue(
                GraphCommand::set_node_data(
                    0,
                    NodeId::from_u128(0xdead),
                    serde_json::json!({ "value": "missing" }),
                ),
                OpenGpuiCommandOrigin::Binding,
            )
            .expect("reserved rejected command");

        resolve_completed(submit_next_batch(&journal, &view));

        assert!(matches!(
            ready_ticket(&mut rejected_ticket).expect("rejected command transport"),
            GraphCommandOutcome::Rejected(_)
        ));
        assert!(
            ready_ticket(&mut suffix_ticket)
                .expect("suffix transport")
                .applied()
                .is_some()
        );
        assert!(!journal.has_pending());
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
            .enqueue_detached(command(0, 1), OpenGpuiCommandOrigin::Binding)
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

    fn submit_next_batch(
        journal: &OpenGpuiCommandJournal,
        view: &ViewSession,
    ) -> Vec<OpenGpuiCompletedCommand> {
        let collector = collector(view);
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect next batch")
            .expect("nonempty command batch");
        journal.mark_submitted(batch).expect("submit next batch");
        let receipt = view
            .submit_feedback(collector.finish().expect("finish next feedback"))
            .expect("submit next feedback");
        journal
            .complete_submitted(batch, receipt)
            .expect("complete next batch")
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

    fn poll_prefix_barrier(
        barrier: &mut OpenGpuiCommandPrefixBarrier,
    ) -> Poll<OpenGpuiCommandPrefixBarrierResult> {
        let mut context = Context::from_waker(Waker::noop());
        Future::poll(Pin::new(barrier), &mut context)
    }

    fn ready_prefix_permit(
        barrier: &mut OpenGpuiCommandPrefixBarrier,
    ) -> OpenGpuiCommandPrefixPermit {
        match poll_prefix_barrier(barrier) {
            Poll::Ready(Ok(permit)) => permit,
            Poll::Ready(Err(error)) => panic!("native command prefix failed: {error}"),
            Poll::Pending => panic!("native command prefix is still pending"),
        }
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
