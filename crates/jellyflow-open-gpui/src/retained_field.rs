use std::fmt;

use jellyflow_core::NodeId;
use jellyflow_runtime::{
    commands::GraphCommand,
    graph::GraphRevision,
    reconciliation::{
        FieldAttempt, FieldConflict, FieldConflictResolution, FieldIntentRejection, FieldPhase,
        FieldReceiptDecision, FieldReconcileDecision, FieldReconciler, FieldRecovery,
        FieldResolutionDecision, FieldRestore, FieldSubmissionDecision, FieldSubmissionIntent,
        FieldSubmissionKind, MutationCommandResult,
    },
    view::NodeInstanceKey,
    view::ViewCloseReason,
};
use open_gpui::App;

use crate::activity::{
    OpenGpuiActivityAcquireError, OpenGpuiActivityKind, OpenGpuiActivityLease,
    OpenGpuiNodeActivityRegistry,
};
use crate::admitted_command::{
    AdmittedCommandDrive, AdmittedCommandEvent, AdmittedCommandTask, AdmittedCommandTaskLease,
    AdmittedCommandUncertainty,
};
use crate::authoring::{
    OpenGpuiAuthoringStamp, OpenGpuiCommandAdmissionError, OpenGpuiNodeAuthoring,
    OpenGpuiNodeAuthoringError, OpenGpuiNodeObservation,
};
use crate::binding::{OpenGpuiMountContext, OpenGpuiMountValidationError};
use crate::command_journal::{OpenGpuiCommandTicketDisposition, OpenGpuiCommandTicketResult};
pub use crate::draft_store::{
    OPEN_GPUI_DEFAULT_RETAINED_FIELD_STORE_CAPACITY, RetainedFieldStore, RetainedFieldStoreError,
};

const RETAINED_FIELD_KEY_MAX_BYTES: usize = 128;
const DISPATCH_ATTEMPT_DROPPED_MESSAGE: &str = "field command outcome is uncertain: the dispatched attempt was dropped before its ticket settled";
const DISPATCH_TASK_INSTALLATION_UNWOUND_MESSAGE: &str =
    "field command outcome is uncertain: task installation unwound after the command was enqueued";
const DISPATCH_HOST_NOTIFICATION_UNWOUND_MESSAGE: &str =
    "field command outcome is uncertain: host notification unwound after the command was enqueued";
const DISPATCH_TASK_PANICKED_MESSAGE: &str =
    "field command outcome is uncertain: the dispatched task panicked before its ticket settled";

fn admitted_command_uncertainty_message(reason: AdmittedCommandUncertainty) -> &'static str {
    match reason {
        AdmittedCommandUncertainty::RunnerDropped => DISPATCH_ATTEMPT_DROPPED_MESSAGE,
        AdmittedCommandUncertainty::RunnerPanicked => DISPATCH_TASK_PANICKED_MESSAGE,
        AdmittedCommandUncertainty::TaskInstallationUnwound => {
            DISPATCH_TASK_INSTALLATION_UNWOUND_MESSAGE
        }
        AdmittedCommandUncertainty::HostNotificationUnwound => {
            DISPATCH_HOST_NOTIFICATION_UNWOUND_MESSAGE
        }
    }
}

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

/// Stable capability for one explicit field submission.
///
/// Runtime allocates the opaque identity globally, so attempts from different fields cannot
/// collide and the adapter does not need a second provenance nonce.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RetainedFieldAttempt {
    sequence: FieldAttempt,
}

impl RetainedFieldAttempt {
    const fn new(sequence: FieldAttempt) -> Self {
        Self { sequence }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.sequence.get()
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

impl From<FieldPhase> for RetainedFieldPhase {
    fn from(value: FieldPhase) -> Self {
        match value {
            FieldPhase::Clean => Self::Clean,
            FieldPhase::Dirty => Self::Dirty,
            FieldPhase::Composing => Self::Composing,
            FieldPhase::Conflict => Self::Conflict,
            FieldPhase::ComposingConflict => Self::ComposingConflict,
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

impl From<FieldConflict> for RetainedFieldConflict {
    fn from(value: FieldConflict) -> Self {
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
    inner: FieldSubmissionIntent<T>,
}

impl<T> RetainedFieldIntent<T> {
    #[must_use]
    pub const fn attempt(&self) -> RetainedFieldAttempt {
        RetainedFieldAttempt::new(self.inner.attempt())
    }

    #[must_use]
    pub const fn mode(&self) -> RetainedFieldSubmitMode {
        match self.inner.kind() {
            FieldSubmissionKind::Commit => RetainedFieldSubmitMode::Commit,
            FieldSubmissionKind::Reapply => RetainedFieldSubmitMode::Reapply,
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
    AuthoritativeMismatch {
        attempt: RetainedFieldAttempt,
        committed_revision: GraphRevision,
    },
    Superseded {
        attempt: RetainedFieldAttempt,
        committed_revision: GraphRevision,
        current_revision: GraphRevision,
    },
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
    DefinitelyNotExecuted {
        attempt: RetainedFieldAttempt,
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
            Self::RefreshRequired { message, .. }
            | Self::DefinitelyNotExecuted { message, .. }
            | Self::Uncertain { message, .. } => Some(message),
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

impl RetainedFieldRecovery {
    fn from_draft(value: FieldRecovery) -> Self {
        if value.is_empty() {
            return Self::default();
        }
        Self {
            interrupted_attempt: value.interrupted_attempt().map(RetainedFieldAttempt::new),
            composition_interrupted: value.composition_interrupted(),
        }
    }
}

/// Private revision-aware reducer shared by the two lifecycle-owning field interfaces.
#[derive(Clone)]
struct RevisionedField<T> {
    draft: FieldReconciler<T>,
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
    #[cfg(test)]
    #[must_use]
    pub fn new(revision: GraphRevision, authoritative: T) -> Self {
        Self {
            draft: FieldReconciler::new(revision, authoritative),
            attempt_state: RetainedFieldAttemptState::Idle,
            notice: None,
        }
    }

    fn from_restore(restored: FieldRestore<T>) -> (Self, RetainedFieldRecovery) {
        let recovery = RetainedFieldRecovery::from_draft(restored.recovery());
        let draft = restored.into_reconciler();
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

    fn retained_attempt(&self, sequence: FieldAttempt) -> RetainedFieldAttempt {
        RetainedFieldAttempt::new(sequence)
    }

    fn is_pending_attempt(&self, attempt: RetainedFieldAttempt) -> bool {
        self.draft.pending_attempt() == Some(attempt.sequence)
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
            return if matches!(self.draft.phase(), FieldPhase::ComposingConflict) {
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
            return self.apply_fresh_uncertain_resolution(attempt, revision, authoritative);
        }

        let decision = self.draft.reconcile_remote(revision, authoritative);
        if let Some(attempt) = self
            .draft
            .pending_attempt()
            .map(|sequence| self.retained_attempt(sequence))
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
            FieldReconcileDecision::Unchanged { .. } => RetainedFieldUpdate::Unchanged,
            FieldReconcileDecision::IgnoredStale { .. } => RetainedFieldUpdate::IgnoredStale,
            FieldReconcileDecision::InconsistentSnapshot { .. } => {
                RetainedFieldUpdate::InconsistentSnapshot
            }
            FieldReconcileDecision::RemoteApplied { .. } => RetainedFieldUpdate::RemoteApplied,
            FieldReconcileDecision::BaseAdvanced { .. } => RetainedFieldUpdate::BaseAdvanced,
            FieldReconcileDecision::Conflict { .. } => RetainedFieldUpdate::Conflict,
            FieldReconcileDecision::AuthoritativeMismatch {
                attempt: sequence,
                committed_revision,
            } => {
                let attempt = self.retained_attempt(sequence);
                self.attempt_state = RetainedFieldAttemptState::Idle;
                self.notice = Some(format!(
                    "field attempt did not match the authoritative value at revision {committed_revision}"
                ));
                RetainedFieldUpdate::AuthoritativeMismatch {
                    attempt,
                    committed_revision,
                }
            }
            FieldReconcileDecision::Superseded {
                attempt: sequence,
                committed_revision,
                current_revision,
            } => {
                let attempt = self.retained_attempt(sequence);
                self.attempt_state = RetainedFieldAttemptState::Idle;
                self.notice = Some(format!(
                    "field attempt committed at revision {committed_revision} was superseded by revision {current_revision}"
                ));
                RetainedFieldUpdate::Superseded {
                    attempt,
                    committed_revision,
                    current_revision,
                }
            }
            FieldReconcileDecision::SelfAcknowledged {
                attempt: sequence,
                revision,
                local_changed_after_submission,
            } => {
                let attempt = self.retained_attempt(sequence);
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
            FieldSubmissionDecision::Submit(inner) => {
                let intent = RetainedFieldIntent { inner };
                self.attempt_state = RetainedFieldAttemptState::Pending {
                    attempt: intent.attempt(),
                };
                self.notice = None;
                Ok(intent)
            }
            FieldSubmissionDecision::Rejected(reason) => Err(map_plan_error(reason)),
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
            RetainedFieldConflictChoice::AcceptRemote => FieldConflictResolution::AcceptRemote,
            RetainedFieldConflictChoice::KeepLocal => FieldConflictResolution::KeepLocal,
            RetainedFieldConflictChoice::ReapplyLocal => FieldConflictResolution::ReapplyLocal,
        };
        match self.draft.resolve_at(choice, command_revision) {
            FieldResolutionDecision::AcceptedRemote { revision } => {
                self.notice = None;
                Ok(RetainedFieldResolution::AcceptedRemote { revision })
            }
            FieldResolutionDecision::KeptLocal { conflict } => {
                Ok(RetainedFieldResolution::KeptLocal {
                    conflict: conflict.into(),
                })
            }
            FieldResolutionDecision::Reapply(inner) => {
                let intent = RetainedFieldIntent { inner };
                self.attempt_state = RetainedFieldAttemptState::Pending {
                    attempt: intent.attempt(),
                };
                self.notice = None;
                Ok(RetainedFieldResolution::Reapply(intent))
            }
            FieldResolutionDecision::Rejected(reason) => Err(map_plan_error(reason)),
        }
    }

    pub fn not_dispatched(
        &mut self,
        attempt: RetainedFieldAttempt,
    ) -> Result<(), RetainedFieldError> {
        if !self.is_pending_attempt(attempt) {
            return Err(RetainedFieldError::AttemptMismatch { attempt });
        }
        self.draft.reject_submission(attempt.sequence);
        self.attempt_state = RetainedFieldAttemptState::Idle;
        Ok(())
    }

    pub fn settle_ticket(
        &mut self,
        attempt: RetainedFieldAttempt,
        result: &OpenGpuiCommandTicketResult,
    ) -> RetainedFieldSettlement {
        if !self.is_pending_attempt(attempt) {
            return RetainedFieldSettlement::Ignored { attempt };
        }
        let completion = match OpenGpuiCommandTicketDisposition::classify(result) {
            OpenGpuiCommandTicketDisposition::Completed(completion) => completion,
            OpenGpuiCommandTicketDisposition::DefinitelyNotExecuted(error) => {
                return self.record_definitely_not_executed(attempt, error.to_string());
            }
            OpenGpuiCommandTicketDisposition::ExecutionUnknown(error) => {
                return self.record_uncertain(
                    attempt,
                    format!(
                        "field command outcome is uncertain: {error}; wait for authoritative refresh"
                    ),
                );
            }
        };
        match MutationCommandResult::from_runtime(completion) {
            MutationCommandResult::Applied { revision } => self.record_receipt(attempt, revision),
            MutationCommandResult::Noop {
                revision: minimum_revision,
            } => {
                self.draft.reject_submission(attempt.sequence);
                let message = format!(
                    "field command made no authoritative change at revision {minimum_revision}; refresh before retry"
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
            MutationCommandResult::Conflict {
                revision: minimum_revision,
                ..
            } => {
                self.draft.reject_submission(attempt.sequence);
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
            MutationCommandResult::Rejected { diagnostic, .. } => {
                self.draft.reject_submission(attempt.sequence);
                let minimum_revision = self.draft.remote_revision();
                let message = format!("field command was rejected: {diagnostic}");
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
            MutationCommandResult::Failed { diagnostic } => {
                self.draft.reject_submission(attempt.sequence);
                let minimum_revision = self.draft.remote_revision();
                let message = format!("field command execution failed: {diagnostic}");
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
            MutationCommandResult::NativeDispatchFailed { .. }
            | MutationCommandResult::NotDispatched { .. }
            | MutationCommandResult::Uncertain { .. } => {
                unreachable!("runtime completion projection cannot contain native evidence")
            }
        }
    }

    fn record_uncertain(
        &mut self,
        attempt: RetainedFieldAttempt,
        message: impl Into<String>,
    ) -> RetainedFieldSettlement {
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
        RetainedFieldSettlement::Uncertain { attempt, message }
    }

    fn record_definitely_not_executed(
        &mut self,
        attempt: RetainedFieldAttempt,
        diagnostic: impl Into<String>,
    ) -> RetainedFieldSettlement {
        self.draft.reject_submission(attempt.sequence);
        let message = format!(
            "field command was definitely not executed: {}",
            diagnostic.into()
        );
        self.attempt_state = RetainedFieldAttemptState::Idle;
        self.notice = Some(message.clone());
        RetainedFieldSettlement::DefinitelyNotExecuted { attempt, message }
    }

    fn record_receipt(
        &mut self,
        attempt: RetainedFieldAttempt,
        committed_revision: GraphRevision,
    ) -> RetainedFieldSettlement {
        if !self.is_pending_attempt(attempt) {
            return RetainedFieldSettlement::Ignored { attempt };
        }
        match self
            .draft
            .record_self_receipt(attempt.sequence, committed_revision)
        {
            FieldReceiptDecision::Acknowledged {
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
            FieldReceiptDecision::Recorded {
                committed_revision, ..
            }
            | FieldReceiptDecision::AlreadyRecorded {
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
            FieldReceiptDecision::Superseded {
                current_revision, ..
            } => self.receipt_requires_refresh(
                attempt,
                current_revision,
                "field receipt was superseded by a newer authoritative revision",
            ),
            FieldReceiptDecision::AuthoritativeMismatch {
                committed_revision, ..
            } => self.receipt_requires_refresh(
                attempt,
                committed_revision,
                "field receipt did not match the authoritative value at its committed revision",
            ),
            FieldReceiptDecision::UnknownAttempt { .. }
            | FieldReceiptDecision::InvalidCommittedRevision { .. } => self
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
        self.draft.reject_submission(attempt.sequence);
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
        if !self.is_pending_attempt(attempt) {
            return Err(RetainedFieldError::AttemptMismatch { attempt });
        }
        Ok(self.record_uncertain(attempt, message))
    }

    /// Resolves an uncertain command only from a snapshot strictly newer than its expectation.
    ///
    /// Older or equal revisions return [`RetainedFieldError::UncertainSnapshotNotFresh`] and
    /// leave the attempt, notice, and local/authoritative values unchanged.
    pub fn resolve_uncertain_from_fresh(
        &mut self,
        revision: GraphRevision,
        authoritative: T,
    ) -> Result<RetainedFieldUpdate, RetainedFieldError> {
        let RetainedFieldAttemptState::Uncertain {
            attempt,
            expected_revision,
        } = self.attempt_state
        else {
            return Err(RetainedFieldError::NoUncertainAttempt);
        };
        if revision <= expected_revision {
            return Err(RetainedFieldError::UncertainSnapshotNotFresh {
                attempt,
                observed_revision: revision,
                expected_revision,
            });
        }
        Ok(self.apply_fresh_uncertain_resolution(attempt, revision, authoritative))
    }

    fn apply_fresh_uncertain_resolution(
        &mut self,
        attempt: RetainedFieldAttempt,
        revision: GraphRevision,
        authoritative: T,
    ) -> RetainedFieldUpdate {
        let submitted_is_authoritative = self
            .draft
            .pending_value()
            .is_some_and(|submitted| *submitted == authoritative);
        if submitted_is_authoritative {
            self.draft.reject_submission(attempt.sequence);
            self.draft.reconcile_remote(revision, authoritative);
        } else {
            self.draft
                .reject_stale(attempt.sequence, revision, authoritative);
        }
        self.attempt_state = RetainedFieldAttemptState::Idle;
        self.notice = None;
        RetainedFieldUpdate::UncertainResolved {
            attempt,
            revision,
            submitted_value_is_authoritative: submitted_is_authoritative,
        }
    }

    pub(crate) fn reducer(&self) -> &FieldReconciler<T> {
        &self.draft
    }
}

struct FieldTransition<T, R> {
    field: RevisionedField<T>,
    output: R,
}

struct FieldOwnerState<T> {
    field: RevisionedField<T>,
}

impl<T> FieldOwnerState<T> {
    fn new(field: RevisionedField<T>) -> Self {
        Self { field }
    }

    fn published(&self) -> &RevisionedField<T> {
        &self.field
    }
}

impl<T> FieldOwnerState<T>
where
    T: Clone + PartialEq,
{
    fn prepare<R>(
        &self,
        operation: impl FnOnce(&mut RevisionedField<T>) -> R,
    ) -> FieldTransition<T, R> {
        let mut field = self.field.clone();
        let output = operation(&mut field);
        FieldTransition { field, output }
    }

    fn try_prepare<R>(
        &self,
        operation: impl FnOnce(&mut RevisionedField<T>) -> Result<R, RetainedFieldError>,
    ) -> Result<FieldTransition<T, R>, RetainedFieldError> {
        let mut field = self.field.clone();
        let output = operation(&mut field)?;
        Ok(FieldTransition { field, output })
    }

    fn publish<R>(&mut self, transition: FieldTransition<T, R>) -> R {
        self.field = transition.field;
        transition.output
    }

    fn clear_notice(&mut self) {
        self.field.clear_notice();
    }
}

/// Exact retained native mount captured by a field owner.
#[derive(Clone)]
pub(crate) struct RetainedFieldMountCapability {
    context: OpenGpuiMountContext,
    activity: OpenGpuiNodeActivityRegistry,
}

impl RetainedFieldMountCapability {
    pub(crate) fn new(
        context: OpenGpuiMountContext,
        activity: OpenGpuiNodeActivityRegistry,
    ) -> Self {
        Self { context, activity }
    }

    fn from_authoring(stamp: &OpenGpuiAuthoringStamp) -> Self {
        Self::new(stamp.context.clone(), stamp.activity.clone())
    }

    fn ensure_current(&self) -> Result<(), RetainedFieldError> {
        self.context
            .ensure_current()
            .map(|_| ())
            .map_err(map_mount_validation_error)
    }
}

fn map_mount_validation_error(error: OpenGpuiMountValidationError) -> RetainedFieldError {
    match error {
        OpenGpuiMountValidationError::RetiredMount { .. }
        | OpenGpuiMountValidationError::RetiredIncarnation { .. } => RetainedFieldError::Retired,
        OpenGpuiMountValidationError::ForeignView => RetainedFieldError::View(
            jellyflow_runtime::view::ViewSessionError::ForeignInteractionFacts,
        ),
        OpenGpuiMountValidationError::Closed { reason } => RetainedFieldError::Closed { reason },
        OpenGpuiMountValidationError::Session(error) => RetainedFieldError::View(error),
    }
}

#[derive(Debug)]
enum FieldRetentionState {
    Idle,
    Protected { _lease: OpenGpuiActivityLease },
    Externalized,
}

impl FieldRetentionState {
    fn public(&self) -> RetainedFieldRetention {
        match self {
            Self::Idle => RetainedFieldRetention::Idle,
            Self::Protected { .. } => RetainedFieldRetention::Protected,
            Self::Externalized => RetainedFieldRetention::Externalized,
        }
    }

    fn is_protected(&self) -> bool {
        matches!(self, Self::Protected { .. })
    }

    fn is_externalized(&self) -> bool {
        matches!(self, Self::Externalized)
    }

    fn release_protection(&mut self) {
        if self.is_protected() {
            *self = Self::Idle;
        }
    }
}

/// Open GPUI-local owner for one retained editable field.
pub struct RetainedField<T> {
    mount: RetainedFieldMountCapability,
    observation: OpenGpuiAuthoringStamp,
    key: RetainedFieldKey,
    store: RetainedFieldStore<T>,
    state: FieldOwnerState<T>,
    recovery: RetainedFieldRecovery,
    retention: FieldRetentionState,
    admitted_command: Option<AdmittedCommandTask<RetainedFieldAttempt>>,
}

impl<T> fmt::Debug for RetainedField<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetainedField")
            .field("instance", self.mount.context.instance())
            .field("key", &self.key)
            .field("field", self.state.published())
            .field("recovery", &self.recovery)
            .field("has_draft_lease", &self.retention.is_protected())
            .field(
                "task_attempt",
                &self
                    .admitted_command
                    .as_ref()
                    .map(AdmittedCommandTask::attempt),
            )
            .field("has_task", &self.admitted_command.is_some())
            .field("externalized", &self.retention.is_externalized())
            .finish()
    }
}

impl<T> RetainedField<T>
where
    T: Clone + PartialEq + 'static,
{
    pub fn mount(
        observation: OpenGpuiNodeObservation<T>,
        key: RetainedFieldKey,
        store: RetainedFieldStore<T>,
    ) -> Result<Self, RetainedFieldError> {
        let (observation, authoritative) = observation.into_parts();
        observation.ensure_current().map_err(map_authoring_error)?;
        let mount = RetainedFieldMountCapability::from_authoring(&observation);
        Self::mount_capability(mount, observation, key, store, authoritative)
    }

    fn mount_capability(
        mount: RetainedFieldMountCapability,
        observation: OpenGpuiAuthoringStamp,
        key: RetainedFieldKey,
        store: RetainedFieldStore<T>,
        authoritative: T,
    ) -> Result<Self, RetainedFieldError> {
        mount.ensure_current()?;
        store.ensure_bound(mount.context.view())?;
        let revision = observation.snapshot.revision();
        let restored = store.restore(
            mount.context.instance(),
            key.as_str(),
            revision,
            authoritative,
        )?;
        let (field, recovery) = RevisionedField::from_restore(restored);
        let mut retained = Self {
            mount,
            observation,
            key,
            store,
            state: FieldOwnerState::new(field),
            recovery,
            retention: FieldRetentionState::Idle,
            admitted_command: None,
        };
        retained.ensure_live()?;
        let candidate = retained.state.prepare(|_| ());
        retained.commit_candidate(candidate)?;
        Ok(retained)
    }

    #[must_use]
    pub fn key(&self) -> &RetainedFieldKey {
        &self.key
    }

    #[must_use]
    pub fn instance(&self) -> &NodeInstanceKey {
        self.mount.context.instance()
    }

    #[must_use]
    pub fn local(&self) -> &T {
        self.state.published().local()
    }

    #[must_use]
    pub fn remote(&self) -> &T {
        self.state.published().remote()
    }

    #[must_use]
    pub const fn base_revision(&self) -> GraphRevision {
        self.state.field.base_revision()
    }

    #[must_use]
    pub const fn remote_revision(&self) -> GraphRevision {
        self.state.field.remote_revision()
    }

    #[must_use]
    pub fn phase(&self) -> RetainedFieldPhase {
        self.state.published().phase()
    }

    #[must_use]
    pub fn conflict(&self) -> Option<RetainedFieldConflict> {
        self.state.published().conflict()
    }

    #[must_use]
    pub const fn attempt_state(&self) -> RetainedFieldAttemptState {
        self.state.field.attempt_state()
    }

    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.state.published().notice()
    }

    #[must_use]
    pub fn can_submit(&self) -> bool {
        self.state.published().can_submit()
    }

    /// Atomically replaces the local value and establishes retention before publishing it.
    pub fn edit(&mut self, value: T) -> Result<RetainedFieldPhase, RetainedFieldError> {
        self.mutate(|field| Ok(field.edit(value)))
    }

    /// Atomically updates framework-owned composition state.
    pub fn set_composing(
        &mut self,
        composing: bool,
    ) -> Result<RetainedFieldPhase, RetainedFieldError> {
        self.mutate(|field| Ok(field.set_composing(composing)))
    }

    /// Atomically reconciles one authoritative value carrying its authoring provenance.
    pub fn observe(
        &mut self,
        observation: OpenGpuiNodeObservation<T>,
        cx: &mut App,
    ) -> Result<RetainedFieldUpdate, RetainedFieldError> {
        let (observation, authoritative) = observation.into_parts();
        self.validate_observation(&observation)?;
        let settlement = self.drain_task_completion(cx)?;
        let revision = observation.snapshot.revision();
        let update = self.mutate(|field| Ok(field.observe(revision, authoritative)))?;
        self.observation = observation;
        if let RetainedFieldUpdate::Acknowledged { attempt, .. }
        | RetainedFieldUpdate::UncertainResolved { attempt, .. } = update
        {
            self.release_task_resources(attempt);
        }
        Ok(prefer_completion_update(settlement, update))
    }

    /// Plans one dispatch against the exact authoring view that supplied the latest observation.
    ///
    /// Dropping the returned plan before dispatch rolls the pending attempt back automatically.
    pub fn plan(
        &mut self,
        authoring: OpenGpuiNodeAuthoring,
        mode: RetainedFieldSubmitMode,
        cx: &mut App,
    ) -> Result<RetainedFieldDispatchPlan<'_, T>, RetainedFieldError> {
        let stamp = authoring.stamp();
        self.validate_planning_authoring(&stamp)?;
        self.drain_task_completion(cx)?;
        let revision = stamp.snapshot.revision();
        let intent = match mode {
            RetainedFieldSubmitMode::Commit => self.mutate(|field| field.plan(revision))?,
            RetainedFieldSubmitMode::Reapply => {
                match self.mutate(|field| {
                    field.resolve(RetainedFieldConflictChoice::ReapplyLocal, revision)
                })? {
                    RetainedFieldResolution::Reapply(intent) => intent,
                    RetainedFieldResolution::AcceptedRemote { .. }
                    | RetainedFieldResolution::KeptLocal { .. } => {
                        return Err(RetainedFieldError::ConflictRequiresResolution);
                    }
                }
            }
        };
        Ok(RetainedFieldDispatchPlan::new(self, authoring, intent))
    }

    /// Atomically applies one explicit conflict choice.
    pub fn resolve(
        &mut self,
        authoring: &OpenGpuiNodeAuthoring,
        choice: RetainedFieldConflictChoice,
        cx: &mut App,
    ) -> Result<RetainedFieldResolution<T>, RetainedFieldError> {
        let stamp = authoring.stamp();
        self.validate_planning_authoring(&stamp)?;
        self.drain_task_completion(cx)?;
        if choice == RetainedFieldConflictChoice::ReapplyLocal {
            return Err(RetainedFieldError::ReapplyRequiresDispatchPlan);
        }
        self.mutate(|field| field.resolve(choice, stamp.snapshot.revision()))
    }

    /// Clears presentation-only diagnostic text without changing retention state.
    pub fn clear_notice(&mut self) {
        self.state.clear_notice();
    }

    #[must_use]
    pub const fn recovery(&self) -> RetainedFieldRecovery {
        self.recovery
    }

    /// Returns the already-established retention state for the current field value.
    #[must_use]
    pub fn retention(&self) -> RetainedFieldRetention {
        self.retention.public()
    }

    /// Resolves an uncertain attempt only from a strictly newer authoritative snapshot.
    pub fn resolve_uncertain_from_fresh(
        &mut self,
        observation: OpenGpuiNodeObservation<T>,
        cx: &mut App,
    ) -> Result<RetainedFieldUpdate, RetainedFieldError> {
        let (observation, authoritative) = observation.into_parts();
        self.validate_observation(&observation)?;
        self.drain_task_completion(cx)?;
        let revision = observation.snapshot.revision();
        let update =
            self.mutate(|field| field.resolve_uncertain_from_fresh(revision, authoritative))?;
        self.observation = observation;
        if let RetainedFieldUpdate::UncertainResolved { attempt, .. } = update {
            self.release_task_resources(attempt);
        }
        Ok(update)
    }

    fn dispatch_planned(
        &mut self,
        authoring: OpenGpuiNodeAuthoring,
        intent: RetainedFieldIntent<T>,
        command: GraphCommand,
        cx: &mut App,
    ) -> Result<(), RetainedFieldError> {
        let attempt = intent.attempt();
        if let Err(error) = self.ensure_live() {
            self.cancel_planned(attempt);
            return Err(error);
        }
        if self.state.published().attempt_state().attempt() != Some(attempt) {
            return Err(RetainedFieldError::AttemptMismatch { attempt });
        }
        let stamp = authoring.stamp();
        if let Err(error) = self.validate_planning_authoring(&stamp) {
            self.cancel_planned(attempt);
            return Err(error);
        }
        if intent.expected_revision() != stamp.snapshot.revision() {
            self.cancel_planned(attempt);
            return Err(RetainedFieldError::RevisionMismatch {
                observed_revision: stamp.snapshot.revision(),
                requested_revision: intent.expected_revision(),
            });
        }

        let lease = match AdmittedCommandTaskLease::acquire(&self.mount.activity) {
            Ok(lease) => lease,
            Err(error) => {
                self.cancel_planned(attempt);
                return Err(error.into());
            }
        };
        let admitted = match authoring.admit(command) {
            Ok(admitted) => admitted,
            Err(error) => {
                self.cancel_planned(attempt);
                return Err(map_admission_error(error));
            }
        };
        let (ticket, notification) = admitted.into_parts();
        self.admitted_command = Some(AdmittedCommandTask::new(
            attempt,
            ticket,
            notification,
            lease,
        ));
        let drive = self
            .admitted_command
            .as_mut()
            .expect("an admitted retained field owns its task before setup")
            .drive(cx);
        match drive {
            Ok(AdmittedCommandDrive::AwaitingTicket) => Ok(()),
            Ok(AdmittedCommandDrive::Unavailable) => {
                self.record_admitted_uncertainty(
                    attempt,
                    AdmittedCommandUncertainty::RunnerDropped,
                );
                Err(RetainedFieldError::TaskInstallationFailed)
            }
            Err(unwind) => {
                self.record_admitted_uncertainty(attempt, unwind.reason());
                unwind.resume()
            }
        }
    }

    fn cancel_planned(&mut self, attempt: RetainedFieldAttempt) {
        let Ok(candidate) = self
            .state
            .try_prepare(|field| field.not_dispatched(attempt))
        else {
            return;
        };
        let _ = self.commit_candidate(candidate);
        self.release_task_resources(attempt);
    }

    fn validate_observation(
        &mut self,
        observation: &OpenGpuiAuthoringStamp,
    ) -> Result<(), RetainedFieldError> {
        self.validate_authoring_identity(observation)?;
        observation.ensure_current().map_err(map_authoring_error)
    }

    fn validate_planning_authoring(
        &mut self,
        authoring: &OpenGpuiAuthoringStamp,
    ) -> Result<(), RetainedFieldError> {
        self.validate_observation(authoring)?;
        if !self.observation.same_snapshot(authoring) {
            return Err(RetainedFieldError::ObservationSnapshotMismatch {
                observed_revision: self.observation.snapshot.revision(),
                authoring_revision: authoring.snapshot.revision(),
            });
        }
        Ok(())
    }

    fn validate_authoring_identity(
        &mut self,
        authoring: &OpenGpuiAuthoringStamp,
    ) -> Result<(), RetainedFieldError> {
        if authoring.context.instance() != self.mount.context.instance() {
            return Err(RetainedFieldError::ForeignAuthoring {
                expected: Box::new(self.mount.context.instance().clone()),
                actual: Box::new(authoring.context.instance().clone()),
            });
        }
        if !authoring
            .context
            .mount()
            .is_same_mount(self.mount.context.mount())
        {
            return Err(RetainedFieldError::CrossedMount {
                node: self.mount.context.instance().node(),
            });
        }
        self.ensure_live()
    }

    fn drain_task_completion(
        &mut self,
        cx: &mut App,
    ) -> Result<Option<RetainedFieldSettlement>, RetainedFieldError> {
        self.resume_admitted_task(cx);
        let event = self
            .admitted_command
            .as_ref()
            .and_then(AdmittedCommandTask::take_event_guard);
        let Some(event) = event else {
            return Ok(None);
        };
        let attempt = event.event().attempt();
        let recoverable_setup = matches!(
            event.event(),
            AdmittedCommandEvent::Uncertain {
                reason: AdmittedCommandUncertainty::TaskInstallationUnwound
                    | AdmittedCommandUncertainty::HostNotificationUnwound,
                ..
            }
        );
        let settlement = self.mutate(|field| match event.event() {
            AdmittedCommandEvent::Settled {
                attempt, result, ..
            } => Ok(field.settle_ticket(*attempt, result)),
            AdmittedCommandEvent::Uncertain { attempt, reason } => {
                field.mark_uncertain(*attempt, admitted_command_uncertainty_message(*reason))
            }
        })?;
        event.commit();
        if !recoverable_setup {
            self.release_task_resources(attempt);
        }
        Ok(Some(settlement))
    }

    fn resume_admitted_task(&mut self, cx: &mut App) {
        let drive = self
            .admitted_command
            .as_mut()
            .map(|task| (task.attempt(), task.drive(cx)));
        match drive {
            Some((_, Ok(AdmittedCommandDrive::AwaitingTicket))) | None => {}
            Some((attempt, Ok(AdmittedCommandDrive::Unavailable))) => {
                self.record_admitted_uncertainty(attempt, AdmittedCommandUncertainty::RunnerDropped)
            }
            Some((attempt, Err(unwind))) => {
                self.record_admitted_uncertainty(attempt, unwind.reason());
                let _ = unwind.suppress();
            }
        }
    }

    fn mutate<R>(
        &mut self,
        operation: impl FnOnce(&mut RevisionedField<T>) -> Result<R, RetainedFieldError>,
    ) -> Result<R, RetainedFieldError> {
        self.ensure_live()?;
        let candidate = self.state.try_prepare(operation)?;
        self.commit_candidate(candidate)
    }

    fn commit_candidate<R>(
        &mut self,
        candidate: FieldTransition<T, R>,
    ) -> Result<R, RetainedFieldError> {
        let active = candidate.field.phase() != RetainedFieldPhase::Clean
            || candidate.field.attempt_state() != RetainedFieldAttemptState::Idle;
        if active && !self.retention.is_protected() {
            match self.mount.activity.acquire(OpenGpuiActivityKind::Draft) {
                Ok(lease) => {
                    if self.retention.is_externalized() {
                        self.store.discard_externalized(
                            self.mount.context.instance(),
                            self.key.as_str(),
                        )?;
                    }
                    self.retention = FieldRetentionState::Protected { _lease: lease };
                    return Ok(self.state.publish(candidate));
                }
                Err(OpenGpuiActivityAcquireError::ProtectedCapacityExceeded { .. }) => {
                    self.store.externalize_overflow(
                        self.mount.context.instance(),
                        self.key.as_str(),
                        candidate.field.reducer(),
                    )?;
                    self.retention = FieldRetentionState::Externalized;
                    return Ok(self.state.publish(candidate));
                }
                Err(error) => return Err(error.into()),
            }
        }

        if self.retention.is_externalized() {
            self.store
                .discard_externalized(self.mount.context.instance(), self.key.as_str())?;
        }
        if !active {
            self.retention = FieldRetentionState::Idle;
        }
        Ok(self.state.publish(candidate))
    }

    fn ensure_live(&mut self) -> Result<(), RetainedFieldError> {
        let result = self.mount.ensure_current();
        if matches!(
            &result,
            Err(RetainedFieldError::Retired | RetainedFieldError::Closed { .. })
        ) {
            self.release_terminal_resources();
        }
        result
    }

    fn release_task_resources(&mut self, attempt: RetainedFieldAttempt) {
        AdmittedCommandTask::release_if(&mut self.admitted_command, attempt);
    }

    fn release_terminal_resources(&mut self) {
        AdmittedCommandTask::shutdown(&mut self.admitted_command);
        self.retention.release_protection();
    }

    fn record_admitted_uncertainty(
        &self,
        attempt: RetainedFieldAttempt,
        reason: AdmittedCommandUncertainty,
    ) {
        if let Some(task) = self
            .admitted_command
            .as_ref()
            .filter(|task| task.attempt() == attempt)
        {
            task.record_uncertainty(reason);
        }
    }
}

/// One planned retained-field submission tied to an exact authoring snapshot.
///
/// This type owns the only public path from a field intent to command dispatch. If command
/// construction returns early, dropping the plan restores the field to a retryable dirty state.
#[must_use = "dropping an undispatched field plan rolls it back"]
pub struct RetainedFieldDispatchPlan<'a, T>
where
    T: Clone + PartialEq + 'static,
{
    field: &'a mut RetainedField<T>,
    authoring: Option<OpenGpuiNodeAuthoring>,
    intent: Option<RetainedFieldIntent<T>>,
}

impl<'a, T> RetainedFieldDispatchPlan<'a, T>
where
    T: Clone + PartialEq + 'static,
{
    fn new(
        field: &'a mut RetainedField<T>,
        authoring: OpenGpuiNodeAuthoring,
        intent: RetainedFieldIntent<T>,
    ) -> Self {
        Self {
            field,
            authoring: Some(authoring),
            intent: Some(intent),
        }
    }

    #[must_use]
    pub fn authoring(&self) -> &OpenGpuiNodeAuthoring {
        self.authoring
            .as_ref()
            .expect("an undispatched field plan retains its authoring view")
    }

    #[must_use]
    pub fn mode(&self) -> RetainedFieldSubmitMode {
        self.intent
            .as_ref()
            .expect("an undispatched field plan retains its intent")
            .mode()
    }

    #[must_use]
    pub fn value(&self) -> &T {
        self.intent
            .as_ref()
            .expect("an undispatched field plan retains its intent")
            .value()
    }

    /// Enqueues the product command and internally supervises its terminal ticket.
    ///
    /// The terminal ticket is intentionally not exposed to product code. The supervisor refreshes
    /// the window after every terminal outcome so the retained field can reconcile on its next
    /// authoritative observation.
    pub fn dispatch(
        mut self,
        command: GraphCommand,
        cx: &mut App,
    ) -> Result<(), RetainedFieldError> {
        let authoring = self
            .authoring
            .take()
            .expect("an undispatched field plan retains its authoring view");
        let intent = self
            .intent
            .take()
            .expect("an undispatched field plan retains its intent");
        self.field.dispatch_planned(authoring, intent, command, cx)
    }
}

impl<T> Drop for RetainedFieldDispatchPlan<'_, T>
where
    T: Clone + PartialEq + 'static,
{
    fn drop(&mut self) {
        if let Some(intent) = self.intent.take() {
            self.field.cancel_planned(intent.attempt());
        }
    }
}

fn prefer_completion_update(
    settlement: Option<RetainedFieldSettlement>,
    observed: RetainedFieldUpdate,
) -> RetainedFieldUpdate {
    match (settlement, observed) {
        (
            Some(RetainedFieldSettlement::Acknowledged {
                attempt,
                revision,
                local_changed_after_submission,
            }),
            RetainedFieldUpdate::Unchanged
            | RetainedFieldUpdate::IgnoredStale
            | RetainedFieldUpdate::InconsistentSnapshot
            | RetainedFieldUpdate::RemoteApplied
            | RetainedFieldUpdate::BaseAdvanced,
        ) => RetainedFieldUpdate::Acknowledged {
            attempt,
            revision,
            local_changed_after_submission,
        },
        (_, observed) => observed,
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
    #[error("retained field reapply must use a revision-bound dispatch plan")]
    ReapplyRequiresDispatchPlan,
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
    #[error("retained field command was not admitted: {message}")]
    CommandNotAdmitted { message: String },
    #[error("retained field command task could not be installed")]
    TaskInstallationFailed,
    #[error(
        "retained field belongs to authoring instance {expected:?}, not foreign instance {actual:?}"
    )]
    ForeignAuthoring {
        expected: Box<NodeInstanceKey>,
        actual: Box<NodeInstanceKey>,
    },
    #[error("retained field authoring came from a different native mount for node {node:?}")]
    CrossedMount { node: NodeId },
    #[error(
        "retained field observation revision {observed_revision} is not the planning authoring snapshot at revision {authoring_revision}"
    )]
    ObservationSnapshotMismatch {
        observed_revision: GraphRevision,
        authoring_revision: GraphRevision,
    },
    #[error(
        "retained field authoring revision {authoring_revision} is stale; current graph revision is {current_revision}"
    )]
    StaleAuthoring {
        authoring_revision: GraphRevision,
        current_revision: GraphRevision,
    },
    #[error("retained field has no uncertain attempt")]
    NoUncertainAttempt,
    #[error(
        "uncertain retained field snapshot revision {observed_revision} is not newer than expected revision {expected_revision} for attempt {attempt:?}"
    )]
    UncertainSnapshotNotFresh {
        attempt: RetainedFieldAttempt,
        observed_revision: GraphRevision,
        expected_revision: GraphRevision,
    },
    #[error("retained field mount is retired")]
    Retired,
    #[error("retained field mount is closed: {reason:?}")]
    Closed { reason: ViewCloseReason },
    #[error(transparent)]
    View(#[from] jellyflow_runtime::view::ViewSessionError),
    #[error(transparent)]
    Store(#[from] RetainedFieldStoreError),
    #[error(transparent)]
    Activity(#[from] OpenGpuiActivityAcquireError),
    #[error(transparent)]
    Authoring(Box<OpenGpuiNodeAuthoringError>),
}

fn map_authoring_error(error: OpenGpuiNodeAuthoringError) -> RetainedFieldError {
    match error {
        OpenGpuiNodeAuthoringError::RetiredMount { .. }
        | OpenGpuiNodeAuthoringError::RetiredIncarnation { .. }
        | OpenGpuiNodeAuthoringError::MissingNode { .. } => RetainedFieldError::Retired,
        OpenGpuiNodeAuthoringError::Closed { reason } => RetainedFieldError::Closed { reason },
        OpenGpuiNodeAuthoringError::SnapshotChanged {
            authoring_revision,
            current_revision,
        }
        | OpenGpuiNodeAuthoringError::StaleRevision {
            authoring_revision,
            current_revision,
        } => RetainedFieldError::StaleAuthoring {
            authoring_revision,
            current_revision,
        },
        error => RetainedFieldError::Authoring(Box::new(error)),
    }
}

fn map_admission_error(error: OpenGpuiCommandAdmissionError) -> RetainedFieldError {
    match error {
        OpenGpuiCommandAdmissionError::Authoring(error) => map_authoring_error(error),
        OpenGpuiCommandAdmissionError::Journal(error) => RetainedFieldError::CommandNotAdmitted {
            message: error.to_string(),
        },
    }
}

fn map_plan_error(reason: FieldIntentRejection) -> RetainedFieldError {
    match reason {
        FieldIntentRejection::Clean => RetainedFieldError::Clean,
        FieldIntentRejection::CompositionActive => RetainedFieldError::CompositionActive,
        FieldIntentRejection::ConflictRequiresResolution => {
            RetainedFieldError::ConflictRequiresResolution
        }
        FieldIntentRejection::NoConflict => RetainedFieldError::NoConflict,
        FieldIntentRejection::UnreconciledRevision {
            observed_revision,
            requested_revision,
        } => RetainedFieldError::RevisionMismatch {
            observed_revision,
            requested_revision,
        },
        FieldIntentRejection::SubmissionPending { attempt } => RetainedFieldError::AttemptPending {
            attempt: Some(RetainedFieldAttempt::new(attempt)),
        },
        FieldIntentRejection::AttemptIdentityExhausted => {
            RetainedFieldError::AttemptIdentityExhausted
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use jellyflow_core::{
        CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
    };
    use jellyflow_runtime::{
        graph::GraphRuntime, reconciliation::MutationCommandResult, view::FrameRequest,
        view::RendererStamp, view::ViewSession, view::ViewSessionOptions,
    };

    use super::*;
    use crate::activity::OpenGpuiActivityCoordinator;
    use crate::binding::OpenGpuiMountCapability;

    fn dirty_field() -> (RevisionedField<String>, RetainedFieldIntent<String>) {
        let mut field = RevisionedField::new(7, "remote".to_owned());
        field.edit("submitted".to_owned());
        let intent = field.plan(7).expect("plan field submission");
        (field, intent)
    }

    #[test]
    fn definitive_transport_failure_releases_the_attempt_while_unknown_execution_blocks_retry() {
        for error in [
            crate::command_journal::OpenGpuiCommandTicketError::HostDropped,
            crate::command_journal::OpenGpuiCommandTicketError::HostClosed {
                reason: ViewCloseReason::HostClosed,
            },
            crate::command_journal::OpenGpuiCommandTicketError::FramePreparation {
                message: "frame preparation failed".to_owned(),
            },
        ] {
            let (mut field, intent) = dirty_field();
            let settlement = field.settle_ticket(
                intent.attempt(),
                &Err(
                    crate::command_journal::OpenGpuiCommandTicketFailure::definitely_not_executed(
                        error,
                    ),
                ),
            );
            assert!(matches!(
                settlement,
                RetainedFieldSettlement::DefinitelyNotExecuted { .. }
            ));
            assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
            assert!(field.can_submit());
            assert!(field.plan(7).is_ok());
        }

        let (mut field, intent) = dirty_field();
        let settlement = field.settle_ticket(
            intent.attempt(),
            &Err(
                crate::command_journal::OpenGpuiCommandTicketFailure::execution_unknown(
                    crate::command_journal::OpenGpuiCommandTicketError::FeedbackOutcomeUnknown,
                ),
            ),
        );
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
    fn edit_is_rejected_atomically_when_protection_and_external_storage_are_full() {
        let (_runtime, view, instance) = retained_field_instance();
        let store = RetainedFieldStore::with_capacity(NonZeroUsize::new(1).unwrap());
        store
            .ensure_bound(&view.downgrade())
            .expect("bind retained field store");
        let mut occupied = FieldReconciler::new(0, "occupied remote".to_owned());
        occupied.edit_local("occupied local".to_owned());
        store
            .externalize_overflow(&instance, "occupied", &occupied)
            .expect("fill external field store");

        let activity = OpenGpuiNodeActivityRegistry::for_host(
            instance.clone(),
            OpenGpuiActivityCoordinator::new(0),
        );
        let context =
            OpenGpuiMountContext::new(instance, view.downgrade(), OpenGpuiMountCapability::new())
                .expect("exact retained field mount");
        let mut retained = mount_capability_for_test(
            RetainedFieldMountCapability::new(context, activity),
            RetainedFieldKey::new("target").unwrap(),
            store.clone(),
            "authoritative".to_owned(),
        )
        .expect("mount retained field");

        assert!(matches!(
            retained.edit("must not publish".to_owned()),
            Err(RetainedFieldError::Store(
                RetainedFieldStoreError::CapacityExceeded { capacity: 1 }
            ))
        ));
        assert_eq!(retained.local(), "authoritative");
        assert_eq!(retained.phase(), RetainedFieldPhase::Clean);
        assert_eq!(retained.retention(), RetainedFieldRetention::Idle);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn retired_mount_is_rejected_before_externalized_draft_is_consumed() {
        let (_runtime, view, instance) = retained_field_instance();
        let store = RetainedFieldStore::new();
        store
            .ensure_bound(&view.downgrade())
            .expect("bind retained field store");
        let mut draft = FieldReconciler::new(0, "authoritative".to_owned());
        draft.edit_local("unsaved".to_owned());
        store
            .externalize_overflow(&instance, "target", &draft)
            .expect("externalize retained draft");

        let mount = OpenGpuiMountCapability::new();
        let context = OpenGpuiMountContext::new(instance.clone(), view.downgrade(), mount.clone())
            .expect("exact retained field mount");
        mount.retire();
        let capability = RetainedFieldMountCapability::new(
            context,
            OpenGpuiNodeActivityRegistry::for_host(instance, OpenGpuiActivityCoordinator::new(1)),
        );

        assert!(matches!(
            mount_capability_for_test(
                capability,
                RetainedFieldKey::new("target").unwrap(),
                store.clone(),
                "authoritative".to_owned(),
            ),
            Err(RetainedFieldError::Retired)
        ));
        assert_eq!(
            store.len(),
            1,
            "a stale mount must not take the saved draft"
        );
    }

    #[test]
    fn foreign_node_and_crossed_mount_observations_fail_without_mutating_the_field() {
        let (_runtime, view, instance) = retained_field_instance();
        let capability = test_capability(&view, instance, 2);
        let mut field = mount_capability_for_test(
            capability,
            RetainedFieldKey::new("target").unwrap(),
            RetainedFieldStore::new(),
            "authoritative".to_owned(),
        )
        .expect("mount retained field");
        field.edit("local".to_owned()).expect("retain local edit");

        let (_foreign_runtime, foreign_view, foreign_instance) = retained_field_instance();
        let foreign = test_capability(&foreign_view, foreign_instance, 2);
        let foreign_observation = observation_for_test(&foreign, "foreign".to_owned());
        assert!(matches!(
            field.validate_observation(foreign_observation.stamp()),
            Err(RetainedFieldError::ForeignAuthoring { .. })
        ));

        let crossed_context = OpenGpuiMountContext::new(
            field.mount.context.instance().clone(),
            field.mount.context.view().clone(),
            OpenGpuiMountCapability::new(),
        )
        .expect("crossed mount retains an exact view identity");
        let crossed =
            RetainedFieldMountCapability::new(crossed_context, field.mount.activity.clone());
        let crossed_observation = observation_for_test(&crossed, "crossed".to_owned());
        assert!(matches!(
            field.validate_observation(crossed_observation.stamp()),
            Err(RetainedFieldError::CrossedMount { .. })
        ));
        assert_eq!(field.local(), "local");
        assert_eq!(field.remote(), "authoritative");
    }

    #[test]
    fn stale_observation_and_planning_stamp_fail_before_reconciliation() {
        let (runtime, view, instance) = retained_field_instance();
        let capability = test_capability(&view, instance, 2);
        let stale_observation = observation_for_test(&capability, "stale".to_owned());
        let stale_planning = stale_observation.stamp().clone();
        let mut field = mount_capability_for_test(
            capability,
            RetainedFieldKey::new("target").unwrap(),
            RetainedFieldStore::new(),
            "authoritative".to_owned(),
        )
        .expect("mount retained field");

        let current_version = runtime
            .snapshot()
            .expect("read retained-field graph version")
            .version();
        let completion = runtime.execute(GraphCommand::set_node_data(
            current_version,
            field.instance().node(),
            serde_json::json!("fresh"),
        ));
        assert!(matches!(
            MutationCommandResult::from_runtime(&completion),
            MutationCommandResult::Applied { revision: 1 }
        ));

        for error in [
            field
                .validate_observation(stale_observation.stamp())
                .expect_err("reject stale observation"),
            field
                .validate_planning_authoring(&stale_planning)
                .expect_err("reject stale planning stamp"),
        ] {
            assert!(matches!(
                error,
                RetainedFieldError::StaleAuthoring {
                    authoring_revision: 0,
                    current_revision: 1,
                }
            ));
        }
        assert_eq!(field.remote(), "authoritative");
    }

    #[test]
    fn retired_and_closed_mounts_reject_matching_observations_and_release_resources() {
        for reason in [None, Some(ViewCloseReason::HostClosed)] {
            let (_runtime, view, instance) = retained_field_instance();
            let capability = test_capability(&view, instance, 2);
            let observation = observation_for_test(&capability, "authoritative".to_owned());
            let mut field = mount_capability_for_test(
                capability,
                RetainedFieldKey::new("target").unwrap(),
                RetainedFieldStore::new(),
                "authoritative".to_owned(),
            )
            .expect("mount retained field");
            field.edit("local".to_owned()).expect("protect local edit");
            match reason {
                Some(reason) => field.mount.context.mount().close(reason),
                None => field.mount.context.mount().retire(),
            }

            let result = field.validate_observation(observation.stamp());
            assert!(matches!(
                (reason, result),
                (None, Err(RetainedFieldError::Retired))
                    | (
                        Some(ViewCloseReason::HostClosed),
                        Err(RetainedFieldError::Closed {
                            reason: ViewCloseReason::HostClosed,
                        })
                    )
            ));
            assert!(field.admitted_command.is_none());
            assert_eq!(field.retention(), RetainedFieldRetention::Idle);
        }
    }

    fn mount_capability_for_test<T>(
        capability: RetainedFieldMountCapability,
        key: RetainedFieldKey,
        store: RetainedFieldStore<T>,
        authoritative: T,
    ) -> Result<RetainedField<T>, RetainedFieldError>
    where
        T: Clone + PartialEq + 'static,
    {
        let observation = OpenGpuiAuthoringStamp {
            context: capability.context.clone(),
            snapshot: capability
                .context
                .view()
                .graph_snapshot()
                .map_err(RetainedFieldError::View)?,
            activity: capability.activity.clone(),
        };
        RetainedField::mount_capability(capability, observation, key, store, authoritative)
    }

    fn test_capability(
        view: &ViewSession,
        instance: NodeInstanceKey,
        protected_capacity: usize,
    ) -> RetainedFieldMountCapability {
        let context = OpenGpuiMountContext::new(
            instance.clone(),
            view.downgrade(),
            OpenGpuiMountCapability::new(),
        )
        .expect("test mount context");
        RetainedFieldMountCapability::new(
            context,
            OpenGpuiNodeActivityRegistry::for_host(
                instance,
                OpenGpuiActivityCoordinator::new(protected_capacity),
            ),
        )
    }

    fn observation_for_test<T>(
        capability: &RetainedFieldMountCapability,
        value: T,
    ) -> OpenGpuiNodeObservation<T> {
        OpenGpuiNodeObservation::from_parts(
            OpenGpuiAuthoringStamp {
                context: capability.context.clone(),
                snapshot: capability
                    .context
                    .view()
                    .graph_snapshot()
                    .expect("test authoring snapshot"),
                activity: capability.activity.clone(),
            },
            value,
        )
    }

    fn retained_field_instance() -> (GraphRuntime, ViewSession, NodeInstanceKey) {
        let node = NodeId::from_u128(0x00a7_011c);
        let graph = GraphBuilder::new(GraphId::from_u128(0x00a7_011d))
            .with_node(
                node,
                Node {
                    kind: NodeKindKey::new("test.atomic-retained-field"),
                    kind_version: 1,
                    pos: CanvasPoint::default(),
                    origin: None,
                    selectable: None,
                    draggable: None,
                    connectable: None,
                    deletable: None,
                    parent: None,
                    extent: None,
                    expand_parent: None,
                    size: Some(CanvasSize {
                        width: 160.0,
                        height: 80.0,
                    }),
                    hidden: false,
                    collapsed: false,
                    ports: Vec::new(),
                    data: serde_json::Value::Null,
                },
            )
            .build()
            .expect("atomic retained field graph");
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("atomic retained field runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("atomic retained field view");
        let frame = view
            .frame(FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint::default(),
                    size: CanvasSize {
                        width: 400.0,
                        height: 300.0,
                    },
                },
                RendererStamp::new("test.atomic-retained-field", 1),
            ))
            .expect("atomic retained field frame");
        let instance = frame.nodes()[0].instance().clone();
        (runtime, view, instance)
    }
}
