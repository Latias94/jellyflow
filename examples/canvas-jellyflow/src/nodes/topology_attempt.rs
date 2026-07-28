use jellyflow_open_gpui::authoring::{
    MutationDispatchError, MutationLayoutEffect, MutationRequest, MutationRetry,
    MutationSettlement, OpenGpuiManagedMutation, OpenGpuiNodeAuthoring, OpenGpuiNodeAuthoringError,
};
use jellyflow_runtime::commands::GraphCommand;
use open_gpui::Context;

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

pub(super) type TopologyAttemptDispatchError = MutationDispatchError;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct TopologyTransitionEffect;

/// Read-only retry plan. Dropping it leaves the managed intent untouched.
pub(super) struct TopologyRetry<I>(MutationRetry<I>);

impl<I> TopologyRetry<I> {
    pub(super) const fn intent(&self) -> &I {
        self.0.intent()
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

/// Temporary product facade while Condition and Blueprint move into separate domain modules.
///
/// Native command supervision belongs entirely to `OpenGpuiManagedMutation`; this wrapper only
/// preserves the two product renderers' shared vocabulary until U8 and U9 delete it.
pub(super) struct TopologyAttempt<I> {
    mutation: OpenGpuiManagedMutation<I>,
    diagnostic: Option<String>,
}

impl<I> TopologyAttempt<I>
where
    I: Clone + 'static,
{
    pub(super) fn new(mutation: OpenGpuiManagedMutation<I>) -> Self {
        Self {
            mutation,
            diagnostic: None,
        }
    }

    pub(super) fn phase(&self) -> TopologyAttemptPhase {
        match self.mutation.settlement() {
            MutationSettlement::Ready | MutationSettlement::Satisfied { .. } => {
                TopologyAttemptPhase::Idle
            }
            MutationSettlement::Pending {
                awaiting_authority: false,
            } => TopologyAttemptPhase::Pending,
            MutationSettlement::Pending {
                awaiting_authority: true,
            } => TopologyAttemptPhase::AwaitingSnapshot,
            MutationSettlement::Retryable { .. } => TopologyAttemptPhase::Recoverable,
            MutationSettlement::Uncertain { .. } => TopologyAttemptPhase::Uncertain,
            MutationSettlement::Retired { .. } => TopologyAttemptPhase::Retired,
            MutationSettlement::Closed { .. } => TopologyAttemptPhase::Closed,
        }
    }

    pub(super) fn active_intent(&self) -> Option<&I> {
        self.mutation.active_intent()
    }

    pub(super) fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }

    pub(super) fn dispatch<N: 'static>(
        &mut self,
        authoring: OpenGpuiNodeAuthoring,
        command: GraphCommand,
        request: TopologyDispatchRequest<I>,
        cx: &mut Context<N>,
    ) -> Result<(), TopologyAttemptDispatchError> {
        let request = match request {
            TopologyDispatchRequest::New(intent) => MutationRequest::new(intent),
            TopologyDispatchRequest::Retry(retry) => MutationRequest::retry(retry.0),
        };
        match self.mutation.dispatch(
            authoring,
            command,
            request,
            MutationLayoutEffect::InvalidateWhenSatisfied,
            cx,
        ) {
            Ok(settlement) => {
                self.update_diagnostic(&settlement);
                Ok(())
            }
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                Err(error)
            }
        }
    }

    pub(super) fn observe<N: 'static>(
        &mut self,
        authoring: &OpenGpuiNodeAuthoring,
        satisfied: impl FnOnce(&I) -> bool,
        cx: &mut Context<N>,
    ) -> TopologyTransitionEffect {
        let settlement = self.mutation.observe(authoring, satisfied, cx);
        self.update_diagnostic(&settlement);
        TopologyTransitionEffect
    }

    pub(super) fn observe_authoring_error(&mut self, error: &OpenGpuiNodeAuthoringError) {
        let settlement = self.mutation.observe_authoring_error(error);
        self.update_diagnostic(&settlement);
    }

    pub(super) fn retry(&self) -> Option<TopologyRetry<I>> {
        self.mutation.retry().map(TopologyRetry)
    }

    pub(super) fn confirm_uncertain_retry(&self) -> Option<TopologyRetry<I>> {
        self.mutation.confirm_uncertain_retry().map(TopologyRetry)
    }

    pub(super) fn can_confirm_uncertain_retry(&self) -> bool {
        self.mutation.can_confirm_uncertain_retry()
    }

    fn update_diagnostic(&mut self, settlement: &MutationSettlement) {
        self.diagnostic = match settlement {
            MutationSettlement::Retryable { diagnostic, .. }
            | MutationSettlement::Uncertain { diagnostic, .. }
            | MutationSettlement::Retired { diagnostic }
            | MutationSettlement::Closed { diagnostic, .. } => {
                Some(diagnostic.message().to_owned())
            }
            MutationSettlement::Satisfied {
                layout:
                    jellyflow_open_gpui::authoring::MutationLayoutSettlement::Failed { diagnostic },
            } => Some(diagnostic.message().to_owned()),
            MutationSettlement::Ready
            | MutationSettlement::Pending { .. }
            | MutationSettlement::Satisfied { .. } => None,
        };
    }
}
