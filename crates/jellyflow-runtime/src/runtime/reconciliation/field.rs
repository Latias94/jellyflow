use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::graph_runtime::GraphRevision;

static NEXT_FIELD_ATTEMPT: AtomicU64 = AtomicU64::new(1);

fn allocate_field_attempt() -> Option<FieldAttempt> {
    NEXT_FIELD_ATTEMPT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .ok()
        .map(FieldAttempt)
}

/// The local editing phase of a revision-aware retained value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldPhase {
    Clean,
    Dirty,
    Composing,
    Conflict,
    ComposingConflict,
}

/// Revision metadata for a local value that diverged from authoritative data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldConflict {
    base_revision: GraphRevision,
    remote_revision: GraphRevision,
}

impl FieldConflict {
    pub const fn new(base_revision: GraphRevision, remote_revision: GraphRevision) -> Self {
        Self {
            base_revision,
            remote_revision,
        }
    }

    pub const fn base_revision(self) -> GraphRevision {
        self.base_revision
    }

    pub const fn remote_revision(self) -> GraphRevision {
        self.remote_revision
    }
}

/// Runtime-wide opaque identity for one caller-dispatched field submission.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FieldAttempt(u64);

impl FieldAttempt {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Why a local field value is being submitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldSubmissionKind {
    Commit,
    Reapply,
}

/// An inert request for the caller to map into an application-owned command.
///
/// Planning this value never dispatches, retries, or mutates the graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldSubmissionIntent<T> {
    attempt: FieldAttempt,
    kind: FieldSubmissionKind,
    expected_revision: GraphRevision,
    value: T,
}

impl<T> FieldSubmissionIntent<T> {
    pub const fn attempt(&self) -> FieldAttempt {
        self.attempt
    }

    pub const fn kind(&self) -> FieldSubmissionKind {
        self.kind
    }

    pub const fn expected_revision(&self) -> GraphRevision {
        self.expected_revision
    }

    pub const fn value(&self) -> &T {
        &self.value
    }
}

/// A reason no submission intent was produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldIntentRejection {
    Clean,
    CompositionActive,
    ConflictRequiresResolution,
    NoConflict,
    UnreconciledRevision {
        observed_revision: GraphRevision,
        requested_revision: GraphRevision,
    },
    SubmissionPending {
        attempt: FieldAttempt,
    },
    AttemptIdentityExhausted,
}

/// Result of explicitly planning an ordinary dirty-value submission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldSubmissionDecision<T> {
    Submit(FieldSubmissionIntent<T>),
    Rejected(FieldIntentRejection),
}

/// Explicit choices offered when local and authoritative values conflict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldConflictResolution {
    AcceptRemote,
    KeepLocal,
    ReapplyLocal,
}

/// Result of applying an explicit conflict choice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldResolutionDecision<T> {
    AcceptedRemote { revision: GraphRevision },
    KeptLocal { conflict: FieldConflict },
    Reapply(FieldSubmissionIntent<T>),
    Rejected(FieldIntentRejection),
}

/// Effect of reconciling one authoritative field snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldReconcileDecision {
    Unchanged {
        revision: GraphRevision,
        phase: FieldPhase,
    },
    IgnoredStale {
        current_revision: GraphRevision,
        received_revision: GraphRevision,
        phase: FieldPhase,
    },
    InconsistentSnapshot {
        revision: GraphRevision,
        phase: FieldPhase,
    },
    RemoteApplied {
        from_revision: GraphRevision,
        to_revision: GraphRevision,
    },
    BaseAdvanced {
        from_revision: GraphRevision,
        to_revision: GraphRevision,
        phase: FieldPhase,
    },
    Conflict {
        conflict: FieldConflict,
    },
    /// The accepted attempt was observed at its committed revision with another value.
    AuthoritativeMismatch {
        attempt: FieldAttempt,
        committed_revision: GraphRevision,
    },
    /// A newer authoritative revision displaced the accepted attempt before it was observed.
    Superseded {
        attempt: FieldAttempt,
        committed_revision: GraphRevision,
        current_revision: GraphRevision,
    },
    SelfAcknowledged {
        attempt: FieldAttempt,
        revision: GraphRevision,
        local_changed_after_submission: bool,
    },
}

/// Effect of recording a receipt known by the caller to belong to this field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldReceiptDecision {
    Recorded {
        attempt: FieldAttempt,
        committed_revision: GraphRevision,
    },
    Acknowledged {
        attempt: FieldAttempt,
        revision: GraphRevision,
        local_changed_after_submission: bool,
    },
    AlreadyRecorded {
        attempt: FieldAttempt,
        committed_revision: GraphRevision,
    },
    UnknownAttempt {
        attempt: FieldAttempt,
    },
    InvalidCommittedRevision {
        attempt: FieldAttempt,
        expected_revision: GraphRevision,
        committed_revision: GraphRevision,
    },
    AuthoritativeMismatch {
        attempt: FieldAttempt,
        committed_revision: GraphRevision,
    },
    Superseded {
        attempt: FieldAttempt,
        committed_revision: GraphRevision,
        current_revision: GraphRevision,
    },
}

/// Effect of a stale expected-revision rejection reported by the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldStaleRejectionDecision {
    ConflictPreserved {
        conflict: FieldConflict,
    },
    DirtyPreserved {
        base_revision: GraphRevision,
    },
    LocalMatchesRemote {
        revision: GraphRevision,
        phase: FieldPhase,
    },
    IgnoredOlderRevision {
        current_revision: GraphRevision,
        received_revision: GraphRevision,
        phase: FieldPhase,
    },
    InconsistentSnapshot {
        revision: GraphRevision,
        phase: FieldPhase,
    },
    UnknownAttempt {
        attempt: FieldAttempt,
    },
    ReceiptAlreadyAccepted {
        attempt: FieldAttempt,
        committed_revision: GraphRevision,
    },
}

/// Effect of explicitly rejecting a caller-dispatched submission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldSubmissionRejectionDecision {
    Rejected {
        attempt: FieldAttempt,
        phase: FieldPhase,
    },
    UnknownAttempt {
        attempt: FieldAttempt,
    },
    ReceiptAlreadyAccepted {
        attempt: FieldAttempt,
        committed_revision: GraphRevision,
    },
}

/// Effect of changing local input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldEditDecision {
    Unchanged { phase: FieldPhase },
    Updated { phase: FieldPhase },
}

/// Effect of changing the framework-owned composition marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldCompositionDecision {
    Marked { phase: FieldPhase },
    Unmarked { phase: FieldPhase },
    Unchanged { phase: FieldPhase },
}

/// Diagnostic metadata retained when a field owner is restored.
///
/// Restoration recreates neither framework nor command ownership. Applications may surface
/// recovery state, but a new submission or composition session must always start explicitly.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FieldRecovery {
    interrupted_attempt: Option<FieldAttempt>,
    composition_interrupted: bool,
}

impl FieldRecovery {
    pub const fn interrupted_attempt(self) -> Option<FieldAttempt> {
        self.interrupted_attempt
    }

    pub const fn composition_interrupted(self) -> bool {
        self.composition_interrupted
    }

    pub const fn is_empty(self) -> bool {
        self.interrupted_attempt.is_none() && !self.composition_interrupted
    }
}

/// Framework-neutral snapshot retained while a field owner is reclaimed.
///
/// A pending attempt is diagnostic state only. Restoring or dispatching it is
/// deliberately outside this type so retirement cannot retry a graph command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldExternalization<T> {
    local: T,
    remote: T,
    base_revision: GraphRevision,
    remote_revision: GraphRevision,
    dirty: bool,
    conflict: Option<FieldConflict>,
    pending_attempt: Option<FieldAttempt>,
    was_composing: bool,
}

impl<T> FieldExternalization<T> {
    /// Restores field state and immediately reconciles it with current authoritative facts.
    pub fn restore_against(self, revision: GraphRevision, authoritative: T) -> FieldRestore<T>
    where
        T: Clone + PartialEq,
    {
        let mut restored = FieldRestore::from_externalized(self);
        restored
            .reconciler
            .reconcile_remote(revision, authoritative);
        if restored.reconciler.phase() == FieldPhase::Clean {
            restored.recovery.interrupted_attempt = None;
        }
        restored
    }
}

/// Result of restoring an externalized field.
///
/// The restored state has no pending submission. The optional attempt is retained separately so
/// an application can surface an honest recovery diagnostic without accidentally retrying it.
#[derive(Clone, Debug)]
pub struct FieldRestore<T> {
    reconciler: FieldReconciler<T>,
    recovery: FieldRecovery,
}

impl<T> FieldRestore<T> {
    pub const fn recovery(&self) -> FieldRecovery {
        self.recovery
    }

    pub fn into_reconciler(self) -> FieldReconciler<T> {
        self.reconciler
    }

    fn from_externalized(externalized: FieldExternalization<T>) -> Self
    where
        T: Clone + PartialEq,
    {
        let recovery = FieldRecovery {
            interrupted_attempt: externalized.pending_attempt,
            composition_interrupted: externalized.was_composing,
        };
        let dirty = externalized.dirty && externalized.local != externalized.remote;
        let base_revision = if !dirty
            && externalized.conflict.is_none()
            && externalized.local == externalized.remote
        {
            externalized.remote_revision
        } else {
            externalized.base_revision
        };
        Self {
            reconciler: FieldReconciler {
                local: externalized.local,
                remote: externalized.remote,
                base_revision,
                remote_revision: externalized.remote_revision,
                dirty,
                // Composition ownership belongs to the reclaimed framework control.
                composing: false,
                conflict: externalized.conflict,
                // A pending command is deliberately not reconstructed. It may already have
                // been accepted while this retained entity was being evicted.
                pending: None,
            },
            recovery,
        }
    }
}

impl<T> FieldRestore<T>
where
    T: Clone + PartialEq,
{
    pub fn fresh(revision: GraphRevision, authoritative: T) -> Self {
        Self {
            reconciler: FieldReconciler::new(revision, authoritative),
            recovery: FieldRecovery::default(),
        }
    }
}

#[derive(Clone, Debug)]
struct PendingFieldSubmission<T> {
    intent: FieldSubmissionIntent<T>,
    accepted_revision: Option<GraphRevision>,
    matching_snapshot_revision: Option<GraphRevision>,
    local_changed_after_submission: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AcceptedFieldObservation {
    Awaiting,
    Acknowledged {
        attempt: FieldAttempt,
        revision: GraphRevision,
        local_changed_after_submission: bool,
    },
    AuthoritativeMismatch {
        attempt: FieldAttempt,
        committed_revision: GraphRevision,
    },
    Superseded {
        attempt: FieldAttempt,
        committed_revision: GraphRevision,
        current_revision: GraphRevision,
    },
}

/// Pure revision-aware reconciliation for retained editable fields.
///
/// The state machine knows only graph revisions and field values. UI focus,
/// framework entities, command construction, dispatch, and retry policy remain
/// owned by the caller.
#[derive(Clone, Debug)]
pub struct FieldReconciler<T> {
    local: T,
    remote: T,
    base_revision: GraphRevision,
    remote_revision: GraphRevision,
    dirty: bool,
    composing: bool,
    conflict: Option<FieldConflict>,
    pending: Option<PendingFieldSubmission<T>>,
}

impl<T> FieldReconciler<T>
where
    T: Clone + PartialEq,
{
    pub fn new(revision: GraphRevision, value: T) -> Self {
        Self {
            local: value.clone(),
            remote: value,
            base_revision: revision,
            remote_revision: revision,
            dirty: false,
            composing: false,
            conflict: None,
            pending: None,
        }
    }

    pub fn local(&self) -> &T {
        &self.local
    }

    pub fn remote(&self) -> &T {
        &self.remote
    }

    pub const fn base_revision(&self) -> GraphRevision {
        self.base_revision
    }

    pub const fn remote_revision(&self) -> GraphRevision {
        self.remote_revision
    }

    pub const fn conflict(&self) -> Option<FieldConflict> {
        self.conflict
    }

    pub fn pending_attempt(&self) -> Option<FieldAttempt> {
        self.pending.as_ref().map(|pending| pending.intent.attempt)
    }

    pub fn pending_expected_revision(&self) -> Option<GraphRevision> {
        self.pending
            .as_ref()
            .map(|pending| pending.intent.expected_revision)
    }

    pub fn pending_value(&self) -> Option<&T> {
        self.pending.as_ref().map(|pending| &pending.intent.value)
    }

    pub fn pending_accepted_revision(&self) -> Option<GraphRevision> {
        self.pending
            .as_ref()
            .and_then(|pending| pending.accepted_revision)
    }

    pub fn pending_matching_snapshot_revision(&self) -> Option<GraphRevision> {
        self.pending
            .as_ref()
            .and_then(|pending| pending.matching_snapshot_revision)
    }

    /// Captures framework-neutral state without transferring ownership or creating a retryable
    /// submission. The snapshot is safe to keep while a retained field owner is reclaimed.
    pub fn externalize(&self) -> FieldExternalization<T> {
        FieldExternalization {
            local: self.local.clone(),
            remote: self.remote.clone(),
            base_revision: self.base_revision,
            remote_revision: self.remote_revision,
            dirty: self.dirty,
            conflict: self.conflict,
            pending_attempt: self.pending_attempt(),
            was_composing: self.composing,
        }
    }

    pub fn phase(&self) -> FieldPhase {
        match (self.composing, self.conflict.is_some(), self.dirty) {
            (true, true, _) => FieldPhase::ComposingConflict,
            (false, true, _) => FieldPhase::Conflict,
            (true, false, _) => FieldPhase::Composing,
            (false, false, true) => FieldPhase::Dirty,
            (false, false, false) => FieldPhase::Clean,
        }
    }

    pub fn edit_local(&mut self, value: T) -> FieldEditDecision {
        if self.local == value {
            return FieldEditDecision::Unchanged {
                phase: self.phase(),
            };
        }

        self.local = value;
        if let Some(pending) = &mut self.pending {
            pending.local_changed_after_submission = true;
        }
        self.normalize_local_against_remote();

        FieldEditDecision::Updated {
            phase: self.phase(),
        }
    }

    pub fn mark_composing(&mut self) -> FieldCompositionDecision {
        if self.composing {
            return FieldCompositionDecision::Unchanged {
                phase: self.phase(),
            };
        }
        self.composing = true;
        FieldCompositionDecision::Marked {
            phase: self.phase(),
        }
    }

    pub fn unmark_composing(&mut self) -> FieldCompositionDecision {
        if !self.composing {
            return FieldCompositionDecision::Unchanged {
                phase: self.phase(),
            };
        }
        self.composing = false;
        FieldCompositionDecision::Unmarked {
            phase: self.phase(),
        }
    }

    pub fn reconcile_remote(
        &mut self,
        revision: GraphRevision,
        value: T,
    ) -> FieldReconcileDecision {
        if revision < self.remote_revision {
            return FieldReconcileDecision::IgnoredStale {
                current_revision: self.remote_revision,
                received_revision: revision,
                phase: self.phase(),
            };
        }

        if revision == self.remote_revision {
            if value != self.remote {
                return FieldReconcileDecision::InconsistentSnapshot {
                    revision,
                    phase: self.phase(),
                };
            }
            self.observe_matching_pending_snapshot(revision);
            if let Some(decision) = self.reconcile_accepted_observation() {
                return decision;
            }
            return FieldReconcileDecision::Unchanged {
                revision,
                phase: self.phase(),
            };
        }

        let from_revision = self.remote_revision;
        let remote_value_changed = self.remote != value;
        self.remote_revision = revision;
        self.remote = value;
        self.observe_matching_pending_snapshot(revision);

        if let Some(decision) = self.reconcile_accepted_observation() {
            return decision;
        }

        let had_conflict = self.conflict.is_some();
        if self.pending.is_none()
            && self.local == self.remote
            && self.normalize_local_against_remote()
        {
            if remote_value_changed || had_conflict {
                return FieldReconcileDecision::RemoteApplied {
                    from_revision,
                    to_revision: revision,
                };
            }
            return FieldReconcileDecision::BaseAdvanced {
                from_revision,
                to_revision: revision,
                phase: self.phase(),
            };
        }

        if let Some(conflict) = self.conflict {
            let conflict = FieldConflict::new(conflict.base_revision, revision);
            self.conflict = Some(conflict);
            return FieldReconcileDecision::Conflict { conflict };
        }

        if !remote_value_changed {
            self.base_revision = revision;
            return FieldReconcileDecision::BaseAdvanced {
                from_revision,
                to_revision: revision,
                phase: self.phase(),
            };
        }

        if !self.dirty && !self.composing && self.pending.is_none() {
            self.local = self.remote.clone();
            self.base_revision = revision;
            return FieldReconcileDecision::RemoteApplied {
                from_revision,
                to_revision: revision,
            };
        }

        let conflict = FieldConflict::new(self.base_revision, revision);
        self.conflict = Some(conflict);
        FieldReconcileDecision::Conflict { conflict }
    }

    pub fn plan_submission_at(
        &mut self,
        command_revision: GraphRevision,
    ) -> FieldSubmissionDecision<T> {
        if command_revision != self.remote_revision {
            return FieldSubmissionDecision::Rejected(FieldIntentRejection::UnreconciledRevision {
                observed_revision: self.remote_revision,
                requested_revision: command_revision,
            });
        }
        if self.composing {
            return FieldSubmissionDecision::Rejected(FieldIntentRejection::CompositionActive);
        }
        if self.conflict.is_some() {
            return FieldSubmissionDecision::Rejected(
                FieldIntentRejection::ConflictRequiresResolution,
            );
        }
        if !self.dirty {
            return FieldSubmissionDecision::Rejected(FieldIntentRejection::Clean);
        }
        self.plan_intent(FieldSubmissionKind::Commit, command_revision)
    }

    pub fn resolve_at(
        &mut self,
        resolution: FieldConflictResolution,
        command_revision: GraphRevision,
    ) -> FieldResolutionDecision<T> {
        let Some(conflict) = self.conflict else {
            return FieldResolutionDecision::Rejected(FieldIntentRejection::NoConflict);
        };
        if let Some(attempt) = self.pending_attempt() {
            return FieldResolutionDecision::Rejected(FieldIntentRejection::SubmissionPending {
                attempt,
            });
        }

        match resolution {
            FieldConflictResolution::AcceptRemote => {
                self.local = self.remote.clone();
                self.base_revision = self.remote_revision;
                self.dirty = false;
                self.composing = false;
                self.conflict = None;
                FieldResolutionDecision::AcceptedRemote {
                    revision: self.remote_revision,
                }
            }
            FieldConflictResolution::KeepLocal => FieldResolutionDecision::KeptLocal { conflict },
            FieldConflictResolution::ReapplyLocal if self.composing => {
                FieldResolutionDecision::Rejected(FieldIntentRejection::CompositionActive)
            }
            FieldConflictResolution::ReapplyLocal if command_revision != self.remote_revision => {
                FieldResolutionDecision::Rejected(FieldIntentRejection::UnreconciledRevision {
                    observed_revision: self.remote_revision,
                    requested_revision: command_revision,
                })
            }
            FieldConflictResolution::ReapplyLocal => {
                match self.plan_intent(FieldSubmissionKind::Reapply, command_revision) {
                    FieldSubmissionDecision::Submit(intent) => {
                        FieldResolutionDecision::Reapply(intent)
                    }
                    FieldSubmissionDecision::Rejected(rejection) => {
                        FieldResolutionDecision::Rejected(rejection)
                    }
                }
            }
        }
    }

    pub fn record_self_receipt(
        &mut self,
        attempt: FieldAttempt,
        committed_revision: GraphRevision,
    ) -> FieldReceiptDecision {
        let Some(pending) = &self.pending else {
            return FieldReceiptDecision::UnknownAttempt { attempt };
        };
        if pending.intent.attempt != attempt {
            return FieldReceiptDecision::UnknownAttempt { attempt };
        }
        if committed_revision < pending.intent.expected_revision {
            return FieldReceiptDecision::InvalidCommittedRevision {
                attempt,
                expected_revision: pending.intent.expected_revision,
                committed_revision,
            };
        }
        if let Some(recorded_revision) = pending.accepted_revision {
            return FieldReceiptDecision::AlreadyRecorded {
                attempt,
                committed_revision: recorded_revision,
            };
        }

        if let Some(pending) = &mut self.pending {
            pending.accepted_revision = Some(committed_revision);
        }

        match self.settle_accepted_observation() {
            AcceptedFieldObservation::Acknowledged {
                attempt,
                revision,
                local_changed_after_submission,
            } => FieldReceiptDecision::Acknowledged {
                attempt,
                revision,
                local_changed_after_submission,
            },
            AcceptedFieldObservation::AuthoritativeMismatch {
                attempt,
                committed_revision,
            } => FieldReceiptDecision::AuthoritativeMismatch {
                attempt,
                committed_revision,
            },
            AcceptedFieldObservation::Superseded {
                attempt,
                committed_revision,
                current_revision,
            } => FieldReceiptDecision::Superseded {
                attempt,
                committed_revision,
                current_revision,
            },
            AcceptedFieldObservation::Awaiting => FieldReceiptDecision::Recorded {
                attempt,
                committed_revision,
            },
        }
    }

    pub fn reject_stale(
        &mut self,
        attempt: FieldAttempt,
        current_revision: GraphRevision,
        current_value: T,
    ) -> FieldStaleRejectionDecision {
        let Some(pending) = &self.pending else {
            return FieldStaleRejectionDecision::UnknownAttempt { attempt };
        };
        if pending.intent.attempt != attempt {
            return FieldStaleRejectionDecision::UnknownAttempt { attempt };
        }
        if let Some(committed_revision) = pending.accepted_revision {
            return FieldStaleRejectionDecision::ReceiptAlreadyAccepted {
                attempt,
                committed_revision,
            };
        }
        self.pending = None;

        if current_revision < self.remote_revision {
            self.normalize_local_against_remote();
            return FieldStaleRejectionDecision::IgnoredOlderRevision {
                current_revision: self.remote_revision,
                received_revision: current_revision,
                phase: self.phase(),
            };
        }
        if current_revision == self.remote_revision && current_value != self.remote {
            self.normalize_local_against_remote();
            return FieldStaleRejectionDecision::InconsistentSnapshot {
                revision: current_revision,
                phase: self.phase(),
            };
        }

        let remote_value_changed = self.remote != current_value;
        self.remote_revision = current_revision;
        self.remote = current_value;

        if self.normalize_local_against_remote() {
            return FieldStaleRejectionDecision::LocalMatchesRemote {
                revision: current_revision,
                phase: self.phase(),
            };
        }

        if let Some(conflict) = self.conflict {
            let conflict = FieldConflict::new(conflict.base_revision, current_revision);
            self.conflict = Some(conflict);
            return FieldStaleRejectionDecision::ConflictPreserved { conflict };
        }

        if remote_value_changed {
            let conflict = FieldConflict::new(self.base_revision, current_revision);
            self.conflict = Some(conflict);
            return FieldStaleRejectionDecision::ConflictPreserved { conflict };
        }

        self.base_revision = current_revision;
        FieldStaleRejectionDecision::DirtyPreserved {
            base_revision: current_revision,
        }
    }

    /// Ends one explicitly rejected attempt without dispatching or planning a retry.
    pub fn reject_submission(&mut self, attempt: FieldAttempt) -> FieldSubmissionRejectionDecision {
        let Some(pending) = &self.pending else {
            return FieldSubmissionRejectionDecision::UnknownAttempt { attempt };
        };
        if pending.intent.attempt != attempt {
            return FieldSubmissionRejectionDecision::UnknownAttempt { attempt };
        }
        if let Some(committed_revision) = pending.accepted_revision {
            return FieldSubmissionRejectionDecision::ReceiptAlreadyAccepted {
                attempt,
                committed_revision,
            };
        }

        self.pending = None;
        self.normalize_local_against_remote();
        FieldSubmissionRejectionDecision::Rejected {
            attempt,
            phase: self.phase(),
        }
    }

    fn plan_intent(
        &mut self,
        kind: FieldSubmissionKind,
        expected_revision: GraphRevision,
    ) -> FieldSubmissionDecision<T> {
        if let Some(attempt) = self.pending_attempt() {
            return FieldSubmissionDecision::Rejected(FieldIntentRejection::SubmissionPending {
                attempt,
            });
        }
        let Some(attempt) = allocate_field_attempt() else {
            return FieldSubmissionDecision::Rejected(
                FieldIntentRejection::AttemptIdentityExhausted,
            );
        };
        let intent = FieldSubmissionIntent {
            attempt,
            kind,
            expected_revision,
            value: self.local.clone(),
        };
        self.pending = Some(PendingFieldSubmission {
            intent: intent.clone(),
            accepted_revision: None,
            matching_snapshot_revision: None,
            local_changed_after_submission: false,
        });
        FieldSubmissionDecision::Submit(intent)
    }

    fn observe_matching_pending_snapshot(&mut self, revision: GraphRevision) {
        let remote = &self.remote;
        if let Some(pending) = &mut self.pending
            && pending.intent.value == *remote
            && revision >= pending.intent.expected_revision
        {
            pending.matching_snapshot_revision = Some(revision);
        }
    }

    fn reconcile_accepted_observation(&mut self) -> Option<FieldReconcileDecision> {
        match self.settle_accepted_observation() {
            AcceptedFieldObservation::Awaiting => None,
            AcceptedFieldObservation::Acknowledged {
                attempt,
                revision,
                local_changed_after_submission,
            } => Some(FieldReconcileDecision::SelfAcknowledged {
                attempt,
                revision,
                local_changed_after_submission,
            }),
            AcceptedFieldObservation::AuthoritativeMismatch {
                attempt,
                committed_revision,
            } => Some(FieldReconcileDecision::AuthoritativeMismatch {
                attempt,
                committed_revision,
            }),
            AcceptedFieldObservation::Superseded {
                attempt,
                committed_revision,
                current_revision,
            } => Some(FieldReconcileDecision::Superseded {
                attempt,
                committed_revision,
                current_revision,
            }),
        }
    }

    fn settle_accepted_observation(&mut self) -> AcceptedFieldObservation {
        let Some(pending) = &self.pending else {
            return AcceptedFieldObservation::Awaiting;
        };
        let Some(committed_revision) = pending.accepted_revision else {
            return AcceptedFieldObservation::Awaiting;
        };
        if self.remote_revision < committed_revision {
            return AcceptedFieldObservation::Awaiting;
        }

        let attempt = pending.intent.attempt;
        if pending.intent.value == self.remote {
            let revision = self.remote_revision;
            let Some(acknowledged) = self.finish_acknowledgement(revision) else {
                return AcceptedFieldObservation::Awaiting;
            };
            return AcceptedFieldObservation::Acknowledged {
                attempt: acknowledged.attempt,
                revision,
                local_changed_after_submission: acknowledged.local_changed_after_submission,
            };
        }

        self.pending = None;
        if !self.normalize_local_against_remote() {
            self.ensure_conflict(self.remote_revision);
        }
        if self.remote_revision == committed_revision {
            return AcceptedFieldObservation::AuthoritativeMismatch {
                attempt,
                committed_revision,
            };
        }
        AcceptedFieldObservation::Superseded {
            attempt,
            committed_revision,
            current_revision: self.remote_revision,
        }
    }

    fn finish_acknowledgement(
        &mut self,
        revision: GraphRevision,
    ) -> Option<AcknowledgedFieldSubmission> {
        let pending = self.pending.take()?;
        self.base_revision = revision;
        self.conflict = None;
        if !pending.local_changed_after_submission {
            self.local = self.remote.clone();
        }
        self.normalize_local_against_remote();
        Some(AcknowledgedFieldSubmission {
            attempt: pending.intent.attempt,
            local_changed_after_submission: pending.local_changed_after_submission,
        })
    }

    fn normalize_local_against_remote(&mut self) -> bool {
        if self.pending.is_none() && self.local == self.remote {
            self.base_revision = self.remote_revision;
            self.dirty = false;
            self.conflict = None;
            true
        } else {
            self.dirty = true;
            false
        }
    }

    fn ensure_conflict(&mut self, remote_revision: GraphRevision) {
        let base_revision = self
            .conflict
            .map_or(self.base_revision, |conflict| conflict.base_revision);
        self.conflict = Some(FieldConflict::new(base_revision, remote_revision));
    }
}

struct AcknowledgedFieldSubmission {
    attempt: FieldAttempt,
    local_changed_after_submission: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planned_field() -> (FieldReconciler<String>, FieldAttempt) {
        let mut field = FieldReconciler::new(1, "remote".to_owned());
        field.edit_local("submitted".to_owned());
        let FieldSubmissionDecision::Submit(intent) = field.plan_submission_at(1) else {
            panic!("dirty field should produce a submission intent");
        };
        (field, intent.attempt())
    }

    #[test]
    fn attempts_are_runtime_wide_and_foreign_receipts_cannot_settle_another_field() {
        let (_first, first_attempt) = planned_field();
        let (mut second, second_attempt) = planned_field();

        assert_ne!(first_attempt, second_attempt);
        assert_eq!(
            second.record_self_receipt(first_attempt, 2),
            FieldReceiptDecision::UnknownAttempt {
                attempt: first_attempt,
            }
        );
        assert_eq!(second.pending_attempt(), Some(second_attempt));
    }

    #[test]
    fn composition_retains_local_value_until_conflict_is_resolved_explicitly() {
        let mut field = FieldReconciler::new(1, "remote".to_owned());
        field.mark_composing();
        field.edit_local("partial composition".to_owned());

        assert_eq!(
            field.reconcile_remote(2, "new remote".to_owned()),
            FieldReconcileDecision::Conflict {
                conflict: FieldConflict::new(1, 2),
            }
        );
        assert_eq!(field.phase(), FieldPhase::ComposingConflict);
        assert_eq!(field.local(), "partial composition");
        assert_eq!(field.remote(), "new remote");

        field.unmark_composing();
        let FieldResolutionDecision::Reapply(intent) =
            field.resolve_at(FieldConflictResolution::ReapplyLocal, 2)
        else {
            panic!("explicit reapply should produce an inert submission intent");
        };
        assert_eq!(intent.kind(), FieldSubmissionKind::Reapply);
        assert_eq!(intent.expected_revision(), 2);
        assert_eq!(intent.value(), "partial composition");
    }

    #[test]
    fn receipt_first_and_snapshot_first_converge_to_clean_state() {
        let (mut receipt_first, receipt_first_attempt) = planned_field();
        assert_eq!(
            receipt_first.record_self_receipt(receipt_first_attempt, 2),
            FieldReceiptDecision::Recorded {
                attempt: receipt_first_attempt,
                committed_revision: 2,
            }
        );
        assert_eq!(
            receipt_first.reconcile_remote(2, "submitted".to_owned()),
            FieldReconcileDecision::SelfAcknowledged {
                attempt: receipt_first_attempt,
                revision: 2,
                local_changed_after_submission: false,
            }
        );

        let (mut snapshot_first, snapshot_first_attempt) = planned_field();
        assert!(matches!(
            snapshot_first.reconcile_remote(2, "submitted".to_owned()),
            FieldReconcileDecision::Conflict { .. }
        ));
        assert_eq!(snapshot_first.pending_matching_snapshot_revision(), Some(2));
        assert_eq!(
            snapshot_first.record_self_receipt(snapshot_first_attempt, 2),
            FieldReceiptDecision::Acknowledged {
                attempt: snapshot_first_attempt,
                revision: 2,
                local_changed_after_submission: false,
            }
        );

        assert_eq!(receipt_first.phase(), FieldPhase::Clean);
        assert_eq!(snapshot_first.phase(), FieldPhase::Clean);
        assert_eq!(receipt_first.local(), snapshot_first.local());
        assert_eq!(receipt_first.remote(), snapshot_first.remote());
        assert_eq!(receipt_first.pending_attempt(), None);
        assert_eq!(snapshot_first.pending_attempt(), None);
    }

    #[test]
    fn acknowledgement_preserves_a_newer_local_edit() {
        let (mut field, attempt) = planned_field();
        field.edit_local("newer local".to_owned());
        assert!(matches!(
            field.record_self_receipt(attempt, 2),
            FieldReceiptDecision::Recorded { .. }
        ));
        assert_eq!(
            field.reconcile_remote(2, "submitted".to_owned()),
            FieldReconcileDecision::SelfAcknowledged {
                attempt,
                revision: 2,
                local_changed_after_submission: true,
            }
        );
        assert_eq!(field.local(), "newer local");
        assert_eq!(field.remote(), "submitted");
        assert_eq!(field.phase(), FieldPhase::Dirty);
    }

    #[test]
    fn receipt_first_same_revision_mismatch_retires_the_attempt() {
        let (mut field, attempt) = planned_field();
        assert_eq!(
            field.record_self_receipt(attempt, 2),
            FieldReceiptDecision::Recorded {
                attempt,
                committed_revision: 2,
            }
        );

        assert_eq!(
            field.reconcile_remote(2, "other".to_owned()),
            FieldReconcileDecision::AuthoritativeMismatch {
                attempt,
                committed_revision: 2,
            }
        );

        assert_eq!(field.pending_attempt(), None);
        assert_eq!(field.local(), "submitted");
        assert_eq!(field.remote(), "other");
        assert_eq!(field.phase(), FieldPhase::Conflict);
        assert_eq!(field.conflict(), Some(FieldConflict::new(1, 2)));
    }

    #[test]
    fn receipt_first_newer_mismatch_retires_the_superseded_attempt() {
        let (mut field, attempt) = planned_field();
        assert_eq!(
            field.record_self_receipt(attempt, 2),
            FieldReceiptDecision::Recorded {
                attempt,
                committed_revision: 2,
            }
        );

        assert_eq!(
            field.reconcile_remote(3, "other".to_owned()),
            FieldReconcileDecision::Superseded {
                attempt,
                committed_revision: 2,
                current_revision: 3,
            }
        );

        assert_eq!(field.pending_attempt(), None);
        assert_eq!(field.local(), "submitted");
        assert_eq!(field.remote(), "other");
        assert_eq!(field.phase(), FieldPhase::Conflict);
        assert_eq!(field.conflict(), Some(FieldConflict::new(1, 3)));
    }

    #[test]
    fn receipt_first_mismatch_cannot_acknowledge_an_unrelated_later_equal_value() {
        let (mut field, attempt) = planned_field();
        assert!(matches!(
            field.record_self_receipt(attempt, 2),
            FieldReceiptDecision::Recorded { .. }
        ));
        assert!(matches!(
            field.reconcile_remote(2, "other".to_owned()),
            FieldReconcileDecision::AuthoritativeMismatch { .. }
        ));

        assert_eq!(
            field.reconcile_remote(3, "submitted".to_owned()),
            FieldReconcileDecision::RemoteApplied {
                from_revision: 2,
                to_revision: 3,
            }
        );
        assert_eq!(field.pending_attempt(), None);
        assert_eq!(field.phase(), FieldPhase::Clean);
    }

    #[test]
    fn superseded_receipt_cannot_acknowledge_a_newer_authoritative_value() {
        let (mut field, attempt) = planned_field();
        assert!(matches!(
            field.reconcile_remote(3, "other".to_owned()),
            FieldReconcileDecision::Conflict { .. }
        ));
        assert_eq!(
            field.record_self_receipt(attempt, 2),
            FieldReceiptDecision::Superseded {
                attempt,
                committed_revision: 2,
                current_revision: 3,
            }
        );
        assert_eq!(field.pending_attempt(), None);
        assert_eq!(field.local(), "submitted");
        assert_eq!(field.remote(), "other");
        assert_eq!(field.phase(), FieldPhase::Conflict);
    }

    #[test]
    fn same_revision_mismatch_retires_attempt_before_a_later_matching_value() {
        let (mut field, attempt) = planned_field();
        assert!(matches!(
            field.reconcile_remote(2, "other".to_owned()),
            FieldReconcileDecision::Conflict { .. }
        ));
        assert_eq!(
            field.record_self_receipt(attempt, 2),
            FieldReceiptDecision::AuthoritativeMismatch {
                attempt,
                committed_revision: 2,
            }
        );
        assert_eq!(field.pending_attempt(), None);
        assert_eq!(field.phase(), FieldPhase::Conflict);

        assert_eq!(
            field.reconcile_remote(3, "submitted".to_owned()),
            FieldReconcileDecision::RemoteApplied {
                from_revision: 2,
                to_revision: 3,
            }
        );
        assert_eq!(field.pending_attempt(), None);
        assert_eq!(field.phase(), FieldPhase::Clean);
    }

    #[test]
    fn externalization_does_not_restore_command_or_composition_ownership() {
        let (mut field, attempt) = planned_field();
        field.mark_composing();

        let restored = field
            .externalize()
            .restore_against(2, "new remote".to_owned());
        assert_eq!(restored.recovery().interrupted_attempt(), Some(attempt));
        assert!(restored.recovery().composition_interrupted());

        let field = restored.into_reconciler();
        assert_eq!(field.pending_attempt(), None);
        assert_eq!(field.phase(), FieldPhase::Conflict);
        assert_eq!(field.local(), "submitted");
        assert_eq!(field.remote(), "new remote");
    }

    #[test]
    fn stale_and_inconsistent_observations_do_not_mutate_authoritative_state() {
        let mut field = FieldReconciler::new(4, "remote".to_owned());
        field.edit_local("local".to_owned());

        assert!(matches!(
            field.reconcile_remote(3, "older".to_owned()),
            FieldReconcileDecision::IgnoredStale { .. }
        ));
        assert!(matches!(
            field.reconcile_remote(4, "inconsistent".to_owned()),
            FieldReconcileDecision::InconsistentSnapshot { .. }
        ));
        assert_eq!(field.remote_revision(), 4);
        assert_eq!(field.remote(), "remote");
        assert_eq!(field.local(), "local");
    }
}
