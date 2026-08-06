//! Renderer-independent Blackboard symbols, source contract, and draft authoring.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use jellyflow_core::{Symbol, SymbolId, TypeDesc};
use jellyflow_runtime::commands::{CommandExecutionCompletion, GraphCommand, SymbolEdit};
use jellyflow_runtime::graph::{
    FactChange, GraphRuntime, GraphRuntimeEvent, GraphSnapshot, GraphVersion, RuntimeFactKey,
    RuntimePatchKind, RuntimeProvenance,
};
use jellyflow_runtime::reconciliation::{
    FieldAttempt, FieldConflictResolution, FieldEditDecision, FieldIntentRejection, FieldPhase,
    FieldReceiptDecision, FieldReconcileDecision, FieldReconciler, FieldResolutionDecision,
    FieldStaleRejectionDecision, FieldSubmissionDecision, FieldSubmissionRejectionDecision,
    MutationAttemptId, MutationBeginError, MutationCommandResult, MutationPhase,
    MutationReconciler, MutationRetryReason,
};

/// Validated product representation of one graph-scoped symbol.
#[derive(Clone, Debug, PartialEq)]
pub struct BlackboardSymbol {
    name: String,
    ty: Option<TypeDesc>,
    default_value: Option<serde_json::Value>,
    meta: serde_json::Value,
}

impl BlackboardSymbol {
    pub fn new(
        name: impl Into<String>,
        ty: Option<TypeDesc>,
        default_value: Option<serde_json::Value>,
        meta: serde_json::Value,
    ) -> Result<Self, BlackboardError> {
        let name = normalize_name(name.into())?;
        Ok(Self {
            name,
            ty,
            default_value,
            meta,
        })
    }

    fn from_graph(symbol: &Symbol) -> Result<Self, BlackboardError> {
        validate_name(&symbol.name)?;
        Ok(Self {
            name: symbol.name.clone(),
            ty: symbol.ty.clone(),
            default_value: symbol.default_value.clone(),
            meta: symbol.meta.clone(),
        })
    }

    fn with_name(&self, name: String) -> Self {
        Self {
            name,
            ty: self.ty.clone(),
            default_value: self.default_value.clone(),
            meta: self.meta.clone(),
        }
    }

    fn to_graph(&self) -> Symbol {
        Symbol {
            name: self.name.clone(),
            ty: self.ty.clone(),
            default_value: self.default_value.clone(),
            meta: self.meta.clone(),
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn ty(&self) -> Option<&TypeDesc> {
        self.ty.as_ref()
    }

    #[must_use]
    pub const fn default_value(&self) -> Option<&serde_json::Value> {
        self.default_value.as_ref()
    }

    #[must_use]
    pub const fn meta(&self) -> &serde_json::Value {
        &self.meta
    }
}

/// One explicit, renderer-independent Blackboard mutation.
#[derive(Clone, Debug, PartialEq)]
pub struct BlackboardIntent {
    kind: BlackboardIntentKind,
}

#[derive(Clone, Debug, PartialEq)]
enum BlackboardIntentKind {
    Add {
        id: SymbolId,
        symbol: BlackboardSymbol,
    },
    UpdateName {
        id: SymbolId,
        name: String,
    },
    Remove {
        id: SymbolId,
    },
}

impl BlackboardIntent {
    #[must_use]
    pub fn add(id: SymbolId, symbol: BlackboardSymbol) -> Self {
        Self {
            kind: BlackboardIntentKind::Add { id, symbol },
        }
    }

    pub fn update_name(id: SymbolId, name: impl Into<String>) -> Result<Self, BlackboardError> {
        Ok(Self {
            kind: BlackboardIntentKind::UpdateName {
                id,
                name: normalize_name(name.into())?,
            },
        })
    }

    #[must_use]
    pub const fn remove(id: SymbolId) -> Self {
        Self {
            kind: BlackboardIntentKind::Remove { id },
        }
    }

    #[must_use]
    pub const fn symbol_id(&self) -> SymbolId {
        match &self.kind {
            BlackboardIntentKind::Add { id, .. }
            | BlackboardIntentKind::UpdateName { id, .. }
            | BlackboardIntentKind::Remove { id } => *id,
        }
    }

    fn normalized_name(&self) -> Option<&str> {
        match &self.kind {
            BlackboardIntentKind::UpdateName { name, .. } => Some(name),
            BlackboardIntentKind::Add { .. } | BlackboardIntentKind::Remove { .. } => None,
        }
    }
}

/// Complete, validated Blackboard facts from one authoritative graph version.
#[derive(Clone, Debug, PartialEq)]
pub struct BlackboardObservation {
    data: Rc<BlackboardObservationData>,
}

#[derive(Debug, PartialEq)]
struct BlackboardObservationData {
    version: GraphVersion,
    symbols: BTreeMap<SymbolId, BlackboardSymbol>,
}

impl BlackboardObservation {
    pub fn from_snapshot(snapshot: &GraphSnapshot) -> Result<Self, BlackboardError> {
        let symbols = snapshot
            .graph()
            .symbols()
            .into_iter()
            .map(|(id, symbol)| Ok((*id, BlackboardSymbol::from_graph(symbol)?)))
            .collect::<Result<_, BlackboardError>>()?;
        Ok(Self {
            data: Rc::new(BlackboardObservationData {
                version: snapshot.version(),
                symbols,
            }),
        })
    }

    #[must_use]
    pub fn version(&self) -> GraphVersion {
        self.data.version
    }

    #[must_use]
    pub fn symbols(&self) -> &BTreeMap<SymbolId, BlackboardSymbol> {
        &self.data.symbols
    }

    #[must_use]
    pub fn symbol(&self, id: SymbolId) -> Option<&BlackboardSymbol> {
        self.data.symbols.get(&id)
    }

    fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.data, &other.data)
    }

    /// Plans one explicit symbol command against exactly this observation.
    pub fn plan(&self, intent: &BlackboardIntent) -> Result<GraphCommand, BlackboardError> {
        let edit = match &intent.kind {
            BlackboardIntentKind::Add { id, symbol } => {
                if self.data.symbols.contains_key(id) {
                    return Err(BlackboardError::SymbolAlreadyExists { symbol: *id });
                }
                SymbolEdit::new().add(*id, symbol.to_graph())
            }
            BlackboardIntentKind::UpdateName { id, name } => {
                let current = self
                    .data
                    .symbols
                    .get(id)
                    .ok_or(BlackboardError::MissingSymbol { symbol: *id })?;
                SymbolEdit::new().update(*id, current.with_name(name.clone()).to_graph())
            }
            BlackboardIntentKind::Remove { id } => {
                if !self.data.symbols.contains_key(id) {
                    return Err(BlackboardError::MissingSymbol { symbol: *id });
                }
                SymbolEdit::new().remove(*id)
            }
        };
        Ok(
            GraphCommand::symbols(self.data.version, edit).with_label(match &intent.kind {
                BlackboardIntentKind::Add { .. } => "add blackboard symbol",
                BlackboardIntentKind::UpdateName { .. } => "edit blackboard symbol",
                BlackboardIntentKind::Remove { .. } => "remove blackboard symbol",
            }),
        )
    }

    /// Returns whether these complete authoritative facts fulfill an intent.
    #[must_use]
    pub fn satisfies(&self, intent: &BlackboardIntent) -> bool {
        match &intent.kind {
            BlackboardIntentKind::Add { id, symbol } => self.data.symbols.get(id) == Some(symbol),
            BlackboardIntentKind::UpdateName { id, name } => self
                .data
                .symbols
                .get(id)
                .is_some_and(|symbol| symbol.name() == name),
            BlackboardIntentKind::Remove { id } => !self.data.symbols.contains_key(id),
        }
    }
}

/// Product-facing phase for one retained symbol-name draft.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlackboardDraftPhase {
    Clean,
    Edited,
    Conflict,
}

/// Submission lifecycle projected independently from the retained field value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlackboardSubmissionPhase {
    Idle,
    Submitting,
    AwaitingAuthoritative,
    Retryable,
    Uncertain,
}

/// Product reason a definitive submission may be retried.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlackboardRetryReason {
    AuthoritativeMismatch,
    RuntimeRejected,
    RuntimeConflict,
    ExecutionFailed,
    NotDispatched,
}

impl From<MutationRetryReason> for BlackboardRetryReason {
    fn from(reason: MutationRetryReason) -> Self {
        match reason {
            MutationRetryReason::AuthoritativeMismatch => Self::AuthoritativeMismatch,
            MutationRetryReason::RuntimeRejected => Self::RuntimeRejected,
            MutationRetryReason::RuntimeConflict => Self::RuntimeConflict,
            MutationRetryReason::RuntimeFailed | MutationRetryReason::NativeDispatchFailed => {
                Self::ExecutionFailed
            }
            MutationRetryReason::NotDispatched => Self::NotDispatched,
        }
    }
}

impl BlackboardDraftPhase {
    #[must_use]
    pub const fn is_dirty(self) -> bool {
        !matches!(self, Self::Clean)
    }
}

/// Exact user action that may produce the next symbol-name command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlackboardApplyAction {
    Commit,
    Reapply,
    Retry,
    ConfirmUncertainRetry,
}

/// Stable render projection that hides reducer attempts and transport stages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlackboardDraftProjection {
    value: String,
    field_phase: BlackboardDraftPhase,
    submission_phase: BlackboardSubmissionPhase,
    retry_reason: Option<BlackboardRetryReason>,
    apply_action: Option<BlackboardApplyAction>,
    can_accept_remote: bool,
    can_discard_submission: bool,
    notice: Option<String>,
}

/// Visible effect of editing one retained symbol-name field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlackboardEditTransition {
    state_changed: bool,
}

impl BlackboardEditTransition {
    #[must_use]
    pub const fn state_changed(self) -> bool {
        self.state_changed
    }
}

impl BlackboardDraftProjection {
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub const fn phase(&self) -> BlackboardDraftPhase {
        self.field_phase
    }

    #[must_use]
    pub const fn submission_phase(&self) -> BlackboardSubmissionPhase {
        self.submission_phase
    }

    #[must_use]
    pub const fn retry_reason(&self) -> Option<BlackboardRetryReason> {
        self.retry_reason
    }

    #[must_use]
    pub const fn apply_action(&self) -> Option<BlackboardApplyAction> {
        self.apply_action
    }

    #[must_use]
    pub const fn can_accept_remote(&self) -> bool {
        self.can_accept_remote
    }

    #[must_use]
    pub const fn can_discard_submission(&self) -> bool {
        self.can_discard_submission
    }

    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    #[must_use]
    pub fn into_text(self) -> (String, Option<String>) {
        (self.value, self.notice)
    }
}

struct BlackboardDraft {
    field: FieldReconciler<String>,
    mutation: MutationReconciler<BlackboardIntent>,
}

impl BlackboardDraft {
    fn new(version: GraphVersion, authoritative: String) -> Self {
        Self {
            field: FieldReconciler::new(version.revision(), authoritative),
            mutation: MutationReconciler::default(),
        }
    }

    fn field_phase(&self) -> BlackboardDraftPhase {
        match self.field.phase() {
            FieldPhase::Clean => BlackboardDraftPhase::Clean,
            FieldPhase::Conflict | FieldPhase::ComposingConflict => BlackboardDraftPhase::Conflict,
            FieldPhase::Dirty | FieldPhase::Composing => BlackboardDraftPhase::Edited,
        }
    }

    fn submission_phase(&self) -> BlackboardSubmissionPhase {
        match self.mutation.phase() {
            MutationPhase::Idle => BlackboardSubmissionPhase::Idle,
            MutationPhase::Pending => BlackboardSubmissionPhase::Submitting,
            MutationPhase::AwaitingSnapshot => BlackboardSubmissionPhase::AwaitingAuthoritative,
            MutationPhase::Retryable => BlackboardSubmissionPhase::Retryable,
            MutationPhase::Uncertain => BlackboardSubmissionPhase::Uncertain,
        }
    }

    fn retry_available(&self) -> bool {
        match self.mutation.phase() {
            MutationPhase::Retryable => self.mutation.retry(false).is_some(),
            MutationPhase::Uncertain => self.mutation.can_confirm_uncertain_retry(),
            MutationPhase::Idle | MutationPhase::Pending | MutationPhase::AwaitingSnapshot => false,
        }
    }

    fn apply_action(
        &self,
        symbol: SymbolId,
        retry_available: bool,
    ) -> Option<BlackboardApplyAction> {
        let current_intent_matches = self.mutation.active_intent().is_some_and(|intent| {
            intent.symbol_id() == symbol
                && intent
                    .normalized_name()
                    .is_some_and(|name| name == self.field.local())
        });
        match self.mutation.phase() {
            MutationPhase::Idle => match self.field.phase() {
                FieldPhase::Dirty => Some(BlackboardApplyAction::Commit),
                FieldPhase::Conflict => Some(BlackboardApplyAction::Reapply),
                FieldPhase::Clean | FieldPhase::Composing | FieldPhase::ComposingConflict => None,
            },
            MutationPhase::Retryable if current_intent_matches && retry_available => {
                Some(BlackboardApplyAction::Retry)
            }
            MutationPhase::Uncertain if current_intent_matches && retry_available => {
                Some(BlackboardApplyAction::ConfirmUncertainRetry)
            }
            MutationPhase::Pending
            | MutationPhase::AwaitingSnapshot
            | MutationPhase::Retryable
            | MutationPhase::Uncertain => None,
        }
    }

    fn is_inactive(&self) -> bool {
        self.mutation.phase() == MutationPhase::Idle
            && self.field.phase() == FieldPhase::Clean
            && self.field.pending_attempt().is_none()
    }
}

/// Domain owner for all retained Blackboard name drafts.
#[derive(Default)]
pub struct BlackboardAuthoring {
    authority: Option<RuntimeProvenance>,
    observed: Option<BlackboardObservation>,
    drafts: BTreeMap<SymbolId, BlackboardDraft>,
}

impl BlackboardAuthoring {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconciles every live draft with one complete authoritative observation.
    pub fn observe(
        &mut self,
        observation: &BlackboardObservation,
    ) -> Result<BlackboardTransition, BlackboardError> {
        self.ensure_authority(observation.version())?;
        if let Some(current) = &self.observed {
            if observation.version().revision() < current.version().revision() {
                return Ok(BlackboardTransition::ignored_stale());
            }
            if observation.version() == current.version()
                && !observation.ptr_eq(current)
                && observation != current
            {
                return Err(BlackboardError::InconsistentObservation {
                    revision: observation.version().revision(),
                });
            }
        }

        let revision = observation.version().revision();
        let mut became_satisfied = false;
        let mut retired = Vec::new();
        for (id, draft) in &mut self.drafts {
            let Some(symbol) = observation.symbol(*id) else {
                retired.push(*id);
                continue;
            };
            let was_uncertain = draft.mutation.phase() == MutationPhase::Uncertain;
            let transition = draft
                .mutation
                .observe(revision, |intent| observation.satisfies(intent));

            let field_decision = if was_uncertain
                && draft
                    .field
                    .pending_expected_revision()
                    .is_some_and(|expected| revision > expected)
            {
                if let Some(attempt) = draft.field.pending_attempt() {
                    let decision =
                        draft
                            .field
                            .reject_stale(attempt, revision, symbol.name().to_owned());
                    validate_stale_rejection(*id, decision)?;
                    None
                } else {
                    Some(
                        draft
                            .field
                            .reconcile_remote(revision, symbol.name().to_owned()),
                    )
                }
            } else {
                Some(
                    draft
                        .field
                        .reconcile_remote(revision, symbol.name().to_owned()),
                )
            };
            if let Some(decision) = field_decision {
                validate_field_reconciliation(*id, decision)?;
            }

            if transition.became_satisfied() {
                if let Some(attempt) = draft.field.pending_attempt() {
                    reject_field_submission(&mut draft.field, *id, attempt)?;
                }
                became_satisfied = true;
            }
            if draft.is_inactive() {
                retired.push(*id);
            }
        }
        for symbol in retired {
            self.drafts.remove(&symbol);
        }
        self.observed = Some(observation.clone());
        Ok(BlackboardTransition::published(became_satisfied))
    }

    pub fn edit_name(
        &mut self,
        observation: &BlackboardObservation,
        symbol: SymbolId,
        value: String,
    ) -> Result<BlackboardEditTransition, BlackboardError> {
        self.ensure_current(observation)?;
        let authoritative = observation
            .symbol(symbol)
            .ok_or(BlackboardError::MissingSymbol { symbol })?;
        let draft = self.drafts.entry(symbol).or_insert_with(|| {
            BlackboardDraft::new(observation.version(), authoritative.name().to_owned())
        });
        let state_changed = matches!(
            draft.field.edit_local(value),
            FieldEditDecision::Updated { .. }
        );
        if draft.is_inactive() {
            self.drafts.remove(&symbol);
        }
        Ok(BlackboardEditTransition { state_changed })
    }

    /// Plans one exact user-selected action without dispatching it.
    pub fn plan_name(
        &mut self,
        observation: &BlackboardObservation,
        symbol: SymbolId,
        action: BlackboardApplyAction,
    ) -> Result<BlackboardSubmission, BlackboardError> {
        self.ensure_current(observation)?;
        let draft = self
            .drafts
            .get_mut(&symbol)
            .ok_or(BlackboardError::NoDraft { symbol })?;
        let intent = BlackboardIntent::update_name(symbol, draft.field.local().clone())?;
        let name = intent
            .normalized_name()
            .ok_or(BlackboardError::InvalidFieldTransition)?;
        let command = observation.plan(&intent)?;
        if draft.apply_action(symbol, draft.retry_available()) != Some(action) {
            return Err(BlackboardError::InvalidApplyAction { symbol, action });
        }

        let mut candidate = draft.field.clone();
        candidate.edit_local(name.to_owned());
        let field_intent = if matches!(
            action,
            BlackboardApplyAction::Reapply
                | BlackboardApplyAction::Retry
                | BlackboardApplyAction::ConfirmUncertainRetry
        ) && candidate.conflict().is_some()
        {
            match candidate.resolve_at(
                FieldConflictResolution::ReapplyLocal,
                observation.version().revision(),
            ) {
                FieldResolutionDecision::Reapply(intent) => intent,
                FieldResolutionDecision::Rejected(reason) => {
                    return Err(BlackboardError::FieldIntentRejected { reason });
                }
                FieldResolutionDecision::AcceptedRemote { .. }
                | FieldResolutionDecision::KeptLocal { .. } => {
                    return Err(BlackboardError::InvalidFieldTransition);
                }
            }
        } else {
            match candidate.plan_submission_at(observation.version().revision()) {
                FieldSubmissionDecision::Submit(intent) => intent,
                FieldSubmissionDecision::Rejected(reason) => {
                    return Err(BlackboardError::FieldIntentRejected { reason });
                }
            }
        };

        let mutation_attempt = match action {
            BlackboardApplyAction::Commit | BlackboardApplyAction::Reapply => draft
                .mutation
                .begin(intent.clone(), observation.version().revision())?,
            BlackboardApplyAction::Retry | BlackboardApplyAction::ConfirmUncertainRetry => {
                let confirm_uncertain = action == BlackboardApplyAction::ConfirmUncertainRetry;
                let retry = draft
                    .mutation
                    .retry(confirm_uncertain)
                    .ok_or(BlackboardError::InvalidApplyAction { symbol, action })?;
                if retry.intent() != &intent {
                    return Err(BlackboardError::InvalidApplyAction { symbol, action });
                }
                draft
                    .mutation
                    .begin_retry(retry, observation.version().revision())?
            }
        };

        let field_attempt = field_intent.attempt();
        draft.field = candidate;
        Ok(BlackboardSubmission {
            symbol,
            field_attempt,
            mutation_attempt,
            command,
        })
    }

    /// Applies one exact source result to the draft that issued it.
    pub fn complete(
        &mut self,
        submission: BlackboardSubmission,
        outcome: BlackboardDispatchOutcome,
    ) -> Result<BlackboardTransition, BlackboardError> {
        let draft = self
            .drafts
            .get_mut(&submission.symbol)
            .ok_or(BlackboardError::NoDraft {
                symbol: submission.symbol,
            })?;
        if draft.mutation.active_attempt() != Some(submission.mutation_attempt)
            || draft.field.pending_attempt() != Some(submission.field_attempt)
        {
            return Err(BlackboardError::ForeignSubmission {
                symbol: submission.symbol,
            });
        }

        let mutation_result = match outcome {
            BlackboardDispatchOutcome::Completed(completion) => {
                if completion.identity() != submission.command.identity() {
                    return Err(BlackboardError::ForeignCompletion {
                        symbol: submission.symbol,
                    });
                }
                let mutation_result = MutationCommandResult::from_runtime(&completion);
                match &mutation_result {
                    MutationCommandResult::Applied { revision } => {
                        let mut candidate = draft.field.clone();
                        match candidate.record_self_receipt(submission.field_attempt, *revision) {
                            FieldReceiptDecision::Recorded { .. }
                            | FieldReceiptDecision::Acknowledged { .. }
                            | FieldReceiptDecision::AlreadyRecorded { .. }
                            | FieldReceiptDecision::AuthoritativeMismatch { .. }
                            | FieldReceiptDecision::Superseded { .. } => {
                                draft.field = candidate;
                            }
                            FieldReceiptDecision::UnknownAttempt { .. }
                            | FieldReceiptDecision::InvalidCommittedRevision { .. } => {
                                return Err(BlackboardError::FieldCorrelation {
                                    symbol: submission.symbol,
                                });
                            }
                        }
                    }
                    MutationCommandResult::Noop { .. }
                    | MutationCommandResult::Rejected { .. }
                    | MutationCommandResult::Conflict { .. }
                    | MutationCommandResult::Failed { .. } => {
                        reject_field_submission(
                            &mut draft.field,
                            submission.symbol,
                            submission.field_attempt,
                        )?;
                    }
                    MutationCommandResult::NativeDispatchFailed { .. }
                    | MutationCommandResult::NotDispatched { .. }
                    | MutationCommandResult::Uncertain { .. } => {
                        unreachable!("runtime completion projection cannot contain native evidence")
                    }
                }
                mutation_result
            }
            BlackboardDispatchOutcome::NotDispatched { diagnostic } => {
                reject_field_submission(
                    &mut draft.field,
                    submission.symbol,
                    submission.field_attempt,
                )?;
                MutationCommandResult::NotDispatched { diagnostic }
            }
            BlackboardDispatchOutcome::Uncertain { diagnostic } => {
                MutationCommandResult::Uncertain { diagnostic }
            }
        };
        let transition = draft
            .mutation
            .complete(submission.mutation_attempt, mutation_result)
            .map_err(|_| BlackboardError::ForeignSubmission {
                symbol: submission.symbol,
            })?;
        Ok(BlackboardTransition::published(
            transition.became_satisfied(),
        ))
    }

    pub fn accept_remote(
        &mut self,
        observation: &BlackboardObservation,
        symbol: SymbolId,
    ) -> Result<(), BlackboardError> {
        self.ensure_current(observation)?;
        let draft = self
            .drafts
            .get_mut(&symbol)
            .ok_or(BlackboardError::NoDraft { symbol })?;
        if draft.field.conflict().is_none() {
            return Err(BlackboardError::FieldIntentRejected {
                reason: FieldIntentRejection::NoConflict,
            });
        }
        match draft.mutation.phase() {
            MutationPhase::Retryable | MutationPhase::Uncertain => {
                let confirm_uncertain = draft.mutation.phase() == MutationPhase::Uncertain;
                let retry = draft
                    .mutation
                    .retry(confirm_uncertain)
                    .ok_or(BlackboardError::SubmissionBusy { symbol })?;
                draft.mutation.abandon_retry(retry);
            }
            MutationPhase::Idle => {}
            MutationPhase::Pending | MutationPhase::AwaitingSnapshot => {
                return Err(BlackboardError::SubmissionBusy { symbol });
            }
        }
        match draft.field.resolve_at(
            FieldConflictResolution::AcceptRemote,
            observation.version().revision(),
        ) {
            FieldResolutionDecision::AcceptedRemote { .. } => {}
            FieldResolutionDecision::Rejected(reason) => {
                return Err(BlackboardError::FieldIntentRejected { reason });
            }
            FieldResolutionDecision::KeptLocal { .. } | FieldResolutionDecision::Reapply(_) => {
                return Err(BlackboardError::InvalidFieldTransition);
            }
        }
        if draft.is_inactive() {
            self.drafts.remove(&symbol);
        }
        Ok(())
    }

    /// Explicitly dismisses a definitive retry or a fresh, confirmed uncertain attempt.
    pub fn discard_submission(
        &mut self,
        observation: &BlackboardObservation,
        symbol: SymbolId,
    ) -> Result<(), BlackboardError> {
        self.ensure_current(observation)?;
        let draft = self
            .drafts
            .get_mut(&symbol)
            .ok_or(BlackboardError::NoDraft { symbol })?;
        let confirm_uncertain = draft.mutation.phase() == MutationPhase::Uncertain;
        let retry = draft
            .mutation
            .retry(confirm_uncertain)
            .ok_or(BlackboardError::SubmissionBusy { symbol })?;
        if !draft.mutation.abandon_retry(retry) {
            return Err(BlackboardError::SubmissionBusy { symbol });
        }
        if draft.is_inactive() {
            self.drafts.remove(&symbol);
        }
        Ok(())
    }

    #[must_use]
    pub fn projection(
        &self,
        observation: &BlackboardObservation,
        symbol: SymbolId,
    ) -> Option<BlackboardDraftProjection> {
        observation
            .symbol(symbol)
            .map(|authoritative| project(symbol, authoritative, self.drafts.get(&symbol)))
    }

    pub fn retire_symbols(&mut self, symbols: impl IntoIterator<Item = SymbolId>) {
        for symbol in symbols {
            self.drafts.remove(&symbol);
        }
    }

    pub fn reset_lifetimes(&mut self) {
        self.drafts.clear();
    }

    fn ensure_authority(&mut self, version: GraphVersion) -> Result<(), BlackboardError> {
        match self.authority {
            Some(expected) if expected != version.provenance() => {
                Err(BlackboardError::ForeignAuthority {
                    expected,
                    actual: version.provenance(),
                })
            }
            Some(_) => Ok(()),
            None => {
                self.authority = Some(version.provenance());
                Ok(())
            }
        }
    }

    fn ensure_current(&self, observation: &BlackboardObservation) -> Result<(), BlackboardError> {
        let Some(current) = &self.observed else {
            return Err(BlackboardError::ObservationNotPublished);
        };
        if current.version() != observation.version() {
            return Err(BlackboardError::StaleObservation {
                expected: current.version(),
                actual: observation.version(),
            });
        }
        Ok(())
    }
}

fn project(
    symbol: SymbolId,
    authoritative: &BlackboardSymbol,
    draft: Option<&BlackboardDraft>,
) -> BlackboardDraftProjection {
    let Some(draft) = draft else {
        return BlackboardDraftProjection {
            value: authoritative.name().to_owned(),
            field_phase: BlackboardDraftPhase::Clean,
            submission_phase: BlackboardSubmissionPhase::Idle,
            retry_reason: None,
            apply_action: None,
            can_accept_remote: false,
            can_discard_submission: false,
            notice: None,
        };
    };
    let retry_available = draft.retry_available();
    BlackboardDraftProjection {
        value: draft.field.local().clone(),
        field_phase: draft.field_phase(),
        submission_phase: draft.submission_phase(),
        retry_reason: draft.mutation.retry_reason().map(Into::into),
        apply_action: draft.apply_action(symbol, retry_available),
        can_accept_remote: draft.field.conflict().is_some()
            && (matches!(draft.mutation.phase(), MutationPhase::Idle) || retry_available),
        can_discard_submission: retry_available,
        notice: draft.mutation.diagnostic().map(str::to_owned),
    }
}

fn reject_field_submission(
    field: &mut FieldReconciler<String>,
    symbol: SymbolId,
    attempt: FieldAttempt,
) -> Result<(), BlackboardError> {
    match field.reject_submission(attempt) {
        FieldSubmissionRejectionDecision::Rejected { .. } => Ok(()),
        FieldSubmissionRejectionDecision::UnknownAttempt { .. }
        | FieldSubmissionRejectionDecision::ReceiptAlreadyAccepted { .. } => {
            Err(BlackboardError::FieldCorrelation { symbol })
        }
    }
}

fn validate_field_reconciliation(
    symbol: SymbolId,
    decision: FieldReconcileDecision,
) -> Result<(), BlackboardError> {
    match decision {
        FieldReconcileDecision::InconsistentSnapshot { revision, .. } => {
            Err(BlackboardError::InconsistentObservation { revision })
        }
        FieldReconcileDecision::IgnoredStale { .. } => {
            Err(BlackboardError::FieldCorrelation { symbol })
        }
        FieldReconcileDecision::Unchanged { .. }
        | FieldReconcileDecision::RemoteApplied { .. }
        | FieldReconcileDecision::BaseAdvanced { .. }
        | FieldReconcileDecision::Conflict { .. }
        | FieldReconcileDecision::AuthoritativeMismatch { .. }
        | FieldReconcileDecision::Superseded { .. }
        | FieldReconcileDecision::SelfAcknowledged { .. } => Ok(()),
    }
}

fn validate_stale_rejection(
    symbol: SymbolId,
    decision: FieldStaleRejectionDecision,
) -> Result<(), BlackboardError> {
    match decision {
        FieldStaleRejectionDecision::InconsistentSnapshot { revision, .. } => {
            Err(BlackboardError::InconsistentObservation { revision })
        }
        FieldStaleRejectionDecision::IgnoredOlderRevision { .. }
        | FieldStaleRejectionDecision::UnknownAttempt { .. }
        | FieldStaleRejectionDecision::ReceiptAlreadyAccepted { .. } => {
            Err(BlackboardError::FieldCorrelation { symbol })
        }
        FieldStaleRejectionDecision::ConflictPreserved { .. }
        | FieldStaleRejectionDecision::DirtyPreserved { .. }
        | FieldStaleRejectionDecision::LocalMatchesRemote { .. } => Ok(()),
    }
}

fn validate_name(name: &str) -> Result<(), BlackboardError> {
    if name.trim().is_empty() {
        return Err(BlackboardError::EmptySymbolName);
    }
    Ok(())
}

fn normalize_name(name: String) -> Result<String, BlackboardError> {
    validate_name(&name)?;
    Ok(name.trim().to_owned())
}

/// Order-independent completion effect from observation or command evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BlackboardTransition {
    ignored_stale: bool,
    became_satisfied: bool,
}

impl BlackboardTransition {
    const fn ignored_stale() -> Self {
        Self {
            ignored_stale: true,
            became_satisfied: false,
        }
    }

    const fn published(became_satisfied: bool) -> Self {
        Self {
            ignored_stale: false,
            became_satisfied,
        }
    }

    #[must_use]
    pub const fn ignored_stale_observation(self) -> bool {
        self.ignored_stale
    }

    #[must_use]
    pub const fn became_satisfied(self) -> bool {
        self.became_satisfied
    }
}

/// One planned command and its exact field/mutation correlation evidence.
pub struct BlackboardSubmission {
    symbol: SymbolId,
    field_attempt: FieldAttempt,
    mutation_attempt: MutationAttemptId,
    command: GraphCommand,
}

impl BlackboardSubmission {
    #[must_use]
    pub const fn symbol(&self) -> SymbolId {
        self.symbol
    }

    #[must_use]
    pub const fn command(&self) -> &GraphCommand {
        &self.command
    }
}

/// Source evidence for one planned Blackboard submission.
pub enum BlackboardDispatchOutcome {
    Completed(Box<CommandExecutionCompletion>),
    NotDispatched { diagnostic: String },
    Uncertain { diagnostic: String },
}

impl BlackboardDispatchOutcome {
    #[must_use]
    pub fn completed(completion: CommandExecutionCompletion) -> Self {
        Self::Completed(Box::new(completion))
    }

    #[must_use]
    pub fn not_dispatched(diagnostic: impl Into<String>) -> Self {
        Self::NotDispatched {
            diagnostic: diagnostic.into(),
        }
    }

    #[must_use]
    pub fn uncertain(diagnostic: impl Into<String>) -> Self {
        Self::Uncertain {
            diagnostic: diagnostic.into(),
        }
    }
}

/// Typed Blackboard parse, planning, reconciliation, or correlation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlackboardError {
    EmptySymbolName,
    MissingSymbol {
        symbol: SymbolId,
    },
    SymbolAlreadyExists {
        symbol: SymbolId,
    },
    NoDraft {
        symbol: SymbolId,
    },
    ObservationNotPublished,
    StaleObservation {
        expected: GraphVersion,
        actual: GraphVersion,
    },
    ForeignAuthority {
        expected: RuntimeProvenance,
        actual: RuntimeProvenance,
    },
    InconsistentObservation {
        revision: u64,
    },
    FieldIntentRejected {
        reason: FieldIntentRejection,
    },
    InvalidApplyAction {
        symbol: SymbolId,
        action: BlackboardApplyAction,
    },
    SubmissionBusy {
        symbol: SymbolId,
    },
    ForeignSubmission {
        symbol: SymbolId,
    },
    ForeignCompletion {
        symbol: SymbolId,
    },
    FieldCorrelation {
        symbol: SymbolId,
    },
    InvalidFieldTransition,
    MutationBegin(MutationBeginError),
}

impl From<MutationBeginError> for BlackboardError {
    fn from(error: MutationBeginError) -> Self {
        Self::MutationBegin(error)
    }
}

impl fmt::Display for BlackboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySymbolName => formatter.write_str("symbol name must not be empty"),
            Self::MissingSymbol { symbol } => {
                write!(formatter, "symbol does not exist: {symbol:?}")
            }
            Self::SymbolAlreadyExists { symbol } => {
                write!(formatter, "symbol already exists: {symbol:?}")
            }
            Self::NoDraft { symbol } => write!(formatter, "symbol has no draft: {symbol:?}"),
            Self::ObservationNotPublished => {
                formatter.write_str("blackboard observation has not been published")
            }
            Self::StaleObservation { expected, actual } => write!(
                formatter,
                "blackboard observation is stale: expected {expected:?}, received {actual:?}"
            ),
            Self::ForeignAuthority { expected, actual } => write!(
                formatter,
                "blackboard observation belongs to foreign runtime {actual:?}; expected {expected:?}"
            ),
            Self::InconsistentObservation { revision } => write!(
                formatter,
                "Blackboard facts changed inside authoritative revision {revision}"
            ),
            Self::FieldIntentRejected { reason } => {
                write!(
                    formatter,
                    "blackboard field intent was rejected: {reason:?}"
                )
            }
            Self::InvalidApplyAction { symbol, action } => write!(
                formatter,
                "symbol does not allow {action:?} in its current state: {symbol:?}"
            ),
            Self::SubmissionBusy { symbol } => {
                write!(formatter, "symbol submission is still active: {symbol:?}")
            }
            Self::ForeignSubmission { symbol } => {
                write!(
                    formatter,
                    "symbol submission is no longer active: {symbol:?}"
                )
            }
            Self::ForeignCompletion { symbol } => {
                write!(
                    formatter,
                    "symbol completion belongs to another command: {symbol:?}"
                )
            }
            Self::FieldCorrelation { symbol } => {
                write!(
                    formatter,
                    "symbol field evidence did not correlate: {symbol:?}"
                )
            }
            Self::InvalidFieldTransition => {
                formatter.write_str("blackboard field entered an invalid transition")
            }
            Self::MutationBegin(error) => {
                write!(formatter, "blackboard mutation could not start: {error:?}")
            }
        }
    }
}

impl std::error::Error for BlackboardError {}

/// Application-level failure while loading or observing graph-scoped symbols.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlackboardSourceError {
    message: String,
}

impl BlackboardSourceError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for BlackboardSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for BlackboardSourceError {}

/// Result of polling one application-owned Blackboard load attempt.
#[derive(Clone, Debug)]
pub enum BlackboardSourceSnapshot {
    Loading,
    Ready(GraphSnapshot),
}

/// Lifecycle evidence delivered with one Blackboard source invalidation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum BlackboardSourceInvalidation {
    #[default]
    Refresh,
    ResetLifetimes,
    RetireSymbols(BTreeSet<SymbolId>),
}

impl BlackboardSourceInvalidation {
    #[must_use]
    pub fn refresh() -> Self {
        Self::Refresh
    }

    #[must_use]
    pub fn reset_lifetimes() -> Self {
        Self::ResetLifetimes
    }

    #[must_use]
    pub fn retire_symbols(symbols: impl IntoIterator<Item = SymbolId>) -> Self {
        Self::RetireSymbols(symbols.into_iter().collect())
    }

    pub(crate) fn apply_to(&self, authoring: &mut BlackboardAuthoring) {
        match self {
            Self::Refresh => {}
            Self::ResetLifetimes => authoring.reset_lifetimes(),
            Self::RetireSymbols(symbols) => authoring.retire_symbols(symbols.iter().copied()),
        }
    }

    pub(crate) fn merge(&mut self, newer: Self) {
        match (&mut *self, newer) {
            (Self::ResetLifetimes, _) | (Self::RetireSymbols(_), Self::Refresh) => {}
            (current @ Self::Refresh, newer) => *current = newer,
            (current @ Self::RetireSymbols(_), Self::ResetLifetimes) => {
                *current = Self::ResetLifetimes;
            }
            (Self::RetireSymbols(current), Self::RetireSymbols(newer)) => {
                current.extend(newer);
            }
        }
    }
}

pub type BlackboardSourceCallback = Rc<dyn Fn(BlackboardSourceInvalidation)>;

/// Cancels one source observation when dropped.
pub struct BlackboardSourceSubscription {
    cancel: Option<Box<dyn FnOnce()>>,
}

impl BlackboardSourceSubscription {
    #[must_use]
    pub fn new(cancel: impl FnOnce() + 'static) -> Self {
        Self {
            cancel: Some(Box::new(cancel)),
        }
    }

    pub(crate) fn cancel(&mut self) -> Result<(), BlackboardSourceError> {
        let Some(cancel) = self.cancel.take() else {
            return Ok(());
        };
        catch_unwind(AssertUnwindSafe(cancel)).map_err(|_| {
            BlackboardSourceError::new("blackboard source subscription cancellation panicked")
        })
    }
}

impl Drop for BlackboardSourceSubscription {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}

/// Application-owned boundary for Blackboard loading, observation, and commands.
pub trait BlackboardSource {
    fn subscribe(
        &self,
        callback: BlackboardSourceCallback,
    ) -> Result<BlackboardSourceSubscription, BlackboardSourceError>;

    fn snapshot(&self) -> Result<BlackboardSourceSnapshot, BlackboardSourceError>;

    fn execute(&self, command: GraphCommand) -> BlackboardDispatchOutcome;
}

/// Production source backed by one authoritative graph runtime.
pub struct GraphRuntimeBlackboardSource {
    runtime: Rc<GraphRuntime>,
}

impl GraphRuntimeBlackboardSource {
    #[must_use]
    pub fn new(runtime: Rc<GraphRuntime>) -> Self {
        Self { runtime }
    }
}

impl BlackboardSource for GraphRuntimeBlackboardSource {
    fn subscribe(
        &self,
        callback: BlackboardSourceCallback,
    ) -> Result<BlackboardSourceSubscription, BlackboardSourceError> {
        let token = self
            .runtime
            .subscribe(move |event| {
                if let Some(invalidation) = runtime_blackboard_invalidation(&event) {
                    callback(invalidation);
                }
            })
            .map_err(|error| BlackboardSourceError::new(error.to_string()))?;
        let runtime = Rc::clone(&self.runtime);
        Ok(BlackboardSourceSubscription::new(move || {
            runtime.unsubscribe(token);
        }))
    }

    fn snapshot(&self) -> Result<BlackboardSourceSnapshot, BlackboardSourceError> {
        self.runtime
            .snapshot()
            .map(BlackboardSourceSnapshot::Ready)
            .map_err(|error| BlackboardSourceError::new(error.to_string()))
    }

    fn execute(&self, command: GraphCommand) -> BlackboardDispatchOutcome {
        BlackboardDispatchOutcome::completed(self.runtime.execute(command))
    }
}

fn runtime_blackboard_invalidation(
    event: &GraphRuntimeEvent,
) -> Option<BlackboardSourceInvalidation> {
    match event {
        GraphRuntimeEvent::Patched { patch } => match patch.kind() {
            RuntimePatchKind::DocumentReset => {
                Some(BlackboardSourceInvalidation::reset_lifetimes())
            }
            RuntimePatchKind::Incremental => {
                let mut affects_symbols = false;
                let retired_symbols = patch.facts().iter().filter_map(|fact| {
                    let RuntimeFactKey::Symbol(symbol) = fact.key() else {
                        return None;
                    };
                    affects_symbols = true;
                    (fact.change() == FactChange::Removed).then_some(symbol)
                });
                let invalidation = BlackboardSourceInvalidation::retire_symbols(retired_symbols);
                affects_symbols.then_some(invalidation)
            }
        },
        GraphRuntimeEvent::HistoryChanged { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_core::{GraphBuilder, GraphId};

    use super::*;

    #[test]
    fn authoring_rejects_different_facts_for_the_same_authoritative_revision() {
        let symbol_id = SymbolId::from_u128(2);
        let runtime = GraphRuntime::builder(Default::default())
            .build(
                GraphBuilder::new(GraphId::from_u128(1))
                    .with_symbol(
                        symbol_id,
                        Symbol {
                            name: "exposure".to_owned(),
                            ty: None,
                            default_value: None,
                            meta: serde_json::Value::Null,
                        },
                    )
                    .build()
                    .expect("valid Blackboard graph"),
            )
            .expect("Blackboard runtime");
        let original =
            BlackboardObservation::from_snapshot(&runtime.snapshot().expect("Blackboard snapshot"))
                .expect("Blackboard observation");
        let mut symbols = original.symbols().clone();
        symbols.get_mut(&symbol_id).expect("symbol").name = "changed".to_owned();
        let changed = BlackboardObservation {
            data: Rc::new(BlackboardObservationData {
                version: original.version(),
                symbols,
            }),
        };
        let mut authoring = BlackboardAuthoring::new();
        authoring.observe(&original).expect("initial observation");

        assert!(matches!(
            authoring.observe(&changed),
            Err(BlackboardError::InconsistentObservation { revision })
                if revision == original.version().revision()
        ));
    }
}
