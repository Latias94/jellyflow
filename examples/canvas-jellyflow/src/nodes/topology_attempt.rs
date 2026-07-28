use std::cell::RefCell;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll};

use futures_util::FutureExt;
use jellyflow_open_gpui::activity::{
    OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiNodeActivityHandle,
};
use jellyflow_open_gpui::authoring::{
    OpenGpuiCommandTicket, OpenGpuiCommandTicketResult, OpenGpuiNodeAuthoring,
    OpenGpuiNodeAuthoringError,
};
use jellyflow_runtime::commands::GraphCommand;
use jellyflow_runtime::graph::GraphRevision;
use jellyflow_runtime::reconciliation::{
    MutationAttemptId, MutationBeginError, MutationCommandResult, MutationPhase,
    MutationReconciler, MutationRetry, MutationTransition,
};
use jellyflow_runtime::view::{NodeInstanceKey, ViewCloseReason};
use open_gpui::{Context, Task};

const TASK_INSTALLATION_UNWOUND_MESSAGE: &str =
    "topology edit outcome is uncertain: task installation unwound after the command was enqueued";
const DISPATCH_ATTEMPT_DROPPED_MESSAGE: &str = "topology edit outcome is uncertain: the dispatched ticket capability was dropped before its ticket settled";
const DISPATCH_TASK_PANICKED_MESSAGE: &str =
    "topology edit outcome is uncertain: the dispatched task panicked before its ticket settled";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TopologyAttemptPhase {
    Idle,
    Pending,
    AwaitingSnapshot,
    Recoverable,
    Uncertain,
    Retired,
    Closed,
}

#[derive(Debug)]
pub(super) enum TopologyAttemptDispatchError {
    Busy,
    RetryExpired,
    StaleRetryBase {
        required_revision: GraphRevision,
        received_revision: GraphRevision,
    },
    IdentityExhausted,
    Retired,
    Closed { reason: ViewCloseReason },
    Activity { message: String },
    NotDispatched { message: String },
}

impl std::fmt::Display for TopologyAttemptDispatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => formatter.write_str("another topology edit is already active"),
            Self::RetryExpired => {
                formatter.write_str("the topology retry capability is no longer current")
            }
            Self::StaleRetryBase {
                required_revision,
                received_revision,
            } => write!(
                formatter,
                "topology retry requires revision {required_revision} or newer, received {received_revision}"
            ),
            Self::IdentityExhausted => {
                formatter.write_str("topology attempt identity space is exhausted")
            }
            Self::Retired => {
                formatter.write_str("the topology edit belongs to a retired node instance")
            }
            Self::Closed { reason } => {
                write!(formatter, "the topology edit owner is closed: {reason:?}")
            }
            Self::Activity { message } => {
                write!(
                    formatter,
                    "failed to acquire topology activity ownership: {message}"
                )
            }
            Self::NotDispatched { message } => {
                write!(formatter, "the topology edit was not dispatched: {message}")
            }
        }
    }
}

impl std::error::Error for TopologyAttemptDispatchError {}

enum TopologyTaskCompletion {
    Ticket {
        attempt: MutationAttemptId,
        result: Box<OpenGpuiCommandTicketResult>,
    },
    OutcomeUnknown {
        attempt: MutationAttemptId,
        message: &'static str,
    },
}

impl TopologyTaskCompletion {
    fn attempt(&self) -> MutationAttemptId {
        match self {
            Self::Ticket { attempt, .. } | Self::OutcomeUnknown { attempt, .. } => *attempt,
        }
    }
}

/// The sole capability that can await an enqueued topology ticket.
#[must_use = "a dispatched topology ticket capability must remain owned by its supervisor"]
struct TopologyDispatchedAttempt {
    attempt: MutationAttemptId,
    ticket: Pin<Box<OpenGpuiCommandTicket>>,
    completion: Rc<RefCell<Option<TopologyTaskCompletion>>>,
    ticket_settled: bool,
}

impl Future for TopologyDispatchedAttempt {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut TaskContext<'_>) -> Poll<Self::Output> {
        let Poll::Ready(result) = self.ticket.as_mut().poll(context) else {
            return Poll::Pending;
        };
        self.ticket_settled = true;
        let mut completion = self.completion.borrow_mut();
        if completion.is_none() {
            *completion = Some(TopologyTaskCompletion::Ticket {
                attempt: self.attempt,
                result: Box::new(result),
            });
        }
        Poll::Ready(())
    }
}

impl Drop for TopologyDispatchedAttempt {
    fn drop(&mut self) {
        if self.ticket_settled {
            return;
        }
        let mut completion = self.completion.borrow_mut();
        if completion.is_none() {
            *completion = Some(TopologyTaskCompletion::OutcomeUnknown {
                attempt: self.attempt,
                message: DISPATCH_ATTEMPT_DROPPED_MESSAGE,
            });
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct TopologyTransitionEffect {
    receipt_recorded: bool,
    topology_became_visible: bool,
}

impl TopologyTransitionEffect {
    pub(super) const fn topology_became_visible(self) -> bool {
        self.topology_became_visible
    }

    fn combine(self, other: Self) -> Self {
        Self {
            receipt_recorded: self.receipt_recorded || other.receipt_recorded,
            topology_became_visible: self.topology_became_visible || other.topology_became_visible,
        }
    }
}

impl From<MutationTransition> for TopologyTransitionEffect {
    fn from(transition: MutationTransition) -> Self {
        Self {
            receipt_recorded: transition.completion_recorded(),
            topology_became_visible: transition.became_satisfied(),
        }
    }
}

/// A read-only retry plan. Dropping it leaves the retryable intent untouched.
pub(super) struct TopologyRetry<I>(MutationRetry<I>);

impl<I> TopologyRetry<I> {
    pub(super) const fn intent(&self) -> &I {
        self.0.intent()
    }
}

pub(super) enum TopologyDispatchRequest<I> {
    New(I),
    Retry(TopologyRetry<I>),
}

impl<I> TopologyDispatchRequest<I> {
    pub(super) const fn new(intent: I) -> Self {
        Self::New(intent)
    }

    pub(super) const fn retry(retry: TopologyRetry<I>) -> Self {
        Self::Retry(retry)
    }

    pub(super) const fn intent(&self) -> &I {
        match self {
            Self::New(intent) => intent,
            Self::Retry(retry) => retry.intent(),
        }
    }
}

enum TopologyTerminal<I> {
    Retired {
        intent: Option<I>,
        diagnostic: String,
    },
    Closed {
        reason: ViewCloseReason,
        intent: Option<I>,
        diagnostic: String,
    },
}

impl<I> TopologyTerminal<I> {
    const fn phase(&self) -> TopologyAttemptPhase {
        match self {
            Self::Retired { .. } => TopologyAttemptPhase::Retired,
            Self::Closed { .. } => TopologyAttemptPhase::Closed,
        }
    }

    fn intent(&self) -> Option<&I> {
        match self {
            Self::Retired { intent, .. } | Self::Closed { intent, .. } => intent.as_ref(),
        }
    }

    fn diagnostic(&self) -> &str {
        match self {
            Self::Retired { diagnostic, .. } | Self::Closed { diagnostic, .. } => diagnostic,
        }
    }
}

/// Owns one node instance's topology command lifecycle and all native async resources.
pub(super) struct TopologyAttempt<I> {
    owner: NodeInstanceKey,
    reconciler: MutationReconciler<I>,
    terminal: Option<TopologyTerminal<I>>,
    task_completion: Rc<RefCell<Option<TopologyTaskCompletion>>>,
    task: Option<Task<()>>,
    task_lease: Option<OpenGpuiActivityLease>,
    retention_lease: Option<OpenGpuiActivityLease>,
}

/// Covers journal admission until the native task is retained by the node owner.
struct TopologyPostEnqueueGuard<'a, I>
where
    I: Clone,
{
    reconciler: &'a mut MutationReconciler<I>,
    task: &'a mut Option<Task<()>>,
    task_lease: &'a mut Option<OpenGpuiActivityLease>,
    attempt: MutationAttemptId,
    diagnostic: Option<String>,
}

impl<'a, I> TopologyPostEnqueueGuard<'a, I>
where
    I: Clone,
{
    fn new(
        reconciler: &'a mut MutationReconciler<I>,
        task: &'a mut Option<Task<()>>,
        task_lease: &'a mut Option<OpenGpuiActivityLease>,
        attempt: MutationAttemptId,
        diagnostic: String,
    ) -> Self {
        Self {
            reconciler,
            task,
            task_lease,
            attempt,
            diagnostic: Some(diagnostic),
        }
    }

    fn retain_task(&mut self, task: Task<()>) {
        if self.reconciler.phase() == MutationPhase::Pending
            && self.reconciler.active_attempt() == Some(self.attempt)
        {
            *self.task = Some(task);
        }
    }

    fn disarm(&mut self) {
        self.diagnostic = None;
    }
}

impl<I> Drop for TopologyPostEnqueueGuard<'_, I>
where
    I: Clone,
{
    fn drop(&mut self) {
        let Some(diagnostic) = self.diagnostic.take() else {
            return;
        };
        let _ = self.reconciler.complete(
            self.attempt,
            MutationCommandResult::Uncertain { diagnostic },
        );
        self.task.take();
        self.task_lease.take();
    }
}

impl<I: Clone> TopologyAttempt<I> {
    pub(super) fn new(owner: NodeInstanceKey) -> Self {
        Self {
            owner,
            reconciler: MutationReconciler::default(),
            terminal: None,
            task_completion: Rc::new(RefCell::new(None)),
            task: None,
            task_lease: None,
            retention_lease: None,
        }
    }

    pub(super) fn phase(&self) -> TopologyAttemptPhase {
        match &self.terminal {
            Some(terminal) => terminal.phase(),
            None => match self.reconciler.phase() {
                MutationPhase::Idle => TopologyAttemptPhase::Idle,
                MutationPhase::Pending => TopologyAttemptPhase::Pending,
                MutationPhase::AwaitingSnapshot => TopologyAttemptPhase::AwaitingSnapshot,
                MutationPhase::Retryable => TopologyAttemptPhase::Recoverable,
                MutationPhase::Uncertain => TopologyAttemptPhase::Uncertain,
            },
        }
    }

    pub(super) fn active_intent(&self) -> Option<&I> {
        match &self.terminal {
            Some(terminal) => terminal.intent(),
            None => self.reconciler.active_intent(),
        }
    }

    pub(super) fn diagnostic(&self) -> Option<&str> {
        match &self.terminal {
            Some(terminal) => Some(terminal.diagnostic()),
            None => self.reconciler.diagnostic(),
        }
    }

    pub(super) fn can_confirm_uncertain_retry(&self) -> bool {
        self.terminal.is_none() && self.reconciler.can_confirm_uncertain_retry()
    }

    /// Enqueues one topology command and retains the supervisor that settles its ticket.
    pub(super) fn dispatch<N: 'static>(
        &mut self,
        authoring: OpenGpuiNodeAuthoring,
        command: GraphCommand,
        request: TopologyDispatchRequest<I>,
        activity: &OpenGpuiNodeActivityHandle,
        cx: &mut Context<N>,
    ) -> Result<(), TopologyAttemptDispatchError> {
        if let Some(terminal) = &self.terminal {
            return Err(match terminal {
                TopologyTerminal::Retired { .. } => TopologyAttemptDispatchError::Retired,
                TopologyTerminal::Closed { reason, .. } => {
                    TopologyAttemptDispatchError::Closed { reason: *reason }
                }
            });
        }

        let task_installation_unwound = TASK_INSTALLATION_UNWOUND_MESSAGE.to_owned();
        let version = authoring.version();
        let base_revision = version.revision();
        let attempt = match request {
            TopologyDispatchRequest::New(intent) => self.reconciler.begin(intent, base_revision),
            TopologyDispatchRequest::Retry(retry) => {
                self.reconciler.begin_retry(retry.0, base_revision)
            }
        }
        .map_err(map_begin_error)?;

        if authoring.instance() != &self.owner {
            self.retire_with(format!(
                "topology owner {:?} was replaced by {:?}",
                self.owner,
                authoring.instance()
            ));
            return Err(TopologyAttemptDispatchError::Retired);
        }
        if command.expected_version() != version {
            let error = TopologyAttemptDispatchError::NotDispatched {
                message: format!(
                    "command version {:?} does not match topology owner version {version:?}",
                    command.expected_version()
                ),
            };
            return Err(self.fail_not_dispatched(attempt, error));
        }
        if self.retention_lease.is_none() {
            let retention_lease = activity
                .acquire(OpenGpuiActivityKind::Draft)
                .map_err(|error| TopologyAttemptDispatchError::Activity {
                    message: error.to_string(),
                });
            match retention_lease {
                Ok(lease) => self.retention_lease = Some(lease),
                Err(error) => return Err(self.fail_not_dispatched(attempt, error)),
            }
        }
        let task_lease = match activity.acquire(OpenGpuiActivityKind::Task) {
            Ok(lease) => lease,
            Err(error) => {
                let error = TopologyAttemptDispatchError::Activity {
                    message: error.to_string(),
                };
                return Err(self.fail_not_dispatched(attempt, error));
            }
        };
        let ticket = match authoring.enqueue(command, cx) {
            Ok(ticket) => ticket,
            Err(error) => {
                let diagnostic = error.to_string();
                return match error {
                    OpenGpuiNodeAuthoringError::RetiredMount { .. }
                    | OpenGpuiNodeAuthoringError::RetiredIncarnation { .. }
                    | OpenGpuiNodeAuthoringError::MissingNode { .. } => {
                        self.retire_with(diagnostic);
                        Err(TopologyAttemptDispatchError::Retired)
                    }
                    OpenGpuiNodeAuthoringError::Closed { reason } => {
                        self.close(reason);
                        Err(TopologyAttemptDispatchError::Closed { reason })
                    }
                    _ => {
                        let error = TopologyAttemptDispatchError::NotDispatched {
                            message: diagnostic,
                        };
                        Err(self.fail_not_dispatched(attempt, error))
                    }
                };
            }
        };

        self.task_lease = Some(task_lease);
        let dispatched = TopologyDispatchedAttempt {
            attempt,
            ticket: Box::pin(ticket),
            completion: Rc::clone(&self.task_completion),
            ticket_settled: false,
        };
        let mut post_enqueue = TopologyPostEnqueueGuard::new(
            &mut self.reconciler,
            &mut self.task,
            &mut self.task_lease,
            attempt,
            task_installation_unwound,
        );
        let completion = Rc::clone(&self.task_completion);
        let task = cx.spawn(async move |_node, cx| {
            let task_panicked = AssertUnwindSafe(dispatched).catch_unwind().await.is_err();
            if task_panicked {
                let mut completion = completion.borrow_mut();
                if completion.is_none() {
                    *completion = Some(TopologyTaskCompletion::OutcomeUnknown {
                        attempt,
                        message: DISPATCH_TASK_PANICKED_MESSAGE,
                    });
                }
            }
            if completion.borrow().is_some() {
                cx.refresh();
            }
        });
        post_enqueue.retain_task(task);
        post_enqueue.disarm();
        Ok(())
    }

    fn fail_not_dispatched(
        &mut self,
        attempt: MutationAttemptId,
        error: TopologyAttemptDispatchError,
    ) -> TopologyAttemptDispatchError {
        let _ = self.reconciler.complete(
            attempt,
            MutationCommandResult::NotDispatched {
                diagnostic: error.to_string(),
            },
        );
        self.sync_resources();
        error
    }

    fn drain_task_completion(&mut self) -> TopologyTransitionEffect {
        let completion = self.task_completion.borrow_mut().take();
        let Some(completion) = completion else {
            return TopologyTransitionEffect::default();
        };
        let attempt = completion.attempt();
        let effect = match completion {
            TopologyTaskCompletion::Ticket { result, .. } => self.settle_ticket(attempt, *result),
            TopologyTaskCompletion::OutcomeUnknown { message, .. } => self
                .reconciler
                .complete(
                    attempt,
                    MutationCommandResult::Uncertain {
                        diagnostic: message.to_owned(),
                    },
                )
                .map(TopologyTransitionEffect::from)
                .unwrap_or_default(),
        };
        self.sync_resources();
        effect
    }

    fn settle_ticket(
        &mut self,
        attempt: MutationAttemptId,
        result: OpenGpuiCommandTicketResult,
    ) -> TopologyTransitionEffect {
        let completion = match result {
            Ok(completion) => MutationCommandResult::from_runtime(&completion),
            Err(error) => MutationCommandResult::Uncertain {
                diagnostic: format!(
                    "topology edit outcome is uncertain: {error}; inspect the refreshed graph before retry"
                ),
            },
        };
        let effect = self
            .reconciler
            .complete(attempt, completion)
            .map(TopologyTransitionEffect::from)
            .unwrap_or_default();
        if effect.receipt_recorded {
            self.task.take();
            self.task_lease.take();
        }
        effect
    }

    pub(super) fn observe(
        &mut self,
        instance: &NodeInstanceKey,
        revision: GraphRevision,
        satisfied: impl FnOnce(&I) -> bool,
    ) -> TopologyTransitionEffect {
        if instance != &self.owner {
            self.retire_with(format!(
                "topology owner {:?} was replaced by {:?}",
                self.owner, instance
            ));
            return TopologyTransitionEffect::default();
        }
        if self.terminal.is_some() {
            return TopologyTransitionEffect::default();
        }
        let completion = self.drain_task_completion();
        let observation =
            TopologyTransitionEffect::from(self.reconciler.observe(revision, satisfied));
        self.sync_resources();
        completion.combine(observation)
    }

    pub(super) fn observe_authoring_error(&mut self, error: &OpenGpuiNodeAuthoringError) {
        match error {
            OpenGpuiNodeAuthoringError::RetiredMount { .. }
            | OpenGpuiNodeAuthoringError::RetiredIncarnation { .. }
            | OpenGpuiNodeAuthoringError::MissingNode { .. } => self.retire_with(error.to_string()),
            OpenGpuiNodeAuthoringError::Closed { reason } => self.close(*reason),
            _ => {}
        }
    }

    pub(super) fn retry(&self) -> Option<TopologyRetry<I>> {
        self.terminal
            .is_none()
            .then(|| self.reconciler.retry(false))
            .flatten()
            .map(TopologyRetry)
    }

    pub(super) fn confirm_uncertain_retry(&self) -> Option<TopologyRetry<I>> {
        self.terminal
            .is_none()
            .then(|| self.reconciler.retry(true))
            .flatten()
            .map(TopologyRetry)
    }

    fn retire_with(&mut self, retirement_diagnostic: String) {
        let intent = self.reconciler.active_intent().cloned();
        let diagnostic = self
            .reconciler
            .diagnostic()
            .map(ToOwned::to_owned)
            .unwrap_or(retirement_diagnostic);
        self.release_native_resources();
        self.reconciler = MutationReconciler::default();
        self.terminal = Some(TopologyTerminal::Retired { intent, diagnostic });
    }

    pub(super) fn close(&mut self, reason: ViewCloseReason) {
        let intent = self.reconciler.active_intent().cloned();
        let diagnostic = self
            .reconciler
            .diagnostic()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("topology owner closed: {reason:?}"));
        self.release_native_resources();
        self.reconciler = MutationReconciler::default();
        self.terminal = Some(TopologyTerminal::Closed {
            reason,
            intent,
            diagnostic,
        });
    }

    fn release_native_resources(&mut self) {
        self.task.take();
        self.task_completion.borrow_mut().take();
        self.task_lease.take();
        self.retention_lease.take();
    }

    fn sync_resources(&mut self) {
        if self.reconciler.phase() != MutationPhase::Pending {
            self.task.take();
            self.task_lease.take();
        }
        if self.reconciler.phase() == MutationPhase::Idle || self.terminal.is_some() {
            self.retention_lease.take();
        }
    }
}

fn map_begin_error(error: MutationBeginError) -> TopologyAttemptDispatchError {
    match error {
        MutationBeginError::IdentityExhausted => TopologyAttemptDispatchError::IdentityExhausted,
        MutationBeginError::Busy => TopologyAttemptDispatchError::Busy,
        MutationBeginError::RetryExpired => TopologyAttemptDispatchError::RetryExpired,
        MutationBeginError::StaleRetryBase {
            required_revision,
            received_revision,
        } => TopologyAttemptDispatchError::StaleRetryBase {
            required_revision,
            received_revision,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_enqueue_guard_turns_abandoned_task_installation_into_uncertainty() {
        let mut reconciler = MutationReconciler::default();
        let attempt = reconciler.begin("add-port", 2).expect("begin attempt");
        let mut task: Option<Task<()>> = None;
        let mut task_lease: Option<OpenGpuiActivityLease> = None;

        {
            let _guard = TopologyPostEnqueueGuard::new(
                &mut reconciler,
                &mut task,
                &mut task_lease,
                attempt,
                TASK_INSTALLATION_UNWOUND_MESSAGE.to_owned(),
            );
        }

        assert_eq!(reconciler.phase(), MutationPhase::Uncertain);
        assert_eq!(
            reconciler.diagnostic(),
            Some(TASK_INSTALLATION_UNWOUND_MESSAGE)
        );
        assert!(task.is_none());
        assert!(task_lease.is_none());
    }

    #[test]
    fn terminal_projection_preserves_intent_diagnostic_and_close_reason() {
        let retired = TopologyTerminal::Retired {
            intent: Some("edit"),
            diagnostic: "owner replaced".to_owned(),
        };
        assert_eq!(retired.phase(), TopologyAttemptPhase::Retired);
        assert_eq!(retired.intent(), Some(&"edit"));
        assert_eq!(retired.diagnostic(), "owner replaced");

        let closed = TopologyTerminal::Closed {
            reason: ViewCloseReason::HostClosed,
            intent: Some("edit"),
            diagnostic: "host closed".to_owned(),
        };
        assert_eq!(closed.phase(), TopologyAttemptPhase::Closed);
        assert_eq!(closed.intent(), Some(&"edit"));
        assert_eq!(closed.diagnostic(), "host closed");
        assert!(matches!(
            closed,
            TopologyTerminal::Closed {
                reason: ViewCloseReason::HostClosed,
                ..
            }
        ));
    }

    #[test]
    fn expired_retry_has_a_distinct_dispatch_error() {
        assert!(matches!(
            map_begin_error(MutationBeginError::RetryExpired),
            TopologyAttemptDispatchError::RetryExpired
        ));
    }
}
