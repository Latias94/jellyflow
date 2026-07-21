use std::fmt;

use jellyflow::runtime::{GraphCommandOutcome, GraphRevision, NodeInstanceKey, ViewCloseReason};
use open_gpui::Task;

use crate::activity::{
    OpenGpuiActivityAcquireError, OpenGpuiActivityKind, OpenGpuiActivityLease,
    OpenGpuiNodeActivityRegistry,
};
use crate::binding::{OpenGpuiMountCapability, OpenGpuiMountState, OpenGpuiNodeBinding};
use crate::command_journal::OpenGpuiCommandTicketResult;
use crate::draft::{
    DraftAttempt, DraftConflict, DraftConflictResolution, DraftIntentRejection, DraftPhase,
    DraftReceiptDecision, DraftReconcileDecision, DraftRecovery, DraftResolutionDecision,
    DraftRestore, DraftSubmissionDecision, DraftSubmissionIntent, DraftSubmissionKind,
    RevisionedDraft,
};
pub use crate::draft_store::{RetainedFieldStore, RetainedFieldStoreError};

const RETAINED_FIELD_KEY_MAX_BYTES: usize = 128;

/// Stable product-owned identity for one editable value inside a native node.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RetainedFieldKey(String);

impl RetainedFieldKey {
    pub fn new(value: impl Into<String>) -> Result<Self, RetainedFieldError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(RetainedFieldError::EmptyKey);
        }
        if value.len() > RETAINED_FIELD_KEY_MAX_BYTES {
            return Err(RetainedFieldError::KeyTooLong {
                actual_bytes: value.len(),
                maximum_bytes: RETAINED_FIELD_KEY_MAX_BYTES,
            });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable identity for one explicit field submission.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RetainedFieldAttempt(DraftAttempt);

impl RetainedFieldAttempt {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Local editing state, independent from command transport state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetainedFieldPhase {
    Clean,
    Dirty,
    Composing,
    Conflict,
    ComposingConflict,
}

impl From<DraftPhase> for RetainedFieldPhase {
    fn from(value: DraftPhase) -> Self {
        match value {
            DraftPhase::Clean => Self::Clean,
            DraftPhase::Dirty => Self::Dirty,
            DraftPhase::Composing => Self::Composing,
            DraftPhase::Conflict => Self::Conflict,
            DraftPhase::ComposingConflict => Self::ComposingConflict,
        }
    }
}

/// Revision evidence for a local/authoritative conflict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetainedFieldConflict {
    base_revision: GraphRevision,
    remote_revision: GraphRevision,
}

impl RetainedFieldConflict {
    #[must_use]
    pub const fn base_revision(self) -> GraphRevision {
        self.base_revision
    }

    #[must_use]
    pub const fn remote_revision(self) -> GraphRevision {
        self.remote_revision
    }
}

impl From<DraftConflict> for RetainedFieldConflict {
    fn from(value: DraftConflict) -> Self {
        Self {
            base_revision: value.base_revision(),
            remote_revision: value.remote_revision(),
        }
    }
}

/// Why an explicit product submission is being planned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetainedFieldSubmitMode {
    Commit,
    Reapply,
}

/// Product-facing, inert command intent. Planning never dispatches or retries.
#[derive(Clone, Debug, PartialEq)]
pub struct RetainedFieldIntent<T> {
    inner: DraftSubmissionIntent<T>,
}

impl<T> RetainedFieldIntent<T> {
    #[must_use]
    pub const fn attempt(&self) -> RetainedFieldAttempt {
        RetainedFieldAttempt(self.inner.attempt())
    }

    #[must_use]
    pub const fn mode(&self) -> RetainedFieldSubmitMode {
        match self.inner.kind() {
            DraftSubmissionKind::Commit => RetainedFieldSubmitMode::Commit,
            DraftSubmissionKind::Reapply => RetainedFieldSubmitMode::Reapply,
        }
    }

    #[must_use]
    pub const fn expected_revision(&self) -> GraphRevision {
        self.inner.expected_revision()
    }

    #[must_use]
    pub const fn value(&self) -> &T {
        self.inner.value()
    }
}

/// Explicit choices for a conflicting local value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetainedFieldConflictChoice {
    AcceptRemote,
    KeepLocal,
    ReapplyLocal,
}

/// Result of applying one explicit conflict choice.
#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFieldResolution<T> {
    AcceptedRemote { revision: GraphRevision },
    KeptLocal { conflict: RetainedFieldConflict },
    Reapply(RetainedFieldIntent<T>),
}

/// Mechanical command state, orthogonal to [`RetainedFieldPhase`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetainedFieldAttemptState {
    Idle,
    Pending {
        attempt: RetainedFieldAttempt,
    },
    SnapshotFirst {
        attempt: RetainedFieldAttempt,
        observed_revision: GraphRevision,
    },
    ReceiptFirst {
        attempt: RetainedFieldAttempt,
        committed_revision: GraphRevision,
    },
    RefreshRequired {
        attempt: Option<RetainedFieldAttempt>,
        minimum_revision: GraphRevision,
    },
    Uncertain {
        attempt: RetainedFieldAttempt,
        expected_revision: GraphRevision,
    },
}

impl RetainedFieldAttemptState {
    #[must_use]
    pub const fn attempt(self) -> Option<RetainedFieldAttempt> {
        match self {
            Self::Idle => None,
            Self::Pending { attempt }
            | Self::SnapshotFirst { attempt, .. }
            | Self::ReceiptFirst { attempt, .. }
            | Self::Uncertain { attempt, .. } => Some(attempt),
            Self::RefreshRequired { attempt, .. } => attempt,
        }
    }

    #[must_use]
    pub const fn blocks_submission(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

/// Compact effect of observing one authoritative field value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetainedFieldUpdate {
    Unchanged,
    IgnoredStale,
    InconsistentSnapshot,
    RemoteApplied,
    BaseAdvanced,
    Conflict,
    SnapshotFirst {
        attempt: RetainedFieldAttempt,
        revision: GraphRevision,
    },
    Acknowledged {
        attempt: RetainedFieldAttempt,
        revision: GraphRevision,
        local_changed_after_submission: bool,
    },
    UncertainResolved {
        attempt: RetainedFieldAttempt,
        revision: GraphRevision,
        submitted_value_is_authoritative: bool,
    },
}

/// Terminal or intermediate effect of one command ticket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetainedFieldSettlement {
    Acknowledged {
        attempt: RetainedFieldAttempt,
        revision: GraphRevision,
        local_changed_after_submission: bool,
    },
    AwaitingSnapshot {
        attempt: RetainedFieldAttempt,
        committed_revision: GraphRevision,
    },
    RefreshRequired {
        attempt: RetainedFieldAttempt,
        minimum_revision: GraphRevision,
        message: String,
    },
    Uncertain {
        attempt: RetainedFieldAttempt,
        message: String,
    },
    Ignored {
        attempt: RetainedFieldAttempt,
    },
}

impl RetainedFieldSettlement {
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::RefreshRequired { message, .. } | Self::Uncertain { message, .. } => {
                Some(message)
            }
            Self::Acknowledged { .. } | Self::AwaitingSnapshot { .. } | Self::Ignored { .. } => {
                None
            }
        }
    }
}

/// Diagnostic evidence restored after native ownership was reclaimed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetainedFieldRecovery {
    interrupted_attempt: Option<RetainedFieldAttempt>,
    composition_interrupted: bool,
}

impl RetainedFieldRecovery {
    #[must_use]
    pub const fn interrupted_attempt(self) -> Option<RetainedFieldAttempt> {
        self.interrupted_attempt
    }

    #[must_use]
    pub const fn composition_interrupted(self) -> bool {
        self.composition_interrupted
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.interrupted_attempt.is_none() && !self.composition_interrupted
    }
}

impl From<DraftRecovery> for RetainedFieldRecovery {
    fn from(value: DraftRecovery) -> Self {
        Self {
            interrupted_attempt: value.interrupted_attempt().map(RetainedFieldAttempt),
            composition_interrupted: value.composition_interrupted(),
        }
    }
}

/// Pure revision-aware field reducer with explicit command-ordering evidence.
pub struct RevisionedField<T> {
    draft: RevisionedDraft<T>,
    attempt_state: RetainedFieldAttemptState,
    notice: Option<String>,
}

impl<T> fmt::Debug for RevisionedField<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RevisionedField")
            .field("draft", &self.draft)
            .field("attempt_state", &self.attempt_state)
            .field("notice", &self.notice)
            .finish()
    }
}

impl<T> RevisionedField<T>
where
    T: Clone + PartialEq,
{
    #[must_use]
    pub fn new(revision: GraphRevision, authoritative: T) -> Self {
        Self {
            draft: RevisionedDraft::new(revision, authoritative),
            attempt_state: RetainedFieldAttemptState::Idle,
            notice: None,
        }
    }

    fn from_restore(restored: DraftRestore<T>) -> (Self, RetainedFieldRecovery) {
        let recovery = RetainedFieldRecovery::from(restored.recovery());
        let draft = restored.into_draft();
        let attempt_state =
            recovery
                .interrupted_attempt()
                .map_or(RetainedFieldAttemptState::Idle, |attempt| {
                    RetainedFieldAttemptState::RefreshRequired {
                        attempt: Some(attempt),
                        minimum_revision: draft.remote_revision(),
                    }
                });
        (
            Self {
                draft,
                attempt_state,
                notice: (!recovery.is_empty())
                    .then(|| "restored field requires an explicit fresh observation".to_owned()),
            },
            recovery,
        )
    }

    #[must_use]
    pub fn local(&self) -> &T {
        self.draft.local()
    }

    #[must_use]
    pub fn remote(&self) -> &T {
        self.draft.remote()
    }

    #[must_use]
    pub const fn base_revision(&self) -> GraphRevision {
        self.draft.base_revision()
    }

    #[must_use]
    pub const fn remote_revision(&self) -> GraphRevision {
        self.draft.remote_revision()
    }

    #[must_use]
    pub fn phase(&self) -> RetainedFieldPhase {
        if matches!(
            self.attempt_state,
            RetainedFieldAttemptState::SnapshotFirst { .. }
        ) && self.draft.pending_matching_snapshot_revision().is_some()
        {
            return if matches!(self.draft.phase(), DraftPhase::ComposingConflict) {
                RetainedFieldPhase::Composing
            } else {
                RetainedFieldPhase::Dirty
            };
        }
        self.draft.phase().into()
    }

    #[must_use]
    pub fn conflict(&self) -> Option<RetainedFieldConflict> {
        if matches!(
            self.attempt_state,
            RetainedFieldAttemptState::SnapshotFirst { .. }
        ) {
            return None;
        }
        self.draft.conflict().map(Into::into)
    }

    #[must_use]
    pub const fn attempt_state(&self) -> RetainedFieldAttemptState {
        self.attempt_state
    }

    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    #[must_use]
    pub fn can_submit(&self) -> bool {
        !self.attempt_state.blocks_submission()
            && !matches!(
                self.phase(),
                RetainedFieldPhase::Clean
                    | RetainedFieldPhase::Composing
                    | RetainedFieldPhase::ComposingConflict
                    | RetainedFieldPhase::Conflict
            )
    }

    pub fn edit(&mut self, value: T) -> RetainedFieldPhase {
        self.draft.edit_local(value);
        self.phase()
    }

    pub fn set_composing(&mut self, composing: bool) -> RetainedFieldPhase {
        if composing {
            self.draft.mark_composing();
        } else {
            self.draft.unmark_composing();
        }
        self.phase()
    }

    pub fn observe(&mut self, revision: GraphRevision, authoritative: T) -> RetainedFieldUpdate {
        if let RetainedFieldAttemptState::Uncertain {
            attempt,
            expected_revision,
        } = self.attempt_state
            && revision > expected_revision
        {
            let submitted_is_authoritative = self
                .draft
                .pending_value()
                .is_some_and(|submitted| *submitted == authoritative);
            if submitted_is_authoritative {
                self.draft.reject_submission(attempt.0);
                self.draft.reconcile_remote(revision, authoritative);
            } else {
                self.draft.reject_stale(attempt.0, revision, authoritative);
            }
            self.attempt_state = RetainedFieldAttemptState::Idle;
            self.notice = None;
            return RetainedFieldUpdate::UncertainResolved {
                attempt,
                revision,
                submitted_value_is_authoritative: submitted_is_authoritative,
            };
        }

        let decision = self.draft.reconcile_remote(revision, authoritative);
        if let Some(attempt) = self.draft.pending_attempt().map(RetainedFieldAttempt)
            && self.draft.pending_accepted_revision().is_none()
            && let Some(observed_revision) = self.draft.pending_matching_snapshot_revision()
        {
            self.attempt_state = RetainedFieldAttemptState::SnapshotFirst {
                attempt,
                observed_revision,
            };
            return RetainedFieldUpdate::SnapshotFirst {
                attempt,
                revision: observed_revision,
            };
        }

        let update = match decision {
            DraftReconcileDecision::Unchanged { .. } => RetainedFieldUpdate::Unchanged,
            DraftReconcileDecision::IgnoredStale { .. } => RetainedFieldUpdate::IgnoredStale,
            DraftReconcileDecision::InconsistentSnapshot { .. } => {
                RetainedFieldUpdate::InconsistentSnapshot
            }
            DraftReconcileDecision::RemoteApplied { .. } => RetainedFieldUpdate::RemoteApplied,
            DraftReconcileDecision::BaseAdvanced { .. } => RetainedFieldUpdate::BaseAdvanced,
            DraftReconcileDecision::Conflict { .. } => RetainedFieldUpdate::Conflict,
            DraftReconcileDecision::SelfAcknowledged {
                attempt,
                revision,
                local_changed_after_submission,
            } => {
                let attempt = RetainedFieldAttempt(attempt);
                self.attempt_state = RetainedFieldAttemptState::Idle;
                self.notice = None;
                RetainedFieldUpdate::Acknowledged {
                    attempt,
                    revision,
                    local_changed_after_submission,
                }
            }
        };

        if let RetainedFieldAttemptState::RefreshRequired {
            minimum_revision, ..
        } = self.attempt_state
            && revision >= minimum_revision
            && self.draft.pending_attempt().is_none()
        {
            self.attempt_state = RetainedFieldAttemptState::Idle;
        }
        update
    }

    pub fn plan(
        &mut self,
        command_revision: GraphRevision,
    ) -> Result<RetainedFieldIntent<T>, RetainedFieldError> {
        if self.attempt_state.blocks_submission() {
            return Err(RetainedFieldError::AttemptPending {
                attempt: self.attempt_state.attempt(),
            });
        }
        match self.draft.plan_submission_at(command_revision) {
            DraftSubmissionDecision::Submit(inner) => {
                let intent = RetainedFieldIntent { inner };
                self.attempt_state = RetainedFieldAttemptState::Pending {
                    attempt: intent.attempt(),
                };
                self.notice = None;
                Ok(intent)
            }
            DraftSubmissionDecision::Rejected(reason) => Err(map_plan_error(reason)),
        }
    }

    pub fn resolve(
        &mut self,
        choice: RetainedFieldConflictChoice,
        command_revision: GraphRevision,
    ) -> Result<RetainedFieldResolution<T>, RetainedFieldError> {
        if self.attempt_state.blocks_submission() {
            return Err(RetainedFieldError::AttemptPending {
                attempt: self.attempt_state.attempt(),
            });
        }
        let choice = match choice {
            RetainedFieldConflictChoice::AcceptRemote => DraftConflictResolution::AcceptRemote,
            RetainedFieldConflictChoice::KeepLocal => DraftConflictResolution::KeepLocal,
            RetainedFieldConflictChoice::ReapplyLocal => DraftConflictResolution::ReapplyLocal,
        };
        match self.draft.resolve_at(choice, command_revision) {
            DraftResolutionDecision::AcceptedRemote { revision } => {
                self.notice = None;
                Ok(RetainedFieldResolution::AcceptedRemote { revision })
            }
            DraftResolutionDecision::KeptLocal { conflict } => {
                Ok(RetainedFieldResolution::KeptLocal {
                    conflict: conflict.into(),
                })
            }
            DraftResolutionDecision::Reapply(inner) => {
                let intent = RetainedFieldIntent { inner };
                self.attempt_state = RetainedFieldAttemptState::Pending {
                    attempt: intent.attempt(),
                };
                self.notice = None;
                Ok(RetainedFieldResolution::Reapply(intent))
            }
            DraftResolutionDecision::Rejected(reason) => Err(map_plan_error(reason)),
        }
    }

    pub fn not_dispatched(
        &mut self,
        attempt: RetainedFieldAttempt,
    ) -> Result<(), RetainedFieldError> {
        if self.draft.pending_attempt() != Some(attempt.0) {
            return Err(RetainedFieldError::AttemptMismatch { attempt });
        }
        self.draft.reject_submission(attempt.0);
        self.attempt_state = RetainedFieldAttemptState::Idle;
        Ok(())
    }

    pub fn settle_ticket(
        &mut self,
        attempt: RetainedFieldAttempt,
        result: OpenGpuiCommandTicketResult,
    ) -> RetainedFieldSettlement {
        if self.draft.pending_attempt() != Some(attempt.0) {
            return RetainedFieldSettlement::Ignored { attempt };
        }
        match result {
            Ok(GraphCommandOutcome::Applied(receipt)) => {
                self.record_receipt(attempt, receipt.revision())
            }
            Ok(GraphCommandOutcome::Noop(noop)) => self.record_receipt(attempt, noop.revision()),
            Ok(GraphCommandOutcome::Conflict(conflict)) => {
                self.draft.reject_submission(attempt.0);
                let minimum_revision = conflict.current().revision();
                let message = format!(
                    "field command conflicted at revision {minimum_revision}; refresh before retry"
                );
                self.attempt_state = RetainedFieldAttemptState::RefreshRequired {
                    attempt: Some(attempt),
                    minimum_revision,
                };
                self.notice = Some(message.clone());
                RetainedFieldSettlement::RefreshRequired {
                    attempt,
                    minimum_revision,
                    message,
                }
            }
            Ok(GraphCommandOutcome::Rejected(rejection)) => {
                self.draft.reject_submission(attempt.0);
                let minimum_revision = self.draft.remote_revision();
                let message = format!("field command was rejected: {}", rejection.message());
                self.attempt_state = RetainedFieldAttemptState::RefreshRequired {
                    attempt: Some(attempt),
                    minimum_revision,
                };
                self.notice = Some(message.clone());
                RetainedFieldSettlement::RefreshRequired {
                    attempt,
                    minimum_revision,
                    message,
                }
            }
            Err(error) => {
                let expected_revision = self
                    .draft
                    .pending_expected_revision()
                    .unwrap_or(self.draft.remote_revision());
                let message = format!(
                    "field command outcome is uncertain: {error}; wait for authoritative refresh"
                );
                self.attempt_state = RetainedFieldAttemptState::Uncertain {
                    attempt,
                    expected_revision,
                };
                self.notice = Some(message.clone());
                RetainedFieldSettlement::Uncertain { attempt, message }
            }
        }
    }

    fn record_receipt(
        &mut self,
        attempt: RetainedFieldAttempt,
        committed_revision: GraphRevision,
    ) -> RetainedFieldSettlement {
        match self
            .draft
            .record_self_receipt(attempt.0, committed_revision)
        {
            DraftReceiptDecision::Acknowledged {
                revision,
                local_changed_after_submission,
                ..
            } => {
                self.attempt_state = RetainedFieldAttemptState::Idle;
                self.notice = None;
                RetainedFieldSettlement::Acknowledged {
                    attempt,
                    revision,
                    local_changed_after_submission,
                }
            }
            DraftReceiptDecision::Recorded {
                committed_revision, ..
            }
            | DraftReceiptDecision::AlreadyRecorded {
                committed_revision, ..
            } => {
                self.attempt_state = RetainedFieldAttemptState::ReceiptFirst {
                    attempt,
                    committed_revision,
                };
                RetainedFieldSettlement::AwaitingSnapshot {
                    attempt,
                    committed_revision,
                }
            }
            DraftReceiptDecision::Superseded {
                current_revision, ..
            } => self.receipt_requires_refresh(
                attempt,
                current_revision,
                "field receipt was superseded by a newer authoritative revision",
            ),
            DraftReceiptDecision::UnknownAttempt { .. }
            | DraftReceiptDecision::InvalidCommittedRevision { .. } => self
                .receipt_requires_refresh(
                    attempt,
                    committed_revision,
                    "field receipt did not match the retained attempt",
                ),
        }
    }

    fn receipt_requires_refresh(
        &mut self,
        attempt: RetainedFieldAttempt,
        minimum_revision: GraphRevision,
        message: &str,
    ) -> RetainedFieldSettlement {
        self.draft.reject_submission(attempt.0);
        self.attempt_state = RetainedFieldAttemptState::RefreshRequired {
            attempt: Some(attempt),
            minimum_revision,
        };
        self.notice = Some(message.to_owned());
        RetainedFieldSettlement::RefreshRequired {
            attempt,
            minimum_revision,
            message: message.to_owned(),
        }
    }

    pub fn mark_uncertain(
        &mut self,
        attempt: RetainedFieldAttempt,
        message: impl Into<String>,
    ) -> Result<RetainedFieldSettlement, RetainedFieldError> {
        if self.draft.pending_attempt() != Some(attempt.0) {
            return Err(RetainedFieldError::AttemptMismatch { attempt });
        }
        let expected_revision = self
            .draft
            .pending_expected_revision()
            .unwrap_or(self.draft.remote_revision());
        let message = message.into();
        self.attempt_state = RetainedFieldAttemptState::Uncertain {
            attempt,
            expected_revision,
        };
        self.notice = Some(message.clone());
        Ok(RetainedFieldSettlement::Uncertain { attempt, message })
    }

    pub fn resolve_uncertain_from_fresh(
        &mut self,
        revision: GraphRevision,
        authoritative: T,
    ) -> Result<RetainedFieldUpdate, RetainedFieldError> {
        let RetainedFieldAttemptState::Uncertain { attempt, .. } = self.attempt_state else {
            return Err(RetainedFieldError::NoUncertainAttempt);
        };
        let submitted_is_authoritative = self
            .draft
            .pending_value()
            .is_some_and(|submitted| *submitted == authoritative);
        if submitted_is_authoritative {
            self.draft.reject_submission(attempt.0);
            self.draft.reconcile_remote(revision, authoritative);
        } else {
            self.draft.reject_stale(attempt.0, revision, authoritative);
        }
        self.attempt_state = RetainedFieldAttemptState::Idle;
        self.notice = None;
        Ok(RetainedFieldUpdate::UncertainResolved {
            attempt,
            revision,
            submitted_value_is_authoritative: submitted_is_authoritative,
        })
    }

    pub(crate) fn reducer(&self) -> &RevisionedDraft<T> {
        &self.draft
    }
}

/// Exact retained native mount captured by a field owner.
#[derive(Clone)]
pub(crate) struct RetainedFieldMountCapability {
    instance: NodeInstanceKey,
    view: jellyflow::runtime::WeakViewSession,
    mount: OpenGpuiMountCapability,
    activity: OpenGpuiNodeActivityRegistry,
}

impl RetainedFieldMountCapability {
    pub(crate) fn new(
        instance: NodeInstanceKey,
        view: jellyflow::runtime::WeakViewSession,
        mount: OpenGpuiMountCapability,
        activity: OpenGpuiNodeActivityRegistry,
    ) -> Self {
        Self {
            instance,
            view,
            mount,
            activity,
        }
    }

    fn state(&self) -> OpenGpuiMountState {
        self.mount.state()
    }
}

/// Open GPUI-local owner for one retained editable field.
pub struct RetainedField<T> {
    mount: RetainedFieldMountCapability,
    key: RetainedFieldKey,
    store: RetainedFieldStore<T>,
    field: RevisionedField<T>,
    recovery: RetainedFieldRecovery,
    draft_lease: Option<OpenGpuiActivityLease>,
    task_lease: Option<OpenGpuiActivityLease>,
    task: Option<Task<()>>,
    externalized: bool,
}

impl<T> fmt::Debug for RetainedField<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetainedField")
            .field("instance", &self.mount.instance)
            .field("key", &self.key)
            .field("field", &self.field)
            .field("recovery", &self.recovery)
            .field("has_draft_lease", &self.draft_lease.is_some())
            .field("has_task", &self.task.is_some())
            .field("externalized", &self.externalized)
            .finish()
    }
}

impl<T> RetainedField<T>
where
    T: Clone + PartialEq + 'static,
{
    pub fn mount(
        binding: &OpenGpuiNodeBinding,
        key: RetainedFieldKey,
        store: RetainedFieldStore<T>,
        revision: GraphRevision,
        authoritative: T,
    ) -> Result<Self, RetainedFieldError> {
        let mount = binding.retained_field_mount();
        store.ensure_bound(mount.instance.view(), &mount.view)?;
        let restored = store.restore(&mount.instance, key.as_str(), revision, authoritative)?;
        let (field, recovery) = RevisionedField::from_restore(restored);
        Ok(Self {
            mount,
            key,
            store,
            field,
            recovery,
            draft_lease: None,
            task_lease: None,
            task: None,
            externalized: false,
        })
    }

    #[must_use]
    pub fn key(&self) -> &RetainedFieldKey {
        &self.key
    }

    #[must_use]
    pub fn instance(&self) -> &NodeInstanceKey {
        &self.mount.instance
    }

    #[must_use]
    pub fn field(&self) -> &RevisionedField<T> {
        &self.field
    }

    pub fn field_mut(&mut self) -> &mut RevisionedField<T> {
        &mut self.field
    }

    #[must_use]
    pub const fn recovery(&self) -> RetainedFieldRecovery {
        self.recovery
    }

    pub fn prepare_task(
        &mut self,
        attempt: RetainedFieldAttempt,
    ) -> Result<(), RetainedFieldError> {
        self.ensure_live()?;
        if self.field.attempt_state().attempt() != Some(attempt) {
            return Err(RetainedFieldError::AttemptMismatch { attempt });
        }
        if self.task_lease.is_none() {
            self.task_lease = Some(self.mount.activity.acquire(OpenGpuiActivityKind::Task)?);
        }
        Ok(())
    }

    pub fn attach_task(
        &mut self,
        attempt: RetainedFieldAttempt,
        task: Task<()>,
    ) -> Result<(), RetainedFieldError> {
        self.ensure_live()?;
        if self.field.attempt_state().attempt() != Some(attempt) || self.task_lease.is_none() {
            return Err(RetainedFieldError::AttemptMismatch { attempt });
        }
        self.task = Some(task);
        Ok(())
    }

    pub fn not_dispatched(
        &mut self,
        attempt: RetainedFieldAttempt,
    ) -> Result<(), RetainedFieldError> {
        self.task.take();
        self.task_lease.take();
        self.field.not_dispatched(attempt)
    }

    pub fn settle_ticket(
        &mut self,
        attempt: RetainedFieldAttempt,
        result: OpenGpuiCommandTicketResult,
    ) -> Result<RetainedFieldSettlement, RetainedFieldError> {
        self.ensure_live()?;
        self.task.take();
        self.task_lease.take();
        Ok(self.field.settle_ticket(attempt, result))
    }

    pub fn sync_retention(&mut self) -> Result<RetainedFieldRetention, RetainedFieldError> {
        self.ensure_live()?;
        let active = self.field.phase() != RetainedFieldPhase::Clean
            || self.field.attempt_state() != RetainedFieldAttemptState::Idle;
        if active && self.draft_lease.is_none() {
            match self.mount.activity.acquire(OpenGpuiActivityKind::Draft) {
                Ok(lease) => self.draft_lease = Some(lease),
                Err(OpenGpuiActivityAcquireError::ProtectedCapacityExceeded { .. }) => {
                    self.store.externalize_overflow(
                        &self.mount.instance,
                        self.key.as_str(),
                        self.field.reducer(),
                    )?;
                    self.externalized = true;
                    return Ok(RetainedFieldRetention::Externalized);
                }
                Err(error) => return Err(error.into()),
            }
        } else if !active {
            self.draft_lease.take();
        }

        if self.externalized && (!active || self.draft_lease.is_some()) {
            self.store
                .discard_externalized(&self.mount.instance, self.key.as_str())?;
            self.externalized = false;
        }
        Ok(if active {
            RetainedFieldRetention::Protected
        } else {
            RetainedFieldRetention::Idle
        })
    }

    fn ensure_live(&mut self) -> Result<(), RetainedFieldError> {
        match self.mount.state() {
            OpenGpuiMountState::Live => Ok(()),
            OpenGpuiMountState::Retired => {
                self.release_terminal_resources();
                Err(RetainedFieldError::Retired)
            }
            OpenGpuiMountState::Closed(reason) => {
                self.release_terminal_resources();
                Err(RetainedFieldError::Closed { reason })
            }
        }
    }

    fn release_terminal_resources(&mut self) {
        self.task.take();
        self.task_lease.take();
        self.draft_lease.take();
    }
}

/// Retention outcome after synchronizing a native field with the host budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetainedFieldRetention {
    Idle,
    Protected,
    Externalized,
}

/// Public error boundary for retained field planning and lifecycle operations.
#[derive(Debug, thiserror::Error)]
pub enum RetainedFieldError {
    #[error("retained field key is empty")]
    EmptyKey,
    #[error("retained field key has {actual_bytes} bytes; maximum is {maximum_bytes}")]
    KeyTooLong {
        actual_bytes: usize,
        maximum_bytes: usize,
    },
    #[error("retained field is clean")]
    Clean,
    #[error("retained field composition is active")]
    CompositionActive,
    #[error("retained field conflict requires an explicit choice")]
    ConflictRequiresResolution,
    #[error("retained field has no conflict to resolve")]
    NoConflict,
    #[error(
        "retained field observed revision {observed_revision}, not requested revision {requested_revision}"
    )]
    RevisionMismatch {
        observed_revision: GraphRevision,
        requested_revision: GraphRevision,
    },
    #[error("retained field attempt is still active: {attempt:?}")]
    AttemptPending {
        attempt: Option<RetainedFieldAttempt>,
    },
    #[error("retained field attempt identity space is exhausted")]
    AttemptIdentityExhausted,
    #[error("retained field attempt does not match: {attempt:?}")]
    AttemptMismatch { attempt: RetainedFieldAttempt },
    #[error("retained field has no uncertain attempt")]
    NoUncertainAttempt,
    #[error("retained field mount is retired")]
    Retired,
    #[error("retained field mount is closed: {reason:?}")]
    Closed { reason: ViewCloseReason },
    #[error(transparent)]
    View(#[from] jellyflow::runtime::ViewSessionError),
    #[error(transparent)]
    Store(#[from] RetainedFieldStoreError),
    #[error(transparent)]
    Activity(#[from] OpenGpuiActivityAcquireError),
}

fn map_plan_error(reason: DraftIntentRejection) -> RetainedFieldError {
    match reason {
        DraftIntentRejection::Clean => RetainedFieldError::Clean,
        DraftIntentRejection::CompositionActive => RetainedFieldError::CompositionActive,
        DraftIntentRejection::ConflictRequiresResolution => {
            RetainedFieldError::ConflictRequiresResolution
        }
        DraftIntentRejection::NoConflict => RetainedFieldError::NoConflict,
        DraftIntentRejection::UnreconciledRevision {
            observed_revision,
            requested_revision,
        } => RetainedFieldError::RevisionMismatch {
            observed_revision,
            requested_revision,
        },
        DraftIntentRejection::SubmissionPending { attempt } => RetainedFieldError::AttemptPending {
            attempt: Some(RetainedFieldAttempt(attempt)),
        },
        DraftIntentRejection::AttemptIdentityExhausted => {
            RetainedFieldError::AttemptIdentityExhausted
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirty_field() -> (RevisionedField<String>, RetainedFieldIntent<String>) {
        let mut field = RevisionedField::new(7, "remote".to_owned());
        field.edit("submitted".to_owned());
        let intent = field.plan(7).expect("plan field submission");
        (field, intent)
    }

    #[test]
    fn receipt_first_waits_for_matching_snapshot() {
        let (mut field, intent) = dirty_field();

        let settlement = field.record_receipt(intent.attempt(), 8);
        assert_eq!(
            settlement,
            RetainedFieldSettlement::AwaitingSnapshot {
                attempt: intent.attempt(),
                committed_revision: 8,
            }
        );
        assert!(matches!(
            field.attempt_state(),
            RetainedFieldAttemptState::ReceiptFirst { .. }
        ));

        assert_eq!(
            field.observe(8, "submitted".to_owned()),
            RetainedFieldUpdate::Acknowledged {
                attempt: intent.attempt(),
                revision: 8,
                local_changed_after_submission: false,
            }
        );
        assert_eq!(field.phase(), RetainedFieldPhase::Clean);
    }

    #[test]
    fn snapshot_first_is_not_exposed_as_a_conflict() {
        let (mut field, intent) = dirty_field();

        assert_eq!(
            field.observe(8, "submitted".to_owned()),
            RetainedFieldUpdate::SnapshotFirst {
                attempt: intent.attempt(),
                revision: 8,
            }
        );
        assert_eq!(field.conflict(), None);
        assert_eq!(field.phase(), RetainedFieldPhase::Dirty);

        assert!(matches!(
            field.record_receipt(intent.attempt(), 8),
            RetainedFieldSettlement::Acknowledged { .. }
        ));
        assert_eq!(field.phase(), RetainedFieldPhase::Clean);
    }

    #[test]
    fn continued_edit_survives_acknowledgement_of_older_value() {
        let (mut field, intent) = dirty_field();
        field.edit("newer local".to_owned());

        field.record_receipt(intent.attempt(), 8);
        assert!(matches!(
            field.observe(8, "submitted".to_owned()),
            RetainedFieldUpdate::Acknowledged {
                local_changed_after_submission: true,
                ..
            }
        ));
        assert_eq!(field.local(), "newer local");
        assert_eq!(field.phase(), RetainedFieldPhase::Dirty);
    }

    #[test]
    fn noop_uses_the_same_receipt_snapshot_ordering() {
        let (mut field, intent) = dirty_field();

        assert!(matches!(
            field.record_receipt(intent.attempt(), 8),
            RetainedFieldSettlement::AwaitingSnapshot { .. }
        ));
        assert!(matches!(
            field.observe(8, "submitted".to_owned()),
            RetainedFieldUpdate::Acknowledged { .. }
        ));
    }

    #[test]
    fn transport_error_keeps_attempt_and_blocks_duplicate_submission() {
        let (mut field, intent) = dirty_field();
        let error = crate::command_journal::OpenGpuiCommandTicketError::MissingReceipt {
            operation_index: 0,
        };

        let settlement = field.settle_ticket(intent.attempt(), Err(error));
        assert!(matches!(
            settlement,
            RetainedFieldSettlement::Uncertain { .. }
        ));
        assert!(matches!(
            field.attempt_state(),
            RetainedFieldAttemptState::Uncertain { .. }
        ));
        assert!(matches!(
            field.plan(7),
            Err(RetainedFieldError::AttemptPending { .. })
        ));
    }

    #[test]
    fn newer_authoritative_snapshot_resolves_uncertain_outcome_without_retry() {
        let (mut field, intent) = dirty_field();
        field
            .mark_uncertain(intent.attempt(), "unknown outcome")
            .expect("mark uncertain");

        assert_eq!(
            field.observe(8, "submitted".to_owned()),
            RetainedFieldUpdate::UncertainResolved {
                attempt: intent.attempt(),
                revision: 8,
                submitted_value_is_authoritative: true,
            }
        );
        assert_eq!(field.phase(), RetainedFieldPhase::Clean);
        assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
    }

    #[test]
    fn definite_not_dispatched_clears_only_the_matching_attempt() {
        let (mut field, intent) = dirty_field();
        field
            .not_dispatched(intent.attempt())
            .expect("clear undispatched attempt");

        assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
        assert_eq!(field.phase(), RetainedFieldPhase::Dirty);
        assert!(field.can_submit());
    }
}
