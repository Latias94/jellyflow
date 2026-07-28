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
use jellyflow_runtime::commands::{CommandExecutionKind, GraphCommand};
use jellyflow_runtime::graph::GraphRevision;
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct Observation {
    revision: GraphRevision,
    satisfied: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AttemptState<I> {
    Idle,
    Pending {
        attempt: u64,
        intent: I,
        base_revision: GraphRevision,
        observation: Option<Observation>,
    },
    AwaitingSnapshot {
        attempt: u64,
        intent: I,
        base_revision: GraphRevision,
        minimum_revision: GraphRevision,
        observation: Option<Observation>,
    },
    Recoverable {
        attempt: u64,
        intent: I,
        base_revision: GraphRevision,
        diagnostic: String,
    },
    Uncertain {
        attempt: u64,
        intent: I,
        base_revision: GraphRevision,
        observation: Option<Observation>,
        diagnostic: String,
    },
    Retired {
        intent: Option<I>,
        diagnostic: Option<String>,
    },
    Closed {
        reason: ViewCloseReason,
        intent: Option<I>,
        diagnostic: Option<String>,
    },
}

struct AttemptMachine<I> {
    state: AttemptState<I>,
    next_attempt: Option<u64>,
}

impl<I> Default for AttemptMachine<I> {
    fn default() -> Self {
        Self {
            state: AttemptState::Idle,
            next_attempt: Some(1),
        }
    }
}

enum TicketSettlement {
    Applied(GraphRevision),
    Noop(GraphRevision),
    Conflict {
        revision: GraphRevision,
        diagnostic: String,
    },
    Rejected(String),
    Uncertain(String),
}

enum TopologyTaskCompletion {
    Ticket {
        attempt: u64,
        result: Box<OpenGpuiCommandTicketResult>,
    },
    OutcomeUnknown {
        attempt: u64,
        message: &'static str,
    },
}

impl TopologyTaskCompletion {
    fn attempt(&self) -> u64 {
        match self {
            Self::Ticket { attempt, .. } | Self::OutcomeUnknown { attempt, .. } => *attempt,
        }
    }
}

/// The sole capability that can await an enqueued topology ticket.
///
/// It records terminal ticket evidence into the owning attempt. Dropping it before completion
/// records an uncertain outcome, so a retained task can never silently abandon a pending command.
#[must_use = "a dispatched topology ticket capability must remain owned by its supervisor"]
struct TopologyDispatchedAttempt {
    attempt: u64,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TopologyRetryKind {
    Recoverable,
    ConfirmedUncertain,
}

/// A read-only retry plan. Dropping it leaves the retryable intent untouched.
pub(super) struct TopologyRetry<I> {
    source_attempt: u64,
    intent: I,
    kind: TopologyRetryKind,
}

impl<I> TopologyRetry<I> {
    pub(super) const fn intent(&self) -> &I {
        &self.intent
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

impl<I: Clone> AttemptMachine<I> {
    fn phase(&self) -> TopologyAttemptPhase {
        match &self.state {
            AttemptState::Idle => TopologyAttemptPhase::Idle,
            AttemptState::Pending { .. } => TopologyAttemptPhase::Pending,
            AttemptState::AwaitingSnapshot { .. } => TopologyAttemptPhase::AwaitingSnapshot,
            AttemptState::Recoverable { .. } => TopologyAttemptPhase::Recoverable,
            AttemptState::Uncertain { .. } => TopologyAttemptPhase::Uncertain,
            AttemptState::Retired { .. } => TopologyAttemptPhase::Retired,
            AttemptState::Closed { .. } => TopologyAttemptPhase::Closed,
        }
    }

    fn allocate_attempt(&mut self) -> Result<u64, TopologyAttemptDispatchError> {
        let attempt = self
            .next_attempt
            .ok_or(TopologyAttemptDispatchError::IdentityExhausted)?;
        self.next_attempt = attempt.checked_add(1);
        Ok(attempt)
    }

    fn retry_is_eligible(&self, retry: &TopologyRetry<I>) -> bool {
        match (&self.state, retry.kind) {
            (AttemptState::Recoverable { attempt, .. }, TopologyRetryKind::Recoverable) => {
                *attempt == retry.source_attempt
            }
            (
                AttemptState::Uncertain {
                    attempt,
                    base_revision,
                    observation: Some(observation),
                    ..
                },
                TopologyRetryKind::ConfirmedUncertain,
            ) => {
                *attempt == retry.source_attempt
                    && observation.revision > *base_revision
                    && !observation.satisfied
            }
            _ => false,
        }
    }

    fn begin_new(
        &mut self,
        intent: I,
        base_revision: GraphRevision,
    ) -> Result<u64, TopologyAttemptDispatchError> {
        match &self.state {
            AttemptState::Idle => {}
            AttemptState::Retired { .. } => return Err(TopologyAttemptDispatchError::Retired),
            AttemptState::Closed { reason, .. } => {
                return Err(TopologyAttemptDispatchError::Closed { reason: *reason });
            }
            _ => return Err(TopologyAttemptDispatchError::Busy),
        }
        let attempt = self.allocate_attempt()?;
        self.state = AttemptState::Pending {
            attempt,
            intent,
            base_revision,
            observation: None,
        };
        Ok(attempt)
    }

    fn begin_retry(
        &mut self,
        retry: TopologyRetry<I>,
        base_revision: GraphRevision,
    ) -> Result<u64, TopologyAttemptDispatchError> {
        if !self.retry_is_eligible(&retry) {
            return Err(TopologyAttemptDispatchError::Busy);
        }

        let attempt = self.allocate_attempt()?;
        let state = std::mem::replace(&mut self.state, AttemptState::Idle);
        let intent = match state {
            AttemptState::Recoverable { intent, .. } | AttemptState::Uncertain { intent, .. } => {
                intent
            }
            state => {
                self.state = state;
                return Err(TopologyAttemptDispatchError::Busy);
            }
        };
        self.state = AttemptState::Pending {
            attempt,
            intent,
            base_revision,
            observation: None,
        };
        Ok(attempt)
    }

    fn begin_request(
        &mut self,
        request: TopologyDispatchRequest<I>,
        base_revision: GraphRevision,
    ) -> Result<u64, TopologyAttemptDispatchError> {
        match request {
            TopologyDispatchRequest::New(intent) => self.begin_new(intent, base_revision),
            TopologyDispatchRequest::Retry(retry) => self.begin_retry(retry, base_revision),
        }
    }

    fn fail_dispatch(&mut self, attempt: u64, diagnostic: String) {
        let state = std::mem::replace(&mut self.state, AttemptState::Idle);
        self.state = match state {
            AttemptState::Pending {
                attempt: active_attempt,
                intent,
                base_revision,
                ..
            } if active_attempt == attempt => AttemptState::Recoverable {
                attempt: active_attempt,
                intent,
                base_revision,
                diagnostic,
            },
            state => state,
        };
    }

    fn mark_uncertain(&mut self, attempt: u64, diagnostic: String) {
        let state = std::mem::replace(&mut self.state, AttemptState::Idle);
        self.state = match state {
            AttemptState::Pending {
                attempt: active_attempt,
                intent,
                base_revision,
                observation,
            } if active_attempt == attempt => AttemptState::Uncertain {
                attempt: active_attempt,
                intent,
                base_revision,
                observation,
                diagnostic,
            },
            state => state,
        };
    }

    fn settle(&mut self, attempt: u64, settlement: TicketSettlement) -> TopologyTransitionEffect {
        let state = std::mem::replace(&mut self.state, AttemptState::Idle);
        let AttemptState::Pending {
            attempt: active_attempt,
            intent,
            base_revision,
            observation,
        } = state
        else {
            self.state = state;
            return TopologyTransitionEffect::default();
        };
        if active_attempt != attempt {
            self.state = AttemptState::Pending {
                attempt: active_attempt,
                intent,
                base_revision,
                observation,
            };
            return TopologyTransitionEffect::default();
        }

        let mut effect = TopologyTransitionEffect {
            receipt_recorded: true,
            topology_became_visible: false,
        };
        self.state = match settlement {
            TicketSettlement::Applied(minimum_revision)
            | TicketSettlement::Noop(minimum_revision) => {
                if observation.as_ref().is_some_and(|observation| {
                    observation.revision >= minimum_revision && observation.satisfied
                }) {
                    effect.topology_became_visible = true;
                    AttemptState::Idle
                } else {
                    AttemptState::AwaitingSnapshot {
                        attempt: active_attempt,
                        intent,
                        base_revision,
                        minimum_revision,
                        observation,
                    }
                }
            }
            TicketSettlement::Conflict {
                revision,
                diagnostic,
            } => {
                if observation.as_ref().is_some_and(|observation| {
                    observation.revision >= revision && observation.satisfied
                }) {
                    effect.topology_became_visible = true;
                    AttemptState::Idle
                } else {
                    AttemptState::Recoverable {
                        attempt: active_attempt,
                        intent,
                        base_revision,
                        diagnostic,
                    }
                }
            }
            TicketSettlement::Rejected(diagnostic) => {
                if observation
                    .as_ref()
                    .is_some_and(|observation| observation.satisfied)
                {
                    effect.topology_became_visible = true;
                    AttemptState::Idle
                } else {
                    AttemptState::Recoverable {
                        attempt: active_attempt,
                        intent,
                        base_revision,
                        diagnostic,
                    }
                }
            }
            TicketSettlement::Uncertain(diagnostic) => AttemptState::Uncertain {
                attempt: active_attempt,
                intent,
                base_revision,
                observation,
                diagnostic,
            },
        };
        effect
    }

    fn observe(
        &mut self,
        revision: GraphRevision,
        satisfied: impl FnOnce(&I) -> bool,
    ) -> TopologyTransitionEffect {
        let mut effect = TopologyTransitionEffect::default();
        let state = std::mem::replace(&mut self.state, AttemptState::Idle);
        self.state = match state {
            AttemptState::Pending {
                attempt,
                intent,
                base_revision,
                ..
            } => {
                let observation = Observation {
                    revision,
                    satisfied: satisfied(&intent),
                };
                AttemptState::Pending {
                    attempt,
                    intent,
                    base_revision,
                    observation: Some(observation),
                }
            }
            AttemptState::AwaitingSnapshot {
                attempt,
                intent,
                base_revision,
                minimum_revision,
                observation,
            } => {
                if revision < minimum_revision {
                    AttemptState::AwaitingSnapshot {
                        attempt,
                        intent,
                        base_revision,
                        minimum_revision,
                        observation,
                    }
                } else if satisfied(&intent) {
                    effect.topology_became_visible = true;
                    AttemptState::Idle
                } else {
                    AttemptState::Recoverable {
                        attempt,
                        intent,
                        base_revision,
                        diagnostic: format!(
                            "topology edit was not reflected by authoritative revision {revision}"
                        ),
                    }
                }
            }
            AttemptState::Recoverable {
                attempt,
                intent,
                base_revision,
                diagnostic,
            } => {
                if satisfied(&intent) {
                    effect.topology_became_visible = true;
                    AttemptState::Idle
                } else {
                    AttemptState::Recoverable {
                        attempt,
                        intent,
                        base_revision,
                        diagnostic,
                    }
                }
            }
            AttemptState::Uncertain {
                attempt,
                intent,
                base_revision,
                diagnostic,
                ..
            } => {
                let observation = Observation {
                    revision,
                    satisfied: satisfied(&intent),
                };
                if observation.satisfied {
                    effect.topology_became_visible = true;
                    AttemptState::Idle
                } else {
                    AttemptState::Uncertain {
                        attempt,
                        intent,
                        base_revision,
                        observation: Some(observation),
                        diagnostic,
                    }
                }
            }
            state => state,
        };
        effect
    }

    fn prepare_retry(&self, confirm_uncertain: bool) -> Option<TopologyRetry<I>> {
        match &self.state {
            AttemptState::Recoverable {
                attempt, intent, ..
            } if !confirm_uncertain => Some(TopologyRetry {
                source_attempt: *attempt,
                intent: intent.clone(),
                kind: TopologyRetryKind::Recoverable,
            }),
            AttemptState::Uncertain {
                attempt,
                intent,
                base_revision,
                observation: Some(observation),
                ..
            } if confirm_uncertain
                && observation.revision > *base_revision
                && !observation.satisfied =>
            {
                Some(TopologyRetry {
                    source_attempt: *attempt,
                    intent: intent.clone(),
                    kind: TopologyRetryKind::ConfirmedUncertain,
                })
            }
            _ => None,
        }
    }

    fn can_confirm_uncertain_retry(&self) -> bool {
        matches!(
            &self.state,
            AttemptState::Uncertain {
                base_revision,
                observation: Some(observation),
                ..
            } if observation.revision > *base_revision && !observation.satisfied
        )
    }

    fn active_intent(&self) -> Option<&I> {
        match &self.state {
            AttemptState::Pending { intent, .. }
            | AttemptState::AwaitingSnapshot { intent, .. }
            | AttemptState::Recoverable { intent, .. }
            | AttemptState::Uncertain { intent, .. } => Some(intent),
            AttemptState::Retired { intent, .. } | AttemptState::Closed { intent, .. } => {
                intent.as_ref()
            }
            AttemptState::Idle => None,
        }
    }

    fn diagnostic(&self) -> Option<&str> {
        match &self.state {
            AttemptState::Recoverable { diagnostic, .. }
            | AttemptState::Uncertain { diagnostic, .. } => Some(diagnostic),
            AttemptState::Retired { diagnostic, .. } | AttemptState::Closed { diagnostic, .. } => {
                diagnostic.as_deref()
            }
            _ => None,
        }
    }

    fn retire(&mut self, retirement_diagnostic: String) {
        let state = std::mem::replace(&mut self.state, AttemptState::Idle);
        let (intent, diagnostic) = Self::into_readonly(state);
        self.state = AttemptState::Retired {
            intent,
            diagnostic: diagnostic.or(Some(retirement_diagnostic)),
        };
    }

    fn close(&mut self, reason: ViewCloseReason) {
        let state = std::mem::replace(&mut self.state, AttemptState::Idle);
        let (intent, diagnostic) = Self::into_readonly(state);
        self.state = AttemptState::Closed {
            reason,
            intent,
            diagnostic: diagnostic.or_else(|| Some(format!("topology owner closed: {reason:?}"))),
        };
    }

    fn into_readonly(state: AttemptState<I>) -> (Option<I>, Option<String>) {
        match state {
            AttemptState::Idle => (None, None),
            AttemptState::Pending { intent, .. }
            | AttemptState::AwaitingSnapshot { intent, .. } => (Some(intent), None),
            AttemptState::Recoverable {
                intent, diagnostic, ..
            }
            | AttemptState::Uncertain {
                intent, diagnostic, ..
            } => (Some(intent), Some(diagnostic)),
            AttemptState::Retired { intent, diagnostic }
            | AttemptState::Closed {
                intent, diagnostic, ..
            } => (intent, diagnostic),
        }
    }
}

/// Owns one node instance's topology command lifecycle and all native async resources.
pub(super) struct TopologyAttempt<I> {
    owner: NodeInstanceKey,
    machine: AttemptMachine<I>,
    task_completion: Rc<RefCell<Option<TopologyTaskCompletion>>>,
    task: Option<Task<()>>,
    task_lease: Option<OpenGpuiActivityLease>,
    retention_lease: Option<OpenGpuiActivityLease>,
}

/// Covers the interval after journal admission and before the task is retained by the node.
///
/// Native supervisor installation may unwind. Once the command is enqueued, that must never
/// strand the attempt in `Pending`: dropping this guard records an uncertain outcome and releases
/// task-scoped activity ownership.
struct TopologyPostEnqueueGuard<'a, I>
where
    I: Clone,
{
    machine: &'a mut AttemptMachine<I>,
    task: &'a mut Option<Task<()>>,
    task_lease: &'a mut Option<OpenGpuiActivityLease>,
    attempt: u64,
    diagnostic: Option<String>,
}

impl<'a, I> TopologyPostEnqueueGuard<'a, I>
where
    I: Clone,
{
    fn new(
        machine: &'a mut AttemptMachine<I>,
        task: &'a mut Option<Task<()>>,
        task_lease: &'a mut Option<OpenGpuiActivityLease>,
        attempt: u64,
        diagnostic: String,
    ) -> Self {
        Self {
            machine,
            task,
            task_lease,
            attempt,
            diagnostic: Some(diagnostic),
        }
    }

    fn retain_task(&mut self, task: Task<()>) {
        if matches!(
            &self.machine.state,
            AttemptState::Pending {
                attempt: active_attempt,
                ..
            } if *active_attempt == self.attempt
        ) {
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
        self.machine.mark_uncertain(self.attempt, diagnostic);
        self.task.take();
        self.task_lease.take();
    }
}

impl<I: Clone> TopologyAttempt<I> {
    pub(super) fn new(owner: NodeInstanceKey) -> Self {
        Self {
            owner,
            machine: AttemptMachine::default(),
            task_completion: Rc::new(RefCell::new(None)),
            task: None,
            task_lease: None,
            retention_lease: None,
        }
    }

    pub(super) fn phase(&self) -> TopologyAttemptPhase {
        self.machine.phase()
    }

    pub(super) fn active_intent(&self) -> Option<&I> {
        self.machine.active_intent()
    }

    pub(super) fn diagnostic(&self) -> Option<&str> {
        self.machine.diagnostic()
    }

    pub(super) fn can_confirm_uncertain_retry(&self) -> bool {
        self.machine.can_confirm_uncertain_retry()
    }

    /// Enqueues one topology command and retains the supervisor that settles its ticket.
    ///
    /// The ticket is immediately wrapped in an internal capability and is never exposed to a
    /// product callback. This keeps journal admission, ticket polling, panic recovery, and task
    /// ownership in one lifecycle boundary.
    pub(super) fn dispatch<N: 'static>(
        &mut self,
        authoring: OpenGpuiNodeAuthoring,
        command: GraphCommand,
        request: TopologyDispatchRequest<I>,
        activity: &OpenGpuiNodeActivityHandle,
        cx: &mut Context<N>,
    ) -> Result<(), TopologyAttemptDispatchError> {
        // Allocate before admission so the post-enqueue guard can transition without allocating
        // while an unwind is already in progress.
        let task_installation_unwound = TASK_INSTALLATION_UNWOUND_MESSAGE.to_owned();
        let version = authoring.version();
        let base_revision = version.revision();
        let attempt = self.machine.begin_request(request, base_revision)?;
        if authoring.instance() != &self.owner {
            self.retire_with(format!(
                "topology owner {:?} was replaced by {:?}",
                self.owner,
                authoring.instance()
            ));
            return Err(TopologyAttemptDispatchError::Retired);
        }
        if command.expected_version() != version {
            let diagnostic = format!(
                "command version {:?} does not match topology owner version {version:?}",
                command.expected_version()
            );
            let error = TopologyAttemptDispatchError::NotDispatched {
                message: diagnostic,
            };
            return Err(self.fail_not_dispatched(attempt, error));
        }
        if self.retention_lease.is_none() {
            let retention_lease = match activity.acquire(OpenGpuiActivityKind::Draft) {
                Ok(lease) => lease,
                Err(error) => {
                    let error = TopologyAttemptDispatchError::Activity {
                        message: error.to_string(),
                    };
                    return Err(self.fail_not_dispatched(attempt, error));
                }
            };
            self.retention_lease = Some(retention_lease);
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
            &mut self.machine,
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
        attempt: u64,
        error: TopologyAttemptDispatchError,
    ) -> TopologyAttemptDispatchError {
        self.machine.fail_dispatch(attempt, error.to_string());
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
            TopologyTaskCompletion::OutcomeUnknown { message, .. } => {
                self.machine.mark_uncertain(attempt, message.to_owned());
                TopologyTransitionEffect::default()
            }
        };
        self.sync_resources();
        effect
    }

    fn settle_ticket(
        &mut self,
        attempt: u64,
        result: OpenGpuiCommandTicketResult,
    ) -> TopologyTransitionEffect {
        let settlement = match result {
            Ok(completion) => match completion.kind() {
                CommandExecutionKind::Applied => TicketSettlement::Applied(
                    completion
                        .applied_revision()
                        .expect("applied command completion carries a revision"),
                ),
                CommandExecutionKind::Noop => TicketSettlement::Noop(
                    completion
                        .observed_revision()
                        .expect("noop command completion carries a revision"),
                ),
                CommandExecutionKind::Conflict => {
                    let revision = completion
                        .observed_revision()
                        .expect("conflict command completion carries a revision");
                    TicketSettlement::Conflict {
                        revision,
                        diagnostic: format!(
                            "topology changed at revision {revision}; refresh before retry"
                        ),
                    }
                }
                CommandExecutionKind::Rejected => TicketSettlement::Rejected(format!(
                    "topology edit was rejected: {}",
                    completion
                        .diagnostic_message()
                        .unwrap_or_else(|| "command rejected".to_owned())
                )),
                CommandExecutionKind::Failed => TicketSettlement::Uncertain(format!(
                    "topology edit outcome is uncertain: {}; inspect the refreshed graph before retry",
                    completion
                        .diagnostic_message()
                        .unwrap_or_else(|| "command execution failed".to_owned())
                )),
            },
            Err(error) => TicketSettlement::Uncertain(format!(
                "topology edit outcome is uncertain: {error}; inspect the refreshed graph before retry"
            )),
        };
        let effect = self.machine.settle(attempt, settlement);
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
        let completion = self.drain_task_completion();
        let observation = self.machine.observe(revision, satisfied);
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
        self.machine.prepare_retry(false)
    }

    pub(super) fn confirm_uncertain_retry(&self) -> Option<TopologyRetry<I>> {
        self.machine.prepare_retry(true)
    }

    fn retire_with(&mut self, diagnostic: String) {
        self.task.take();
        self.task_completion.borrow_mut().take();
        self.task_lease.take();
        self.retention_lease.take();
        self.machine.retire(diagnostic);
    }

    pub(super) fn close(&mut self, reason: ViewCloseReason) {
        self.task.take();
        self.task_completion.borrow_mut().take();
        self.task_lease.take();
        self.retention_lease.take();
        self.machine.close(reason);
    }

    fn sync_resources(&mut self) {
        if self.machine.phase() != TopologyAttemptPhase::Pending {
            self.task.take();
            self.task_lease.take();
        }
        if matches!(
            self.machine.phase(),
            TopologyAttemptPhase::Idle
                | TopologyAttemptPhase::Retired
                | TopologyAttemptPhase::Closed
        ) {
            self.retention_lease.take();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Intent(&'static str);

    fn pending(intent: Intent, revision: GraphRevision) -> (AttemptMachine<Intent>, u64) {
        let mut machine = AttemptMachine::default();
        let attempt = machine.begin_new(intent, revision).expect("begin attempt");
        (machine, attempt)
    }

    #[test]
    fn receipt_first_waits_for_matching_authoritative_snapshot() {
        let (mut machine, attempt) = pending(Intent("add"), 4);
        let receipt = machine.settle(attempt, TicketSettlement::Applied(5));
        assert!(receipt.receipt_recorded);
        assert!(!receipt.topology_became_visible());
        assert_eq!(machine.phase(), TopologyAttemptPhase::AwaitingSnapshot);

        assert!(!machine.observe(4, |_| true).topology_became_visible());
        assert_eq!(machine.phase(), TopologyAttemptPhase::AwaitingSnapshot);
        assert!(
            machine
                .observe(5, |intent| intent.0 == "add")
                .topology_became_visible()
        );
        assert_eq!(machine.phase(), TopologyAttemptPhase::Idle);
        assert!(!machine.observe(5, |_| true).topology_became_visible());
    }

    #[test]
    fn snapshot_first_is_retained_until_the_receipt_arrives() {
        let (mut machine, attempt) = pending(Intent("reorder"), 9);
        assert!(
            !machine
                .observe(10, |intent| intent.0 == "reorder")
                .topology_became_visible()
        );
        assert_eq!(machine.phase(), TopologyAttemptPhase::Pending);

        assert!(
            machine
                .settle(attempt, TicketSettlement::Applied(10))
                .topology_became_visible()
        );
        assert_eq!(machine.phase(), TopologyAttemptPhase::Idle);
        assert!(
            !machine
                .settle(attempt, TicketSettlement::Applied(10))
                .topology_became_visible()
        );
    }

    #[test]
    fn noop_receipt_waits_for_the_same_matching_snapshot_evidence() {
        let (mut machine, attempt) = pending(Intent("already-applied"), 20);
        let receipt = machine.settle(attempt, TicketSettlement::Noop(21));
        assert!(receipt.receipt_recorded);
        assert!(!receipt.topology_became_visible());

        assert!(
            machine
                .observe(21, |intent| intent.0 == "already-applied")
                .topology_became_visible()
        );
        assert_eq!(machine.phase(), TopologyAttemptPhase::Idle);
    }

    #[test]
    fn unknown_ticket_result_never_becomes_automatically_retryable() {
        let (mut machine, attempt) = pending(Intent("remove"), 12);
        machine.settle(
            attempt,
            TicketSettlement::Uncertain("receipt lost".to_owned()),
        );
        assert_eq!(machine.phase(), TopologyAttemptPhase::Uncertain);
        assert!(machine.prepare_retry(false).is_none());
        assert!(!machine.can_confirm_uncertain_retry());

        machine.observe(13, |_| false);
        assert_eq!(machine.phase(), TopologyAttemptPhase::Uncertain);
        assert!(machine.prepare_retry(false).is_none());
        assert!(machine.can_confirm_uncertain_retry());
        let retry = machine
            .prepare_retry(true)
            .expect("fresh observation permits explicit retry");
        assert_eq!(retry.intent(), &Intent("remove"));
        assert_eq!(machine.phase(), TopologyAttemptPhase::Uncertain);
        assert_eq!(machine.active_intent(), Some(&Intent("remove")));

        machine
            .begin_retry(retry, 13)
            .expect("confirmed retry commits after planning");
        assert_eq!(machine.phase(), TopologyAttemptPhase::Pending);
    }

    #[test]
    fn conflict_is_recoverable_but_does_not_retry_itself() {
        let (mut machine, attempt) = pending(Intent("add"), 2);
        machine.settle(
            attempt,
            TicketSettlement::Conflict {
                revision: 3,
                diagnostic: "conflict".to_owned(),
            },
        );
        assert_eq!(machine.phase(), TopologyAttemptPhase::Recoverable);
        assert_eq!(machine.active_intent(), Some(&Intent("add")));
        let retry = machine
            .prepare_retry(false)
            .expect("conflict exposes retry intent");
        assert_eq!(retry.intent(), &Intent("add"));
        assert_eq!(machine.phase(), TopologyAttemptPhase::Recoverable);
        assert_eq!(machine.active_intent(), Some(&Intent("add")));
    }

    #[test]
    fn failed_retry_planning_does_not_consume_recoverable_intent() {
        let (mut machine, attempt) = pending(Intent("add"), 2);
        machine.settle(
            attempt,
            TicketSettlement::Rejected("planner must be retried".to_owned()),
        );

        {
            let abandoned_plan = machine
                .prepare_retry(false)
                .expect("recoverable attempt prepares a retry");
            assert_eq!(abandoned_plan.intent(), &Intent("add"));
        }

        assert_eq!(machine.phase(), TopologyAttemptPhase::Recoverable);
        assert_eq!(machine.active_intent(), Some(&Intent("add")));
        assert_eq!(machine.diagnostic(), Some("planner must be retried"));

        let retry = machine
            .prepare_retry(false)
            .expect("retry remains available after planning failure");
        let retried_attempt = machine
            .begin_retry(retry, 3)
            .expect("successful planning commits the retry");
        assert!(retried_attempt > attempt);
        assert_eq!(machine.phase(), TopologyAttemptPhase::Pending);
        assert_eq!(machine.active_intent(), Some(&Intent("add")));
    }

    #[test]
    fn not_dispatched_transition_preserves_new_stable_intent() {
        for diagnostic in ["activity lease unavailable", "command enqueue unavailable"] {
            let (mut machine, attempt) = pending(Intent("stable-port-id"), 2);
            machine.fail_dispatch(attempt, diagnostic.to_owned());

            assert_eq!(machine.phase(), TopologyAttemptPhase::Recoverable);
            assert_eq!(machine.active_intent(), Some(&Intent("stable-port-id")));
            assert_eq!(machine.diagnostic(), Some(diagnostic));
            assert_eq!(
                machine
                    .prepare_retry(false)
                    .expect("not-dispatched intent remains retryable")
                    .intent(),
                &Intent("stable-port-id")
            );
        }
    }

    #[test]
    fn task_installation_unwind_becomes_uncertain_until_a_fresh_snapshot_confirms_retry() {
        let (mut machine, attempt) = pending(Intent("stable-port-id"), 2);
        let mut task: Option<Task<()>> = None;
        let mut task_lease: Option<OpenGpuiActivityLease> = None;

        {
            let _post_enqueue = TopologyPostEnqueueGuard::new(
                &mut machine,
                &mut task,
                &mut task_lease,
                attempt,
                TASK_INSTALLATION_UNWOUND_MESSAGE.to_owned(),
            );
        }

        assert_eq!(machine.phase(), TopologyAttemptPhase::Uncertain);
        assert_eq!(machine.active_intent(), Some(&Intent("stable-port-id")));
        assert_eq!(
            machine.diagnostic(),
            Some(TASK_INSTALLATION_UNWOUND_MESSAGE)
        );
        assert!(machine.prepare_retry(false).is_none());
        assert!(!machine.can_confirm_uncertain_retry());
        assert!(task.is_none());
        assert!(task_lease.is_none());

        machine.observe(3, |_| false);
        assert!(machine.can_confirm_uncertain_retry());
    }

    #[test]
    fn stale_completion_cannot_settle_the_current_attempt() {
        let (mut machine, attempt) = pending(Intent("add"), 7);
        let effect = machine.settle(attempt + 1, TicketSettlement::Applied(8));
        assert_eq!(effect, TopologyTransitionEffect::default());
        assert_eq!(machine.phase(), TopologyAttemptPhase::Pending);

        let effect = machine.settle(attempt, TicketSettlement::Applied(8));
        assert!(effect.receipt_recorded);
        assert!(!effect.topology_became_visible());
        assert_eq!(machine.phase(), TopologyAttemptPhase::AwaitingSnapshot);
    }

    #[test]
    fn old_incarnation_retains_diagnostics_and_ignores_late_snapshot_and_ticket() {
        let (mut retired, attempt) = pending(Intent("stable-port-id"), 1);
        retired.retire("node incarnation was replaced".to_owned());
        assert_eq!(retired.phase(), TopologyAttemptPhase::Retired);
        assert_eq!(retired.active_intent(), Some(&Intent("stable-port-id")));
        assert_eq!(retired.diagnostic(), Some("node incarnation was replaced"));

        assert!(!retired.observe(2, |_| true).topology_became_visible());
        assert_eq!(
            retired.settle(attempt, TicketSettlement::Applied(2)),
            TopologyTransitionEffect::default()
        );
        assert_eq!(retired.phase(), TopologyAttemptPhase::Retired);
        assert_eq!(retired.active_intent(), Some(&Intent("stable-port-id")));
        assert!(matches!(
            retired.begin_new(Intent("again"), 2),
            Err(TopologyAttemptDispatchError::Retired)
        ));
    }

    #[test]
    fn close_is_terminal() {
        let mut closed = AttemptMachine::<Intent>::default();
        closed.close(ViewCloseReason::HostClosed);
        assert_eq!(closed.phase(), TopologyAttemptPhase::Closed);
        assert!(matches!(
            closed.begin_new(Intent("again"), 2),
            Err(TopologyAttemptDispatchError::Closed {
                reason: ViewCloseReason::HostClosed
            })
        ));
    }
}
