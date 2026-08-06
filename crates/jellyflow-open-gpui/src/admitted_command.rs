use std::cell::{Cell, RefCell};
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use open_gpui::{App, Task};

use crate::activity::{
    OpenGpuiActivityAcquireError, OpenGpuiActivityKind, OpenGpuiActivityLease,
    OpenGpuiNodeActivityRegistry,
};
use crate::command_journal::{
    OpenGpuiCommandNotification, OpenGpuiCommandTicket, OpenGpuiCommandTicketResult,
};
use crate::panic_sink::{OpenGpuiPanicAccumulator, OpenGpuiPanicPayload};

/// Post-admission evidence produced without interpreting the runtime command outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdmittedCommandUncertainty {
    RunnerDropped,
    RunnerPanicked,
    TaskInstallationUnwound,
    HostNotificationUnwound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdmittedCommandTaskPhase {
    NeedsInstall,
    NeedsNotify,
    AwaitingTicket,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdmittedCommandDrive {
    AwaitingTicket,
    Unavailable,
}

pub(crate) struct AdmittedCommandDriveUnwind {
    reason: AdmittedCommandUncertainty,
    payload: OpenGpuiPanicPayload,
}

impl AdmittedCommandDriveUnwind {
    fn new(reason: AdmittedCommandUncertainty, payload: OpenGpuiPanicPayload) -> Self {
        Self { reason, payload }
    }

    pub(crate) const fn reason(&self) -> AdmittedCommandUncertainty {
        self.reason
    }

    pub(crate) fn resume(self) -> ! {
        resume_unwind(self.payload)
    }

    pub(crate) fn suppress(self) -> bool {
        let mut panics = OpenGpuiPanicAccumulator::default();
        panics.push(Some(self.payload));
        panics.suppress()
    }
}

/// One attempt-correlated event emitted by an admitted command task.
#[derive(Debug)]
pub(crate) enum AdmittedCommandEvent<A> {
    Settled {
        attempt: A,
        result: Box<OpenGpuiCommandTicketResult>,
    },
    Uncertain {
        attempt: A,
        reason: AdmittedCommandUncertainty,
    },
}

impl<A: Copy> AdmittedCommandEvent<A> {
    pub(crate) const fn attempt(&self) -> A {
        match self {
            Self::Settled { attempt, .. } | Self::Uncertain { attempt, .. } => *attempt,
        }
    }

    const fn priority(&self) -> u8 {
        match self {
            Self::Settled { .. } => 4,
            Self::Uncertain {
                reason: AdmittedCommandUncertainty::RunnerPanicked,
                ..
            } => 3,
            Self::Uncertain {
                reason:
                    AdmittedCommandUncertainty::TaskInstallationUnwound
                    | AdmittedCommandUncertainty::HostNotificationUnwound,
                ..
            } => 2,
            Self::Uncertain {
                reason: AdmittedCommandUncertainty::RunnerDropped,
                ..
            } => 1,
        }
    }
}

/// Transactional ownership of one admitted-command event.
///
/// Dropping an uncommitted guard restores the event without replacing any more precise event that
/// arrived while the consumer was preparing its state transition.
pub(crate) struct AdmittedCommandEventGuard<A>
where
    A: Copy,
{
    cell: Rc<AdmittedCommandAttemptCell<A>>,
    event: Option<AdmittedCommandEvent<A>>,
}

impl<A> AdmittedCommandEventGuard<A>
where
    A: Copy,
{
    fn new(cell: Rc<AdmittedCommandAttemptCell<A>>, event: AdmittedCommandEvent<A>) -> Self {
        Self {
            cell,
            event: Some(event),
        }
    }

    pub(crate) fn event(&self) -> &AdmittedCommandEvent<A> {
        self.event
            .as_ref()
            .expect("an uncommitted admitted-command event guard owns its event")
    }

    pub(crate) fn commit(mut self) {
        self.event.take();
    }
}

impl<A> Drop for AdmittedCommandEventGuard<A>
where
    A: Copy,
{
    fn drop(&mut self) {
        if let Some(event) = self.event.take() {
            self.cell.record(event);
        }
    }
}

struct AdmittedCommandAttemptCell<A> {
    attempt: A,
    event: RefCell<Option<AdmittedCommandEvent<A>>>,
    suppress_runner_drop: Cell<bool>,
}

impl<A: Copy> AdmittedCommandAttemptCell<A> {
    fn new(attempt: A) -> Self {
        Self {
            attempt,
            event: RefCell::new(None),
            suppress_runner_drop: Cell::new(false),
        }
    }

    fn record_settlement(&self, result: OpenGpuiCommandTicketResult) {
        self.record(AdmittedCommandEvent::Settled {
            attempt: self.attempt,
            result: Box::new(result),
        });
    }

    fn record_uncertainty(&self, reason: AdmittedCommandUncertainty) {
        if reason == AdmittedCommandUncertainty::RunnerDropped && self.suppress_runner_drop.get() {
            return;
        }
        self.record(AdmittedCommandEvent::Uncertain {
            attempt: self.attempt,
            reason,
        });
    }

    fn record(&self, candidate: AdmittedCommandEvent<A>) {
        let mut current = self.event.borrow_mut();
        if current
            .as_ref()
            .is_some_and(|event| event.priority() >= candidate.priority())
        {
            return;
        }
        *current = Some(candidate);
    }

    fn take_event(&self) -> Option<AdmittedCommandEvent<A>> {
        self.event.borrow_mut().take()
    }

    fn has_event(&self) -> bool {
        self.event.borrow().is_some()
    }

    fn shutdown(&self) {
        self.suppress_runner_drop.set(true);
        self.event.borrow_mut().take();
    }
}

/// Opaque proof that the task-retention activity lease has the correct activity kind.
pub(crate) struct AdmittedCommandTaskLease {
    _lease: OpenGpuiActivityLease,
}

impl AdmittedCommandTaskLease {
    pub(crate) fn acquire(
        activity: &OpenGpuiNodeActivityRegistry,
    ) -> Result<Self, OpenGpuiActivityAcquireError> {
        activity
            .acquire(OpenGpuiActivityKind::Task)
            .map(|lease| Self { _lease: lease })
    }
}

/// Future that owns a raw command ticket after journal admission.
///
/// Panics from polling the ticket are converted before this future is dropped, so the more precise
/// runner-panic event cannot be overwritten by the unresolved-drop fallback.
struct AdmittedCommandRunner<A, F>
where
    A: Copy,
    F: Future<Output = OpenGpuiCommandTicketResult>,
{
    ticket: Pin<Box<F>>,
    cell: Rc<AdmittedCommandAttemptCell<A>>,
    terminal: bool,
}

impl<A, F> AdmittedCommandRunner<A, F>
where
    A: Copy,
    F: Future<Output = OpenGpuiCommandTicketResult>,
{
    fn new(ticket: F, cell: Rc<AdmittedCommandAttemptCell<A>>) -> Self {
        Self {
            ticket: Box::pin(ticket),
            cell,
            terminal: false,
        }
    }
}

impl<A, F> Future for AdmittedCommandRunner<A, F>
where
    A: Copy + Unpin,
    F: Future<Output = OpenGpuiCommandTicketResult>,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let ticket = catch_unwind(AssertUnwindSafe(|| self.ticket.as_mut().poll(context)));
        match ticket {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(result)) => {
                self.terminal = true;
                self.cell.record_settlement(result);
                Poll::Ready(())
            }
            Err(panic) => {
                self.terminal = true;
                self.cell
                    .record_uncertainty(AdmittedCommandUncertainty::RunnerPanicked);
                let mut panics = OpenGpuiPanicAccumulator::default();
                panics.push(Some(panic));
                let _ = panics.suppress();
                Poll::Ready(())
            }
        }
    }
}

impl<A, F> Drop for AdmittedCommandRunner<A, F>
where
    A: Copy,
    F: Future<Output = OpenGpuiCommandTicketResult>,
{
    fn drop(&mut self) {
        if !self.terminal {
            self.cell
                .record_uncertainty(AdmittedCommandUncertainty::RunnerDropped);
        }
    }
}

/// Attempt-keyed owner for all native resources created after command admission.
///
/// Every owner has a private completion cell. Replacing or dropping an old attempt therefore
/// cannot occupy, clear, or suppress the successor attempt's mailbox.
pub(crate) struct AdmittedCommandTask<A>
where
    A: Copy,
{
    attempt: A,
    runner: Rc<RefCell<Option<AdmittedCommandRunner<A, OpenGpuiCommandTicket>>>>,
    task: Option<Task<()>>,
    notification: Option<OpenGpuiCommandNotification>,
    lease: Option<AdmittedCommandTaskLease>,
    cell: Rc<AdmittedCommandAttemptCell<A>>,
    phase: AdmittedCommandTaskPhase,
    #[cfg(test)]
    setup_fault: bool,
}

impl<A> AdmittedCommandTask<A>
where
    A: Copy + Eq + Unpin + 'static,
{
    pub(crate) fn new(
        attempt: A,
        ticket: OpenGpuiCommandTicket,
        notification: OpenGpuiCommandNotification,
        lease: AdmittedCommandTaskLease,
    ) -> Self {
        let cell = Rc::new(AdmittedCommandAttemptCell::new(attempt));
        Self {
            attempt,
            runner: Rc::new(RefCell::new(Some(AdmittedCommandRunner::new(
                ticket,
                Rc::clone(&cell),
            )))),
            task: None,
            notification: Some(notification),
            lease: Some(lease),
            cell,
            phase: AdmittedCommandTaskPhase::NeedsInstall,
            #[cfg(test)]
            setup_fault: false,
        }
    }

    pub(crate) const fn attempt(&self) -> A {
        self.attempt
    }

    /// Advances only the missing native setup stages for this exact admitted command.
    ///
    /// An unwind leaves the current phase and every unconsumed resource intact so the owner can
    /// retry without re-enqueuing the graph command.
    pub(crate) fn drive(
        &mut self,
        cx: &mut App,
    ) -> Result<AdmittedCommandDrive, AdmittedCommandDriveUnwind> {
        if self.phase == AdmittedCommandTaskPhase::NeedsInstall {
            if self.runner.borrow().is_none() {
                return Ok(AdmittedCommandDrive::Unavailable);
            }
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| self.install(cx))) {
                return Err(AdmittedCommandDriveUnwind::new(
                    AdmittedCommandUncertainty::TaskInstallationUnwound,
                    payload,
                ));
            }
            self.phase = AdmittedCommandTaskPhase::NeedsNotify;
        }
        if self.phase == AdmittedCommandTaskPhase::NeedsNotify {
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| self.wake_host(cx))) {
                return Err(AdmittedCommandDriveUnwind::new(
                    AdmittedCommandUncertainty::HostNotificationUnwound,
                    payload,
                ));
            }
            self.phase = AdmittedCommandTaskPhase::AwaitingTicket;
        }
        Ok(AdmittedCommandDrive::AwaitingTicket)
    }

    fn install(&mut self, cx: &mut App) {
        if self.task.is_some() {
            return;
        }
        #[cfg(test)]
        if std::mem::take(&mut self.setup_fault) {
            panic!("injected admitted command setup fault");
        }
        let runner = Rc::clone(&self.runner);
        let cell = Rc::clone(&self.cell);
        self.task = Some(cx.spawn(async move |cx| {
            let runner = runner.borrow_mut().take();
            if let Some(runner) = runner {
                runner.await;
            }
            if cell.has_event() {
                cx.refresh();
            }
        }));
    }

    /// Wakes the exact host that owns the admitted journal entry.
    ///
    /// The notification remains owned until the call returns, so an unwind can be retried without
    /// replaying or re-enqueuing the command.
    fn wake_host(&mut self, cx: &mut App) {
        if let Some(notification) = self.notification {
            notification.notify(cx);
            self.notification = None;
        }
    }

    pub(crate) fn take_event_guard(&self) -> Option<AdmittedCommandEventGuard<A>> {
        self.cell
            .take_event()
            .map(|event| AdmittedCommandEventGuard::new(Rc::clone(&self.cell), event))
    }

    pub(crate) fn record_uncertainty(&self, reason: AdmittedCommandUncertainty) {
        self.cell.record_uncertainty(reason);
    }

    #[cfg(test)]
    pub(crate) fn inject_setup_fault_for_test(&mut self) {
        self.setup_fault = true;
    }

    /// Cancels one exact attempt without manufacturing an unresolved-drop event.
    pub(crate) fn release_if(slot: &mut Option<Self>, attempt: A) -> bool {
        let Some(mut active) = slot.take_if(|active| active.attempt == attempt) else {
            return false;
        };
        active.suppress_and_cancel();
        true
    }

    /// Cancels any active attempt during retirement or host shutdown.
    pub(crate) fn shutdown(slot: &mut Option<Self>) {
        if let Some(mut active) = slot.take() {
            active.suppress_and_cancel();
        }
    }

    fn suppress_and_cancel(&mut self) {
        self.cell.shutdown();
        self.runner.borrow_mut().take();
        self.task.take();
        self.notification.take();
        self.lease.take();
    }

    #[cfg(test)]
    pub(crate) fn without_transport(attempt: A) -> Self {
        let cell = Rc::new(AdmittedCommandAttemptCell::new(attempt));
        Self {
            attempt,
            runner: Rc::new(RefCell::new(None)),
            task: None,
            notification: None,
            lease: None,
            cell,
            phase: AdmittedCommandTaskPhase::NeedsInstall,
            setup_fault: false,
        }
    }
}

impl<A> Drop for AdmittedCommandTask<A>
where
    A: Copy,
{
    fn drop(&mut self) {
        self.cell.shutdown();
        self.runner.borrow_mut().take();
        self.task.take();
        self.notification.take();
        self.lease.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PanickingTicket;

    impl Future for PanickingTicket {
        type Output = OpenGpuiCommandTicketResult;

        fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
            panic!("ticket runner panic")
        }
    }

    #[test]
    fn runner_panic_is_not_overwritten_by_runner_drop() {
        let cell = Rc::new(AdmittedCommandAttemptCell::new(7_u64));
        let runner = AdmittedCommandRunner::new(PanickingTicket, Rc::clone(&cell));
        let mut runner = Box::pin(runner);
        let mut context = Context::from_waker(std::task::Waker::noop());

        assert_eq!(runner.as_mut().poll(&mut context), Poll::Ready(()));
        drop(runner);

        assert!(matches!(
            cell.take_event(),
            Some(AdmittedCommandEvent::Uncertain {
                attempt: 7,
                reason: AdmittedCommandUncertainty::RunnerPanicked,
            })
        ));
    }

    #[test]
    fn attempt_cells_isolate_late_old_events() {
        let old = Rc::new(AdmittedCommandAttemptCell::new(1_u64));
        let current = Rc::new(AdmittedCommandAttemptCell::new(2_u64));

        old.record_uncertainty(AdmittedCommandUncertainty::RunnerDropped);

        assert!(current.take_event().is_none());
        assert_eq!(old.take_event().map(|event| event.attempt()), Some(1),);
    }

    #[test]
    fn shutdown_and_release_suppress_synthetic_uncertainty() {
        let task = AdmittedCommandTask::without_transport(3_u64);
        let cell = Rc::clone(&task.cell);
        let mut slot = Some(task);

        assert!(AdmittedCommandTask::release_if(&mut slot, 3));
        cell.record_uncertainty(AdmittedCommandUncertainty::RunnerDropped);

        assert!(slot.is_none());
        assert!(cell.take_event().is_none());

        let task = AdmittedCommandTask::without_transport(4_u64);
        let cell = Rc::clone(&task.cell);
        let mut slot = Some(task);
        AdmittedCommandTask::shutdown(&mut slot);
        cell.record_uncertainty(AdmittedCommandUncertainty::RunnerDropped);

        assert!(slot.is_none());
        assert!(cell.take_event().is_none());
    }
}
