use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, Poll, Waker};

use jellyflow_runtime::{
    commands::CommandCompletionCorrelationError, commands::CommandExecutionCompletion,
    commands::CommandExecutionCompletions, commands::GraphCommand, commands::GraphCommandIdentity,
    view::FeedbackCollector, view::FeedbackCollectorError, view::FeedbackReceipt,
    view::FeedbackSubmissionError, view::ViewCloseReason, view::ViewSessionError,
};
use open_gpui::{App, EntityId};

use jellyflow_runtime::interaction::InteractionTicketId;

use crate::panic_sink::OpenGpuiPanicAccumulator;

/// Maximum number of commands owned by one native host across its queue and in-flight batch.
pub(crate) const OPEN_GPUI_COMMAND_JOURNAL_CAPACITY: usize = 256;

/// Failure to enqueue a command for the next real native frame.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub(crate) enum OpenGpuiCommandEnqueueError {
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
    #[error("native command prefix reservation belongs to another host journal")]
    ForeignPrefixReservation,
    #[error("native command journal is busy in another callback")]
    Busy,
}

/// Terminal result transport failure for one enqueued native command.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OpenGpuiCommandTicketError {
    #[error("native command host was dropped")]
    HostDropped,
    #[error("native command host closed: {reason:?}")]
    HostClosed { reason: ViewCloseReason },
    #[error("native command result is unknown because host close panicked: {reason:?}")]
    ClosePanicked { reason: ViewCloseReason },
    #[error(
        "native command result is unknown because the closed view returned uncorrelated command completions: {reason:?}"
    )]
    CloseOutcomeUnknown { reason: ViewCloseReason },
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
}

/// Transport-only failure classified while the journal still owns submission-stage evidence.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OpenGpuiCommandTicketFailure {
    #[error("{0}")]
    DefinitelyNotExecuted(OpenGpuiCommandTicketError),
    #[error("{0}")]
    ExecutionUnknown(OpenGpuiCommandTicketError),
}

impl OpenGpuiCommandTicketFailure {
    pub(crate) fn definitely_not_executed(error: OpenGpuiCommandTicketError) -> Self {
        Self::DefinitelyNotExecuted(error)
    }

    pub(crate) fn execution_unknown(error: OpenGpuiCommandTicketError) -> Self {
        Self::ExecutionUnknown(error)
    }
}

pub(crate) type OpenGpuiCommandTicketResult =
    Result<CommandExecutionCompletion, OpenGpuiCommandTicketFailure>;

/// One centrally classified terminal command-ticket result.
#[derive(Debug)]
pub(crate) enum OpenGpuiCommandTicketDisposition<'a> {
    Completed(&'a CommandExecutionCompletion),
    DefinitelyNotExecuted(&'a OpenGpuiCommandTicketError),
    ExecutionUnknown(&'a OpenGpuiCommandTicketError),
}

impl<'a> OpenGpuiCommandTicketDisposition<'a> {
    pub(crate) fn classify(result: &'a OpenGpuiCommandTicketResult) -> Self {
        match result {
            Ok(completion) => Self::Completed(completion),
            Err(OpenGpuiCommandTicketFailure::DefinitelyNotExecuted(error)) => {
                Self::DefinitelyNotExecuted(error)
            }
            Err(OpenGpuiCommandTicketFailure::ExecutionUnknown(error)) => {
                Self::ExecutionUnknown(error)
            }
        }
    }
}

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

impl OpenGpuiCommandPrefixBarrierError {
    /// Returns whether no command was admitted and a later capture may succeed after journal
    /// progress or callback completion.
    pub(crate) const fn is_pre_admission_backpressure(&self) -> bool {
        matches!(self, Self::Busy | Self::QueueFull { .. })
    }
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

    fn admit(
        mut self,
        command: GraphCommand,
        notification: OpenGpuiCommandNotification,
    ) -> Result<OpenGpuiAdmittedCommand, OpenGpuiCommandEnqueueError> {
        let sequence = self
            .sequence
            .ok_or(OpenGpuiCommandEnqueueError::PrefixReservationInactive)?;
        let journal = self
            .journal
            .upgrade()
            .ok_or(OpenGpuiCommandEnqueueError::HostUnavailable)?;
        let ticket = journal
            .enqueue_reserved(sequence, command, OpenGpuiCommandOrigin::Binding, true)?
            .ok_or(OpenGpuiCommandEnqueueError::IdentityExhausted)?;
        self.sequence = None;
        Ok(OpenGpuiAdmittedCommand {
            ticket,
            notification,
        })
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
pub(crate) struct OpenGpuiCommandTicket {
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

    pub(crate) fn admit(
        &self,
        command: GraphCommand,
    ) -> Result<OpenGpuiAdmittedCommand, OpenGpuiCommandEnqueueError> {
        let journal = self
            .journal
            .upgrade()
            .ok_or(OpenGpuiCommandEnqueueError::HostUnavailable)?;
        let ticket = journal.enqueue(command, OpenGpuiCommandOrigin::Binding)?;
        Ok(OpenGpuiAdmittedCommand {
            ticket,
            notification: OpenGpuiCommandNotification { host: self.host },
        })
    }

    pub(crate) fn admit_reserved(
        &self,
        command: GraphCommand,
        permit: OpenGpuiCommandPrefixPermit,
    ) -> Result<OpenGpuiAdmittedCommand, OpenGpuiCommandEnqueueError> {
        if !Weak::ptr_eq(&self.journal, &permit.journal) {
            return Err(OpenGpuiCommandEnqueueError::ForeignPrefixReservation);
        }
        permit.admit(command, OpenGpuiCommandNotification { host: self.host })
    }
}

/// Journal-admission proof split from the fallible native host notification.
pub(crate) struct OpenGpuiAdmittedCommand {
    ticket: OpenGpuiCommandTicket,
    notification: OpenGpuiCommandNotification,
}

impl OpenGpuiAdmittedCommand {
    pub(crate) fn into_parts(self) -> (OpenGpuiCommandTicket, OpenGpuiCommandNotification) {
        (self.ticket, self.notification)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct OpenGpuiCommandNotification {
    host: EntityId,
}

impl OpenGpuiCommandNotification {
    pub(crate) fn notify(self, cx: &mut App) {
        cx.notify(self.host);
    }
}

#[derive(Debug, Clone)]
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
    deferred_completions: VecDeque<OpenGpuiCompletedCommand>,
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
    let mut panics = OpenGpuiPanicAccumulator::default();
    for barrier in barriers {
        panics.capture(|| barrier.resolve(result.clone()));
    }
    panics.resume();
}

#[derive(Debug, Clone, Copy)]
enum CloseTerminalCertainty {
    DefinitelyNotExecuted,
    OutcomeUnknown,
    Panicked,
}

#[derive(Debug, Clone, Copy)]
struct CloseTerminalSettlement {
    reason: ViewCloseReason,
    certainty: CloseTerminalCertainty,
}

#[derive(Debug, Clone, Copy)]
enum CloseDropDisposition {
    Rollback,
    Terminal(CloseTerminalSettlement),
}

enum DeferredCloseRecovery {
    Rollback(OpenGpuiCommandCloseEntries),
    Terminal {
        entries: OpenGpuiCommandCloseEntries,
        settlement: CloseTerminalSettlement,
    },
    Completed(PreparedCloseCompletion),
}

struct PreparedCloseCompletion {
    completed: Vec<OpenGpuiCompletedCommand>,
    settlement: CloseTerminalSettlement,
}

pub(crate) struct OpenGpuiCommandJournal {
    state: RefCell<JournalState>,
    deferred_close_recoveries: RefCell<VecDeque<DeferredCloseRecovery>>,
    deferred_prefix_abandonments: RefCell<BTreeSet<u64>>,
    deferred_batch_abandonments: RefCell<BTreeSet<u64>>,
}

impl OpenGpuiCommandJournal {
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self {
            state: RefCell::new(JournalState {
                status: JournalStatus::Open,
                queue: VecDeque::new(),
                in_flight: None,
                deferred_completions: VecDeque::new(),
                next_ticket_id: 0,
                next_batch_id: 0,
                next_entry_sequence: 0,
                terminal_prefix_exclusive: 0,
                terminal_sequences: BTreeSet::new(),
                prefix_reservations: BTreeSet::new(),
                prefix_barriers: Vec::new(),
            }),
            deferred_close_recoveries: RefCell::new(VecDeque::new()),
            deferred_prefix_abandonments: RefCell::new(BTreeSet::new()),
            deferred_batch_abandonments: RefCell::new(BTreeSet::new()),
        })
    }

    fn resolve_prefix_barriers(
        &self,
        barriers: Vec<Rc<PrefixBarrierCompletion>>,
        result: PrefixBarrierCompletionResult,
    ) {
        resolve_prefix_barriers(barriers, result);
    }

    pub(crate) fn enqueue(
        &self,
        command: GraphCommand,
        origin: OpenGpuiCommandOrigin,
    ) -> Result<OpenGpuiCommandTicket, OpenGpuiCommandEnqueueError> {
        self.enqueue_entry(command, origin, true)?
            .ok_or(OpenGpuiCommandEnqueueError::IdentityExhausted)
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.drain_deferred_abandonments();
        if !self.deferred_close_recoveries.borrow().is_empty() {
            return true;
        }
        self.state.try_borrow().map_or(true, |state| {
            state.in_flight.is_some()
                || !state.queue.is_empty()
                || !state.deferred_completions.is_empty()
                || !state.prefix_reservations.is_empty()
        })
    }

    pub(crate) fn defer_completions(
        &self,
        completed: Vec<OpenGpuiCompletedCommand>,
    ) -> Vec<OpenGpuiPendingCommandResolution> {
        self.drain_deferred_abandonments();
        let mut resolutions = Vec::new();
        let mut deferred = VecDeque::new();
        for command in completed {
            if matches!(&command.origin, OpenGpuiCommandOrigin::Binding) {
                if let Some(resolution) = command.into_resolution() {
                    resolutions.push(resolution);
                }
            } else {
                deferred.push_back(command);
            }
        }
        self.state
            .borrow_mut()
            .deferred_completions
            .append(&mut deferred);
        resolutions
    }

    pub(crate) fn retain_completions_for_host(&self, completed: Vec<OpenGpuiCompletedCommand>) {
        self.drain_deferred_abandonments();
        self.state
            .borrow_mut()
            .deferred_completions
            .extend(completed);
    }

    pub(crate) fn take_deferred_completions(&self) -> Vec<OpenGpuiCompletedCommand> {
        self.drain_deferred_abandonments();
        self.state
            .borrow_mut()
            .deferred_completions
            .drain(..)
            .collect()
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
        self.drain_deferred_abandonments();
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
            self.resolve_prefix_barriers(vec![Rc::clone(&completion)], Ok(()));
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
        self.drain_deferred_abandonments();
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
        self.drain_deferred_abandonments();
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
        self.deferred_prefix_abandonments
            .borrow_mut()
            .insert(sequence);
        self.drain_deferred_prefix_abandonments();
    }

    fn drain_deferred_abandonments(&self) {
        self.drain_deferred_close_recoveries();
        self.drain_deferred_prefix_abandonments();
        self.drain_deferred_batch_abandonments();
    }

    fn defer_close_recovery(&self, recovery: DeferredCloseRecovery) {
        {
            let mut recoveries = self.deferred_close_recoveries.borrow_mut();
            recoveries.push_back(recovery);
        }
        let mut panics = OpenGpuiPanicAccumulator::default();
        panics.capture(|| self.drain_deferred_abandonments());
        let _ = panics.suppress();
    }

    fn drain_deferred_close_recoveries(&self) {
        let mut recoveries = std::mem::take(&mut *self.deferred_close_recoveries.borrow_mut());
        if recoveries.is_empty() {
            return;
        }
        let mut prefix_resolutions = Vec::new();
        {
            let Ok(mut state) = self.state.try_borrow_mut() else {
                let mut deferred = self.deferred_close_recoveries.borrow_mut();
                recoveries.append(&mut deferred);
                *deferred = recoveries;
                return;
            };
            for recovery in recoveries {
                if let Some(resolution) = apply_close_recovery(&mut state, recovery) {
                    prefix_resolutions.push(resolution);
                }
            }
        }
        for (barriers, result) in prefix_resolutions {
            self.resolve_prefix_barriers(barriers, result);
        }
    }

    fn drain_deferred_prefix_abandonments(&self) {
        let mut deferred = std::mem::take(&mut *self.deferred_prefix_abandonments.borrow_mut());
        if deferred.is_empty() {
            return;
        }

        let barriers = {
            let Ok(mut state) = self.state.try_borrow_mut() else {
                self.deferred_prefix_abandonments
                    .borrow_mut()
                    .append(&mut deferred);
                return;
            };
            for sequence in deferred {
                if state.prefix_reservations.remove(&sequence) {
                    state.remove_prefix_barrier(sequence);
                    state.mark_sequence_terminal(sequence);
                }
            }
            state.take_ready_prefix_barriers()
        };
        self.resolve_prefix_barriers(barriers, Ok(()));
    }

    pub(crate) fn collect_next_batch(
        self: &Rc<Self>,
        collector: &FeedbackCollector,
    ) -> Result<Option<OpenGpuiCommandBatch>, OpenGpuiCommandCollectError> {
        self.drain_deferred_abandonments();
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
        Ok(Some(OpenGpuiCommandBatch {
            id,
            journal: Rc::downgrade(self),
        }))
    }

    pub(crate) fn mark_submitted(
        &self,
        batch: &OpenGpuiCommandBatch,
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
        batch: &OpenGpuiCommandBatch,
        mut receipt: FeedbackReceipt,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError> {
        let expected = self.batch_command_identities(batch.id, InFlightPhase::Submitted)?;
        let completions = receipt.take_exact_command_completions(&expected)?;
        let entries = self.take_batch(batch.id, Some(InFlightPhase::Submitted))?;
        Ok(entries
            .into_iter()
            .zip(completions)
            .map(|(entry, completion)| OpenGpuiCompletedCommand::new(entry, Ok(completion)))
            .collect())
    }

    fn batch_command_identities(
        &self,
        batch_id: u64,
        phase: InFlightPhase,
    ) -> Result<Vec<GraphCommandIdentity>, OpenGpuiCommandBatchStateError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| OpenGpuiCommandBatchStateError::Busy)?;
        let batch = state
            .in_flight
            .as_ref()
            .filter(|batch| batch.id == batch_id && batch.phase == phase)
            .ok_or(OpenGpuiCommandBatchStateError::UnknownBatch)?;
        Ok(batch
            .entries
            .iter()
            .map(|entry| entry.command.identity().clone())
            .collect())
    }

    pub(crate) fn fail_batch(
        &self,
        batch: OpenGpuiCommandBatch,
        mut failure: impl FnMut() -> OpenGpuiCommandTicketFailure,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError> {
        let entries = self.take_batch(batch.id, None)?;
        Ok(entries
            .into_iter()
            .map(|entry| OpenGpuiCompletedCommand::new(entry, Err(failure())))
            .collect())
    }

    pub(crate) fn fail_batch_from_phase(
        &self,
        batch: OpenGpuiCommandBatch,
        mut error: impl FnMut() -> OpenGpuiCommandTicketError,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandBatchStateError> {
        let (phase, entries) = self.take_batch_with_phase(batch.id, None)?;
        Ok(entries
            .into_iter()
            .map(|entry| {
                let failure = match phase {
                    InFlightPhase::Collected => {
                        OpenGpuiCommandTicketFailure::definitely_not_executed(error())
                    }
                    InFlightPhase::Submitted => {
                        OpenGpuiCommandTicketFailure::execution_unknown(error())
                    }
                };
                OpenGpuiCompletedCommand::new(entry, Err(failure))
            })
            .collect())
    }

    fn abandon_batch(&self, batch_id: u64) {
        self.deferred_batch_abandonments
            .borrow_mut()
            .insert(batch_id);
        self.drain_deferred_batch_abandonments();
    }

    fn drain_deferred_batch_abandonments(&self) {
        let mut deferred = std::mem::take(&mut *self.deferred_batch_abandonments.borrow_mut());
        if deferred.is_empty() {
            return;
        }
        let (batch, barriers) = {
            let Ok(mut state) = self.state.try_borrow_mut() else {
                self.deferred_batch_abandonments
                    .borrow_mut()
                    .append(&mut deferred);
                return;
            };
            let should_abandon = state
                .in_flight
                .as_ref()
                .is_some_and(|batch| deferred.contains(&batch.id));
            if !should_abandon {
                if state.status == JournalStatus::Closing {
                    self.deferred_batch_abandonments
                        .borrow_mut()
                        .append(&mut deferred);
                }
                return;
            }
            let batch = state
                .in_flight
                .take()
                .expect("a matching in-flight batch was checked before removal");
            let barriers = state.advance_terminal_prefix(&batch.entries);
            (batch, barriers)
        };
        self.resolve_prefix_barriers(barriers, Ok(()));
        let phase = batch.phase;
        let completed = batch
            .entries
            .into_iter()
            .map(|entry| {
                let failure = match phase {
                    InFlightPhase::Collected => {
                        OpenGpuiCommandTicketFailure::definitely_not_executed(
                            OpenGpuiCommandTicketError::FramePreparation {
                                message:
                                    "native command batch was dropped during frame preparation"
                                        .to_owned(),
                            },
                        )
                    }
                    InFlightPhase::Submitted => OpenGpuiCommandTicketFailure::execution_unknown(
                        OpenGpuiCommandTicketError::FeedbackOutcomeUnknown,
                    ),
                };
                OpenGpuiCompletedCommand::new(entry, Err(failure))
            })
            .collect();
        resolve_command_tickets(self.defer_completions(completed));
    }

    pub(crate) fn begin_close(
        self: &Rc<Self>,
    ) -> Result<OpenGpuiCommandCloseTransaction, OpenGpuiCommandCloseError> {
        self.drain_deferred_abandonments();
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
        Ok(OpenGpuiCommandCloseTransaction {
            journal: Rc::clone(self),
            entries: Some(OpenGpuiCommandCloseEntries {
                collected: state.in_flight.take(),
                queued: std::mem::take(&mut state.queue),
            }),
            prepared: None,
            drop_disposition: CloseDropDisposition::Rollback,
        })
    }

    pub(crate) fn terminate(&self, reason: ViewCloseReason) -> Vec<OpenGpuiCompletedCommand> {
        self.drain_deferred_abandonments();
        let (in_flight, queued, barriers) = {
            let mut state = self.state.borrow_mut();
            if matches!(
                state.status,
                JournalStatus::Closed(_) | JournalStatus::Dropped
            ) {
                return Vec::new();
            }
            state.status = JournalStatus::Closed(reason);
            let in_flight = state.in_flight.take();
            let queued = state.queue.drain(..).collect::<Vec<_>>();
            let barriers = state.terminate_prefixes();
            (in_flight, queued, barriers)
        };
        self.resolve_prefix_barriers(
            barriers,
            Err(OpenGpuiCommandPrefixBarrierError::HostClosed { reason }),
        );
        let mut completed = Vec::new();
        if let Some(batch) = in_flight {
            let phase = batch.phase;
            completed.extend(batch.entries.into_iter().map(|entry| {
                let error = OpenGpuiCommandTicketError::HostClosed { reason };
                let failure = match phase {
                    InFlightPhase::Collected => {
                        OpenGpuiCommandTicketFailure::definitely_not_executed(error)
                    }
                    InFlightPhase::Submitted => {
                        OpenGpuiCommandTicketFailure::execution_unknown(error)
                    }
                };
                OpenGpuiCompletedCommand::new(entry, Err(failure))
            }));
        }
        completed.extend(queued.into_iter().map(|entry| {
            OpenGpuiCompletedCommand::new(
                entry,
                Err(OpenGpuiCommandTicketFailure::definitely_not_executed(
                    OpenGpuiCommandTicketError::HostClosed { reason },
                )),
            )
        }));
        completed
    }

    pub(crate) fn terminate_dropped(&self) -> Vec<OpenGpuiCompletedCommand> {
        self.drain_deferred_abandonments();
        let (in_flight, queued, barriers) = {
            let mut state = self.state.borrow_mut();
            if state.status == JournalStatus::Dropped {
                return Vec::new();
            }
            state.status = JournalStatus::Dropped;
            let in_flight = state.in_flight.take();
            let queued = state.queue.drain(..).collect::<Vec<_>>();
            let barriers = state.terminate_prefixes();
            (in_flight, queued, barriers)
        };
        self.resolve_prefix_barriers(
            barriers,
            Err(OpenGpuiCommandPrefixBarrierError::HostDropped),
        );
        let mut completed = Vec::new();
        if let Some(batch) = in_flight {
            let phase = batch.phase;
            completed.extend(batch.entries.into_iter().map(|entry| {
                let failure = match phase {
                    InFlightPhase::Collected => {
                        OpenGpuiCommandTicketFailure::definitely_not_executed(
                            OpenGpuiCommandTicketError::HostDropped,
                        )
                    }
                    InFlightPhase::Submitted => OpenGpuiCommandTicketFailure::execution_unknown(
                        OpenGpuiCommandTicketError::HostDropped,
                    ),
                };
                OpenGpuiCompletedCommand::new(entry, Err(failure))
            }));
        }
        completed.extend(queued.into_iter().map(|entry| {
            OpenGpuiCompletedCommand::new(
                entry,
                Err(OpenGpuiCommandTicketFailure::definitely_not_executed(
                    OpenGpuiCommandTicketError::HostDropped,
                )),
            )
        }));
        completed
    }

    fn take_batch(
        &self,
        batch_id: u64,
        required_phase: Option<InFlightPhase>,
    ) -> Result<Vec<JournalEntry>, OpenGpuiCommandBatchStateError> {
        self.take_batch_with_phase(batch_id, required_phase)
            .map(|(_, entries)| entries)
    }

    fn take_batch_with_phase(
        &self,
        batch_id: u64,
        required_phase: Option<InFlightPhase>,
    ) -> Result<(InFlightPhase, Vec<JournalEntry>), OpenGpuiCommandBatchStateError> {
        self.drain_deferred_abandonments();
        let (phase, entries, barriers) = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| OpenGpuiCommandBatchStateError::Busy)?;
            if !state.in_flight.as_ref().is_some_and(|current| {
                current.id == batch_id && required_phase.is_none_or(|phase| current.phase == phase)
            }) {
                return Err(OpenGpuiCommandBatchStateError::UnknownBatch);
            }
            let batch = state
                .in_flight
                .take()
                .expect("a matching command batch was checked before removal");
            let phase = batch.phase;
            let entries = batch.entries;
            let barriers = state.advance_terminal_prefix(&entries);
            (phase, entries, barriers)
        };
        self.resolve_prefix_barriers(barriers, Ok(()));
        Ok((phase, entries))
    }
}

impl Drop for OpenGpuiCommandJournal {
    fn drop(&mut self) {
        self.deferred_prefix_abandonments.get_mut().clear();
        self.deferred_batch_abandonments.get_mut().clear();
        let recoveries = std::mem::take(self.deferred_close_recoveries.get_mut());
        let (close_prefix_resolutions, barriers, ticket_resolutions) = {
            let state = self.state.get_mut();
            let close_prefix_resolutions = recoveries
                .into_iter()
                .filter_map(|recovery| apply_close_recovery(state, recovery))
                .collect::<Vec<_>>();
            state.status = JournalStatus::Dropped;
            let in_flight = state.in_flight.take();
            let queued = state.queue.drain(..).collect::<Vec<_>>();
            let barriers = state.terminate_prefixes();
            let mut ticket_resolutions = state
                .deferred_completions
                .drain(..)
                .filter_map(OpenGpuiCompletedCommand::into_resolution)
                .collect::<Vec<_>>();
            if let Some(batch) = in_flight {
                let phase = batch.phase;
                ticket_resolutions.extend(batch.entries.into_iter().filter_map(|entry| {
                    let failure = match phase {
                        InFlightPhase::Collected => {
                            OpenGpuiCommandTicketFailure::definitely_not_executed(
                                OpenGpuiCommandTicketError::HostDropped,
                            )
                        }
                        InFlightPhase::Submitted => {
                            OpenGpuiCommandTicketFailure::execution_unknown(
                                OpenGpuiCommandTicketError::HostDropped,
                            )
                        }
                    };
                    OpenGpuiCompletedCommand::new(entry, Err(failure)).into_resolution()
                }));
            }
            ticket_resolutions.extend(queued.into_iter().filter_map(|entry| {
                OpenGpuiCompletedCommand::new(
                    entry,
                    Err(OpenGpuiCommandTicketFailure::definitely_not_executed(
                        OpenGpuiCommandTicketError::HostDropped,
                    )),
                )
                .into_resolution()
            }));
            (close_prefix_resolutions, barriers, ticket_resolutions)
        };
        for (barriers, result) in close_prefix_resolutions {
            self.resolve_prefix_barriers(barriers, result);
        }
        self.resolve_prefix_barriers(
            barriers,
            Err(OpenGpuiCommandPrefixBarrierError::HostDropped),
        );
        drop(ticket_resolutions);
    }
}

#[must_use = "command batches must remain alive until receipt pairing or close transfer"]
pub(crate) struct OpenGpuiCommandBatch {
    id: u64,
    journal: Weak<OpenGpuiCommandJournal>,
}

impl std::fmt::Debug for OpenGpuiCommandBatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenGpuiCommandBatch")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Drop for OpenGpuiCommandBatch {
    fn drop(&mut self) {
        if let Some(journal) = self.journal.upgrade() {
            journal.abandon_batch(self.id);
        }
    }
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
    #[error(transparent)]
    Correlation(#[from] CommandCompletionCorrelationError),
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
    #[error(transparent)]
    Correlation(#[from] CommandCompletionCorrelationError),
}

struct OpenGpuiCommandCloseEntries {
    collected: Option<InFlightBatch>,
    queued: VecDeque<JournalEntry>,
}

impl OpenGpuiCommandCloseEntries {
    fn command_identities(&self) -> Vec<GraphCommandIdentity> {
        self.collected
            .iter()
            .flat_map(|batch| batch.entries.iter())
            .chain(self.queued.iter())
            .map(|entry| entry.command.identity().clone())
            .collect()
    }

    fn combined_request(
        &self,
        caller: &jellyflow_runtime::view::ViewCloseRequest,
    ) -> jellyflow_runtime::view::ViewCloseRequest {
        let mut request = jellyflow_runtime::view::ViewCloseRequest::new();
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

    fn restore_into(self, state: &mut JournalState) {
        let Self {
            collected,
            mut queued,
        } = self;
        if state.in_flight.is_none() {
            state.in_flight = collected;
        } else if let Some(collected) = collected {
            queued.extend(collected.entries);
        }
        queued.append(&mut state.queue);
        if queued.len() > 1 {
            let mut queued = queued.into_iter().collect::<Vec<_>>();
            queued.sort_by_key(|entry| entry.sequence);
            state.queue = queued.into();
        } else {
            state.queue = queued;
        }
    }
}

/// Owns one extracted close prefix until it is atomically rolled back or terminally settled.
#[must_use = "native command close transactions must be finished, aborted, or dropped"]
pub(crate) struct OpenGpuiCommandCloseTransaction {
    journal: Rc<OpenGpuiCommandJournal>,
    entries: Option<OpenGpuiCommandCloseEntries>,
    prepared: Option<PreparedCloseCompletion>,
    drop_disposition: CloseDropDisposition,
}

impl OpenGpuiCommandCloseTransaction {
    pub(crate) fn command_identities(&self) -> Vec<GraphCommandIdentity> {
        self.entries
            .as_ref()
            .expect("an active close transaction owns its extracted entries")
            .command_identities()
    }

    pub(crate) fn combined_request(
        &self,
        caller: &jellyflow_runtime::view::ViewCloseRequest,
    ) -> jellyflow_runtime::view::ViewCloseRequest {
        self.entries
            .as_ref()
            .expect("an active close transaction owns its extracted entries")
            .combined_request(caller)
    }

    /// Records that the close request reached a terminal view before completion extraction begins.
    pub(crate) fn confirm_view_terminated(&mut self, reason: ViewCloseReason) {
        self.drop_disposition = CloseDropDisposition::Terminal(CloseTerminalSettlement {
            reason,
            certainty: CloseTerminalCertainty::OutcomeUnknown,
        });
    }

    pub(crate) fn abort(mut self) -> Result<(), OpenGpuiCommandCloseError> {
        if matches!(self.drop_disposition, CloseDropDisposition::Terminal(_)) {
            return Err(OpenGpuiCommandCloseError::InvalidState);
        }
        self.journal.drain_deferred_abandonments();
        let journal = Rc::clone(&self.journal);
        let mut state = journal
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandCloseError::Busy)?;
        if state.status != JournalStatus::Closing {
            return Err(OpenGpuiCommandCloseError::InvalidState);
        }
        let entries = self
            .entries
            .take()
            .expect("an active close transaction owns its extracted entries");
        entries.restore_into(&mut state);
        state.status = JournalStatus::Open;
        drop(state);
        self.journal.drain_deferred_abandonments();
        Ok(())
    }

    pub(crate) fn finish(
        mut self,
        reason: ViewCloseReason,
        completions: Vec<CommandExecutionCompletion>,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandCloseError> {
        self.confirm_view_terminated(reason);
        let identities = self.command_identities();
        let mut completions = CommandExecutionCompletions::try_from_ordered(completions)?;
        let completions = completions.take_exact(&identities)?;
        self.commit_with_completions(reason, completions)
    }

    pub(crate) fn finish_without_execution(
        mut self,
        reason: ViewCloseReason,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandCloseError> {
        let settlement = CloseTerminalSettlement {
            reason,
            certainty: CloseTerminalCertainty::DefinitelyNotExecuted,
        };
        self.drop_disposition = CloseDropDisposition::Terminal(settlement);
        self.commit_with_settlement(settlement)
    }

    pub(crate) fn finish_outcome_unknown(
        mut self,
        reason: ViewCloseReason,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandCloseError> {
        let settlement = CloseTerminalSettlement {
            reason,
            certainty: CloseTerminalCertainty::OutcomeUnknown,
        };
        self.drop_disposition = CloseDropDisposition::Terminal(settlement);
        self.commit_with_settlement(settlement)
    }

    pub(crate) fn finish_after_panic(
        mut self,
        reason: ViewCloseReason,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandCloseError> {
        let settlement = CloseTerminalSettlement {
            reason,
            certainty: CloseTerminalCertainty::Panicked,
        };
        self.drop_disposition = CloseDropDisposition::Terminal(settlement);
        self.commit_with_settlement(settlement)
    }

    fn commit_with_completions(
        &mut self,
        reason: ViewCloseReason,
        completions: Vec<CommandExecutionCompletion>,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandCloseError> {
        let entries = self
            .entries
            .take()
            .expect("an active close transaction owns its extracted entries")
            .into_entries();
        let settlement = CloseTerminalSettlement {
            reason,
            certainty: CloseTerminalCertainty::OutcomeUnknown,
        };
        self.prepared = Some(PreparedCloseCompletion {
            completed: entries
                .into_iter()
                .zip(completions)
                .map(|(entry, completion)| OpenGpuiCompletedCommand::new(entry, Ok(completion)))
                .collect(),
            settlement,
        });
        let barriers = self.commit_terminal_state(reason)?;
        self.journal.resolve_prefix_barriers(
            barriers,
            Err(OpenGpuiCommandPrefixBarrierError::HostClosed { reason }),
        );
        Ok(self
            .prepared
            .take()
            .expect("a prepared close transaction owns its completed commands")
            .completed)
    }

    fn commit_with_settlement(
        &mut self,
        settlement: CloseTerminalSettlement,
    ) -> Result<Vec<OpenGpuiCompletedCommand>, OpenGpuiCommandCloseError> {
        let entries = self
            .entries
            .take()
            .expect("an active close transaction owns its extracted entries");
        self.prepared = Some(PreparedCloseCompletion {
            completed: complete_close_entries(entries, settlement),
            settlement,
        });
        let barriers = self.commit_terminal_state(settlement.reason)?;
        self.journal
            .resolve_prefix_barriers(barriers, close_prefix_result(settlement));
        Ok(self
            .prepared
            .take()
            .expect("a prepared close transaction owns its completed commands")
            .completed)
    }

    fn commit_terminal_state(
        &self,
        reason: ViewCloseReason,
    ) -> Result<Vec<Rc<PrefixBarrierCompletion>>, OpenGpuiCommandCloseError> {
        self.journal.drain_deferred_abandonments();
        let mut state = self
            .journal
            .state
            .try_borrow_mut()
            .map_err(|_| OpenGpuiCommandCloseError::Busy)?;
        if state.status != JournalStatus::Closing
            || state.in_flight.is_some()
            || !state.queue.is_empty()
        {
            return Err(OpenGpuiCommandCloseError::InvalidState);
        }
        state.status = JournalStatus::Closed(reason);
        Ok(state.terminate_prefixes())
    }
}

impl std::fmt::Debug for OpenGpuiCommandCloseTransaction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenGpuiCommandCloseTransaction")
            .field("active", &self.entries.is_some())
            .field("prepared", &self.prepared.is_some())
            .finish_non_exhaustive()
    }
}

impl Drop for OpenGpuiCommandCloseTransaction {
    fn drop(&mut self) {
        let recovery = if let Some(completed) = self.prepared.take() {
            Some(DeferredCloseRecovery::Completed(completed))
        } else {
            self.entries
                .take()
                .map(|entries| match self.drop_disposition {
                    CloseDropDisposition::Rollback => DeferredCloseRecovery::Rollback(entries),
                    CloseDropDisposition::Terminal(settlement) => DeferredCloseRecovery::Terminal {
                        entries,
                        settlement,
                    },
                })
        };
        if let Some(recovery) = recovery {
            self.journal.defer_close_recovery(recovery);
        }
    }
}

fn close_ticket_failure(settlement: CloseTerminalSettlement) -> OpenGpuiCommandTicketFailure {
    match settlement.certainty {
        CloseTerminalCertainty::DefinitelyNotExecuted => {
            OpenGpuiCommandTicketFailure::definitely_not_executed(
                OpenGpuiCommandTicketError::HostClosed {
                    reason: settlement.reason,
                },
            )
        }
        CloseTerminalCertainty::OutcomeUnknown => OpenGpuiCommandTicketFailure::execution_unknown(
            OpenGpuiCommandTicketError::CloseOutcomeUnknown {
                reason: settlement.reason,
            },
        ),
        CloseTerminalCertainty::Panicked => OpenGpuiCommandTicketFailure::execution_unknown(
            OpenGpuiCommandTicketError::ClosePanicked {
                reason: settlement.reason,
            },
        ),
    }
}

fn complete_close_entries(
    entries: OpenGpuiCommandCloseEntries,
    settlement: CloseTerminalSettlement,
) -> Vec<OpenGpuiCompletedCommand> {
    entries
        .into_entries()
        .into_iter()
        .map(|entry| OpenGpuiCompletedCommand::new(entry, Err(close_ticket_failure(settlement))))
        .collect()
}

fn complete_current_journal_entries(
    state: &mut JournalState,
    settlement: CloseTerminalSettlement,
) -> Vec<OpenGpuiCompletedCommand> {
    let mut completed = Vec::new();
    if let Some(batch) = state.in_flight.take() {
        let settlement = if batch.phase == InFlightPhase::Submitted
            && matches!(
                settlement.certainty,
                CloseTerminalCertainty::DefinitelyNotExecuted
            ) {
            CloseTerminalSettlement {
                reason: settlement.reason,
                certainty: CloseTerminalCertainty::OutcomeUnknown,
            }
        } else {
            settlement
        };
        completed.extend(batch.entries.into_iter().map(|entry| {
            OpenGpuiCompletedCommand::new(entry, Err(close_ticket_failure(settlement)))
        }));
    }
    completed.extend(
        state.queue.drain(..).map(|entry| {
            OpenGpuiCompletedCommand::new(entry, Err(close_ticket_failure(settlement)))
        }),
    );
    completed
}

fn complete_dropped_close_entries(
    entries: OpenGpuiCommandCloseEntries,
) -> Vec<OpenGpuiCompletedCommand> {
    entries
        .into_entries()
        .into_iter()
        .map(|entry| {
            OpenGpuiCompletedCommand::new(
                entry,
                Err(OpenGpuiCommandTicketFailure::definitely_not_executed(
                    OpenGpuiCommandTicketError::HostDropped,
                )),
            )
        })
        .collect()
}

fn close_prefix_result(settlement: CloseTerminalSettlement) -> PrefixBarrierCompletionResult {
    match settlement.certainty {
        CloseTerminalCertainty::Panicked => Err(OpenGpuiCommandPrefixBarrierError::ClosePanicked {
            reason: settlement.reason,
        }),
        CloseTerminalCertainty::DefinitelyNotExecuted | CloseTerminalCertainty::OutcomeUnknown => {
            Err(OpenGpuiCommandPrefixBarrierError::HostClosed {
                reason: settlement.reason,
            })
        }
    }
}

fn apply_close_recovery(
    state: &mut JournalState,
    recovery: DeferredCloseRecovery,
) -> Option<(
    Vec<Rc<PrefixBarrierCompletion>>,
    PrefixBarrierCompletionResult,
)> {
    match recovery {
        DeferredCloseRecovery::Rollback(entries) => match state.status {
            JournalStatus::Open | JournalStatus::Closing => {
                entries.restore_into(state);
                state.status = JournalStatus::Open;
                None
            }
            JournalStatus::Closed(reason) => {
                state.deferred_completions.extend(complete_close_entries(
                    entries,
                    CloseTerminalSettlement {
                        reason,
                        certainty: CloseTerminalCertainty::DefinitelyNotExecuted,
                    },
                ));
                None
            }
            JournalStatus::Dropped => {
                state
                    .deferred_completions
                    .extend(complete_dropped_close_entries(entries));
                None
            }
        },
        DeferredCloseRecovery::Terminal {
            entries,
            settlement,
        } => {
            let barriers = match state.status {
                JournalStatus::Open | JournalStatus::Closing => {
                    let current = complete_current_journal_entries(state, settlement);
                    state.deferred_completions.extend(current);
                    state.status = JournalStatus::Closed(settlement.reason);
                    state.terminate_prefixes()
                }
                JournalStatus::Closed(_) | JournalStatus::Dropped => Vec::new(),
            };
            state
                .deferred_completions
                .extend(complete_close_entries(entries, settlement));
            (!barriers.is_empty()).then(|| (barriers, close_prefix_result(settlement)))
        }
        DeferredCloseRecovery::Completed(prepared) => {
            let barriers = match state.status {
                JournalStatus::Open | JournalStatus::Closing => {
                    let current = complete_current_journal_entries(state, prepared.settlement);
                    state.deferred_completions.extend(current);
                    state.status = JournalStatus::Closed(prepared.settlement.reason);
                    state.terminate_prefixes()
                }
                JournalStatus::Closed(_) | JournalStatus::Dropped => Vec::new(),
            };
            state.deferred_completions.extend(prepared.completed);
            (!barriers.is_empty()).then(|| (barriers, close_prefix_result(prepared.settlement)))
        }
    }
}

pub(crate) struct OpenGpuiCompletedCommand {
    pub(crate) origin: OpenGpuiCommandOrigin,
    result: Option<OpenGpuiCommandTicketResult>,
    completion: Option<Rc<CommandCompletion>>,
}

impl OpenGpuiCompletedCommand {
    fn new(entry: JournalEntry, result: OpenGpuiCommandTicketResult) -> Self {
        Self {
            origin: entry.origin,
            result: Some(result),
            completion: entry.completion,
        }
    }

    pub(crate) fn into_resolution(mut self) -> Option<OpenGpuiPendingCommandResolution> {
        self.take_resolution()
    }

    pub(crate) fn into_detached_result(
        mut self,
    ) -> Result<OpenGpuiCommandTicketResult, Box<OpenGpuiCompletedCommand>> {
        if self.completion.is_none() {
            match self.result.take() {
                Some(result) => Ok(result),
                None => Err(Box::new(self)),
            }
        } else {
            Err(Box::new(self))
        }
    }

    fn take_resolution(&mut self) -> Option<OpenGpuiPendingCommandResolution> {
        let result = self.result.take()?;
        let Some(completion) = self.completion.take() else {
            self.result = Some(result);
            return None;
        };
        Some(OpenGpuiPendingCommandResolution {
            completion,
            result: Some(result),
        })
    }
}

impl Drop for OpenGpuiCompletedCommand {
    fn drop(&mut self) {
        if let Some(resolution) = self.take_resolution() {
            let mut panics = OpenGpuiPanicAccumulator::default();
            panics.capture(|| resolution.resolve());
            let _ = panics.suppress();
        }
    }
}

/// Keeps an extracted terminal result live until explicit delivery or stack unwinding.
pub(crate) struct OpenGpuiPendingCommandResolution {
    completion: Rc<CommandCompletion>,
    result: Option<OpenGpuiCommandTicketResult>,
}

impl OpenGpuiPendingCommandResolution {
    pub(crate) fn resolve(mut self) {
        if let Some(result) = self.result.take() {
            self.completion.resolve(result);
        }
    }
}

impl Drop for OpenGpuiPendingCommandResolution {
    fn drop(&mut self) {
        if let Some(result) = self.result.take() {
            let mut panics = OpenGpuiPanicAccumulator::default();
            panics.capture(|| self.completion.resolve(result));
            let _ = panics.suppress();
        }
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Poll, Wake};

    use jellyflow_core::{
        CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
    };
    use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
    use jellyflow_runtime::{
        commands::CommandExecutionKind, geometry::ViewPoint, geometry::ViewRect,
        geometry::ViewSize, graph::GraphRuntime, graph::GraphVersion, view::AdapterFrame,
        view::FrameRequest, view::RendererStamp, view::ViewCloseRequest, view::ViewSession,
        view::ViewSessionOptions,
    };

    use super::*;

    const KIND: &str = "test.open-gpui-command-journal";
    const RENDERER: &str = "test.open-gpui.command-journal";

    #[derive(Default)]
    struct CountingWake {
        count: AtomicUsize,
    }

    impl Wake for CountingWake {
        fn wake(self: Arc<Self>) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn rejects_the_newest_command_when_queue_capacity_is_reached() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut tickets = Vec::with_capacity(OPEN_GPUI_COMMAND_JOURNAL_CAPACITY);

        for index in 0..OPEN_GPUI_COMMAND_JOURNAL_CAPACITY {
            tickets.push(
                journal
                    .enqueue(command(&view, index), OpenGpuiCommandOrigin::Binding)
                    .expect("command within capacity"),
            );
        }

        assert!(matches!(
            journal.enqueue(
                command(&view, OPEN_GPUI_COMMAND_JOURNAL_CAPACITY),
                OpenGpuiCommandOrigin::Binding,
            ),
            Err(OpenGpuiCommandEnqueueError::QueueFull { capacity })
                if capacity == OPEN_GPUI_COMMAND_JOURNAL_CAPACITY
        ));
        assert_eq!(tickets.len(), OPEN_GPUI_COMMAND_JOURNAL_CAPACITY);
    }

    #[test]
    fn full_journal_rejects_an_additional_prefix_reservation() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        for index in 0..OPEN_GPUI_COMMAND_JOURNAL_CAPACITY {
            drop(
                journal
                    .enqueue(command(&view, index), OpenGpuiCommandOrigin::Binding)
                    .expect("command within capacity"),
            );
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
            drop(
                journal
                    .enqueue(command(&view, index), OpenGpuiCommandOrigin::Binding)
                    .expect("command within capacity"),
            );
        }
        let first_collector = collector(&view);
        let first_batch = journal
            .collect_next_batch(&first_collector)
            .expect("collect batch")
            .expect("nonempty batch");

        assert!(matches!(
            journal
                .enqueue(command(&view, usize::MAX), OpenGpuiCommandOrigin::Binding,)
                .map(drop),
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
                OpenGpuiCommandTicketFailure::definitely_not_executed(
                    OpenGpuiCommandTicketError::FramePreparation {
                        message: "test releases the batch".to_owned(),
                    },
                )
            })
            .expect("release collected batch");
        assert_eq!(completed.len(), OPEN_GPUI_COMMAND_JOURNAL_CAPACITY);
        drop(
            journal
                .enqueue(command(&view, usize::MAX), OpenGpuiCommandOrigin::Binding)
                .expect("capacity is released with the batch"),
        );
        assert!(
            journal
                .collect_next_batch(&collector(&view))
                .expect("collect replacement batch")
                .is_some()
        );
    }

    #[test]
    fn dropping_collected_batch_terminalizes_ticket_and_releases_journal() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut abandoned_ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("abandoned collected ticket");
        let batch = journal
            .collect_next_batch(&collector(&view))
            .expect("collect abandoned batch")
            .expect("abandoned batch");

        drop(batch);

        assert!(!journal.has_pending());
        assert!(matches!(
            ready_ticket(&mut abandoned_ticket),
            Err(OpenGpuiCommandTicketFailure::DefinitelyNotExecuted(
                OpenGpuiCommandTicketError::FramePreparation { .. }
            ))
        ));
        assert!(matches!(poll_ticket(&mut abandoned_ticket), Poll::Pending));

        let mut retry_ticket = journal
            .enqueue(command(&view, 2), OpenGpuiCommandOrigin::Binding)
            .expect("retry after abandoned collected batch");
        resolve_completed(submit_next_batch(&journal, &view));
        assert!(
            ready_ticket(&mut retry_ticket)
                .expect("retry ticket transport")
                .applied_revision()
                .is_some()
        );
    }

    #[test]
    fn dropping_submitted_batch_terminalizes_unknown_ticket_and_releases_journal() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut abandoned_ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("abandoned submitted ticket");
        let collector = collector(&view);
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect submitted batch")
            .expect("submitted batch");
        journal
            .mark_submitted(&batch)
            .expect("mark submitted batch");

        drop(batch);

        assert!(!journal.has_pending());
        assert!(matches!(
            ready_ticket(&mut abandoned_ticket),
            Err(OpenGpuiCommandTicketFailure::ExecutionUnknown(
                OpenGpuiCommandTicketError::FeedbackOutcomeUnknown
            ))
        ));
        assert!(matches!(poll_ticket(&mut abandoned_ticket), Poll::Pending));

        let mut retry_ticket = journal
            .enqueue(command(&view, 2), OpenGpuiCommandOrigin::Binding)
            .expect("retry after abandoned submitted batch");
        resolve_completed(submit_next_batch(&journal, &view));
        assert!(
            ready_ticket(&mut retry_ticket)
                .expect("retry ticket transport")
                .applied_revision()
                .is_some()
        );
    }

    #[test]
    fn submitted_receipts_resolve_each_ticket_by_exact_command_identity() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut applied_ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("first ticket");
        let mut conflict_ticket = journal
            .enqueue(command(&view, 2), OpenGpuiCommandOrigin::Binding)
            .expect("second ticket");
        let collector = collector(&view);
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect commands")
            .expect("command batch");
        journal.mark_submitted(&batch).expect("submitted batch");
        let receipt = view
            .submit_feedback(collector.finish().expect("finish feedback"))
            .expect("submit feedback");
        let completed = journal
            .complete_submitted(&batch, receipt)
            .expect("pair command receipts");
        resolve_completed(completed);

        let applied = ready_ticket(&mut applied_ticket).expect("first ticket transport");
        assert!(applied.applied_revision().is_some());
        let conflict = ready_ticket(&mut conflict_ticket).expect("second ticket transport");
        assert_eq!(conflict.kind(), CommandExecutionKind::Conflict);
    }

    #[test]
    fn dropping_completed_command_resolves_its_ticket_exactly_once() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("ticket owned by completed command");
        let completed = submit_next_batch(&journal, &view)
            .into_iter()
            .next()
            .expect("completed command");
        let waker = std::task::Waker::from(Arc::new(PanickingWake));
        let mut context = Context::from_waker(&waker);

        assert!(matches!(
            Pin::new(&mut ticket).poll(&mut context),
            Poll::Pending
        ));
        drop(completed);

        assert!(
            ready_ticket(&mut ticket)
                .expect("completed command drop resolves the ticket")
                .applied_revision()
                .is_some()
        );
        assert!(matches!(poll_ticket(&mut ticket), Poll::Pending));
    }

    #[test]
    fn unwinding_after_completed_command_construction_resolves_its_ticket() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("ticket owned by completed command");
        let completed = submit_next_batch(&journal, &view)
            .into_iter()
            .next()
            .expect("completed command");

        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _completed = completed;
            panic!("injected unwind after completed command construction");
        }));
        assert!(unwind.is_err());
        assert!(
            ready_ticket(&mut ticket)
                .expect("completed command survives unwind through RAII")
                .applied_revision()
                .is_some()
        );
    }

    #[test]
    fn transferring_completed_command_to_resolution_disarms_its_drop_once() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("ticket owned by completed command");
        let completed = submit_next_batch(&journal, &view)
            .into_iter()
            .next()
            .expect("completed command");
        let wakes = Arc::new(CountingWake::default());
        let waker = std::task::Waker::from(Arc::clone(&wakes));
        let mut context = Context::from_waker(&waker);
        assert!(matches!(
            Pin::new(&mut ticket).poll(&mut context),
            Poll::Pending
        ));

        let resolution = completed
            .into_resolution()
            .expect("ticketed command resolution");
        assert_eq!(wakes.count.load(Ordering::SeqCst), 0);

        resolution.resolve();
        assert_eq!(wakes.count.load(Ordering::SeqCst), 1);
        assert!(
            ready_ticket(&mut ticket)
                .expect("transferred resolution delivers the ticket")
                .applied_revision()
                .is_some()
        );
        assert!(matches!(poll_ticket(&mut ticket), Poll::Pending));
    }

    #[test]
    fn detached_completed_command_returns_result_without_ticket_resolution() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("capture empty detached prefix");
        ready_prefix_permit(&mut barrier)
            .enqueue_detached(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("detached command");
        let completed = submit_next_batch(&journal, &view)
            .into_iter()
            .next()
            .expect("detached completed command");

        let result = match completed.into_detached_result() {
            Ok(result) => result,
            Err(_) => panic!("detached completed command must not retain a ticket completion"),
        };
        assert!(
            result
                .expect("detached command transport")
                .applied_revision()
                .is_some()
        );
    }

    #[test]
    fn unwinding_after_resolution_extraction_delivers_its_exact_terminal_result_once() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("ticket with extracted resolution");
        let resolution = submit_next_batch(&journal, &view)
            .into_iter()
            .next()
            .expect("completed command")
            .into_resolution()
            .expect("ticketed command resolution");
        let waker = std::task::Waker::from(Arc::new(PanickingWake));
        let mut context = Context::from_waker(&waker);

        assert!(matches!(
            Pin::new(&mut ticket).poll(&mut context),
            Poll::Pending
        ));
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _resolution = resolution;
            panic!("injected unwind after command resolution extraction");
        }));
        assert!(unwind.is_err());

        assert!(
            ready_ticket(&mut ticket)
                .expect("dropped resolution preserves the command result")
                .applied_revision()
                .is_some()
        );
        assert!(matches!(poll_ticket(&mut ticket), Poll::Pending));
    }

    #[test]
    fn interleaved_layout_does_not_shift_command_completion_identity() {
        let (_runtime, view) = runtime_and_view();
        let frame = command_frame(&view);
        let journal = OpenGpuiCommandJournal::new();
        let mut first_ticket = journal
            .enqueue(
                command_at(frame.graph_version(), 1),
                OpenGpuiCommandOrigin::Binding,
            )
            .expect("first ticket");
        let mut second_ticket = journal
            .enqueue(
                command_at(frame.graph_version(), 2),
                OpenGpuiCommandOrigin::Binding,
            )
            .expect("second ticket");
        let collector = frame.collector();
        collector
            .layout(
                frame.nodes()[0]
                    .begin_layout(ViewRect::new(
                        ViewPoint::default(),
                        ViewSize::new(180.0, 100.0),
                    ))
                    .finish()
                    .expect("build valid layout prefix"),
            )
            .expect("interleave a valid layout before the commands");
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect commands")
            .expect("command batch");
        journal.mark_submitted(&batch).expect("submitted batch");
        let receipt = view
            .submit_feedback(collector.finish().expect("finish feedback"))
            .expect("submit feedback");
        let completed = journal
            .complete_submitted(&batch, receipt)
            .expect("complete keyed command receipts");
        resolve_completed(completed);

        assert!(
            ready_ticket(&mut first_ticket)
                .expect("first ticket transport")
                .applied_revision()
                .is_some()
        );
        assert_eq!(
            ready_ticket(&mut second_ticket)
                .expect("second ticket transport")
                .kind(),
            CommandExecutionKind::Conflict
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
    fn reserved_admission_rejects_a_foreign_host_journal() {
        let (_runtime, view) = runtime_and_view();
        let permit_journal = OpenGpuiCommandJournal::new();
        let sink_journal = OpenGpuiCommandJournal::new();
        let sink = OpenGpuiCommandSink::new(Rc::downgrade(&sink_journal), EntityId::from(1));
        let mut barrier = permit_journal
            .capture_prefix_barrier()
            .expect("capture empty prefix");
        let permit = ready_prefix_permit(&mut barrier);

        assert!(matches!(
            sink.admit_reserved(command(&view, 1), permit),
            Err(OpenGpuiCommandEnqueueError::ForeignPrefixReservation)
        ));
        assert!(!permit_journal.has_pending());
        assert!(!sink_journal.has_pending());
    }

    #[test]
    fn prefix_permit_blocks_its_suffix_and_inserts_before_it() {
        let (_runtime, view) = runtime_and_view();
        let initial_version = graph_version(&view);
        let journal = OpenGpuiCommandJournal::new();
        drop(
            journal
                .enqueue(
                    command_at(initial_version, 1),
                    OpenGpuiCommandOrigin::Binding,
                )
                .expect("captured command"),
        );
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("capture queued prefix");
        let mut suffix_ticket = journal
            .enqueue(
                command_at(initial_version, 3),
                OpenGpuiCommandOrigin::Binding,
            )
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

        assert!(
            journal
                .collect_next_batch(&collector(&view))
                .expect("held permit blocks suffix collection")
                .is_none()
        );

        permit
            .enqueue_detached(command(&view, 2), OpenGpuiCommandOrigin::Binding)
            .expect("fill reserved prefix slot");

        resolve_completed(submit_next_batch(&journal, &view));
        assert_eq!(
            ready_ticket(&mut suffix_ticket)
                .expect("suffix command transport")
                .kind(),
            CommandExecutionKind::Conflict
        );
        assert_eq!(
            view.graph_snapshot()
                .expect("snapshot after priority and suffix")
                .revision(),
            2
        );
    }

    #[test]
    fn prefix_barrier_waits_for_in_flight_and_queued_prefix_but_not_suffix() {
        let (_runtime, view) = runtime_and_view();
        let initial_version = graph_version(&view);
        let journal = OpenGpuiCommandJournal::new();
        let mut first_ticket = journal
            .enqueue(
                command_at(initial_version, 1),
                OpenGpuiCommandOrigin::Binding,
            )
            .expect("first prefix command");
        let first_collector = collector(&view);
        let first_batch = journal
            .collect_next_batch(&first_collector)
            .expect("collect first prefix")
            .expect("first prefix batch");
        journal
            .mark_submitted(&first_batch)
            .expect("submit first prefix");
        let mut second_ticket = journal
            .enqueue(
                command_at(initial_version, 2),
                OpenGpuiCommandOrigin::Binding,
            )
            .expect("second prefix command");
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("capture in-flight and queued prefix");
        let mut suffix_ticket = journal
            .enqueue(
                command_at(initial_version, 4),
                OpenGpuiCommandOrigin::Binding,
            )
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
                .complete_submitted(&first_batch, first_receipt)
                .expect("complete first prefix"),
        );
        assert!(
            ready_ticket(&mut first_ticket)
                .expect("first prefix transport")
                .applied_revision()
                .is_some()
        );
        assert!(matches!(poll_prefix_barrier(&mut barrier), Poll::Pending));

        resolve_completed(submit_next_batch(&journal, &view));
        assert_eq!(
            ready_ticket(&mut second_ticket)
                .expect("second prefix transport")
                .kind(),
            CommandExecutionKind::Conflict
        );
        let permit = ready_prefix_permit(&mut barrier);
        assert!(matches!(poll_ticket(&mut suffix_ticket), Poll::Pending));

        let sink = OpenGpuiCommandSink::new(Rc::downgrade(&journal), EntityId::from(1));
        let (mut priority_ticket, _) = sink
            .admit_reserved(command(&view, 3), permit)
            .expect("priority command admission")
            .into_parts();
        resolve_completed(submit_next_batch(&journal, &view));
        assert!(
            ready_ticket(&mut priority_ticket)
                .expect("priority transport")
                .applied_revision()
                .is_some()
        );
        assert_eq!(
            ready_ticket(&mut suffix_ticket)
                .expect("suffix transport")
                .kind(),
            CommandExecutionKind::Conflict
        );
    }

    #[test]
    fn later_barrier_waits_for_an_earlier_permit_and_its_suffix_in_sequence_order() {
        let (_runtime, view) = runtime_and_view();
        let initial_version = graph_version(&view);
        let journal = OpenGpuiCommandJournal::new();
        drop(
            journal
                .enqueue(
                    command_at(initial_version, 1),
                    OpenGpuiCommandOrigin::Binding,
                )
                .expect("initial prefix"),
        );
        let mut first_barrier = journal.capture_prefix_barrier().expect("first barrier");
        let mut suffix_ticket = journal
            .enqueue(
                command_at(initial_version, 3),
                OpenGpuiCommandOrigin::Binding,
            )
            .expect("first suffix");
        let mut second_barrier = journal.capture_prefix_barrier().expect("second barrier");
        let mut final_ticket = journal
            .enqueue(
                command_at(initial_version, 4),
                OpenGpuiCommandOrigin::Binding,
            )
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
            .enqueue_detached(command(&view, 2), OpenGpuiCommandOrigin::Binding)
            .expect("fill first reservation");
        resolve_completed(submit_next_batch(&journal, &view));
        assert_eq!(
            ready_ticket(&mut suffix_ticket)
                .expect("first suffix transport")
                .kind(),
            CommandExecutionKind::Conflict
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
        assert_eq!(
            ready_ticket(&mut final_ticket)
                .expect("final suffix transport")
                .kind(),
            CommandExecutionKind::Conflict
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
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
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
                .applied_revision()
                .is_some()
        );
        assert!(!journal.has_pending());
    }

    #[test]
    fn aborting_close_restores_the_prefix_without_completing_its_barrier() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        drop(
            journal
                .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
                .expect("pending command"),
        );
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("capture pending prefix");
        let close = journal.begin_close().expect("begin close");

        assert!(matches!(poll_prefix_barrier(&mut barrier), Poll::Pending));
        close.abort().expect("abort close");
        assert!(matches!(poll_prefix_barrier(&mut barrier), Poll::Pending));

        resolve_completed(submit_next_batch(&journal, &view));
        ready_prefix_permit(&mut barrier).abandon();
    }

    #[test]
    fn dropping_close_transaction_restores_its_exact_pending_ticket() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("pending close ticket");
        let collector = collector(&view);
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect close batch")
            .expect("pending close batch");

        let close = journal.begin_close().expect("begin close transaction");
        drop(close);
        journal
            .mark_submitted(&batch)
            .expect("restored batch keeps its identity");
        let receipt = view
            .submit_feedback(collector.finish().expect("finish restored feedback"))
            .expect("submit restored feedback");
        resolve_completed(
            journal
                .complete_submitted(&batch, receipt)
                .expect("complete restored batch"),
        );
        assert!(
            ready_ticket(&mut ticket)
                .expect("restored ticket transport")
                .applied_revision()
                .is_some()
        );
    }

    #[test]
    fn invalid_terminal_finish_preserves_the_ticket_under_existing_close_reason() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("pending close ticket");
        let close = journal.begin_close().expect("begin close transaction");
        let reason = ViewCloseReason::HostClosed;

        assert!(journal.terminate(reason).is_empty());
        assert!(matches!(
            close.finish_without_execution(reason),
            Err(OpenGpuiCommandCloseError::InvalidState)
        ));
        resolve_completed(journal.take_deferred_completions());

        assert!(matches!(
            ready_ticket(&mut ticket),
            Err(OpenGpuiCommandTicketFailure::DefinitelyNotExecuted(
                OpenGpuiCommandTicketError::HostClosed {
                    reason: ViewCloseReason::HostClosed
                }
            ))
        ));
        assert!(matches!(
            journal.begin_close(),
            Err(OpenGpuiCommandCloseError::Closed {
                reason: ViewCloseReason::HostClosed
            })
        ));
    }

    #[test]
    fn uncorrelated_close_completion_terminally_settles_instead_of_losing_the_ticket() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("pending close ticket");
        let close = journal.begin_close().expect("begin close transaction");
        let reason = ViewCloseReason::HostClosed;

        assert!(matches!(
            close.finish(reason, Vec::new()),
            Err(OpenGpuiCommandCloseError::Correlation(
                CommandCompletionCorrelationError::MissingIdentity { .. }
            ))
        ));
        resolve_completed(journal.take_deferred_completions());

        assert!(matches!(
            ready_ticket(&mut ticket),
            Err(OpenGpuiCommandTicketFailure::ExecutionUnknown(
                OpenGpuiCommandTicketError::CloseOutcomeUnknown {
                    reason: ViewCloseReason::HostClosed
                }
            ))
        ));
        assert!(matches!(
            journal.begin_close(),
            Err(OpenGpuiCommandCloseError::Closed {
                reason: ViewCloseReason::HostClosed
            })
        ));
    }

    struct PanickingWake;

    impl Wake for PanickingWake {
        fn wake(self: Arc<Self>) {
            panic!("test prefix wake panics during close commit");
        }
    }

    #[test]
    fn finish_unwind_retains_completed_commands_after_terminal_state_commit() {
        let (runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("pending close ticket");
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("capture pending prefix");
        let mut later_barrier = journal
            .capture_prefix_barrier()
            .expect("capture later pending prefix");
        let waker = std::task::Waker::from(Arc::new(PanickingWake));
        let mut context = Context::from_waker(&waker);
        assert!(matches!(
            Pin::new(&mut barrier).poll(&mut context),
            Poll::Pending
        ));

        let mut close = journal.begin_close().expect("begin close transaction");
        let identities = close.command_identities();
        let request = close.combined_request(&ViewCloseRequest::default());
        let mut outcome = view.close_with_request(request).expect("close view");
        let reason = outcome.reason();
        close.confirm_view_terminated(reason);
        let completions = outcome
            .take_command_completions(&identities)
            .expect("correlated close completions");

        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = close.finish(reason, completions);
        }));
        assert!(unwind.is_err());
        resolve_completed(journal.take_deferred_completions());

        assert!(
            ready_ticket(&mut ticket)
                .expect("completion retained through finish unwind")
                .applied_revision()
                .is_some()
        );
        assert_eq!(runtime.snapshot().expect("closed graph").revision(), 1);
        assert!(matches!(
            poll_prefix_barrier(&mut barrier),
            Poll::Ready(Err(OpenGpuiCommandPrefixBarrierError::HostClosed {
                reason: ViewCloseReason::HostClosed
            }))
        ));
        assert!(matches!(
            poll_prefix_barrier(&mut later_barrier),
            Poll::Ready(Err(OpenGpuiCommandPrefixBarrierError::HostClosed {
                reason: ViewCloseReason::HostClosed
            }))
        ));
        assert!(!journal.has_pending());
    }

    #[test]
    fn close_drop_and_close_panic_terminate_prefix_barriers() {
        let (_runtime, view) = runtime_and_view();
        let reason = ViewCloseReason::HostClosed;

        let close_journal = OpenGpuiCommandJournal::new();
        drop(
            close_journal
                .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
                .expect("close command"),
        );
        let mut close_barrier = close_journal
            .capture_prefix_barrier()
            .expect("close barrier");
        let close = close_journal.begin_close().expect("begin normal close");
        let _ = close
            .finish_without_execution(reason)
            .expect("finish normal close");
        assert!(matches!(
            poll_prefix_barrier(&mut close_barrier),
            Poll::Ready(Err(OpenGpuiCommandPrefixBarrierError::HostClosed {
                reason: ViewCloseReason::HostClosed
            }))
        ));

        let panic_journal = OpenGpuiCommandJournal::new();
        drop(
            panic_journal
                .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
                .expect("panic command"),
        );
        let mut panic_barrier = panic_journal
            .capture_prefix_barrier()
            .expect("panic barrier");
        let close = panic_journal.begin_close().expect("begin panic close");
        let _ = close
            .finish_after_panic(reason)
            .expect("finish panic close");
        assert!(matches!(
            poll_prefix_barrier(&mut panic_barrier),
            Poll::Ready(Err(OpenGpuiCommandPrefixBarrierError::ClosePanicked {
                reason: ViewCloseReason::HostClosed
            }))
        ));

        let mut drop_barrier = {
            let drop_journal = OpenGpuiCommandJournal::new();
            drop(
                drop_journal
                    .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
                    .expect("drop command"),
            );
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
        let stale_version = graph_version(&view);
        let journal = OpenGpuiCommandJournal::new();
        drop(
            journal
                .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
                .expect("prefix command"),
        );
        let mut barrier = journal.capture_prefix_barrier().expect("capture prefix");
        resolve_completed(submit_next_batch(&journal, &view));
        let permit = ready_prefix_permit(&mut barrier);
        assert_eq!(runtime.snapshot().expect("fresh snapshot").revision(), 1);

        let sink = OpenGpuiCommandSink::new(Rc::downgrade(&journal), EntityId::from(1));
        let (mut stale_ticket, _) = sink
            .admit_reserved(command_at(stale_version, 2), permit)
            .expect("stale follow-up command admission")
            .into_parts();
        resolve_completed(submit_next_batch(&journal, &view));

        let completion = ready_ticket(&mut stale_ticket).expect("stale command transport");
        assert_eq!(completion.kind(), CommandExecutionKind::Conflict);
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
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("suffix command");
        let permit = ready_prefix_permit(&mut barrier);
        let sink = OpenGpuiCommandSink::new(Rc::downgrade(&journal), EntityId::from(1));
        let (mut rejected_ticket, _) = sink
            .admit_reserved(
                GraphCommand::set_node_data(
                    graph_version(&view),
                    NodeId::from_u128(0xdead),
                    serde_json::json!({ "value": "missing" }),
                ),
                permit,
            )
            .expect("reserved rejected command admission")
            .into_parts();

        resolve_completed(submit_next_batch(&journal, &view));

        assert_eq!(
            ready_ticket(&mut rejected_ticket)
                .expect("rejected command transport")
                .kind(),
            CommandExecutionKind::Rejected
        );
        assert!(
            ready_ticket(&mut suffix_ticket)
                .expect("suffix transport")
                .applied_revision()
                .is_some()
        );
        assert!(!journal.has_pending());
    }

    #[test]
    fn close_drains_collected_then_queued_commands_before_caller_commands() {
        let (runtime, view) = runtime_and_view();
        let close_version = graph_version(&view);
        let journal = OpenGpuiCommandJournal::new();
        let mut collected_ticket = journal
            .enqueue(command_at(close_version, 1), OpenGpuiCommandOrigin::Binding)
            .expect("collected ticket");
        let batch_collector = collector(&view);
        let _batch_lease = journal
            .collect_next_batch(&batch_collector)
            .expect("collect first close command")
            .expect("collected close batch");
        let mut queued_ticket = journal
            .enqueue(command_at(close_version, 2), OpenGpuiCommandOrigin::Binding)
            .expect("queued ticket");
        let mut close = journal.begin_close().expect("begin close transaction");
        let journal_identities = close.command_identities();
        assert_eq!(journal_identities.len(), 2);
        let caller_command = command_at(close_version, 3);
        let caller_identity = caller_command.identity().clone();
        let request = close.combined_request(&ViewCloseRequest::new().with_command(caller_command));
        let mut outcome = view.close_with_request(request).expect("close view");
        let reason = outcome.reason();
        close.confirm_view_terminated(reason);
        let mut journal_completions = outcome
            .take_command_completions(&journal_identities)
            .expect("take journal completions by identity");
        journal_completions.reverse();
        let completed = close
            .finish(reason, journal_completions)
            .expect("finish journal close");
        resolve_completed(completed);

        assert_eq!(
            ready_ticket(&mut collected_ticket)
                .expect("collected command transport")
                .applied_revision()
                .expect("collected command applied"),
            1
        );
        assert_eq!(
            ready_ticket(&mut queued_ticket)
                .expect("queued command transport")
                .kind(),
            CommandExecutionKind::Conflict
        );
        let caller_completion = outcome
            .take_command_completion(&caller_identity)
            .expect("take caller completion by identity");
        assert!(
            caller_completion
                .into_result()
                .expect("caller command transport")
                .conflict()
                .is_some()
        );
        assert_eq!(
            runtime
                .snapshot()
                .expect("final snapshot")
                .graph()
                .node(&node_id())
                .expect("journal node")
                .data,
            serde_json::json!({ "value": 1 })
        );
    }

    #[test]
    fn close_rejects_a_submitted_batch_without_retrying_it() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        drop(
            journal
                .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
                .expect("command"),
        );
        let collector = collector(&view);
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect batch")
            .expect("command batch");
        journal.mark_submitted(&batch).expect("submit batch");

        assert!(matches!(
            journal.begin_close(),
            Err(OpenGpuiCommandCloseError::BatchSubmitting)
        ));
        assert!(journal.has_pending());
    }

    #[test]
    fn dropping_the_host_resolution_cancels_every_pending_ticket() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("pending ticket");

        assert!(matches!(poll_ticket(&mut ticket), Poll::Pending));
        resolve_completed(journal.terminate_dropped());
        assert!(matches!(
            ready_ticket(&mut ticket),
            Err(OpenGpuiCommandTicketFailure::DefinitelyNotExecuted(
                OpenGpuiCommandTicketError::HostDropped
            ))
        ));
    }

    #[test]
    fn dropping_the_host_after_submission_reports_unknown_execution() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("submitted ticket");
        let collector = collector(&view);
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect submitted batch")
            .expect("submitted batch");
        journal
            .mark_submitted(&batch)
            .expect("mark batch submitted");

        resolve_completed(journal.terminate_dropped());

        assert!(matches!(
            ready_ticket(&mut ticket),
            Err(OpenGpuiCommandTicketFailure::ExecutionUnknown(
                OpenGpuiCommandTicketError::HostDropped
            ))
        ));
        drop(batch);
    }

    #[test]
    fn closing_the_host_after_submission_reports_unknown_execution() {
        let (_runtime, view) = runtime_and_view();
        let journal = OpenGpuiCommandJournal::new();
        let mut ticket = journal
            .enqueue(command(&view, 1), OpenGpuiCommandOrigin::Binding)
            .expect("submitted ticket");
        let collector = collector(&view);
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect submitted batch")
            .expect("submitted batch");
        journal
            .mark_submitted(&batch)
            .expect("mark batch submitted");

        resolve_completed(journal.terminate(ViewCloseReason::HostClosed));

        assert!(matches!(
            ready_ticket(&mut ticket),
            Err(OpenGpuiCommandTicketFailure::ExecutionUnknown(
                OpenGpuiCommandTicketError::HostClosed {
                    reason: ViewCloseReason::HostClosed
                }
            ))
        ));
        drop(batch);
    }

    fn resolve_completed(completed: Vec<OpenGpuiCompletedCommand>) {
        resolve_command_tickets(
            completed
                .into_iter()
                .filter_map(OpenGpuiCompletedCommand::into_resolution),
        );
    }

    fn submit_next_batch(
        journal: &Rc<OpenGpuiCommandJournal>,
        view: &ViewSession,
    ) -> Vec<OpenGpuiCompletedCommand> {
        let collector = collector(view);
        let batch = journal
            .collect_next_batch(&collector)
            .expect("collect next batch")
            .expect("nonempty command batch");
        journal.mark_submitted(&batch).expect("submit next batch");
        let receipt = view
            .submit_feedback(collector.finish().expect("finish next feedback"))
            .expect("submit next feedback");
        journal
            .complete_submitted(&batch, receipt)
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
            .register(NodeKindSpec::builder(KIND).build())
            .expect("command journal kind");
        let runtime = GraphRuntime::builder(catalog)
            .build(graph.build().expect("command journal graph"))
            .expect("command journal runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("command journal view");
        (runtime, view)
    }

    fn command_frame(view: &ViewSession) -> AdapterFrame {
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
    }

    fn collector(view: &ViewSession) -> FeedbackCollector {
        command_frame(view).collector()
    }

    fn graph_version(view: &ViewSession) -> GraphVersion {
        view.graph_snapshot()
            .expect("command journal graph version")
            .version()
    }

    fn command(view: &ViewSession, value: usize) -> GraphCommand {
        command_at(graph_version(view), value)
    }

    fn command_at(expected_version: GraphVersion, value: usize) -> GraphCommand {
        GraphCommand::set_node_data(
            expected_version,
            node_id(),
            serde_json::json!({ "value": value }),
        )
    }

    fn node_id() -> NodeId {
        NodeId::from_u128(0xc002)
    }
}
