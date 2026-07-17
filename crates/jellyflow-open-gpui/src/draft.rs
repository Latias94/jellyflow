use jellyflow::runtime::GraphRevision;

/// The local editing phase of a revision-aware retained value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPhase {
    Clean,
    Dirty,
    Composing,
    Conflict,
    ComposingConflict,
}

/// Revision metadata for a local value that diverged from authoritative data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftConflict {
    base_revision: GraphRevision,
    remote_revision: GraphRevision,
}

impl DraftConflict {
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

/// Stable identity for one caller-dispatched draft submission.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftAttempt(u64);

impl DraftAttempt {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Why a draft value is being submitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftSubmissionKind {
    Commit,
    Reapply,
}

/// An inert request for the caller to map into an application-owned command.
///
/// Planning this value never dispatches, retries, or mutates the graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftSubmissionIntent<T> {
    attempt: DraftAttempt,
    kind: DraftSubmissionKind,
    expected_revision: GraphRevision,
    value: T,
}

impl<T> DraftSubmissionIntent<T> {
    pub const fn attempt(&self) -> DraftAttempt {
        self.attempt
    }

    pub const fn kind(&self) -> DraftSubmissionKind {
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
pub enum DraftIntentRejection {
    Clean,
    CompositionActive,
    ConflictRequiresResolution,
    NoConflict,
    UnreconciledRevision {
        observed_revision: GraphRevision,
        requested_revision: GraphRevision,
    },
    SubmissionPending {
        attempt: DraftAttempt,
    },
    AttemptIdentityExhausted,
}

/// Result of explicitly planning an ordinary dirty-value submission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftSubmissionDecision<T> {
    Submit(DraftSubmissionIntent<T>),
    Rejected(DraftIntentRejection),
}

/// Explicit choices offered when local and authoritative values conflict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftConflictResolution {
    AcceptRemote,
    KeepLocal,
    ReapplyLocal,
}

/// Result of applying an explicit conflict choice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftResolutionDecision<T> {
    AcceptedRemote { revision: GraphRevision },
    KeptLocal { conflict: DraftConflict },
    Reapply(DraftSubmissionIntent<T>),
    Rejected(DraftIntentRejection),
}

/// Effect of reconciling one authoritative field snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftReconcileDecision {
    Unchanged {
        revision: GraphRevision,
        phase: DraftPhase,
    },
    IgnoredStale {
        current_revision: GraphRevision,
        received_revision: GraphRevision,
        phase: DraftPhase,
    },
    InconsistentSnapshot {
        revision: GraphRevision,
        phase: DraftPhase,
    },
    RemoteApplied {
        from_revision: GraphRevision,
        to_revision: GraphRevision,
    },
    BaseAdvanced {
        from_revision: GraphRevision,
        to_revision: GraphRevision,
        phase: DraftPhase,
    },
    Conflict {
        conflict: DraftConflict,
    },
    SelfAcknowledged {
        attempt: DraftAttempt,
        revision: GraphRevision,
        local_changed_after_submission: bool,
    },
}

/// Effect of recording a receipt known by the caller to belong to this draft.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftReceiptDecision {
    Recorded {
        attempt: DraftAttempt,
        committed_revision: GraphRevision,
    },
    Acknowledged {
        attempt: DraftAttempt,
        revision: GraphRevision,
        local_changed_after_submission: bool,
    },
    AlreadyRecorded {
        attempt: DraftAttempt,
        committed_revision: GraphRevision,
    },
    UnknownAttempt {
        attempt: DraftAttempt,
    },
    InvalidCommittedRevision {
        attempt: DraftAttempt,
        expected_revision: GraphRevision,
        committed_revision: GraphRevision,
    },
    Superseded {
        attempt: DraftAttempt,
        committed_revision: GraphRevision,
        current_revision: GraphRevision,
    },
}

/// Effect of a stale expected-revision rejection reported by the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftStaleRejectionDecision {
    ConflictPreserved {
        conflict: DraftConflict,
    },
    DirtyPreserved {
        base_revision: GraphRevision,
    },
    LocalMatchesRemote {
        revision: GraphRevision,
        phase: DraftPhase,
    },
    IgnoredOlderRevision {
        current_revision: GraphRevision,
        received_revision: GraphRevision,
        phase: DraftPhase,
    },
    InconsistentSnapshot {
        revision: GraphRevision,
        phase: DraftPhase,
    },
    UnknownAttempt {
        attempt: DraftAttempt,
    },
    ReceiptAlreadyAccepted {
        attempt: DraftAttempt,
        committed_revision: GraphRevision,
    },
}

/// Effect of explicitly rejecting a caller-dispatched submission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftSubmissionRejectionDecision {
    Rejected {
        attempt: DraftAttempt,
        phase: DraftPhase,
    },
    UnknownAttempt {
        attempt: DraftAttempt,
    },
    ReceiptAlreadyAccepted {
        attempt: DraftAttempt,
        committed_revision: GraphRevision,
    },
}

/// Effect of changing local input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftEditDecision {
    Unchanged { phase: DraftPhase },
    Updated { phase: DraftPhase },
}

/// Effect of changing the framework-owned composition marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftCompositionDecision {
    Marked { phase: DraftPhase },
    Unmarked { phase: DraftPhase },
    Unchanged { phase: DraftPhase },
}

/// Diagnostic metadata retained when an evicted native editor is restored.
///
/// Neither field recreates framework or command ownership. Applications may surface recovery
/// state, but a new submission or IME session must always be started explicitly.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DraftRecovery {
    interrupted_attempt: Option<DraftAttempt>,
    composition_interrupted: bool,
}

impl DraftRecovery {
    pub const fn interrupted_attempt(self) -> Option<DraftAttempt> {
        self.interrupted_attempt
    }

    pub const fn composition_interrupted(self) -> bool {
        self.composition_interrupted
    }

    pub const fn is_empty(self) -> bool {
        self.interrupted_attempt.is_none() && !self.composition_interrupted
    }
}

/// Adapter-local snapshot held by `ExternalizedDraftStore` while a retained owner is reclaimed.
///
/// A pending attempt is diagnostic state only. Restoring or dispatching it is
/// deliberately outside this type so retirement cannot retry a graph command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExternalizedDraft<T> {
    local: T,
    remote: T,
    base_revision: GraphRevision,
    remote_revision: GraphRevision,
    dirty: bool,
    conflict: Option<DraftConflict>,
    pending_attempt: Option<DraftAttempt>,
    was_composing: bool,
    next_attempt: Option<u64>,
}

impl<T> ExternalizedDraft<T> {
    /// Restores adapter-local state and immediately reconciles it with current graph facts.
    pub(crate) fn restore_against(
        self,
        revision: GraphRevision,
        authoritative: T,
    ) -> DraftRestore<T>
    where
        T: Clone + PartialEq,
    {
        let mut restored = DraftRestore::from_externalized(self);
        restored.draft.reconcile_remote(revision, authoritative);
        if restored.draft.phase() == DraftPhase::Clean {
            restored.recovery.interrupted_attempt = None;
        }
        restored
    }
}

/// Result of restoring an externalized draft.
///
/// The restored state has no pending submission. The optional attempt is retained separately so
/// an application can surface an honest recovery diagnostic without accidentally retrying it.
#[derive(Clone, Debug)]
pub struct DraftRestore<T> {
    draft: RevisionedDraft<T>,
    recovery: DraftRecovery,
}

impl<T> DraftRestore<T> {
    pub fn draft(&self) -> &RevisionedDraft<T> {
        &self.draft
    }

    pub const fn recovery(&self) -> DraftRecovery {
        self.recovery
    }

    pub fn into_draft(self) -> RevisionedDraft<T> {
        self.draft
    }

    fn from_externalized(externalized: ExternalizedDraft<T>) -> Self
    where
        T: Clone + PartialEq,
    {
        let recovery = DraftRecovery {
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
            draft: RevisionedDraft {
                local: externalized.local,
                remote: externalized.remote,
                base_revision,
                remote_revision: externalized.remote_revision,
                dirty,
                // GPUI's marked range and IME ownership were tied to the reclaimed entity.
                composing: false,
                conflict: externalized.conflict,
                // A pending command is deliberately not reconstructed. It may already have
                // been accepted while this retained entity was being evicted.
                pending: None,
                next_attempt: externalized.next_attempt,
            },
            recovery,
        }
    }
}

impl<T> DraftRestore<T>
where
    T: Clone + PartialEq,
{
    pub(crate) fn fresh(revision: GraphRevision, authoritative: T) -> Self {
        Self {
            draft: RevisionedDraft::new(revision, authoritative),
            recovery: DraftRecovery::default(),
        }
    }
}

#[derive(Clone, Debug)]
struct PendingSubmission<T> {
    intent: DraftSubmissionIntent<T>,
    accepted_revision: Option<GraphRevision>,
    matching_snapshot_revision: Option<GraphRevision>,
    local_changed_after_submission: bool,
}

/// Pure revision-aware draft reconciliation for retained native controls.
///
/// The state machine knows only graph revisions and field values. UI focus,
/// framework entities, command construction, dispatch, and retry policy remain
/// owned by the caller.
#[derive(Clone, Debug)]
pub struct RevisionedDraft<T> {
    local: T,
    remote: T,
    base_revision: GraphRevision,
    remote_revision: GraphRevision,
    dirty: bool,
    composing: bool,
    conflict: Option<DraftConflict>,
    pending: Option<PendingSubmission<T>>,
    next_attempt: Option<u64>,
}

impl<T> RevisionedDraft<T>
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
            next_attempt: Some(1),
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

    pub const fn conflict(&self) -> Option<DraftConflict> {
        self.conflict
    }

    pub fn pending_attempt(&self) -> Option<DraftAttempt> {
        self.pending.as_ref().map(|pending| pending.intent.attempt)
    }

    /// Captures adapter-local state without transferring ownership or creating a retryable
    /// submission. The snapshot is safe to keep while the retained native entity is reclaimed.
    pub(crate) fn externalized_snapshot(&self) -> ExternalizedDraft<T> {
        ExternalizedDraft {
            local: self.local.clone(),
            remote: self.remote.clone(),
            base_revision: self.base_revision,
            remote_revision: self.remote_revision,
            dirty: self.dirty,
            conflict: self.conflict,
            pending_attempt: self.pending_attempt(),
            was_composing: self.composing,
            next_attempt: self.next_attempt,
        }
    }

    pub fn phase(&self) -> DraftPhase {
        match (self.composing, self.conflict.is_some(), self.dirty) {
            (true, true, _) => DraftPhase::ComposingConflict,
            (false, true, _) => DraftPhase::Conflict,
            (true, false, _) => DraftPhase::Composing,
            (false, false, true) => DraftPhase::Dirty,
            (false, false, false) => DraftPhase::Clean,
        }
    }

    pub fn edit_local(&mut self, value: T) -> DraftEditDecision {
        if self.local == value {
            return DraftEditDecision::Unchanged {
                phase: self.phase(),
            };
        }

        self.local = value;
        if let Some(pending) = &mut self.pending {
            pending.local_changed_after_submission = true;
        }
        self.normalize_local_against_remote();

        DraftEditDecision::Updated {
            phase: self.phase(),
        }
    }

    pub fn mark_composing(&mut self) -> DraftCompositionDecision {
        if self.composing {
            return DraftCompositionDecision::Unchanged {
                phase: self.phase(),
            };
        }
        self.composing = true;
        DraftCompositionDecision::Marked {
            phase: self.phase(),
        }
    }

    pub fn unmark_composing(&mut self) -> DraftCompositionDecision {
        if !self.composing {
            return DraftCompositionDecision::Unchanged {
                phase: self.phase(),
            };
        }
        self.composing = false;
        DraftCompositionDecision::Unmarked {
            phase: self.phase(),
        }
    }

    pub fn reconcile_remote(
        &mut self,
        revision: GraphRevision,
        value: T,
    ) -> DraftReconcileDecision {
        if revision < self.remote_revision {
            return DraftReconcileDecision::IgnoredStale {
                current_revision: self.remote_revision,
                received_revision: revision,
                phase: self.phase(),
            };
        }

        if revision == self.remote_revision {
            if value != self.remote {
                return DraftReconcileDecision::InconsistentSnapshot {
                    revision,
                    phase: self.phase(),
                };
            }
            self.observe_matching_pending_snapshot(revision);
            if self.pending_is_acknowledged_by(revision) {
                return self.finish_snapshot_acknowledgement(revision);
            }
            return DraftReconcileDecision::Unchanged {
                revision,
                phase: self.phase(),
            };
        }

        let from_revision = self.remote_revision;
        let remote_value_changed = self.remote != value;
        self.remote_revision = revision;
        self.remote = value;
        self.observe_matching_pending_snapshot(revision);

        if self.pending_is_acknowledged_by(revision) {
            return self.finish_snapshot_acknowledgement(revision);
        }

        let accepted_attempt_was_superseded = self
            .pending
            .as_ref()
            .and_then(|pending| pending.accepted_revision)
            .is_some_and(|accepted_revision| accepted_revision < revision);
        if accepted_attempt_was_superseded {
            self.pending = None;
        }

        let had_conflict = self.conflict.is_some();
        if self.pending.is_none()
            && self.local == self.remote
            && self.normalize_local_against_remote()
        {
            if remote_value_changed || accepted_attempt_was_superseded || had_conflict {
                return DraftReconcileDecision::RemoteApplied {
                    from_revision,
                    to_revision: revision,
                };
            }
            return DraftReconcileDecision::BaseAdvanced {
                from_revision,
                to_revision: revision,
                phase: self.phase(),
            };
        }

        if let Some(conflict) = self.conflict {
            let conflict = DraftConflict::new(conflict.base_revision, revision);
            self.conflict = Some(conflict);
            return DraftReconcileDecision::Conflict { conflict };
        }

        if !remote_value_changed && !accepted_attempt_was_superseded {
            self.base_revision = revision;
            return DraftReconcileDecision::BaseAdvanced {
                from_revision,
                to_revision: revision,
                phase: self.phase(),
            };
        }

        if !self.dirty && !self.composing && self.pending.is_none() {
            self.local = self.remote.clone();
            self.base_revision = revision;
            return DraftReconcileDecision::RemoteApplied {
                from_revision,
                to_revision: revision,
            };
        }

        let conflict = DraftConflict::new(self.base_revision, revision);
        self.conflict = Some(conflict);
        DraftReconcileDecision::Conflict { conflict }
    }

    pub fn plan_submission_at(
        &mut self,
        command_revision: GraphRevision,
    ) -> DraftSubmissionDecision<T> {
        if command_revision != self.remote_revision {
            return DraftSubmissionDecision::Rejected(DraftIntentRejection::UnreconciledRevision {
                observed_revision: self.remote_revision,
                requested_revision: command_revision,
            });
        }
        if self.composing {
            return DraftSubmissionDecision::Rejected(DraftIntentRejection::CompositionActive);
        }
        if self.conflict.is_some() {
            return DraftSubmissionDecision::Rejected(
                DraftIntentRejection::ConflictRequiresResolution,
            );
        }
        if !self.dirty {
            return DraftSubmissionDecision::Rejected(DraftIntentRejection::Clean);
        }
        self.plan_intent(DraftSubmissionKind::Commit, command_revision)
    }

    pub fn resolve_at(
        &mut self,
        resolution: DraftConflictResolution,
        command_revision: GraphRevision,
    ) -> DraftResolutionDecision<T> {
        let Some(conflict) = self.conflict else {
            return DraftResolutionDecision::Rejected(DraftIntentRejection::NoConflict);
        };
        if let Some(attempt) = self.pending_attempt() {
            return DraftResolutionDecision::Rejected(DraftIntentRejection::SubmissionPending {
                attempt,
            });
        }

        match resolution {
            DraftConflictResolution::AcceptRemote => {
                self.local = self.remote.clone();
                self.base_revision = self.remote_revision;
                self.dirty = false;
                self.composing = false;
                self.conflict = None;
                DraftResolutionDecision::AcceptedRemote {
                    revision: self.remote_revision,
                }
            }
            DraftConflictResolution::KeepLocal => DraftResolutionDecision::KeptLocal { conflict },
            DraftConflictResolution::ReapplyLocal if self.composing => {
                DraftResolutionDecision::Rejected(DraftIntentRejection::CompositionActive)
            }
            DraftConflictResolution::ReapplyLocal if command_revision != self.remote_revision => {
                DraftResolutionDecision::Rejected(DraftIntentRejection::UnreconciledRevision {
                    observed_revision: self.remote_revision,
                    requested_revision: command_revision,
                })
            }
            DraftConflictResolution::ReapplyLocal => {
                match self.plan_intent(DraftSubmissionKind::Reapply, command_revision) {
                    DraftSubmissionDecision::Submit(intent) => {
                        DraftResolutionDecision::Reapply(intent)
                    }
                    DraftSubmissionDecision::Rejected(rejection) => {
                        DraftResolutionDecision::Rejected(rejection)
                    }
                }
            }
        }
    }

    pub fn record_self_receipt(
        &mut self,
        attempt: DraftAttempt,
        committed_revision: GraphRevision,
    ) -> DraftReceiptDecision {
        let Some(pending) = &self.pending else {
            return DraftReceiptDecision::UnknownAttempt { attempt };
        };
        if pending.intent.attempt != attempt {
            return DraftReceiptDecision::UnknownAttempt { attempt };
        }
        if committed_revision < pending.intent.expected_revision {
            return DraftReceiptDecision::InvalidCommittedRevision {
                attempt,
                expected_revision: pending.intent.expected_revision,
                committed_revision,
            };
        }
        if let Some(recorded_revision) = pending.accepted_revision {
            return DraftReceiptDecision::AlreadyRecorded {
                attempt,
                committed_revision: recorded_revision,
            };
        }

        let matching_observation_revision =
            pending
                .matching_snapshot_revision
                .filter(|matching_revision| {
                    *matching_revision >= committed_revision
                        && *matching_revision == self.remote_revision
                        && self.remote == pending.intent.value
                });

        if self.remote_revision > committed_revision && matching_observation_revision.is_none() {
            self.pending = None;
            if !self.normalize_local_against_remote() {
                self.ensure_conflict(self.remote_revision);
            }
            return DraftReceiptDecision::Superseded {
                attempt,
                committed_revision,
                current_revision: self.remote_revision,
            };
        }

        self.base_revision = committed_revision;
        if let Some(pending) = &mut self.pending {
            pending.accepted_revision = Some(committed_revision);
        }

        if let Some(observation_revision) = matching_observation_revision {
            if let Some(acknowledged) = self.finish_acknowledgement(observation_revision) {
                return DraftReceiptDecision::Acknowledged {
                    attempt: acknowledged.attempt,
                    revision: observation_revision,
                    local_changed_after_submission: acknowledged.local_changed_after_submission,
                };
            }
            return DraftReceiptDecision::UnknownAttempt { attempt };
        }

        DraftReceiptDecision::Recorded {
            attempt,
            committed_revision,
        }
    }

    pub fn reject_stale(
        &mut self,
        attempt: DraftAttempt,
        current_revision: GraphRevision,
        current_value: T,
    ) -> DraftStaleRejectionDecision {
        let Some(pending) = &self.pending else {
            return DraftStaleRejectionDecision::UnknownAttempt { attempt };
        };
        if pending.intent.attempt != attempt {
            return DraftStaleRejectionDecision::UnknownAttempt { attempt };
        }
        if let Some(committed_revision) = pending.accepted_revision {
            return DraftStaleRejectionDecision::ReceiptAlreadyAccepted {
                attempt,
                committed_revision,
            };
        }
        self.pending = None;

        if current_revision < self.remote_revision {
            self.normalize_local_against_remote();
            return DraftStaleRejectionDecision::IgnoredOlderRevision {
                current_revision: self.remote_revision,
                received_revision: current_revision,
                phase: self.phase(),
            };
        }
        if current_revision == self.remote_revision && current_value != self.remote {
            self.normalize_local_against_remote();
            return DraftStaleRejectionDecision::InconsistentSnapshot {
                revision: current_revision,
                phase: self.phase(),
            };
        }

        let remote_value_changed = self.remote != current_value;
        self.remote_revision = current_revision;
        self.remote = current_value;

        if self.normalize_local_against_remote() {
            return DraftStaleRejectionDecision::LocalMatchesRemote {
                revision: current_revision,
                phase: self.phase(),
            };
        }

        if let Some(conflict) = self.conflict {
            let conflict = DraftConflict::new(conflict.base_revision, current_revision);
            self.conflict = Some(conflict);
            return DraftStaleRejectionDecision::ConflictPreserved { conflict };
        }

        if remote_value_changed {
            let conflict = DraftConflict::new(self.base_revision, current_revision);
            self.conflict = Some(conflict);
            return DraftStaleRejectionDecision::ConflictPreserved { conflict };
        }

        self.base_revision = current_revision;
        DraftStaleRejectionDecision::DirtyPreserved {
            base_revision: current_revision,
        }
    }

    /// Ends one explicitly rejected attempt without dispatching or planning a retry.
    pub fn reject_submission(&mut self, attempt: DraftAttempt) -> DraftSubmissionRejectionDecision {
        let Some(pending) = &self.pending else {
            return DraftSubmissionRejectionDecision::UnknownAttempt { attempt };
        };
        if pending.intent.attempt != attempt {
            return DraftSubmissionRejectionDecision::UnknownAttempt { attempt };
        }
        if let Some(committed_revision) = pending.accepted_revision {
            return DraftSubmissionRejectionDecision::ReceiptAlreadyAccepted {
                attempt,
                committed_revision,
            };
        }

        self.pending = None;
        self.normalize_local_against_remote();
        DraftSubmissionRejectionDecision::Rejected {
            attempt,
            phase: self.phase(),
        }
    }

    fn plan_intent(
        &mut self,
        kind: DraftSubmissionKind,
        expected_revision: GraphRevision,
    ) -> DraftSubmissionDecision<T> {
        if let Some(attempt) = self.pending_attempt() {
            return DraftSubmissionDecision::Rejected(DraftIntentRejection::SubmissionPending {
                attempt,
            });
        }
        let Some(attempt_id) = self.next_attempt else {
            return DraftSubmissionDecision::Rejected(
                DraftIntentRejection::AttemptIdentityExhausted,
            );
        };
        let attempt = DraftAttempt(attempt_id);
        self.next_attempt = attempt_id.checked_add(1);
        let intent = DraftSubmissionIntent {
            attempt,
            kind,
            expected_revision,
            value: self.local.clone(),
        };
        self.pending = Some(PendingSubmission {
            intent: intent.clone(),
            accepted_revision: None,
            matching_snapshot_revision: None,
            local_changed_after_submission: false,
        });
        DraftSubmissionDecision::Submit(intent)
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

    fn pending_is_acknowledged_by(&self, revision: GraphRevision) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            pending
                .accepted_revision
                .is_some_and(|accepted_revision| accepted_revision <= revision)
                && pending.matching_snapshot_revision == Some(revision)
                && pending.intent.value == self.remote
        })
    }

    fn finish_snapshot_acknowledgement(
        &mut self,
        revision: GraphRevision,
    ) -> DraftReconcileDecision {
        match self.finish_acknowledgement(revision) {
            Some(acknowledged) => DraftReconcileDecision::SelfAcknowledged {
                attempt: acknowledged.attempt,
                revision,
                local_changed_after_submission: acknowledged.local_changed_after_submission,
            },
            None => DraftReconcileDecision::Unchanged {
                revision,
                phase: self.phase(),
            },
        }
    }

    fn finish_acknowledgement(
        &mut self,
        revision: GraphRevision,
    ) -> Option<AcknowledgedSubmission> {
        let pending = self.pending.take()?;
        self.base_revision = revision;
        self.conflict = None;
        if !pending.local_changed_after_submission {
            self.local = self.remote.clone();
        }
        self.normalize_local_against_remote();
        Some(AcknowledgedSubmission {
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
        self.conflict = Some(DraftConflict::new(base_revision, remote_revision));
    }
}

struct AcknowledgedSubmission {
    attempt: DraftAttempt,
    local_changed_after_submission: bool,
}
