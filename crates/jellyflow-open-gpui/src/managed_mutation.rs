#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;

use jellyflow_runtime::commands::GraphCommand;
use jellyflow_runtime::graph::GraphRevision;
use jellyflow_runtime::reconciliation::{
    MutationAbandonment as RuntimeMutationAbandonment, MutationAttemptId, MutationBeginError,
    MutationCommandResult, MutationPhase, MutationReconciler,
    MutationRetry as RuntimeMutationRetry, MutationRetryReason,
};
use jellyflow_runtime::view::{FeedbackSubmissionError, ViewCloseReason};
use open_gpui::Context;

use crate::activity::{OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiNodeActivityRegistry};
use crate::admitted_command::{
    AdmittedCommandEvent, AdmittedCommandTask, AdmittedCommandTaskLease, AdmittedCommandUncertainty,
};
use crate::authoring::{
    OpenGpuiCommandAdmissionError, OpenGpuiNodeAuthoring, OpenGpuiNodeAuthoringError,
};
use crate::binding::{OpenGpuiLayoutInvalidator, OpenGpuiMountContext, OpenGpuiMountState};
use crate::command_journal::OpenGpuiCommandTicketError;

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

/// Structured diagnostic projected without exposing native command transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MutationDiagnostic {
    source: MutationDiagnosticSource,
    message: String,
}

impl MutationDiagnostic {
    fn new(source: MutationDiagnosticSource, message: impl Into<String>) -> Self {
        Self {
            source,
            message: message.into(),
        }
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

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManagedMutationPostAdmissionFault {
    TaskInstallationUnwound,
    HostNotificationUnwound,
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
    reconciler: MutationReconciler<I>,
    terminal: Option<MutationTerminal<I>>,
    task: Rc<RefCell<Option<AdmittedCommandTask<MutationAttemptId>>>>,
    retention_leases: Vec<OpenGpuiActivityLease>,
    layout_effect: Option<(MutationAttemptId, MutationLayoutEffect)>,
    layout_failure: Option<MutationDiagnostic>,
    diagnostic_source: Option<(MutationAttemptId, MutationDiagnosticSource)>,
    #[cfg(test)]
    post_admission_fault: Cell<Option<ManagedMutationPostAdmissionFault>>,
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
            reconciler: MutationReconciler::default(),
            terminal: None,
            task: Rc::new(RefCell::new(None)),
            retention_leases: Vec::new(),
            layout_effect: None,
            layout_failure: None,
            diagnostic_source: None,
            #[cfg(test)]
            post_admission_fault: Cell::new(None),
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
    pub(crate) fn inject_post_admission_fault_for_test(
        &self,
        fault: ManagedMutationPostAdmissionFault,
    ) {
        self.post_admission_fault.set(Some(fault));
    }

    #[cfg(test)]
    fn inject_post_admission_fault(&self, fault: ManagedMutationPostAdmissionFault) {
        if self.post_admission_fault.get() == Some(fault) {
            self.post_admission_fault.set(None);
            panic!("injected managed mutation post-admission fault: {fault:?}");
        }
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
        self.is_live()
            .then(|| self.reconciler.retry(false))
            .flatten()
            .map(MutationRetry)
    }

    #[must_use]
    pub fn confirm_uncertain_retry(&self) -> Option<MutationRetry<I>> {
        self.is_live()
            .then(|| self.reconciler.retry(true))
            .flatten()
            .map(MutationRetry)
    }

    #[must_use]
    pub fn can_confirm_uncertain_retry(&self) -> bool {
        self.is_live() && self.reconciler.can_confirm_uncertain_retry()
    }

    /// Returns the last layout failure until another explicit mutation starts.
    #[must_use]
    pub fn layout_failure(&self) -> Option<&MutationDiagnostic> {
        self.layout_failure.as_ref()
    }

    /// Abandons an exact retry after the product can no longer plan its retained intent.
    ///
    /// Pending commands cannot be abandoned through this API. A successful abandonment releases
    /// the native mutation retention immediately and leaves the owner ready for a new intent.
    pub fn abandon_retry(&mut self, retry: MutationRetry<I>) -> bool {
        self.synchronize_terminal();
        if !self.is_live() || !self.reconciler.abandon_retry(retry.0) {
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
        self.diagnostic_source = None;

        if let Err(diagnostic) = self.ensure_retention() {
            self.synchronize_terminal();
            self.ensure_dispatchable()?;
            return Err(self.fail_not_dispatched(attempt, diagnostic));
        }

        let task_lease = match AdmittedCommandTaskLease::acquire(&self.primary_owner().activity) {
            Ok(lease) => lease,
            Err(error) => {
                let diagnostic = MutationDiagnostic::new(
                    MutationDiagnosticSource::NativeActivity,
                    error.to_string(),
                );
                return Err(self.fail_not_dispatched(attempt, diagnostic));
            }
        };
        let admitted = match admit(authoring, command) {
            Ok(admitted) => admitted,
            Err(error) => return Err(self.handle_admission_failure(attempt, error)),
        };
        let (ticket, notification) = admitted.into_parts();

        *self.task.borrow_mut() = Some(AdmittedCommandTask::new(attempt, ticket, task_lease));
        let spawned = catch_unwind(AssertUnwindSafe(|| {
            #[cfg(test)]
            if self.post_admission_fault.get()
                == Some(ManagedMutationPostAdmissionFault::TaskInstallationUnwound)
            {
                self.post_admission_fault.set(None);
                self.task
                    .borrow_mut()
                    .as_mut()
                    .expect("an admitted mutation owns its task before installation")
                    .inject_installation_fault_for_test();
            }
            self.task
                .borrow_mut()
                .as_mut()
                .is_some_and(|task| task.spawn(cx))
        }));
        match spawned {
            Ok(true) => {
                let notified = catch_unwind(AssertUnwindSafe(|| {
                    #[cfg(test)]
                    self.inject_post_admission_fault(
                        ManagedMutationPostAdmissionFault::HostNotificationUnwound,
                    );
                    notification.notify(cx);
                }));
                if let Err(panic) = notified {
                    self.record_post_admission_uncertainty(
                        attempt,
                        AdmittedCommandUncertainty::HostNotificationUnwound,
                    );
                    resume_unwind(panic);
                }
            }
            Ok(false) => {
                self.record_post_admission_uncertainty(
                    attempt,
                    AdmittedCommandUncertainty::TaskInstallationUnwound,
                );
            }
            Err(panic) => {
                self.record_post_admission_uncertainty(
                    attempt,
                    AdmittedCommandUncertainty::TaskInstallationUnwound,
                );
                resume_unwind(panic);
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
                    diagnostic: MutationDiagnostic::new(
                        self.reconciler
                            .active_attempt()
                            .and_then(|attempt| {
                                self.diagnostic_source
                                    .filter(|(source_attempt, _)| *source_attempt == attempt)
                                    .map(|(_, source)| source)
                            })
                            .unwrap_or_else(|| diagnostic_source_for_retry(reason)),
                        self.reconciler
                            .diagnostic()
                            .expect("retryable mutation has one diagnostic"),
                    ),
                }
            }
            MutationPhase::Uncertain => MutationSettlement::Uncertain {
                diagnostic: MutationDiagnostic::new(
                    self.reconciler
                        .active_attempt()
                        .and_then(|attempt| {
                            self.diagnostic_source
                                .filter(|(source_attempt, _)| *source_attempt == attempt)
                                .map(|(_, source)| source)
                        })
                        .unwrap_or(MutationDiagnosticSource::NativeTransport),
                    self.reconciler
                        .diagnostic()
                        .expect("uncertain mutation has one diagnostic"),
                ),
                retry_confirmable: self.reconciler.can_confirm_uncertain_retry(),
            },
        }
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
            let task = Rc::downgrade(&self.task);
            let lease = owner
                .activity
                .acquire_with_revocation(OpenGpuiActivityKind::Mutation, move || {
                    if let Some(task) = task.upgrade()
                        && let Ok(mut task) = task.try_borrow_mut()
                    {
                        AdmittedCommandTask::shutdown(&mut task);
                    }
                })
                .map_err(|error| {
                    MutationDiagnostic::new(
                        MutationDiagnosticSource::NativeActivity,
                        error.to_string(),
                    )
                })?;
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
        let primary = self.primary_owner();
        stamp.context.instance() == primary.mount.instance()
            && stamp.context.mount().is_same_mount(primary.mount.mount())
            && stamp.context.provenance() == primary.mount.provenance()
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
        self.diagnostic_source = Some((attempt, diagnostic.source));
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
        let event = {
            let task = self.task.borrow();
            let Some(task) = task.as_ref().filter(|task| task.attempt() == attempt) else {
                return;
            };
            task.record_uncertainty(reason);
            task.take_event()
        };
        if let Some(AdmittedCommandEvent::Uncertain { reason, .. }) = event {
            self.diagnostic_source = Some((attempt, diagnostic_source_for_uncertainty(reason)));
            let _ = self.reconciler.complete(
                attempt,
                MutationCommandResult::Uncertain {
                    diagnostic: uncertainty_diagnostic(reason),
                },
            );
        }
        AdmittedCommandTask::release_if(&mut self.task.borrow_mut(), attempt);
        self.synchronize_resources();
    }

    fn drain_task_completion<N: 'static>(
        &mut self,
        cx: &mut Context<N>,
    ) -> Option<MutationSettlement> {
        let event = self
            .task
            .borrow()
            .as_ref()
            .and_then(AdmittedCommandTask::take_event)?;
        self.synchronize_terminal();
        if self.terminal.is_some() {
            return Some(self.current_settlement());
        }
        let attempt = event.attempt();
        let diagnostic_source = match &event {
            AdmittedCommandEvent::Uncertain { reason, .. } => {
                Some(diagnostic_source_for_uncertainty(*reason))
            }
            AdmittedCommandEvent::Settled { .. } => None,
        };
        let completion = match event {
            AdmittedCommandEvent::Settled { result, .. } => match *result {
                Ok(completion) => MutationCommandResult::from_runtime(&completion),
                Err(error) => command_transport_result(error),
            },
            AdmittedCommandEvent::Uncertain { reason, .. } => MutationCommandResult::Uncertain {
                diagnostic: uncertainty_diagnostic(reason),
            },
        };
        self.diagnostic_source = diagnostic_source.map(|source| (attempt, source));
        let active_attempt = self.reconciler.active_attempt();
        let transition = self.reconciler.complete(attempt, completion);
        AdmittedCommandTask::release_if(&mut self.task.borrow_mut(), attempt);
        let Ok(transition) = transition else {
            self.synchronize_resources();
            return Some(self.current_settlement());
        };
        if transition.became_satisfied() {
            return Some(self.finish_satisfied(active_attempt, cx));
        }
        self.synchronize_resources();
        Some(self.current_settlement())
    }

    fn finish_satisfied<N: 'static>(
        &mut self,
        attempt: Option<MutationAttemptId>,
        cx: &mut Context<N>,
    ) -> MutationSettlement {
        if let Some(attempt) = attempt {
            AdmittedCommandTask::release_if(&mut self.task.borrow_mut(), attempt);
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
                    Err(_) => MutationLayoutSettlement::Failed {
                        diagnostic: MutationDiagnostic::new(
                            MutationDiagnosticSource::NativeLayout,
                            "native layout invalidation panicked after mutation satisfaction",
                        ),
                    },
                }
            }
        };
        self.layout_failure = match &layout {
            MutationLayoutSettlement::Failed { diagnostic } => Some(diagnostic.clone()),
            MutationLayoutSettlement::NotRequested
            | MutationLayoutSettlement::Invalidated
            | MutationLayoutSettlement::NoLongerMounted => None,
        };
        MutationSettlement::Satisfied { layout }
    }

    fn synchronize_resources(&mut self) {
        if self.reconciler.phase() != MutationPhase::Pending {
            let attempt = self
                .task
                .borrow()
                .as_ref()
                .map(AdmittedCommandTask::attempt);
            if let Some(attempt) = attempt {
                AdmittedCommandTask::release_if(&mut self.task.borrow_mut(), attempt);
            }
        }
        if self.reconciler.phase() == MutationPhase::Idle || self.terminal.is_some() {
            self.retention_leases.clear();
            self.layout_effect.take();
            self.diagnostic_source.take();
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
        self.retention_leases.clear();
        self.layout_effect.take();
        self.layout_failure.take();
        self.diagnostic_source.take();
    }

    fn primary_owner(&self) -> &OpenGpuiManagedMutationOwner {
        self.owners
            .first()
            .expect("managed mutations always own at least one mount")
    }

    fn owner_terminal_signal(&self) -> Option<MutationTerminalSignal> {
        let mut retired = false;
        for owner in &self.owners {
            match owner.mount.mount().state() {
                OpenGpuiMountState::Live => {}
                OpenGpuiMountState::Retired => retired = true,
                OpenGpuiMountState::Closed(reason) => {
                    return Some(MutationTerminalSignal::Closed(reason));
                }
            }
        }
        retired.then_some(MutationTerminalSignal::Retired)
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

fn command_transport_result(error: OpenGpuiCommandTicketError) -> MutationCommandResult {
    let diagnostic = error.to_string();
    match error {
        OpenGpuiCommandTicketError::HostDropped
        | OpenGpuiCommandTicketError::HostClosed { .. }
        | OpenGpuiCommandTicketError::FramePreparation { .. } => {
            MutationCommandResult::NativeDispatchFailed { diagnostic }
        }
        OpenGpuiCommandTicketError::FeedbackSubmission(error)
            if matches!(
                error.as_ref(),
                FeedbackSubmissionError::AfterCommandAdmission { receipt: None, .. }
            ) =>
        {
            MutationCommandResult::Uncertain { diagnostic }
        }
        OpenGpuiCommandTicketError::FeedbackSubmission(_) => {
            MutationCommandResult::NativeDispatchFailed { diagnostic }
        }
        OpenGpuiCommandTicketError::ClosePanicked { .. }
        | OpenGpuiCommandTicketError::FeedbackOutcomeUnknown
        | OpenGpuiCommandTicketError::Session(_) => MutationCommandResult::Uncertain { diagnostic },
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
            command_transport_result(OpenGpuiCommandTicketError::FramePreparation {
                message: "layout failed".to_owned(),
            }),
            MutationCommandResult::NativeDispatchFailed { .. }
        ));
        assert!(matches!(
            command_transport_result(OpenGpuiCommandTicketError::FeedbackOutcomeUnknown),
            MutationCommandResult::Uncertain { .. }
        ));
        assert!(matches!(
            command_transport_result(OpenGpuiCommandTicketError::FeedbackSubmission(Rc::new(
                FeedbackSubmissionError::AfterCommandAdmission {
                    source: jellyflow_runtime::view::ViewSessionError::Closed {
                        reason: ViewCloseReason::HostClosed,
                    },
                    receipt: None,
                },
            ))),
            MutationCommandResult::Uncertain { .. }
        ));
    }
}
