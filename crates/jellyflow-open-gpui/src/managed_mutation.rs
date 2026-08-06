use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use jellyflow_runtime::commands::GraphCommand;
use jellyflow_runtime::graph::GraphRevision;
use jellyflow_runtime::reconciliation::{
    MutationAbandonment as RuntimeMutationAbandonment, MutationAttemptId, MutationBeginError,
    MutationCommandResult, MutationCompletionError, MutationPhase, MutationReconciler,
    MutationRetry as RuntimeMutationRetry, MutationRetryReason,
};
#[cfg(test)]
use jellyflow_runtime::view::FeedbackSubmissionError;
use jellyflow_runtime::view::ViewCloseReason;
use open_gpui::Context;

use crate::activity::{
    OpenGpuiActivityAcquireError, OpenGpuiActivityKind, OpenGpuiActivityLease,
    OpenGpuiNodeActivityRegistry,
};
use crate::admitted_command::{
    AdmittedCommandDrive, AdmittedCommandEvent, AdmittedCommandTask, AdmittedCommandTaskLease,
    AdmittedCommandUncertainty,
};
use crate::authoring::{
    OpenGpuiCommandAdmissionError, OpenGpuiNodeAuthoring, OpenGpuiNodeAuthoringError,
};
use crate::binding::{OpenGpuiLayoutInvalidator, OpenGpuiMountContext, OpenGpuiMountState};
use crate::command_journal::OpenGpuiCommandTicketDisposition;
#[cfg(test)]
use crate::command_journal::{OpenGpuiCommandTicketError, OpenGpuiCommandTicketFailure};
use crate::panic_sink::OpenGpuiPanicAccumulator;

/// Origin of a managed mutation diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationDiagnosticSource {
    AuthoritativeObservation,
    Runtime,
    NativeAdmission,
    NativeDispatch,
    NativeTransport,
    NativeActivity,
    NativeLayout,
    NativeLifecycle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MutationDiagnosticKind {
    Standard,
    NativeActivityBackpressure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MutationDiagnosticProjection {
    source: MutationDiagnosticSource,
    kind: MutationDiagnosticKind,
}

impl MutationDiagnosticProjection {
    const fn standard(source: MutationDiagnosticSource) -> Self {
        Self {
            source,
            kind: MutationDiagnosticKind::Standard,
        }
    }
}

/// Structured diagnostic projected without exposing native command transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MutationDiagnostic {
    source: MutationDiagnosticSource,
    kind: MutationDiagnosticKind,
    message: String,
}

impl MutationDiagnostic {
    fn new(source: MutationDiagnosticSource, message: impl Into<String>) -> Self {
        Self {
            source,
            kind: MutationDiagnosticKind::Standard,
            message: message.into(),
        }
    }

    fn from_projection(
        projection: MutationDiagnosticProjection,
        message: impl Into<String>,
    ) -> Self {
        Self {
            source: projection.source,
            kind: projection.kind,
            message: message.into(),
        }
    }

    fn native_activity(error: OpenGpuiActivityAcquireError) -> Self {
        let kind = match error {
            OpenGpuiActivityAcquireError::ProtectedCapacityExceeded { .. } => {
                MutationDiagnosticKind::NativeActivityBackpressure
            }
            OpenGpuiActivityAcquireError::RegistryRevoked
            | OpenGpuiActivityAcquireError::IdentityExhausted => MutationDiagnosticKind::Standard,
        };
        Self {
            source: MutationDiagnosticSource::NativeActivity,
            kind,
            message: error.to_string(),
        }
    }

    const fn projection(&self) -> MutationDiagnosticProjection {
        MutationDiagnosticProjection {
            source: self.source,
            kind: self.kind,
        }
    }

    pub(crate) const fn is_native_activity_backpressure(&self) -> bool {
        matches!(
            self.kind,
            MutationDiagnosticKind::NativeActivityBackpressure
        )
    }

    #[must_use]
    pub const fn source(&self) -> MutationDiagnosticSource {
        self.source
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for MutationDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// Native layout action requested for one explicit mutation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MutationLayoutEffect {
    #[default]
    Preserve,
    InvalidateWhenSatisfied,
}

/// Result of the single layout action attached to a satisfied transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MutationLayoutSettlement {
    NotRequested,
    Invalidated,
    NoLongerMounted,
    Failed { diagnostic: MutationDiagnostic },
}

/// Small public projection of the complete managed mutation lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MutationSettlement {
    Ready,
    Pending {
        awaiting_authority: bool,
    },
    Retryable {
        reason: MutationRetryReason,
        diagnostic: MutationDiagnostic,
    },
    Uncertain {
        diagnostic: MutationDiagnostic,
        retry_confirmable: bool,
    },
    Satisfied {
        layout: MutationLayoutSettlement,
    },
    Retired {
        diagnostic: MutationDiagnostic,
    },
    Closed {
        reason: ViewCloseReason,
        diagnostic: MutationDiagnostic,
    },
}

impl MutationSettlement {
    /// Returns the application-facing diagnostic carried by this settlement, when present.
    ///
    /// Native admission, runtime settlement, retirement, close, and layout failure remain owned by
    /// the managed mutation. Product renderers can project the diagnostic without reproducing the
    /// lifecycle variant map.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&MutationDiagnostic> {
        match self {
            Self::Retryable { diagnostic, .. }
            | Self::Uncertain { diagnostic, .. }
            | Self::Retired { diagnostic }
            | Self::Closed { diagnostic, .. }
            | Self::Satisfied {
                layout: MutationLayoutSettlement::Failed { diagnostic },
            } => Some(diagnostic),
            Self::Ready | Self::Pending { .. } | Self::Satisfied { .. } => None,
        }
    }
}

/// Read-only capability for an application-controlled retry.
#[derive(Debug)]
pub struct MutationRetry<I>(RuntimeMutationRetry<I>);

impl<I> MutationRetry<I> {
    #[must_use]
    pub const fn intent(&self) -> &I {
        self.0.intent()
    }
}

/// Attempt-bound capability for discarding definitive completion without product facts.
#[derive(Debug)]
pub struct MutationAbandonment(RuntimeMutationAbandonment);

/// One new product intent or an explicit retry of the current intent.
#[derive(Debug)]
pub enum MutationRequest<I> {
    New(I),
    Retry(MutationRetry<I>),
}

impl<I> MutationRequest<I> {
    #[must_use]
    pub const fn new(intent: I) -> Self {
        Self::New(intent)
    }

    #[must_use]
    pub const fn retry(retry: MutationRetry<I>) -> Self {
        Self::Retry(retry)
    }

    #[must_use]
    pub const fn intent(&self) -> &I {
        match self {
            Self::New(intent) => intent,
            Self::Retry(retry) => retry.intent(),
        }
    }
}

/// Failure before a managed command can become a pending native task.
#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum MutationDispatchError {
    #[error("another mutation is already active")]
    Busy,
    #[error("an admitted mutation transport remains live and must settle before retry")]
    AdmittedTransportLive,
    #[error("the mutation retry capability is no longer current")]
    RetryExpired,
    #[error(
        "mutation retry requires revision {required_revision} or newer, received {received_revision}"
    )]
    StaleRetryBase {
        required_revision: GraphRevision,
        received_revision: GraphRevision,
    },
    #[error("mutation attempt identity space is exhausted")]
    IdentityExhausted,
    #[error("the mutation owner is retired")]
    Retired,
    #[error("the mutation owner is closed: {reason:?}")]
    Closed { reason: ViewCloseReason },
    #[error("mutation was not dispatched: {diagnostic}")]
    NotDispatched { diagnostic: MutationDiagnostic },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MutationTerminalSignal {
    Retired,
    Closed(ViewCloseReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManagedMutationRetirementPolicy {
    Terminal,
    DrainAdmittedWithSurvivors,
}

enum MutationTerminal<I> {
    Retired {
        intent: Option<I>,
        diagnostic: MutationDiagnostic,
    },
    Closed {
        reason: ViewCloseReason,
        intent: Option<I>,
        diagnostic: MutationDiagnostic,
    },
}

impl<I> MutationTerminal<I> {
    fn intent(&self) -> Option<&I> {
        match self {
            Self::Retired { intent, .. } | Self::Closed { intent, .. } => intent.as_ref(),
        }
    }

    fn settlement(&self) -> MutationSettlement {
        match self {
            Self::Retired { diagnostic, .. } => MutationSettlement::Retired {
                diagnostic: diagnostic.clone(),
            },
            Self::Closed {
                reason, diagnostic, ..
            } => MutationSettlement::Closed {
                reason: *reason,
                diagnostic: diagnostic.clone(),
            },
        }
    }
}

/// Owns native admission and asynchronous resources for an exact retained mount owner.
///
/// The application still owns validation, command construction, satisfaction predicates, and
/// every retry decision. Public bindings create a single-mount owner; adapter internals may bind
/// one atomic command to an exact mount set. This owner never rewrites or automatically replays a
/// command.
pub struct OpenGpuiManagedMutation<I> {
    owners: Vec<OpenGpuiManagedMutationOwner>,
    retirement_policy: ManagedMutationRetirementPolicy,
    admitted_attempt: Option<MutationAttemptId>,
    reconciler: MutationReconciler<I>,
    terminal: Option<MutationTerminal<I>>,
    task: Rc<RefCell<Option<AdmittedCommandTask<MutationAttemptId>>>>,
    recoverable_admitted_attempt: Option<MutationAttemptId>,
    retention_leases: Vec<OpenGpuiActivityLease>,
    layout_effect: Option<(MutationAttemptId, MutationLayoutEffect)>,
    layout_failure: Option<MutationDiagnostic>,
    diagnostic_projection: Option<(MutationAttemptId, MutationDiagnosticProjection)>,
    #[cfg(test)]
    post_admission_fault: bool,
}

#[derive(Clone)]
pub(crate) struct OpenGpuiManagedMutationOwner {
    mount: OpenGpuiMountContext,
    activity: OpenGpuiNodeActivityRegistry,
    layout_invalidator: OpenGpuiLayoutInvalidator,
}

impl OpenGpuiManagedMutationOwner {
    pub(crate) fn new(
        mount: OpenGpuiMountContext,
        activity: OpenGpuiNodeActivityRegistry,
        layout_invalidator: OpenGpuiLayoutInvalidator,
    ) -> Self {
        Self {
            mount,
            activity,
            layout_invalidator,
        }
    }

    #[cfg(test)]
    pub(crate) fn retire_mount_for_test(&self) {
        self.mount.mount().retire();
    }

    #[cfg(test)]
    pub(crate) fn close_mount_for_test(&self, reason: ViewCloseReason) {
        self.mount.mount().close(reason);
    }
}

impl<I> Drop for OpenGpuiManagedMutation<I> {
    fn drop(&mut self) {
        AdmittedCommandTask::shutdown(&mut self.task.borrow_mut());
        self.retention_leases.clear();
    }
}

impl<I> std::fmt::Debug for OpenGpuiManagedMutation<I>
where
    I: std::fmt::Debug + Clone + 'static,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenGpuiManagedMutation")
            .field(
                "instances",
                &self
                    .owners
                    .iter()
                    .map(|owner| owner.mount.instance())
                    .collect::<Vec<_>>(),
            )
            .field("settlement", &self.current_settlement())
            .field("active_intent", &self.active_intent())
            .finish_non_exhaustive()
    }
}

impl<I> OpenGpuiManagedMutation<I> {
    pub(crate) fn mount(
        mount: OpenGpuiMountContext,
        activity: OpenGpuiNodeActivityRegistry,
        layout_invalidator: OpenGpuiLayoutInvalidator,
    ) -> Self {
        Self::mount_set(vec![OpenGpuiManagedMutationOwner::new(
            mount,
            activity,
            layout_invalidator,
        )])
        .expect("one exact mount always forms a valid mutation owner set")
    }

    pub(crate) fn mount_set(owners: Vec<OpenGpuiManagedMutationOwner>) -> Option<Self> {
        Self::mount_set_with_retirement(owners, ManagedMutationRetirementPolicy::Terminal)
    }

    pub(crate) fn mount_set_draining_admitted(
        owners: Vec<OpenGpuiManagedMutationOwner>,
    ) -> Option<Self> {
        Self::mount_set_with_retirement(
            owners,
            ManagedMutationRetirementPolicy::DrainAdmittedWithSurvivors,
        )
    }

    fn mount_set_with_retirement(
        owners: Vec<OpenGpuiManagedMutationOwner>,
        retirement_policy: ManagedMutationRetirementPolicy,
    ) -> Option<Self> {
        let primary = owners.first()?;
        let provenance = primary.mount.provenance();
        let mut instances = std::collections::BTreeSet::new();
        if owners.iter().any(|owner| {
            owner.mount.provenance() != provenance
                || !instances.insert(owner.mount.instance().clone())
        }) {
            return None;
        }
        Some(Self {
            owners,
            retirement_policy,
            admitted_attempt: None,
            reconciler: MutationReconciler::default(),
            terminal: None,
            task: Rc::new(RefCell::new(None)),
            recoverable_admitted_attempt: None,
            retention_leases: Vec::new(),
            layout_effect: None,
            layout_failure: None,
            diagnostic_projection: None,
            #[cfg(test)]
            post_admission_fault: false,
        })
    }

    #[cfg(test)]
    pub(crate) fn replace_layout_invalidator_for_test(
        &mut self,
        layout_invalidator: OpenGpuiLayoutInvalidator,
    ) {
        self.owners
            .first_mut()
            .expect("managed mutations always own at least one mount")
            .layout_invalidator = layout_invalidator;
    }

    #[cfg(test)]
    pub(crate) fn replace_activity_for_test(
        &mut self,
        activity: OpenGpuiNodeActivityRegistry,
    ) -> OpenGpuiNodeActivityRegistry {
        let primary = self
            .owners
            .first_mut()
            .expect("managed mutations always own at least one mount");
        std::mem::replace(&mut primary.activity, activity)
    }

    #[cfg(test)]
    pub(crate) fn inject_post_admission_fault_for_test(&mut self) {
        self.post_admission_fault = true;
    }
}

impl<I> OpenGpuiManagedMutation<I>
where
    I: Clone + 'static,
{
    #[must_use]
    pub fn active_intent(&self) -> Option<&I> {
        self.terminal
            .as_ref()
            .and_then(MutationTerminal::intent)
            .or_else(|| self.reconciler.active_intent())
    }

    #[must_use]
    pub fn retry(&self) -> Option<MutationRetry<I>> {
        (self.is_live() && !self.has_live_admitted_transport())
            .then(|| self.reconciler.retry(false))
            .flatten()
            .map(MutationRetry)
    }

    #[must_use]
    pub fn confirm_uncertain_retry(&self) -> Option<MutationRetry<I>> {
        (self.is_live() && !self.has_live_admitted_transport())
            .then(|| self.reconciler.retry(true))
            .flatten()
            .map(MutationRetry)
    }

    #[must_use]
    pub fn can_confirm_uncertain_retry(&self) -> bool {
        self.is_live()
            && !self.has_live_admitted_transport()
            && self.reconciler.can_confirm_uncertain_retry()
    }

    /// Returns the last layout failure until another explicit mutation starts.
    #[must_use]
    pub fn layout_failure(&self) -> Option<&MutationDiagnostic> {
        self.layout_failure.as_ref()
    }

    /// Rejects a product request before dispatch because current authoritative facts cannot plan it.
    ///
    /// A new request owns no retained runtime capability and is simply consumed. Rejecting an exact
    /// retry abandons that capability, releases native mutation retention immediately, and leaves
    /// the owner ready for another intent. Pending commands cannot be abandoned through this API.
    pub fn reject_request(&mut self, request: MutationRequest<I>) -> bool {
        match request {
            MutationRequest::New(_) => true,
            MutationRequest::Retry(retry) => self.abandon_retry(retry),
        }
    }

    fn abandon_retry(&mut self, retry: MutationRetry<I>) -> bool {
        self.synchronize_terminal();
        if !self.is_live()
            || self.has_live_admitted_transport()
            || !self.reconciler.abandon_retry(retry.0)
        {
            return false;
        }
        self.synchronize_resources();
        true
    }

    /// Drains native completion while product facts are temporarily unavailable.
    ///
    /// This does not invent a satisfaction result. Applied or noop completion remains awaiting an
    /// authoritative product observation; rejection and uncertainty retain their normal phase.
    pub fn observe_unavailable<N: 'static>(
        &mut self,
        authoring: &OpenGpuiNodeAuthoring,
        cx: &mut Context<N>,
    ) -> MutationSettlement {
        self.synchronize_terminal();
        if self.terminal.is_some() {
            return self.current_settlement();
        }
        if !self.same_owner(authoring) {
            self.retire("mutation observation belongs to another native mount".to_owned());
            return self.current_settlement();
        }
        self.observe_without_authority(cx)
    }

    /// Drains one admitted completion when no member of an exact mount set can currently provide
    /// an authoritative snapshot.
    ///
    /// A member-local authoring failure cannot classify the whole mount set. Terminal owner signals
    /// still win, while applied or noop completion remains awaiting a later authoritative snapshot.
    pub(crate) fn observe_without_authority<N: 'static>(
        &mut self,
        cx: &mut Context<N>,
    ) -> MutationSettlement {
        self.synchronize_terminal();
        if self.terminal.is_some() {
            return self.current_settlement();
        }
        if let Some(settlement) = self.drain_task_completion(cx) {
            return settlement;
        }
        self.synchronize_resources();
        self.current_settlement()
    }

    /// Returns an exact capability when definitive completion cannot be observed by the product.
    #[must_use]
    pub fn unobservable_abandonment(&self) -> Option<MutationAbandonment> {
        if !self.is_live() || self.task.borrow().is_some() {
            return None;
        }
        self.reconciler
            .unobservable_abandonment()
            .map(MutationAbandonment)
    }

    /// Explicitly abandons a completed attempt that cannot be reconciled with product facts.
    ///
    /// Pending and uncertain commands never mint this capability. Exact attempt matching prevents
    /// a stale UI action from discarding a successor mutation.
    pub fn abandon_unobservable(&mut self, abandonment: MutationAbandonment) -> bool {
        self.synchronize_terminal();
        if !self.is_live()
            || self.task.borrow().is_some()
            || !self.reconciler.abandon_unobservable(abandonment.0)
        {
            return false;
        }
        self.synchronize_resources();
        true
    }

    /// Returns the current projection, including asynchronous mount revocation signals.
    #[must_use]
    pub fn settlement(&self) -> MutationSettlement {
        self.current_settlement()
    }

    /// Admits one explicit command and retains all native task resources internally.
    pub fn dispatch<N: 'static>(
        &mut self,
        authoring: OpenGpuiNodeAuthoring,
        command: GraphCommand,
        request: MutationRequest<I>,
        layout_effect: MutationLayoutEffect,
        cx: &mut Context<N>,
    ) -> Result<MutationSettlement, MutationDispatchError> {
        self.dispatch_with_admission(
            authoring,
            command,
            request,
            layout_effect,
            OpenGpuiNodeAuthoring::admit,
            cx,
        )
    }

    pub(crate) fn dispatch_with_admission<N: 'static>(
        &mut self,
        authoring: OpenGpuiNodeAuthoring,
        command: GraphCommand,
        request: MutationRequest<I>,
        layout_effect: MutationLayoutEffect,
        admit: impl FnOnce(
            OpenGpuiNodeAuthoring,
            GraphCommand,
        ) -> Result<
            crate::command_journal::OpenGpuiAdmittedCommand,
            OpenGpuiCommandAdmissionError,
        >,
        cx: &mut Context<N>,
    ) -> Result<MutationSettlement, MutationDispatchError> {
        self.synchronize_terminal();
        self.ensure_dispatchable()?;
        if self.has_live_admitted_transport() {
            return Err(MutationDispatchError::AdmittedTransportLive);
        }
        if !self.same_owner(&authoring) {
            self.retire("mutation authoring belongs to another native mount".to_owned());
            return Err(MutationDispatchError::Retired);
        }
        if self.owners.len() > 1 && layout_effect != MutationLayoutEffect::Preserve {
            return Err(MutationDispatchError::NotDispatched {
                diagnostic: MutationDiagnostic::new(
                    MutationDiagnosticSource::NativeLayout,
                    "exact mount-set mutations cannot invalidate one member independently",
                ),
            });
        }

        let base_revision = authoring.version().revision();
        let attempt = match request {
            MutationRequest::New(intent) => self.reconciler.begin(intent, base_revision),
            MutationRequest::Retry(retry) => self.reconciler.begin_retry(retry.0, base_revision),
        };
        let attempt = match attempt {
            Ok(attempt) => attempt,
            Err(error) => {
                if self.reconciler.phase() == MutationPhase::Idle {
                    self.retention_leases.clear();
                }
                return Err(map_begin_error(error));
            }
        };
        self.layout_failure = None;
        self.layout_effect = Some((attempt, layout_effect));
        self.diagnostic_projection = None;

        if let Err(diagnostic) = self.ensure_retention() {
            self.synchronize_terminal();
            self.ensure_dispatchable()?;
            return Err(self.fail_not_dispatched(attempt, diagnostic));
        }

        let task_lease = match AdmittedCommandTaskLease::acquire(&self.primary_owner().activity) {
            Ok(lease) => lease,
            Err(error) => {
                let diagnostic = MutationDiagnostic::native_activity(error);
                return Err(self.fail_not_dispatched(attempt, diagnostic));
            }
        };
        let admitted = match admit(authoring, command) {
            Ok(admitted) => admitted,
            Err(error) => return Err(self.handle_admission_failure(attempt, error)),
        };
        let (ticket, notification) = admitted.into_parts();
        self.admitted_attempt = Some(attempt);
        self.recoverable_admitted_attempt = None;

        *self.task.borrow_mut() = Some(AdmittedCommandTask::new(
            attempt,
            ticket,
            notification,
            task_lease,
        ));
        #[cfg(test)]
        if std::mem::take(&mut self.post_admission_fault) {
            let mut task = self.task.borrow_mut();
            let task = task
                .as_mut()
                .expect("an admitted mutation owns its task before setup");
            task.inject_setup_fault_for_test();
        }
        let drive = self
            .task
            .borrow_mut()
            .as_mut()
            .expect("an admitted mutation owns its task before setup")
            .drive(cx);
        match drive {
            Ok(AdmittedCommandDrive::AwaitingTicket) => {}
            Ok(AdmittedCommandDrive::Unavailable) => {
                self.record_post_admission_uncertainty(
                    attempt,
                    AdmittedCommandUncertainty::TaskInstallationUnwound,
                );
            }
            Err(unwind) => {
                self.record_post_admission_uncertainty(attempt, unwind.reason());
                unwind.resume();
            }
        }
        self.synchronize_terminal();
        Ok(self.current_settlement())
    }

    /// Reconciles the latest authoritative facts and drains one native completion if available.
    pub fn observe<N: 'static>(
        &mut self,
        authoring: &OpenGpuiNodeAuthoring,
        satisfied: impl FnOnce(&I) -> bool,
        cx: &mut Context<N>,
    ) -> MutationSettlement {
        self.synchronize_terminal();
        if self.terminal.is_some() {
            return self.current_settlement();
        }
        if !self.same_owner(authoring) {
            self.retire("mutation observation belongs to another native mount".to_owned());
            return self.current_settlement();
        }

        let completion = self.drain_task_completion(cx);
        if matches!(completion, Some(MutationSettlement::Satisfied { .. })) {
            return completion.expect("a satisfied completion was matched");
        }
        let attempt = self.reconciler.active_attempt();
        let transition = self
            .reconciler
            .observe(authoring.version().revision(), satisfied);
        if transition.became_satisfied() {
            return self.finish_satisfied(attempt, cx);
        }
        self.synchronize_resources();
        self.current_settlement()
    }

    /// Applies a terminal authoring failure when no current authoring view can be opened.
    pub fn observe_authoring_error(
        &mut self,
        error: &OpenGpuiNodeAuthoringError,
    ) -> MutationSettlement {
        self.synchronize_terminal();
        if self.terminal.is_some() {
            return self.current_settlement();
        }
        match error {
            OpenGpuiNodeAuthoringError::RetiredMount { .. }
            | OpenGpuiNodeAuthoringError::RetiredIncarnation { .. }
            | OpenGpuiNodeAuthoringError::MissingNode { .. } => self.retire(error.to_string()),
            OpenGpuiNodeAuthoringError::Closed { reason } => self.close(*reason, error.to_string()),
            _ => self.synchronize_terminal(),
        }
        self.current_settlement()
    }

    fn current_settlement(&self) -> MutationSettlement {
        if let Some(terminal) = &self.terminal {
            return terminal.settlement();
        }
        let lifecycle = self.pending_terminal_signal();
        if let Some(lifecycle) = lifecycle {
            let message = match lifecycle {
                MutationTerminalSignal::Retired => "native mutation owner was retired".to_owned(),
                MutationTerminalSignal::Closed(reason) => {
                    format!("native mutation owner closed: {reason:?}")
                }
            };
            let diagnostic =
                MutationDiagnostic::new(MutationDiagnosticSource::NativeLifecycle, message);
            return match lifecycle {
                MutationTerminalSignal::Retired => MutationSettlement::Retired { diagnostic },
                MutationTerminalSignal::Closed(reason) => {
                    MutationSettlement::Closed { reason, diagnostic }
                }
            };
        }
        match self.reconciler.phase() {
            MutationPhase::Idle => MutationSettlement::Ready,
            MutationPhase::Pending => MutationSettlement::Pending {
                awaiting_authority: false,
            },
            MutationPhase::AwaitingSnapshot => MutationSettlement::Pending {
                awaiting_authority: true,
            },
            MutationPhase::Retryable => {
                let reason = self
                    .reconciler
                    .retry_reason()
                    .expect("retryable mutation has one reason");
                MutationSettlement::Retryable {
                    reason,
                    diagnostic: MutationDiagnostic::from_projection(
                        self.reconciler
                            .active_attempt()
                            .and_then(|attempt| {
                                self.diagnostic_projection
                                    .filter(|(source_attempt, _)| *source_attempt == attempt)
                                    .map(|(_, projection)| projection)
                            })
                            .unwrap_or_else(|| {
                                MutationDiagnosticProjection::standard(diagnostic_source_for_retry(
                                    reason,
                                ))
                            }),
                        self.reconciler
                            .diagnostic()
                            .expect("retryable mutation has one diagnostic"),
                    ),
                }
            }
            MutationPhase::Uncertain => MutationSettlement::Uncertain {
                diagnostic: MutationDiagnostic::from_projection(
                    self.reconciler
                        .active_attempt()
                        .and_then(|attempt| {
                            self.diagnostic_projection
                                .filter(|(source_attempt, _)| *source_attempt == attempt)
                                .map(|(_, projection)| projection)
                        })
                        .unwrap_or_else(|| {
                            MutationDiagnosticProjection::standard(
                                MutationDiagnosticSource::NativeTransport,
                            )
                        }),
                    self.reconciler
                        .diagnostic()
                        .expect("uncertain mutation has one diagnostic"),
                ),
                retry_confirmable: self.can_confirm_uncertain_retry(),
            },
        }
    }

    fn has_live_admitted_transport(&self) -> bool {
        self.task.borrow().is_some()
    }

    fn ensure_dispatchable(&self) -> Result<(), MutationDispatchError> {
        match &self.terminal {
            Some(MutationTerminal::Retired { .. }) => Err(MutationDispatchError::Retired),
            Some(MutationTerminal::Closed { reason, .. }) => {
                Err(MutationDispatchError::Closed { reason: *reason })
            }
            None => Ok(()),
        }
    }

    fn is_live(&self) -> bool {
        self.terminal.is_none() && self.pending_terminal_signal().is_none()
    }

    fn pending_terminal_signal(&self) -> Option<MutationTerminalSignal> {
        self.owner_terminal_signal()
    }

    fn ensure_retention(&mut self) -> Result<(), MutationDiagnostic> {
        if self.retention_leases.len() == self.owners.len() {
            return Ok(());
        }
        self.retention_leases.clear();
        let mut leases = Vec::with_capacity(self.owners.len());
        for owner in &self.owners {
            let lease = if self.retirement_policy == ManagedMutationRetirementPolicy::Terminal {
                let task = Rc::downgrade(&self.task);
                owner
                    .activity
                    .acquire_with_revocation(OpenGpuiActivityKind::Mutation, move || {
                        if let Some(task) = task.upgrade()
                            && let Ok(mut task) = task.try_borrow_mut()
                        {
                            AdmittedCommandTask::shutdown(&mut task);
                        }
                    })
            } else {
                owner.activity.acquire(OpenGpuiActivityKind::Mutation)
            }
            .map_err(MutationDiagnostic::native_activity)?;
            leases.push(lease);
        }
        self.retention_leases = leases;
        self.synchronize_terminal();
        self.ensure_dispatchable().map_err(|error| {
            MutationDiagnostic::new(MutationDiagnosticSource::NativeLifecycle, error.to_string())
        })
    }

    fn same_owner(&self, authoring: &OpenGpuiNodeAuthoring) -> bool {
        let stamp = authoring.stamp();
        let matches = |owner: &OpenGpuiManagedMutationOwner| {
            stamp.context.instance() == owner.mount.instance()
                && stamp.context.mount().is_same_mount(owner.mount.mount())
                && stamp.context.provenance() == owner.mount.provenance()
        };
        match self.retirement_policy {
            ManagedMutationRetirementPolicy::Terminal => matches(self.primary_owner()),
            ManagedMutationRetirementPolicy::DrainAdmittedWithSurvivors => {
                self.owners.iter().any(matches)
            }
        }
    }

    fn handle_admission_failure(
        &mut self,
        attempt: MutationAttemptId,
        error: OpenGpuiCommandAdmissionError,
    ) -> MutationDispatchError {
        match error {
            OpenGpuiCommandAdmissionError::Authoring(error) => match error {
                OpenGpuiNodeAuthoringError::RetiredMount { .. }
                | OpenGpuiNodeAuthoringError::RetiredIncarnation { .. }
                | OpenGpuiNodeAuthoringError::MissingNode { .. } => {
                    self.retire(error.to_string());
                    MutationDispatchError::Retired
                }
                OpenGpuiNodeAuthoringError::Closed { reason } => {
                    self.close(reason, error.to_string());
                    MutationDispatchError::Closed { reason }
                }
                error => self.fail_not_dispatched(
                    attempt,
                    MutationDiagnostic::new(
                        MutationDiagnosticSource::NativeAdmission,
                        error.to_string(),
                    ),
                ),
            },
            OpenGpuiCommandAdmissionError::Journal(error) => self.fail_not_dispatched(
                attempt,
                MutationDiagnostic::new(
                    MutationDiagnosticSource::NativeAdmission,
                    error.to_string(),
                ),
            ),
        }
    }

    fn fail_not_dispatched(
        &mut self,
        attempt: MutationAttemptId,
        diagnostic: MutationDiagnostic,
    ) -> MutationDispatchError {
        self.diagnostic_projection = Some((attempt, diagnostic.projection()));
        let _ = self.reconciler.complete(
            attempt,
            MutationCommandResult::NotDispatched {
                diagnostic: diagnostic.message.clone(),
            },
        );
        self.synchronize_resources();
        MutationDispatchError::NotDispatched { diagnostic }
    }

    fn record_post_admission_uncertainty(
        &mut self,
        attempt: MutationAttemptId,
        reason: AdmittedCommandUncertainty,
    ) {
        let owns_transport = self
            .task
            .borrow()
            .as_ref()
            .is_some_and(|task| task.attempt() == attempt);
        self.recoverable_admitted_attempt = owns_transport.then_some(attempt);
        self.diagnostic_projection = Some((
            attempt,
            MutationDiagnosticProjection::standard(diagnostic_source_for_uncertainty(reason)),
        ));
        let _ = self.reconciler.complete(
            attempt,
            MutationCommandResult::Uncertain {
                diagnostic: uncertainty_diagnostic(reason),
            },
        );
        self.synchronize_resources();
    }

    fn drain_task_completion<N: 'static>(
        &mut self,
        cx: &mut Context<N>,
    ) -> Option<MutationSettlement> {
        self.resume_recoverable_admitted_task(cx);
        let event = self
            .task
            .borrow()
            .as_ref()
            .and_then(AdmittedCommandTask::take_event_guard)?;
        self.synchronize_terminal();
        if self.terminal.is_some() {
            return Some(self.current_settlement());
        }
        let attempt = event.event().attempt();
        let recoverable_setup = matches!(
            event.event(),
            AdmittedCommandEvent::Uncertain {
                reason: AdmittedCommandUncertainty::TaskInstallationUnwound
                    | AdmittedCommandUncertainty::HostNotificationUnwound,
                ..
            }
        );
        if recoverable_setup {
            self.recoverable_admitted_attempt = Some(attempt);
        } else if self.recoverable_admitted_attempt == Some(attempt) {
            self.recoverable_admitted_attempt = None;
        }
        let diagnostic_projection = match event.event() {
            AdmittedCommandEvent::Uncertain { reason, .. } => Some(
                MutationDiagnosticProjection::standard(diagnostic_source_for_uncertainty(*reason)),
            ),
            AdmittedCommandEvent::Settled { .. } => None,
        };
        let completion = match event.event() {
            AdmittedCommandEvent::Settled { result, .. } => command_ticket_result(result),
            AdmittedCommandEvent::Uncertain { reason, .. } => MutationCommandResult::Uncertain {
                diagnostic: uncertainty_diagnostic(*reason),
            },
        };
        self.diagnostic_projection = diagnostic_projection.map(|projection| (attempt, projection));
        let active_attempt = self.reconciler.active_attempt();
        let transition = self.reconciler.complete(attempt, completion);
        let transition = match transition {
            Ok(transition) => transition,
            Err(error @ MutationCompletionError::InvalidRevision { .. }) => {
                self.recoverable_admitted_attempt = None;
                self.diagnostic_projection = Some((
                    attempt,
                    MutationDiagnosticProjection::standard(MutationDiagnosticSource::Runtime),
                ));
                let diagnostic = format!(
                    "mutation completion violated the runtime revision contract: {error:?}"
                );
                let transition = self
                    .reconciler
                    .complete(attempt, MutationCommandResult::Uncertain { diagnostic });
                let Ok(transition) = transition else {
                    self.synchronize_resources();
                    return Some(self.current_settlement());
                };
                transition
            }
            Err(
                MutationCompletionError::NotPending { .. }
                | MutationCompletionError::ForeignAttempt { .. },
            ) => {
                self.recoverable_admitted_attempt = None;
                event.commit();
                AdmittedCommandTask::release_if(&mut self.task.borrow_mut(), attempt);
                self.synchronize_resources();
                return Some(self.current_settlement());
            }
        };
        event.commit();
        if !recoverable_setup {
            AdmittedCommandTask::release_if(&mut self.task.borrow_mut(), attempt);
        }
        if transition.became_satisfied() {
            return Some(self.finish_satisfied(active_attempt, cx));
        }
        self.synchronize_resources();
        Some(self.current_settlement())
    }

    fn resume_recoverable_admitted_task<N: 'static>(&mut self, cx: &mut Context<N>) {
        let Some(attempt) = self.recoverable_admitted_attempt else {
            return;
        };
        let drive = self
            .task
            .borrow_mut()
            .as_mut()
            .filter(|task| task.attempt() == attempt)
            .map(|task| task.drive(cx));
        match drive {
            Some(Ok(AdmittedCommandDrive::AwaitingTicket)) => {}
            Some(Ok(AdmittedCommandDrive::Unavailable)) | None => {
                self.recoverable_admitted_attempt = None;
            }
            Some(Err(unwind)) => {
                if let Some(task) = self
                    .task
                    .borrow()
                    .as_ref()
                    .filter(|task| task.attempt() == attempt)
                {
                    task.record_uncertainty(unwind.reason());
                }
                let _ = unwind.suppress();
            }
        }
    }

    fn finish_satisfied<N: 'static>(
        &mut self,
        attempt: Option<MutationAttemptId>,
        cx: &mut Context<N>,
    ) -> MutationSettlement {
        if let Some(attempt) = attempt {
            AdmittedCommandTask::release_if(&mut self.task.borrow_mut(), attempt);
            if self.recoverable_admitted_attempt == Some(attempt) {
                self.recoverable_admitted_attempt = None;
            }
        }
        self.retention_leases.clear();
        let layout_effect = attempt
            .and_then(|attempt| self.layout_effect.take_if(|(active, _)| *active == attempt))
            .map_or(MutationLayoutEffect::Preserve, |(_, effect)| effect);
        let layout = match layout_effect {
            MutationLayoutEffect::Preserve => MutationLayoutSettlement::NotRequested,
            MutationLayoutEffect::InvalidateWhenSatisfied => {
                let primary = self.primary_owner();
                match catch_unwind(AssertUnwindSafe(|| {
                    (primary.layout_invalidator)(
                        primary.mount.instance(),
                        primary.mount.mount(),
                        cx,
                    )
                })) {
                    Ok(Ok(true)) => MutationLayoutSettlement::Invalidated,
                    Ok(Ok(false)) => MutationLayoutSettlement::NoLongerMounted,
                    Ok(Err(error)) => MutationLayoutSettlement::Failed {
                        diagnostic: MutationDiagnostic::new(
                            MutationDiagnosticSource::NativeLayout,
                            error.to_string(),
                        ),
                    },
                    Err(panic) => {
                        let mut panics = OpenGpuiPanicAccumulator::default();
                        panics.push(Some(panic));
                        let _ = panics.suppress();
                        MutationLayoutSettlement::Failed {
                            diagnostic: MutationDiagnostic::new(
                                MutationDiagnosticSource::NativeLayout,
                                "native layout invalidation panicked after mutation satisfaction",
                            ),
                        }
                    }
                }
            }
        };
        self.layout_failure = match &layout {
            MutationLayoutSettlement::Failed { diagnostic } => Some(diagnostic.clone()),
            MutationLayoutSettlement::NotRequested
            | MutationLayoutSettlement::Invalidated
            | MutationLayoutSettlement::NoLongerMounted => None,
        };
        self.admitted_attempt = None;
        MutationSettlement::Satisfied { layout }
    }

    fn synchronize_resources(&mut self) {
        let task_attempt = self
            .task
            .borrow()
            .as_ref()
            .map(AdmittedCommandTask::attempt);
        let retain_recoverable_transport = self.reconciler.phase() == MutationPhase::Uncertain
            && task_attempt.is_some()
            && task_attempt == self.recoverable_admitted_attempt;
        if self.reconciler.phase() != MutationPhase::Pending && !retain_recoverable_transport {
            if let Some(attempt) = task_attempt {
                AdmittedCommandTask::release_if(&mut self.task.borrow_mut(), attempt);
            }
            self.recoverable_admitted_attempt = None;
        }
        if self.reconciler.phase() == MutationPhase::Idle || self.terminal.is_some() {
            self.admitted_attempt = None;
            self.recoverable_admitted_attempt = None;
            self.retention_leases.clear();
            self.layout_effect.take();
            self.diagnostic_projection.take();
        }
    }

    fn synchronize_terminal(&mut self) {
        match self.owner_terminal_signal() {
            Some(MutationTerminalSignal::Retired) => {
                self.retire("native mutation owner was retired".to_owned())
            }
            Some(MutationTerminalSignal::Closed(reason)) => {
                self.close(reason, format!("native mutation owner closed: {reason:?}"))
            }
            None => {}
        }
    }

    fn retire(&mut self, message: String) {
        if self.terminal.is_some() {
            return;
        }
        let intent = self.reconciler.active_intent().cloned();
        let diagnostic =
            MutationDiagnostic::new(MutationDiagnosticSource::NativeLifecycle, message);
        self.release_native_resources();
        self.reconciler = MutationReconciler::default();
        self.terminal = Some(MutationTerminal::Retired { intent, diagnostic });
    }

    fn close(&mut self, reason: ViewCloseReason, message: String) {
        if self.terminal.is_some() {
            return;
        }
        let intent = self.reconciler.active_intent().cloned();
        let diagnostic =
            MutationDiagnostic::new(MutationDiagnosticSource::NativeLifecycle, message);
        self.release_native_resources();
        self.reconciler = MutationReconciler::default();
        self.terminal = Some(MutationTerminal::Closed {
            reason,
            intent,
            diagnostic,
        });
    }

    fn release_native_resources(&mut self) {
        AdmittedCommandTask::shutdown(&mut self.task.borrow_mut());
        self.admitted_attempt = None;
        self.recoverable_admitted_attempt = None;
        self.retention_leases.clear();
        self.layout_effect.take();
        self.layout_failure.take();
        self.diagnostic_projection.take();
    }

    fn primary_owner(&self) -> &OpenGpuiManagedMutationOwner {
        self.owners
            .first()
            .expect("managed mutations always own at least one mount")
    }

    fn owner_terminal_signal(&self) -> Option<MutationTerminalSignal> {
        let mut retired = false;
        let mut live = false;
        for owner in &self.owners {
            match owner.mount.mount().state() {
                OpenGpuiMountState::Live => live = true,
                OpenGpuiMountState::Retired => retired = true,
                OpenGpuiMountState::Closed(reason) => {
                    return Some(MutationTerminalSignal::Closed(reason));
                }
            }
        }
        if !retired {
            return None;
        }
        if self.retirement_policy == ManagedMutationRetirementPolicy::DrainAdmittedWithSurvivors
            && live
            && self.admitted_attempt.is_some()
        {
            return None;
        }
        Some(MutationTerminalSignal::Retired)
    }
}

fn map_begin_error(error: MutationBeginError) -> MutationDispatchError {
    match error {
        MutationBeginError::Busy => MutationDispatchError::Busy,
        MutationBeginError::IdentityExhausted => MutationDispatchError::IdentityExhausted,
        MutationBeginError::RetryExpired => MutationDispatchError::RetryExpired,
        MutationBeginError::StaleRetryBase {
            required_revision,
            received_revision,
        } => MutationDispatchError::StaleRetryBase {
            required_revision,
            received_revision,
        },
    }
}

fn diagnostic_source_for_retry(reason: MutationRetryReason) -> MutationDiagnosticSource {
    match reason {
        MutationRetryReason::AuthoritativeMismatch => {
            MutationDiagnosticSource::AuthoritativeObservation
        }
        MutationRetryReason::RuntimeRejected
        | MutationRetryReason::RuntimeConflict
        | MutationRetryReason::RuntimeFailed => MutationDiagnosticSource::Runtime,
        MutationRetryReason::NativeDispatchFailed => MutationDiagnosticSource::NativeDispatch,
        MutationRetryReason::NotDispatched => MutationDiagnosticSource::NativeAdmission,
    }
}

fn command_ticket_result(
    result: &crate::command_journal::OpenGpuiCommandTicketResult,
) -> MutationCommandResult {
    match OpenGpuiCommandTicketDisposition::classify(result) {
        OpenGpuiCommandTicketDisposition::Completed(completion) => {
            MutationCommandResult::from_runtime(completion)
        }
        OpenGpuiCommandTicketDisposition::DefinitelyNotExecuted(error) => {
            MutationCommandResult::NativeDispatchFailed {
                diagnostic: error.to_string(),
            }
        }
        OpenGpuiCommandTicketDisposition::ExecutionUnknown(error) => {
            MutationCommandResult::Uncertain {
                diagnostic: error.to_string(),
            }
        }
    }
}

fn uncertainty_diagnostic(cause: AdmittedCommandUncertainty) -> String {
    match cause {
        AdmittedCommandUncertainty::TaskInstallationUnwound => {
            "mutation outcome is uncertain: native task installation unwound after journal admission"
        }
        AdmittedCommandUncertainty::HostNotificationUnwound => {
            "mutation outcome is uncertain: native host notification unwound after journal admission"
        }
        AdmittedCommandUncertainty::RunnerPanicked => {
            "mutation outcome is uncertain: native ticket runner panicked"
        }
        AdmittedCommandUncertainty::RunnerDropped => {
            "mutation outcome is uncertain: native ticket runner was dropped before settlement"
        }
    }
    .to_owned()
}

fn diagnostic_source_for_uncertainty(
    cause: AdmittedCommandUncertainty,
) -> MutationDiagnosticSource {
    match cause {
        AdmittedCommandUncertainty::TaskInstallationUnwound
        | AdmittedCommandUncertainty::HostNotificationUnwound => {
            MutationDiagnosticSource::NativeDispatch
        }
        AdmittedCommandUncertainty::RunnerPanicked | AdmittedCommandUncertainty::RunnerDropped => {
            MutationDiagnosticSource::NativeTransport
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_projection_is_irreversible_and_preserves_intent() {
        let mut reconciler = MutationReconciler::default();
        reconciler.begin("add-port", 4).expect("begin");
        let diagnostic =
            MutationDiagnostic::new(MutationDiagnosticSource::NativeLifecycle, "owner retired");
        let terminal = MutationTerminal::Retired {
            intent: reconciler.active_intent().cloned(),
            diagnostic: diagnostic.clone(),
        };

        assert_eq!(terminal.intent(), Some(&"add-port"));
        assert_eq!(
            terminal.settlement(),
            MutationSettlement::Retired { diagnostic }
        );
    }

    #[test]
    fn definitive_and_ambiguous_ticket_failures_remain_distinct() {
        assert!(matches!(
            command_ticket_result(&Err(OpenGpuiCommandTicketFailure::definitely_not_executed(
                OpenGpuiCommandTicketError::FramePreparation {
                    message: "layout failed".to_owned(),
                },
            ),)),
            MutationCommandResult::NativeDispatchFailed { .. }
        ));
        assert!(matches!(
            command_ticket_result(&Err(OpenGpuiCommandTicketFailure::execution_unknown(
                OpenGpuiCommandTicketError::FeedbackOutcomeUnknown,
            ))),
            MutationCommandResult::Uncertain { .. }
        ));
        assert!(matches!(
            command_ticket_result(&Err(OpenGpuiCommandTicketFailure::execution_unknown(
                OpenGpuiCommandTicketError::FeedbackSubmission(Rc::new(
                    FeedbackSubmissionError::AfterCommandAdmission {
                        source: jellyflow_runtime::view::ViewSessionError::Closed {
                            reason: ViewCloseReason::HostClosed,
                        },
                        receipt: None,
                    },
                )),
            ))),
            MutationCommandResult::Uncertain { .. }
        ));
    }
}
