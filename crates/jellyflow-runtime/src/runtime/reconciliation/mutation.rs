use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::commands::{CommandExecutionCompletion, CommandExecutionKind};
use crate::runtime::graph_runtime::GraphRevision;

static NEXT_MUTATION_ATTEMPT_ID: AtomicU64 = AtomicU64::new(1);

fn allocate_mutation_attempt() -> Option<MutationAttemptId> {
    NEXT_MUTATION_ATTEMPT_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .ok()
        .map(MutationAttemptId)
}

/// Opaque identity for one product intent handled by a mutation reducer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MutationAttemptId(u64);

impl MutationAttemptId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Why a mutation could not be started.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationBeginError {
    Busy,
    IdentityExhausted,
    RetryExpired,
    StaleRetryBase {
        required_revision: GraphRevision,
        received_revision: GraphRevision,
    },
}

/// Public phase projected by the pure reducer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationPhase {
    Idle,
    Pending,
    AwaitingSnapshot,
    Retryable,
    Uncertain,
}

/// Runtime or adapter evidence that terminates one mutation dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MutationCommandResult {
    Applied {
        revision: GraphRevision,
    },
    Noop {
        revision: GraphRevision,
    },
    Rejected {
        revision: GraphRevision,
        diagnostic: String,
    },
    Conflict {
        revision: GraphRevision,
        diagnostic: String,
    },
    Failed {
        diagnostic: String,
    },
    NativeDispatchFailed {
        diagnostic: String,
    },
    NotDispatched {
        diagnostic: String,
    },
    Uncertain {
        diagnostic: String,
    },
}

impl MutationCommandResult {
    /// Projects the semantic runtime completion without exposing runtime internals to products.
    #[must_use]
    pub fn from_runtime(completion: &CommandExecutionCompletion) -> Self {
        match completion.kind() {
            CommandExecutionKind::Applied => Self::Applied {
                revision: completion
                    .applied_revision()
                    .expect("applied command completion carries its committed revision"),
            },
            CommandExecutionKind::Noop => Self::Noop {
                revision: completion
                    .observed_revision()
                    .expect("noop command completion carries its observed revision"),
            },
            CommandExecutionKind::Rejected => Self::Rejected {
                revision: completion
                    .observed_revision()
                    .expect("rejected command completion carries its observed revision"),
                diagnostic: completion
                    .diagnostic_message()
                    .unwrap_or_else(|| "command rejected".to_owned()),
            },
            CommandExecutionKind::Conflict => Self::Conflict {
                revision: completion
                    .observed_revision()
                    .expect("conflict command completion carries its observed revision"),
                diagnostic: completion
                    .diagnostic_message()
                    .unwrap_or_else(|| "command conflicted".to_owned()),
            },
            CommandExecutionKind::Failed => Self::Failed {
                diagnostic: completion
                    .diagnostic_message()
                    .unwrap_or_else(|| "command execution failed".to_owned()),
            },
        }
    }

    fn revision(&self) -> Option<GraphRevision> {
        match self {
            Self::Applied { revision }
            | Self::Noop { revision }
            | Self::Rejected { revision, .. }
            | Self::Conflict { revision, .. } => Some(*revision),
            Self::Failed { .. }
            | Self::NativeDispatchFailed { .. }
            | Self::NotDispatched { .. }
            | Self::Uncertain { .. } => None,
        }
    }
}

fn completion_revision_is_valid(
    completion: &MutationCommandResult,
    base_revision: GraphRevision,
) -> bool {
    match completion {
        MutationCommandResult::Applied { revision } => {
            base_revision.checked_add(1) == Some(*revision)
        }
        MutationCommandResult::Noop { revision }
        | MutationCommandResult::Rejected { revision, .. } => *revision == base_revision,
        MutationCommandResult::Conflict { revision, .. } => *revision > base_revision,
        MutationCommandResult::Failed { .. }
        | MutationCommandResult::NativeDispatchFailed { .. }
        | MutationCommandResult::NotDispatched { .. }
        | MutationCommandResult::Uncertain { .. } => true,
    }
}

/// Explicit retry capability.  Dropping it leaves the current retryable state unchanged.
#[derive(Debug)]
pub struct MutationRetry<I> {
    source_attempt: MutationAttemptId,
    intent: I,
    kind: MutationRetryKind,
}

impl<I> MutationRetry<I> {
    #[must_use]
    pub const fn source_attempt(&self) -> MutationAttemptId {
        self.source_attempt
    }

    #[must_use]
    pub const fn kind(&self) -> MutationRetryKind {
        self.kind
    }

    #[must_use]
    pub const fn intent(&self) -> &I {
        &self.intent
    }

    /// Rebinds product facts while preserving the exact retry authority.
    ///
    /// Callers use this after refreshing an intent against the authoritative retry base. The
    /// source attempt and retry classification remain unchanged and are still validated by
    /// [`MutationReconciler::begin_retry`].
    #[must_use]
    pub fn rebind_intent(mut self, intent: I) -> Self {
        self.intent = intent;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationRetryKind {
    Retryable,
    ConfirmedUncertain,
}

/// Opaque capability for explicitly forgetting one definitive, unobservable attempt.
///
/// The capability is never minted for an uncertain execution outcome.
#[derive(Debug)]
pub struct MutationAbandonment {
    source_attempt: MutationAttemptId,
    kind: MutationAbandonmentKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MutationAbandonmentKind {
    AwaitingSnapshot,
    Retryable,
}

/// Why a definitive mutation attempt is eligible for an explicit retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationRetryReason {
    AuthoritativeMismatch,
    RuntimeRejected,
    RuntimeConflict,
    RuntimeFailed,
    NativeDispatchFailed,
    NotDispatched,
}

/// Observable effects from one reducer transition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MutationTransition {
    completion_recorded: bool,
    became_satisfied: bool,
    state_changed: bool,
}

impl MutationTransition {
    #[must_use]
    pub const fn completion_recorded(self) -> bool {
        self.completion_recorded
    }

    #[must_use]
    pub const fn became_satisfied(self) -> bool {
        self.became_satisfied
    }

    #[must_use]
    pub const fn state_changed(self) -> bool {
        self.state_changed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Observation {
    revision: GraphRevision,
    satisfied: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum State<I> {
    Idle,
    Pending {
        attempt: MutationAttemptId,
        intent: I,
        base_revision: GraphRevision,
        observation: Option<Observation>,
    },
    AwaitingSnapshot {
        attempt: MutationAttemptId,
        intent: I,
        minimum_revision: GraphRevision,
    },
    Retryable {
        attempt: MutationAttemptId,
        intent: I,
        minimum_revision: GraphRevision,
        observation: Option<Observation>,
        reason: MutationRetryReason,
        diagnostic: String,
    },
    Uncertain {
        attempt: MutationAttemptId,
        intent: I,
        base_revision: GraphRevision,
        observation: Option<Observation>,
        diagnostic: String,
    },
}

fn retryable_or_satisfied<I>(
    attempt: MutationAttemptId,
    intent: I,
    minimum_revision: GraphRevision,
    observation: Option<Observation>,
    reason: MutationRetryReason,
    diagnostic: String,
    transition: &mut MutationTransition,
) -> State<I> {
    if observation
        .as_ref()
        .is_some_and(|value| value.revision >= minimum_revision && value.satisfied)
    {
        transition.became_satisfied = true;
        State::Idle
    } else {
        State::Retryable {
            attempt,
            intent,
            minimum_revision,
            observation,
            reason,
            diagnostic,
        }
    }
}

/// Runtime-owned mutation reconciliation.  It has no task or renderer dependencies.
#[derive(Debug, Eq, PartialEq)]
pub struct MutationReconciler<I> {
    state: State<I>,
}

impl<I> Default for MutationReconciler<I> {
    fn default() -> Self {
        Self { state: State::Idle }
    }
}

impl<I: Clone> MutationReconciler<I> {
    #[must_use]
    pub fn phase(&self) -> MutationPhase {
        match &self.state {
            State::Idle => MutationPhase::Idle,
            State::Pending { .. } => MutationPhase::Pending,
            State::AwaitingSnapshot { .. } => MutationPhase::AwaitingSnapshot,
            State::Retryable { .. } => MutationPhase::Retryable,
            State::Uncertain { .. } => MutationPhase::Uncertain,
        }
    }

    #[must_use]
    pub fn active_intent(&self) -> Option<&I> {
        match &self.state {
            State::Pending { intent, .. }
            | State::AwaitingSnapshot { intent, .. }
            | State::Retryable { intent, .. }
            | State::Uncertain { intent, .. } => Some(intent),
            State::Idle => None,
        }
    }

    #[must_use]
    pub fn active_attempt(&self) -> Option<MutationAttemptId> {
        match &self.state {
            State::Pending { attempt, .. }
            | State::AwaitingSnapshot { attempt, .. }
            | State::Retryable { attempt, .. }
            | State::Uncertain { attempt, .. } => Some(*attempt),
            State::Idle => None,
        }
    }

    #[must_use]
    pub fn diagnostic(&self) -> Option<&str> {
        match &self.state {
            State::Retryable { diagnostic, .. } | State::Uncertain { diagnostic, .. } => {
                Some(diagnostic)
            }
            State::Idle | State::Pending { .. } | State::AwaitingSnapshot { .. } => None,
        }
    }

    #[must_use]
    pub fn retry_reason(&self) -> Option<MutationRetryReason> {
        match &self.state {
            State::Retryable { reason, .. } => Some(*reason),
            State::Idle
            | State::Pending { .. }
            | State::AwaitingSnapshot { .. }
            | State::Uncertain { .. } => None,
        }
    }

    #[must_use]
    pub fn can_confirm_uncertain_retry(&self) -> bool {
        matches!(
            &self.state,
            State::Uncertain {
                base_revision,
                observation: Some(observation),
                ..
            } if observation.revision > *base_revision && !observation.satisfied
        )
    }

    pub fn begin(
        &mut self,
        intent: I,
        base_revision: GraphRevision,
    ) -> Result<MutationAttemptId, MutationBeginError> {
        if !matches!(self.state, State::Idle) {
            return Err(MutationBeginError::Busy);
        }
        let attempt = allocate_mutation_attempt().ok_or(MutationBeginError::IdentityExhausted)?;
        self.state = State::Pending {
            attempt,
            intent,
            base_revision,
            observation: None,
        };
        Ok(attempt)
    }

    pub fn begin_retry(
        &mut self,
        retry: MutationRetry<I>,
        base_revision: GraphRevision,
    ) -> Result<MutationAttemptId, MutationBeginError> {
        let required_revision = match (&self.state, retry.kind) {
            (
                State::Retryable {
                    attempt,
                    minimum_revision,
                    observation: Some(observation),
                    ..
                },
                MutationRetryKind::Retryable,
            ) if *attempt == retry.source_attempt
                && observation.revision >= *minimum_revision
                && !observation.satisfied =>
            {
                Some(observation.revision)
            }
            (
                State::Uncertain {
                    attempt,
                    base_revision: original_base,
                    observation: Some(observation),
                    ..
                },
                MutationRetryKind::ConfirmedUncertain,
            ) if *attempt == retry.source_attempt
                && observation.revision > *original_base
                && !observation.satisfied =>
            {
                Some(observation.revision)
            }
            _ => None,
        };
        let Some(required_revision) = required_revision else {
            return Err(MutationBeginError::RetryExpired);
        };
        if base_revision < required_revision {
            return Err(MutationBeginError::StaleRetryBase {
                required_revision,
                received_revision: base_revision,
            });
        }
        let attempt = allocate_mutation_attempt().ok_or(MutationBeginError::IdentityExhausted)?;
        let state = std::mem::replace(&mut self.state, State::Idle);
        match state {
            State::Retryable { .. } | State::Uncertain { .. } => {}
            state => {
                self.state = state;
                return Err(MutationBeginError::RetryExpired);
            }
        }
        self.state = State::Pending {
            attempt,
            intent: retry.intent,
            base_revision,
            observation: None,
        };
        Ok(attempt)
    }

    #[must_use]
    pub fn retry(&self, confirm_uncertain: bool) -> Option<MutationRetry<I>> {
        match &self.state {
            State::Retryable {
                attempt,
                intent,
                minimum_revision,
                observation: Some(observation),
                ..
            } if !confirm_uncertain
                && observation.revision >= *minimum_revision
                && !observation.satisfied =>
            {
                Some(MutationRetry {
                    source_attempt: *attempt,
                    intent: intent.clone(),
                    kind: MutationRetryKind::Retryable,
                })
            }
            State::Uncertain {
                attempt,
                intent,
                base_revision,
                observation: Some(observation),
                ..
            } if confirm_uncertain
                && observation.revision > *base_revision
                && !observation.satisfied =>
            {
                Some(MutationRetry {
                    source_attempt: *attempt,
                    intent: intent.clone(),
                    kind: MutationRetryKind::ConfirmedUncertain,
                })
            }
            _ => None,
        }
    }

    /// Abandons the exact retry capability after current product facts make its intent invalid.
    ///
    /// This cannot cancel a pending command. It only consumes a capability that was already
    /// eligible for an explicit retry and returns the reducer to idle.
    pub fn abandon_retry(&mut self, retry: MutationRetry<I>) -> bool {
        let matches_active = match &self.state {
            State::Retryable { attempt, .. } => {
                retry.kind == MutationRetryKind::Retryable && *attempt == retry.source_attempt
            }
            State::Uncertain { attempt, .. } => {
                retry.kind == MutationRetryKind::ConfirmedUncertain
                    && *attempt == retry.source_attempt
            }
            State::Idle | State::Pending { .. } | State::AwaitingSnapshot { .. } => false,
        };
        if matches_active {
            self.state = State::Idle;
        }
        matches_active
    }

    /// Returns an attempt-bound capability when execution is definitive but facts cannot parse.
    #[must_use]
    pub fn unobservable_abandonment(&self) -> Option<MutationAbandonment> {
        match &self.state {
            State::AwaitingSnapshot { attempt, .. } => Some(MutationAbandonment {
                source_attempt: *attempt,
                kind: MutationAbandonmentKind::AwaitingSnapshot,
            }),
            State::Retryable { attempt, .. } => Some(MutationAbandonment {
                source_attempt: *attempt,
                kind: MutationAbandonmentKind::Retryable,
            }),
            State::Idle | State::Pending { .. } | State::Uncertain { .. } => None,
        }
    }

    /// Consumes an exact capability without cancelling pending or uncertain execution.
    pub fn abandon_unobservable(&mut self, abandonment: MutationAbandonment) -> bool {
        let matches_active = match &self.state {
            State::AwaitingSnapshot { attempt, .. } => {
                abandonment.kind == MutationAbandonmentKind::AwaitingSnapshot
                    && *attempt == abandonment.source_attempt
            }
            State::Retryable { attempt, .. } => {
                abandonment.kind == MutationAbandonmentKind::Retryable
                    && *attempt == abandonment.source_attempt
            }
            State::Idle | State::Pending { .. } | State::Uncertain { .. } => false,
        };
        if matches_active {
            self.state = State::Idle;
        }
        matches_active
    }

    pub fn complete(
        &mut self,
        attempt: MutationAttemptId,
        completion: MutationCommandResult,
    ) -> Result<MutationTransition, MutationCompletionError> {
        let (active, base_revision) = match &self.state {
            State::Pending {
                attempt,
                base_revision,
                ..
            }
            | State::Uncertain {
                attempt,
                base_revision,
                ..
            } => (*attempt, *base_revision),
            _ => return Err(MutationCompletionError::NotPending { attempt }),
        };
        if active != attempt {
            return Err(MutationCompletionError::ForeignAttempt { attempt, active });
        }
        if let Some(received_revision) = completion.revision()
            && !completion_revision_is_valid(&completion, base_revision)
        {
            return Err(MutationCompletionError::InvalidRevision {
                attempt,
                base_revision,
                received_revision,
            });
        }

        let state = std::mem::replace(&mut self.state, State::Idle);
        let (intent, observation, was_uncertain) = match state {
            State::Pending {
                intent,
                observation,
                ..
            } => (intent, observation, false),
            State::Uncertain {
                intent,
                observation,
                ..
            } => (intent, observation, true),
            _ => unreachable!("mutation completion preflight accepted only active states"),
        };

        let mut transition = MutationTransition {
            completion_recorded: true,
            became_satisfied: false,
            state_changed: true,
        };
        self.state = match completion {
            MutationCommandResult::Applied { revision }
            | MutationCommandResult::Noop { revision } => {
                let reflected = observation
                    .as_ref()
                    .filter(|value| value.revision >= revision)
                    .map(|value| (value.revision, value.satisfied));
                match reflected {
                    Some((_, true)) => {
                        transition.became_satisfied = true;
                        State::Idle
                    }
                    Some((observed_revision, false)) => State::Retryable {
                        attempt,
                        intent,
                        minimum_revision: revision,
                        observation,
                        reason: MutationRetryReason::AuthoritativeMismatch,
                        diagnostic: format!(
                            "mutation was not reflected by authoritative revision {observed_revision}"
                        ),
                    },
                    None => State::AwaitingSnapshot {
                        attempt,
                        intent,
                        minimum_revision: revision,
                    },
                }
            }
            MutationCommandResult::Conflict {
                revision,
                diagnostic,
            } => retryable_or_satisfied(
                attempt,
                intent,
                revision,
                observation,
                MutationRetryReason::RuntimeConflict,
                diagnostic,
                &mut transition,
            ),
            MutationCommandResult::Rejected {
                revision,
                diagnostic,
            } => retryable_or_satisfied(
                attempt,
                intent,
                revision,
                observation,
                MutationRetryReason::RuntimeRejected,
                diagnostic,
                &mut transition,
            ),
            MutationCommandResult::Failed { diagnostic } => retryable_or_satisfied(
                attempt,
                intent,
                base_revision,
                observation,
                MutationRetryReason::RuntimeFailed,
                diagnostic,
                &mut transition,
            ),
            MutationCommandResult::NativeDispatchFailed { diagnostic } => retryable_or_satisfied(
                attempt,
                intent,
                base_revision,
                observation,
                MutationRetryReason::NativeDispatchFailed,
                diagnostic,
                &mut transition,
            ),
            MutationCommandResult::NotDispatched { diagnostic } => retryable_or_satisfied(
                attempt,
                intent,
                base_revision,
                observation,
                MutationRetryReason::NotDispatched,
                diagnostic,
                &mut transition,
            ),
            MutationCommandResult::Uncertain { diagnostic } => {
                if was_uncertain {
                    State::Uncertain {
                        attempt,
                        intent,
                        base_revision,
                        observation,
                        diagnostic,
                    }
                } else if observation
                    .as_ref()
                    .is_some_and(|value| value.revision > base_revision && value.satisfied)
                {
                    transition.became_satisfied = true;
                    State::Idle
                } else {
                    State::Uncertain {
                        attempt,
                        intent,
                        base_revision,
                        observation,
                        diagnostic,
                    }
                }
            }
        };
        Ok(transition)
    }

    pub fn observe(
        &mut self,
        revision: GraphRevision,
        satisfied: impl FnOnce(&I) -> bool,
    ) -> MutationTransition {
        let intent = match &self.state {
            State::Pending {
                intent,
                base_revision,
                observation,
                ..
            } if revision >= *base_revision
                && observation
                    .as_ref()
                    .is_none_or(|current| current.revision < revision) =>
            {
                Some(intent)
            }
            State::AwaitingSnapshot {
                intent,
                minimum_revision,
                ..
            } if revision >= *minimum_revision => Some(intent),
            State::Retryable {
                intent,
                minimum_revision,
                observation,
                ..
            } if revision >= *minimum_revision
                && observation
                    .as_ref()
                    .is_none_or(|current| current.revision < revision) =>
            {
                Some(intent)
            }
            State::Uncertain {
                intent,
                base_revision,
                observation,
                ..
            } if revision > *base_revision
                && observation
                    .as_ref()
                    .is_none_or(|current| current.revision < revision) =>
            {
                Some(intent)
            }
            State::Idle
            | State::Pending { .. }
            | State::AwaitingSnapshot { .. }
            | State::Retryable { .. }
            | State::Uncertain { .. } => None,
        };
        // Evaluate application code before moving state out of the reducer. If it panics, the
        // active attempt and all previously recorded evidence remain intact.
        let satisfaction = intent.map(satisfied);
        let state = std::mem::replace(&mut self.state, State::Idle);
        let mut transition = MutationTransition::default();
        self.state = match state {
            State::Pending {
                attempt,
                intent,
                base_revision,
                observation,
            } => {
                if revision < base_revision
                    || observation
                        .as_ref()
                        .is_some_and(|current| current.revision >= revision)
                {
                    State::Pending {
                        attempt,
                        intent,
                        base_revision,
                        observation,
                    }
                } else {
                    transition.state_changed = true;
                    State::Pending {
                        attempt,
                        observation: Some(Observation {
                            revision,
                            satisfied: satisfaction.expect("pending observation was preflighted"),
                        }),
                        intent,
                        base_revision,
                    }
                }
            }
            State::AwaitingSnapshot {
                attempt,
                intent,
                minimum_revision,
            } if revision < minimum_revision => State::AwaitingSnapshot {
                attempt,
                intent,
                minimum_revision,
            },
            State::AwaitingSnapshot {
                attempt,
                intent,
                minimum_revision,
            } => {
                let observation = Observation {
                    revision,
                    satisfied: satisfaction.expect("awaiting-snapshot observation was preflighted"),
                };
                transition.state_changed = true;
                if observation.satisfied {
                    transition.became_satisfied = true;
                    State::Idle
                } else {
                    State::Retryable {
                        attempt,
                        intent,
                        minimum_revision,
                        observation: Some(observation),
                        reason: MutationRetryReason::AuthoritativeMismatch,
                        diagnostic: format!(
                            "mutation was not reflected by authoritative revision {revision}"
                        ),
                    }
                }
            }
            State::Retryable {
                attempt,
                intent,
                minimum_revision,
                observation,
                reason,
                diagnostic,
            } => {
                if revision < minimum_revision
                    || observation
                        .as_ref()
                        .is_some_and(|current| current.revision >= revision)
                {
                    State::Retryable {
                        attempt,
                        intent,
                        minimum_revision,
                        observation,
                        reason,
                        diagnostic,
                    }
                } else {
                    let observation = Observation {
                        revision,
                        satisfied: satisfaction.expect("retryable observation was preflighted"),
                    };
                    transition.state_changed = true;
                    if observation.satisfied {
                        transition.became_satisfied = true;
                        State::Idle
                    } else {
                        State::Retryable {
                            attempt,
                            intent,
                            minimum_revision,
                            observation: Some(observation),
                            reason,
                            diagnostic,
                        }
                    }
                }
            }
            State::Uncertain {
                attempt,
                intent,
                base_revision,
                observation,
                diagnostic,
            } => {
                if revision <= base_revision
                    || observation
                        .as_ref()
                        .is_some_and(|current| current.revision >= revision)
                {
                    State::Uncertain {
                        attempt,
                        intent,
                        base_revision,
                        observation,
                        diagnostic,
                    }
                } else {
                    let observation = Observation {
                        revision,
                        satisfied: satisfaction.expect("uncertain observation was preflighted"),
                    };
                    transition.state_changed = true;
                    if observation.satisfied {
                        transition.became_satisfied = true;
                        State::Idle
                    } else {
                        State::Uncertain {
                            attempt,
                            intent,
                            base_revision,
                            observation: Some(observation),
                            diagnostic,
                        }
                    }
                }
            }
            State::Idle => State::Idle,
        };
        transition
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationCompletionError {
    NotPending {
        attempt: MutationAttemptId,
    },
    ForeignAttempt {
        attempt: MutationAttemptId,
        active: MutationAttemptId,
    },
    InvalidRevision {
        attempt: MutationAttemptId,
        base_revision: GraphRevision,
        received_revision: GraphRevision,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(revision: GraphRevision) -> MutationCommandResult {
        MutationCommandResult::Applied { revision }
    }

    #[test]
    fn revision_bearing_completion_cannot_predate_its_admitted_base() {
        let completions = [
            MutationCommandResult::Applied { revision: 9 },
            MutationCommandResult::Noop { revision: 9 },
            MutationCommandResult::Rejected {
                revision: 9,
                diagnostic: "rejected".to_owned(),
            },
            MutationCommandResult::Conflict {
                revision: 9,
                diagnostic: "conflict".to_owned(),
            },
        ];

        for completion in completions {
            let mut reducer = MutationReconciler::default();
            let attempt = reducer.begin("add-port", 10).expect("begin");
            reducer.observe(10, |_| true);
            let before = reducer.state.clone();

            assert_eq!(
                reducer.complete(attempt, completion),
                Err(MutationCompletionError::InvalidRevision {
                    attempt,
                    base_revision: 10,
                    received_revision: 9,
                })
            );
            assert_eq!(reducer.state, before);
            assert_eq!(reducer.phase(), MutationPhase::Pending);
            assert_eq!(reducer.active_attempt(), Some(attempt));
            assert_eq!(reducer.active_intent(), Some(&"add-port"));
        }
    }

    #[test]
    fn completion_revision_rules_match_runtime_execution_kinds() {
        let cases = [
            (MutationCommandResult::Applied { revision: 10 }, false),
            (MutationCommandResult::Applied { revision: 11 }, true),
            (MutationCommandResult::Noop { revision: 10 }, true),
            (MutationCommandResult::Noop { revision: 11 }, false),
            (
                MutationCommandResult::Rejected {
                    revision: 10,
                    diagnostic: "rejected".to_owned(),
                },
                true,
            ),
            (
                MutationCommandResult::Rejected {
                    revision: 11,
                    diagnostic: "rejected".to_owned(),
                },
                false,
            ),
            (
                MutationCommandResult::Conflict {
                    revision: 10,
                    diagnostic: "conflict".to_owned(),
                },
                false,
            ),
            (
                MutationCommandResult::Conflict {
                    revision: 11,
                    diagnostic: "conflict".to_owned(),
                },
                true,
            ),
        ];

        for (completion, valid) in cases {
            let mut reducer = MutationReconciler::default();
            let attempt = reducer.begin("edit", 10).expect("begin");
            let description = format!("{completion:?}");
            let result = reducer.complete(attempt, completion);
            assert_eq!(result.is_ok(), valid, "completion={description}");
            if !valid {
                assert_eq!(reducer.phase(), MutationPhase::Pending);
            }
        }
    }

    #[test]
    fn receipt_first_and_snapshot_first_both_satisfy_once() {
        for snapshot_first in [true, false] {
            let mut reducer = MutationReconciler::default();
            let attempt = reducer.begin("add-port", 1).expect("begin");
            if snapshot_first {
                assert!(
                    !reducer
                        .observe(2, |intent| *intent == "add-port")
                        .became_satisfied()
                );
            }
            let transition = reducer.complete(attempt, applied(2)).expect("complete");
            if !snapshot_first {
                assert!(!transition.became_satisfied());
            }
            if snapshot_first {
                assert!(transition.became_satisfied());
            } else {
                assert_eq!(reducer.phase(), MutationPhase::AwaitingSnapshot);
                assert!(reducer.observe(2, |_| true).became_satisfied());
            }
            assert_eq!(reducer.phase(), MutationPhase::Idle);
        }
    }

    #[test]
    fn definitive_native_dispatch_failure_is_retryable_without_becoming_uncertain() {
        let mut reducer = MutationReconciler::default();
        let attempt = reducer.begin("add-port", 7).expect("begin");

        reducer
            .complete(
                attempt,
                MutationCommandResult::NativeDispatchFailed {
                    diagnostic: "frame preparation failed".to_owned(),
                },
            )
            .expect("complete");

        assert_eq!(reducer.phase(), MutationPhase::Retryable);
        assert_eq!(
            reducer.retry_reason(),
            Some(MutationRetryReason::NativeDispatchFailed)
        );
        assert_eq!(reducer.diagnostic(), Some("frame preparation failed"));
        assert!(
            reducer.retry(false).is_none(),
            "retry waits for an authoritative unsatisfied observation"
        );
        reducer.observe(7, |_| false);
        assert!(reducer.retry(false).is_some());
        assert!(!reducer.can_confirm_uncertain_retry());
    }

    #[test]
    fn abandoning_an_exact_retry_returns_to_idle_and_expires_duplicate_capabilities() {
        let mut reducer = MutationReconciler::default();
        let attempt = reducer.begin("reorder-case", 7).expect("begin");
        reducer
            .complete(
                attempt,
                MutationCommandResult::Conflict {
                    revision: 8,
                    diagnostic: "stale topology".to_owned(),
                },
            )
            .expect("complete");
        reducer.observe(8, |_| false);
        let retry = reducer.retry(false).expect("eligible retry");
        let duplicate = reducer.retry(false).expect("duplicate read capability");

        assert!(reducer.abandon_retry(retry));
        assert_eq!(reducer.phase(), MutationPhase::Idle);
        assert!(reducer.active_intent().is_none());
        assert!(!reducer.abandon_retry(duplicate));
    }

    #[test]
    fn unobservable_abandonment_never_cancels_pending_but_releases_a_completed_attempt() {
        let mut reducer = MutationReconciler::default();
        let attempt = reducer.begin("add-case", 7).expect("begin");

        assert!(reducer.unobservable_abandonment().is_none());
        assert_eq!(reducer.phase(), MutationPhase::Pending);
        reducer
            .complete(attempt, MutationCommandResult::Applied { revision: 8 })
            .expect("complete");
        assert_eq!(reducer.phase(), MutationPhase::AwaitingSnapshot);

        let abandonment = reducer
            .unobservable_abandonment()
            .expect("definitive completion can be explicitly abandoned");
        let duplicate = reducer
            .unobservable_abandonment()
            .expect("capability reads do not mutate reducer state");
        assert!(reducer.abandon_unobservable(abandonment));
        assert_eq!(reducer.phase(), MutationPhase::Idle);
        assert!(reducer.active_intent().is_none());
        assert!(!reducer.abandon_unobservable(duplicate));
    }

    #[test]
    fn uncertain_execution_never_mints_an_unobservable_abandonment() {
        let mut reducer = MutationReconciler::default();
        let attempt = reducer.begin("add-case", 7).expect("begin");
        reducer
            .complete(
                attempt,
                MutationCommandResult::Uncertain {
                    diagnostic: "callback lost".to_owned(),
                },
            )
            .expect("complete");

        assert_eq!(reducer.phase(), MutationPhase::Uncertain);
        assert!(reducer.unobservable_abandonment().is_none());
        assert_eq!(reducer.active_intent(), Some(&"add-case"));
    }

    #[test]
    fn snapshot_first_unsatisfied_observation_becomes_retryable_without_another_snapshot() {
        let mut reducer = MutationReconciler::default();
        let attempt = reducer.begin("add-port", 4).expect("begin");

        assert!(!reducer.observe(5, |_| false).became_satisfied());
        let transition = reducer.complete(attempt, applied(5)).expect("complete");

        assert!(transition.completion_recorded());
        assert!(!transition.became_satisfied());
        assert!(transition.state_changed());
        assert_eq!(reducer.phase(), MutationPhase::Retryable);
        assert_eq!(
            reducer.retry_reason(),
            Some(MutationRetryReason::AuthoritativeMismatch)
        );
        assert_eq!(
            reducer.diagnostic(),
            Some("mutation was not reflected by authoritative revision 5")
        );
    }

    #[test]
    fn snapshot_first_satisfaction_resolves_an_uncertain_completion() {
        let mut reducer = MutationReconciler::default();
        let attempt = reducer.begin("remove-port", 7).expect("begin");

        reducer.observe(8, |_| true);
        let transition = reducer
            .complete(
                attempt,
                MutationCommandResult::Uncertain {
                    diagnostic: "callback lost".to_owned(),
                },
            )
            .expect("complete");

        assert!(transition.completion_recorded());
        assert!(transition.became_satisfied());
        assert_eq!(reducer.phase(), MutationPhase::Idle);
    }

    #[test]
    fn late_noop_at_the_base_revision_resolves_uncertainty_only_with_definitive_receipt() {
        for satisfied in [true, false] {
            let mut reducer = MutationReconciler::default();
            let attempt = reducer.begin("already-applied", 5).expect("begin");
            reducer
                .complete(
                    attempt,
                    MutationCommandResult::Uncertain {
                        diagnostic: "notification lost".to_owned(),
                    },
                )
                .expect("uncertain");

            let transition = reducer
                .complete(attempt, MutationCommandResult::Noop { revision: 5 })
                .expect("late noop");
            if satisfied {
                assert!(!transition.became_satisfied());
                assert!(reducer.observe(5, |_| true).became_satisfied());
                assert_eq!(reducer.phase(), MutationPhase::Idle);
            } else {
                assert!(!transition.became_satisfied());
                assert_eq!(reducer.phase(), MutationPhase::AwaitingSnapshot);
                assert!(reducer.observe(5, |_| false).state_changed());
                assert_eq!(reducer.phase(), MutationPhase::Retryable);
            }
        }
    }

    #[test]
    fn equal_base_observation_cannot_resolve_uncertainty_without_a_receipt() {
        let mut reducer = MutationReconciler::default();
        let attempt = reducer.begin("already-applied", 5).expect("begin");
        reducer
            .complete(
                attempt,
                MutationCommandResult::Uncertain {
                    diagnostic: "notification lost".to_owned(),
                },
            )
            .expect("uncertain");

        assert!(!reducer.observe(5, |_| true).became_satisfied());
        assert_eq!(reducer.phase(), MutationPhase::Uncertain);
    }

    #[test]
    fn noop_at_the_base_revision_is_order_independent() {
        for snapshot_first in [true, false] {
            let mut reducer = MutationReconciler::default();
            let attempt = reducer.begin("already-applied", 5).expect("begin");
            if snapshot_first {
                reducer.observe(5, |_| true);
            }

            let transition = reducer
                .complete(attempt, MutationCommandResult::Noop { revision: 5 })
                .expect("complete");
            if snapshot_first {
                assert!(transition.became_satisfied());
            } else {
                assert_eq!(reducer.phase(), MutationPhase::AwaitingSnapshot);
                assert!(reducer.observe(5, |_| true).became_satisfied());
            }
            assert_eq!(reducer.phase(), MutationPhase::Idle);
        }
    }

    #[test]
    fn stale_observations_do_not_replace_newer_pending_or_uncertain_evidence() {
        let mut reducer = MutationReconciler::default();
        let attempt = reducer.begin("edit", 10).expect("begin");

        assert!(reducer.observe(12, |_| true).state_changed());
        assert!(
            !reducer
                .observe(11, |_| panic!(
                    "stale pending observation must not be evaluated"
                ))
                .state_changed()
        );
        assert!(
            reducer
                .complete(attempt, applied(11))
                .unwrap()
                .became_satisfied()
        );

        let attempt = reducer.begin("retry", 12).expect("begin successor");
        assert!(reducer.observe(14, |_| false).state_changed());
        reducer
            .complete(
                attempt,
                MutationCommandResult::Uncertain {
                    diagnostic: "transport lost".to_owned(),
                },
            )
            .expect("complete");
        assert!(
            !reducer
                .observe(13, |_| panic!(
                    "stale uncertain observation must not be evaluated"
                ))
                .state_changed()
        );

        assert_eq!(reducer.phase(), MutationPhase::Uncertain);
        assert!(reducer.can_confirm_uncertain_retry());
    }

    #[test]
    fn conflict_is_retryable_but_uncertain_requires_new_evidence() {
        let mut reducer = MutationReconciler::default();
        let attempt = reducer.begin("edit", 4).expect("begin");
        reducer
            .complete(
                attempt,
                MutationCommandResult::Conflict {
                    revision: 5,
                    diagnostic: "stale".to_owned(),
                },
            )
            .expect("complete");
        assert_eq!(reducer.phase(), MutationPhase::Retryable);
        assert_eq!(
            reducer.retry_reason(),
            Some(MutationRetryReason::RuntimeConflict)
        );
        assert!(reducer.retry(false).is_none());
        reducer.observe(5, |_| false);

        let retry = reducer.retry(false).expect("retry");
        let attempt = reducer.begin_retry(retry, 5).expect("begin retry");
        reducer
            .complete(
                attempt,
                MutationCommandResult::Uncertain {
                    diagnostic: "callback lost".to_owned(),
                },
            )
            .expect("uncertain");
        assert_eq!(reducer.phase(), MutationPhase::Uncertain);
        assert!(reducer.retry(true).is_none());
        reducer.observe(6, |_| false);
        assert!(reducer.retry(true).is_some());
    }

    #[test]
    fn retry_authority_can_rebind_refreshed_intent_without_changing_attempt_identity() {
        let mut reducer = MutationReconciler::default();
        let original = reducer.begin("original", 4).expect("begin original");
        reducer
            .complete(
                original,
                MutationCommandResult::Uncertain {
                    diagnostic: "completion lost".to_owned(),
                },
            )
            .expect("record uncertainty");
        reducer.observe(5, |_| false);
        let retry = reducer
            .retry(true)
            .expect("later absence authorizes retry")
            .rebind_intent("refreshed");
        assert_eq!(retry.source_attempt(), original);

        let successor = reducer.begin_retry(retry, 5).expect("begin rebound retry");
        assert_ne!(successor, original);
        assert_eq!(reducer.active_intent(), Some(&"refreshed"));
        assert_eq!(reducer.active_attempt(), Some(successor));
    }

    #[test]
    fn rejected_and_conflict_completions_require_their_authoritative_revision() {
        for (completion, reason, satisfying_revision) in [
            (
                MutationCommandResult::Rejected {
                    revision: 4,
                    diagnostic: "rejected".to_owned(),
                },
                MutationRetryReason::RuntimeRejected,
                5,
            ),
            (
                MutationCommandResult::Conflict {
                    revision: 6,
                    diagnostic: "conflict".to_owned(),
                },
                MutationRetryReason::RuntimeConflict,
                6,
            ),
        ] {
            let mut reducer = MutationReconciler::default();
            let attempt = reducer.begin("edit", 4).expect("begin");
            reducer.observe(4, |_| false);
            reducer.complete(attempt, completion).expect("complete");

            assert_eq!(reducer.phase(), MutationPhase::Retryable);
            assert_eq!(reducer.retry_reason(), Some(reason));
            assert!(
                reducer
                    .observe(satisfying_revision, |_| true)
                    .became_satisfied()
            );
            assert_eq!(reducer.phase(), MutationPhase::Idle);
        }
    }

    #[test]
    fn failed_is_retryable_while_transport_uncertainty_requires_fresh_observation() {
        let mut reducer = MutationReconciler::default();
        let first = reducer.begin("edit", 3).expect("begin");
        reducer
            .complete(
                first,
                MutationCommandResult::Failed {
                    diagnostic: "execution failed".to_owned(),
                },
            )
            .expect("failed completion");
        assert_eq!(reducer.phase(), MutationPhase::Retryable);
        assert_eq!(
            reducer.retry_reason(),
            Some(MutationRetryReason::RuntimeFailed)
        );

        assert!(reducer.retry(false).is_none());
        reducer.observe(3, |_| false);
        let retry = reducer.retry(false).expect("known failure can be retried");
        let second = reducer.begin_retry(retry, 3).expect("begin retry");
        assert!(second.get() > first.get());
        reducer
            .complete(
                second,
                MutationCommandResult::Uncertain {
                    diagnostic: "callback lost".to_owned(),
                },
            )
            .expect("uncertain completion");
        assert_eq!(reducer.phase(), MutationPhase::Uncertain);

        assert!(
            !reducer
                .observe(2, |_| panic!(
                    "pre-base snapshot must not settle uncertainty"
                ))
                .state_changed()
        );
        assert!(
            !reducer
                .observe(3, |_| panic!("base snapshot must not settle uncertainty"))
                .state_changed()
        );
        assert!(!reducer.can_confirm_uncertain_retry());
        reducer.observe(4, |_| false);
        assert!(reducer.can_confirm_uncertain_retry());
    }

    #[test]
    fn retry_capability_expires_after_a_successor_attempt_starts() {
        let mut reducer = MutationReconciler::default();
        let first = reducer.begin("edit", 1).expect("begin");
        reducer
            .complete(
                first,
                MutationCommandResult::NotDispatched {
                    diagnostic: "capacity unavailable".to_owned(),
                },
            )
            .expect("not dispatched");

        assert!(reducer.retry(false).is_none());
        reducer.observe(1, |_| false);
        let accepted = reducer.retry(false).expect("retry");
        let stale = reducer.retry(false).expect("second plan");
        let successor = reducer.begin_retry(accepted, 1).expect("begin retry");
        assert!(successor.get() > first.get());
        assert_eq!(
            reducer.begin_retry(stale, 1),
            Err(MutationBeginError::RetryExpired)
        );
        assert_eq!(reducer.active_attempt(), Some(successor));
        assert_eq!(reducer.active_intent(), Some(&"edit"));
    }

    #[test]
    fn stale_retry_base_is_rejected_without_consuming_an_attempt_identity() {
        let mut reducer = MutationReconciler::default();
        let first = reducer.begin("edit", 2).expect("begin");
        reducer
            .complete(
                first,
                MutationCommandResult::Conflict {
                    revision: 5,
                    diagnostic: "conflict".to_owned(),
                },
            )
            .expect("conflict");
        reducer.observe(7, |_| false);

        let stale = reducer.retry(false).expect("retry plan");
        assert_eq!(
            reducer.begin_retry(stale, 6),
            Err(MutationBeginError::StaleRetryBase {
                required_revision: 7,
                received_revision: 6,
            })
        );

        let current = reducer.retry(false).expect("replacement retry plan");
        let successor = reducer.begin_retry(current, 7).expect("begin retry");
        assert!(successor.get() > first.get());
    }

    #[test]
    fn retry_capability_cannot_cross_reconciler_ownership() {
        let mut first = MutationReconciler::default();
        let first_attempt = first.begin("first", 1).expect("begin first");
        first
            .complete(
                first_attempt,
                MutationCommandResult::NotDispatched {
                    diagnostic: "first not dispatched".to_owned(),
                },
            )
            .expect("complete first");
        first.observe(1, |_| false);

        let mut second = MutationReconciler::default();
        let second_attempt = second.begin("second", 1).expect("begin second");
        second
            .complete(
                second_attempt,
                MutationCommandResult::NotDispatched {
                    diagnostic: "second not dispatched".to_owned(),
                },
            )
            .expect("complete second");
        second.observe(1, |_| false);

        let foreign = first.retry(false).expect("first retry capability");
        assert_eq!(
            second.begin_retry(foreign, 1),
            Err(MutationBeginError::RetryExpired)
        );
        assert_eq!(second.active_attempt(), Some(second_attempt));
        assert_eq!(second.active_intent(), Some(&"second"));
    }

    #[test]
    fn panicking_satisfaction_predicate_preserves_the_active_attempt() {
        let mut reducer = MutationReconciler::default();
        let attempt = reducer.begin("add-port", 4).expect("begin");

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            reducer.observe(5, |_| panic!("injected satisfaction panic"));
        }));

        assert!(panic.is_err());
        assert_eq!(reducer.phase(), MutationPhase::Pending);
        assert_eq!(reducer.active_attempt(), Some(attempt));
        assert_eq!(reducer.active_intent(), Some(&"add-port"));
        assert!(reducer.observe(5, |_| true).state_changed());
        assert!(
            reducer
                .complete(attempt, applied(5))
                .expect("completion after recovered predicate panic")
                .became_satisfied()
        );
    }

    #[test]
    fn foreign_completion_cannot_settle_active_intent() {
        let mut reducer = MutationReconciler::default();
        let active = reducer.begin("active", 0).expect("begin");
        let foreign = MutationAttemptId(active.get() + 1);
        assert_eq!(
            reducer.complete(foreign, applied(1)),
            Err(MutationCompletionError::ForeignAttempt {
                attempt: foreign,
                active
            })
        );
        assert_eq!(reducer.phase(), MutationPhase::Pending);
        assert_eq!(reducer.active_intent(), Some(&"active"));
    }

    #[test]
    fn late_completion_cannot_settle_a_successor_intent() {
        let mut reducer = MutationReconciler::default();
        let first = reducer.begin("first", 0).expect("begin first");
        reducer.observe(1, |_| true);
        assert!(
            reducer
                .complete(first, applied(1))
                .unwrap()
                .became_satisfied()
        );

        let successor = reducer.begin("successor", 1).expect("begin successor");
        assert_eq!(
            reducer.complete(first, applied(2)),
            Err(MutationCompletionError::ForeignAttempt {
                attempt: first,
                active: successor,
            })
        );
        assert_eq!(reducer.active_attempt(), Some(successor));
        assert_eq!(reducer.active_intent(), Some(&"successor"));
    }
}
