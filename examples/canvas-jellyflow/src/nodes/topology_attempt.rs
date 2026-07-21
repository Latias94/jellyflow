use jellyflow_open_gpui::authoring::{OpenGpuiNodeAuthoring, OpenGpuiNodeAuthoringError};
use jellyflow_open_gpui::{
    OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiCommandTicket,
    OpenGpuiCommandTicketResult, OpenGpuiNodeActivityRegistry,
};
use jellyflow_runtime::{
    GraphCommand, GraphCommandOutcome, GraphRevision, NodeInstanceKey, ViewCloseReason,
};
use open_gpui::{App, Task};

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

#[must_use = "a topology dispatch ticket must be awaited"]
pub(super) struct TopologyDispatch {
    attempt: u64,
    ticket: OpenGpuiCommandTicket,
}

impl TopologyDispatch {
    pub(super) const fn attempt(&self) -> u64 {
        self.attempt
    }

    pub(super) fn into_ticket(self) -> OpenGpuiCommandTicket {
        self.ticket
    }
}

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
    Retired,
    Closed(ViewCloseReason),
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SettlementEffect {
    accepted: bool,
    content_changed: bool,
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
            AttemptState::Retired => TopologyAttemptPhase::Retired,
            AttemptState::Closed(_) => TopologyAttemptPhase::Closed,
        }
    }

    fn allocate_attempt(&mut self) -> Result<u64, TopologyAttemptDispatchError> {
        let attempt = self
            .next_attempt
            .ok_or(TopologyAttemptDispatchError::IdentityExhausted)?;
        self.next_attempt = attempt.checked_add(1);
        Ok(attempt)
    }

    fn validate_request(
        &self,
        request: &TopologyDispatchRequest<I>,
    ) -> Result<(), TopologyAttemptDispatchError> {
        match &self.state {
            AttemptState::Retired => return Err(TopologyAttemptDispatchError::Retired),
            AttemptState::Closed(reason) => {
                return Err(TopologyAttemptDispatchError::Closed { reason: *reason });
            }
            _ => {}
        }
        match request {
            TopologyDispatchRequest::New(_) if matches!(&self.state, AttemptState::Idle) => Ok(()),
            TopologyDispatchRequest::Retry(retry) if self.retry_is_eligible(retry) => Ok(()),
            TopologyDispatchRequest::New(_) | TopologyDispatchRequest::Retry(_) => {
                Err(TopologyAttemptDispatchError::Busy)
            }
        }
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
            AttemptState::Retired => return Err(TopologyAttemptDispatchError::Retired),
            AttemptState::Closed(reason) => {
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

    fn settle(&mut self, attempt: u64, settlement: TicketSettlement) -> SettlementEffect {
        let state = std::mem::replace(&mut self.state, AttemptState::Idle);
        let AttemptState::Pending {
            attempt: active_attempt,
            intent,
            base_revision,
            observation,
        } = state
        else {
            self.state = state;
            return SettlementEffect::default();
        };
        if active_attempt != attempt {
            self.state = AttemptState::Pending {
                attempt: active_attempt,
                intent,
                base_revision,
                observation,
            };
            return SettlementEffect::default();
        }

        let content_changed = matches!(&settlement, TicketSettlement::Applied(_));
        self.state = match settlement {
            TicketSettlement::Applied(minimum_revision)
            | TicketSettlement::Noop(minimum_revision) => {
                if observation.as_ref().is_some_and(|observation| {
                    observation.revision >= minimum_revision && observation.satisfied
                }) {
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
        SettlementEffect {
            accepted: true,
            content_changed,
        }
    }

    fn observe(&mut self, revision: GraphRevision, satisfied: impl FnOnce(&I) -> bool) {
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
            AttemptState::Idle | AttemptState::Retired | AttemptState::Closed(_) => None,
        }
    }

    fn diagnostic(&self) -> Option<&str> {
        match &self.state {
            AttemptState::Recoverable { diagnostic, .. }
            | AttemptState::Uncertain { diagnostic, .. } => Some(diagnostic),
            _ => None,
        }
    }

    fn retire(&mut self) {
        self.state = AttemptState::Retired;
    }

    fn close(&mut self, reason: ViewCloseReason) {
        self.state = AttemptState::Closed(reason);
    }
}

/// Owns one node instance's topology command lifecycle and all native async resources.
pub(super) struct TopologyAttempt<I> {
    owner: NodeInstanceKey,
    machine: AttemptMachine<I>,
    task: Option<Task<()>>,
    task_lease: Option<OpenGpuiActivityLease>,
    retention_lease: Option<OpenGpuiActivityLease>,
}

impl<I: Clone> TopologyAttempt<I> {
    pub(super) fn new(owner: NodeInstanceKey) -> Self {
        Self {
            owner,
            machine: AttemptMachine::default(),
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

    pub(super) fn dispatch(
        &mut self,
        authoring: OpenGpuiNodeAuthoring,
        command: GraphCommand,
        request: TopologyDispatchRequest<I>,
        activity: &OpenGpuiNodeActivityRegistry,
        cx: &mut App,
    ) -> Result<TopologyDispatch, TopologyAttemptDispatchError> {
        if authoring.instance() != &self.owner {
            self.retire();
            return Err(TopologyAttemptDispatchError::Retired);
        }
        let base_revision = authoring.revision();
        self.machine.validate_request(&request)?;
        let retention_lease = if self.retention_lease.is_none() {
            Some(
                activity
                    .acquire(OpenGpuiActivityKind::Draft)
                    .map_err(|error| TopologyAttemptDispatchError::Activity {
                        message: error.to_string(),
                    })?,
            )
        } else {
            None
        };
        let task_lease = activity
            .acquire(OpenGpuiActivityKind::Task)
            .map_err(|error| TopologyAttemptDispatchError::Activity {
                message: error.to_string(),
            })?;
        let attempt = self.machine.begin_request(request, base_revision)?;
        if let Some(retention_lease) = retention_lease {
            self.retention_lease = Some(retention_lease);
        }
        if command.expected_revision() != base_revision {
            let diagnostic = format!(
                "command revision {} does not match topology owner revision {base_revision}",
                command.expected_revision()
            );
            self.machine.fail_dispatch(attempt, diagnostic.clone());
            return Err(TopologyAttemptDispatchError::NotDispatched {
                message: diagnostic,
            });
        }
        let ticket = match authoring.enqueue(command, cx) {
            Ok(ticket) => ticket,
            Err(error) => {
                let diagnostic = error.to_string();
                return match error {
                    OpenGpuiNodeAuthoringError::RetiredMount { .. }
                    | OpenGpuiNodeAuthoringError::RetiredIncarnation { .. }
                    | OpenGpuiNodeAuthoringError::MissingNode { .. } => {
                        self.retire();
                        Err(TopologyAttemptDispatchError::Retired)
                    }
                    OpenGpuiNodeAuthoringError::Closed { reason } => {
                        self.close(reason);
                        Err(TopologyAttemptDispatchError::Closed { reason })
                    }
                    _ => {
                        self.machine.fail_dispatch(attempt, diagnostic.clone());
                        Err(TopologyAttemptDispatchError::NotDispatched {
                            message: diagnostic,
                        })
                    }
                };
            }
        };
        self.task_lease = Some(task_lease);
        Ok(TopologyDispatch { attempt, ticket })
    }

    pub(super) fn own_task(&mut self, attempt: u64, task: Task<()>) {
        if matches!(
            &self.machine.state,
            AttemptState::Pending {
                attempt: active_attempt,
                ..
            } if *active_attempt == attempt
        ) {
            self.task = Some(task);
        }
    }

    pub(super) fn finish(&mut self, attempt: u64, result: OpenGpuiCommandTicketResult) -> bool {
        let settlement = match result {
            Ok(GraphCommandOutcome::Applied(receipt)) => {
                TicketSettlement::Applied(receipt.revision())
            }
            Ok(GraphCommandOutcome::Noop(noop)) => TicketSettlement::Noop(noop.revision()),
            Ok(GraphCommandOutcome::Conflict(conflict)) => TicketSettlement::Conflict {
                revision: conflict.current().revision(),
                diagnostic: format!(
                    "topology changed at revision {}; refresh before retry",
                    conflict.current().revision()
                ),
            },
            Ok(GraphCommandOutcome::Rejected(rejection)) => TicketSettlement::Rejected(format!(
                "topology edit was rejected: {}",
                rejection.message()
            )),
            Err(error) => TicketSettlement::Uncertain(format!(
                "topology edit outcome is uncertain: {error}; inspect the refreshed graph before retry"
            )),
        };
        let effect = self.machine.settle(attempt, settlement);
        if effect.accepted {
            self.task.take();
            self.task_lease.take();
        }
        self.sync_resources();
        effect.content_changed
    }

    pub(super) fn observe(
        &mut self,
        instance: &NodeInstanceKey,
        revision: GraphRevision,
        satisfied: impl FnOnce(&I) -> bool,
    ) {
        if instance != &self.owner {
            self.retire();
            return;
        }
        self.machine.observe(revision, satisfied);
        self.sync_resources();
    }

    pub(super) fn observe_authoring_error(&mut self, error: &OpenGpuiNodeAuthoringError) {
        match error {
            OpenGpuiNodeAuthoringError::RetiredMount { .. }
            | OpenGpuiNodeAuthoringError::RetiredIncarnation { .. }
            | OpenGpuiNodeAuthoringError::MissingNode { .. } => self.retire(),
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

    pub(super) fn retire(&mut self) {
        self.task.take();
        self.task_lease.take();
        self.retention_lease.take();
        self.machine.retire();
    }

    pub(super) fn close(&mut self, reason: ViewCloseReason) {
        self.task.take();
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
        assert!(
            machine
                .settle(attempt, TicketSettlement::Applied(5))
                .content_changed
        );
        assert_eq!(machine.phase(), TopologyAttemptPhase::AwaitingSnapshot);

        machine.observe(4, |_| true);
        assert_eq!(machine.phase(), TopologyAttemptPhase::AwaitingSnapshot);
        machine.observe(5, |intent| intent.0 == "add");
        assert_eq!(machine.phase(), TopologyAttemptPhase::Idle);
    }

    #[test]
    fn snapshot_first_is_retained_until_the_receipt_arrives() {
        let (mut machine, attempt) = pending(Intent("reorder"), 9);
        machine.observe(10, |intent| intent.0 == "reorder");
        assert_eq!(machine.phase(), TopologyAttemptPhase::Pending);

        machine.settle(attempt, TicketSettlement::Applied(10));
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
    fn stale_completion_cannot_settle_the_current_attempt() {
        let (mut machine, attempt) = pending(Intent("add"), 7);
        let effect = machine.settle(attempt + 1, TicketSettlement::Applied(8));
        assert_eq!(effect, SettlementEffect::default());
        assert_eq!(machine.phase(), TopologyAttemptPhase::Pending);

        let effect = machine.settle(attempt, TicketSettlement::Applied(8));
        assert!(effect.accepted);
        assert_eq!(machine.phase(), TopologyAttemptPhase::AwaitingSnapshot);
    }

    #[test]
    fn retirement_and_close_are_terminal() {
        let (mut retired, _) = pending(Intent("add"), 1);
        retired.retire();
        assert_eq!(retired.phase(), TopologyAttemptPhase::Retired);
        assert!(matches!(
            retired.begin_new(Intent("again"), 2),
            Err(TopologyAttemptDispatchError::Retired)
        ));

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
