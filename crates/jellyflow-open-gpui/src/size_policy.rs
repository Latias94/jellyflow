use jellyflow::core::CanvasSize;

/// Default material-difference tolerance in canvas units.
pub const OPEN_GPUI_DEFAULT_SIZE_EPSILON: f32 = 0.5;
/// Default number of consecutive equivalent 1:1 samples required before a command.
pub const OPEN_GPUI_DEFAULT_STABLE_SIZE_SAMPLES: u16 = 2;
/// Default maximum number of size commands emitted during one content/environment epoch.
pub const OPEN_GPUI_DEFAULT_SIZE_ATTEMPT_BOUND: u16 = 8;
/// Default minimum content-driven node size.
pub const OPEN_GPUI_DEFAULT_MIN_CONTENT_SIZE: CanvasSize = CanvasSize {
    width: 80.0,
    height: 40.0,
};
/// Default maximum content-driven node size.
pub const OPEN_GPUI_DEFAULT_MAX_CONTENT_SIZE: CanvasSize = CanvasSize {
    width: 1_200.0,
    height: 1_600.0,
};

/// Validated convergence controls shared by content-driven size policies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiNodeSizeConvergence {
    epsilon_in_canvas: f32,
    stable_samples: u16,
    attempt_bound: u16,
}

impl OpenGpuiNodeSizeConvergence {
    /// Creates bounded convergence controls.
    ///
    /// # Errors
    ///
    /// Returns an error when epsilon is not positive and finite, fewer than two stable samples
    /// are requested, or the attempt bound is zero.
    pub fn new(
        epsilon_in_canvas: f32,
        stable_samples: u16,
        attempt_bound: u16,
    ) -> Result<Self, OpenGpuiNodeSizeConfigError> {
        if !epsilon_in_canvas.is_finite() || epsilon_in_canvas <= 0.0 {
            return Err(OpenGpuiNodeSizeConfigError::InvalidEpsilon);
        }
        if stable_samples < 2 {
            return Err(OpenGpuiNodeSizeConfigError::TooFewStableSamples);
        }
        if attempt_bound == 0 {
            return Err(OpenGpuiNodeSizeConfigError::InvalidAttemptBound);
        }
        Ok(Self {
            epsilon_in_canvas,
            stable_samples,
            attempt_bound,
        })
    }

    /// Returns the per-axis material-difference tolerance in canvas units.
    #[must_use]
    pub const fn epsilon_in_canvas(self) -> f32 {
        self.epsilon_in_canvas
    }

    /// Returns the number of consecutive equivalent samples required for stability.
    #[must_use]
    pub const fn stable_samples(self) -> u16 {
        self.stable_samples
    }

    /// Returns the maximum number of commands permitted during one epoch.
    #[must_use]
    pub const fn attempt_bound(self) -> u16 {
        self.attempt_bound
    }
}

impl Default for OpenGpuiNodeSizeConvergence {
    fn default() -> Self {
        Self {
            epsilon_in_canvas: OPEN_GPUI_DEFAULT_SIZE_EPSILON,
            stable_samples: OPEN_GPUI_DEFAULT_STABLE_SIZE_SAMPLES,
            attempt_bound: OPEN_GPUI_DEFAULT_SIZE_ATTEMPT_BOUND,
        }
    }
}

/// Bounds and convergence controls for fixed-width, content-driven height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiContentHeightPolicy {
    min_height: f32,
    max_height: f32,
    convergence: OpenGpuiNodeSizeConvergence,
}

impl OpenGpuiContentHeightPolicy {
    /// Creates validated height bounds with default convergence controls.
    ///
    /// # Errors
    ///
    /// Returns an error unless both bounds are positive and finite and minimum does not exceed
    /// maximum.
    pub fn new(min_height: f32, max_height: f32) -> Result<Self, OpenGpuiNodeSizeConfigError> {
        if !valid_height_bounds(min_height, max_height) {
            return Err(OpenGpuiNodeSizeConfigError::InvalidHeightBounds);
        }
        Ok(Self {
            min_height,
            max_height,
            convergence: OpenGpuiNodeSizeConvergence::default(),
        })
    }

    /// Replaces convergence controls with an already validated configuration.
    #[must_use]
    pub const fn with_convergence(mut self, convergence: OpenGpuiNodeSizeConvergence) -> Self {
        self.convergence = convergence;
        self
    }

    #[must_use]
    pub const fn min_height(self) -> f32 {
        self.min_height
    }

    #[must_use]
    pub const fn max_height(self) -> f32 {
        self.max_height
    }

    #[must_use]
    pub const fn convergence(self) -> OpenGpuiNodeSizeConvergence {
        self.convergence
    }
}

impl Default for OpenGpuiContentHeightPolicy {
    fn default() -> Self {
        Self {
            min_height: OPEN_GPUI_DEFAULT_MIN_CONTENT_SIZE.height,
            max_height: OPEN_GPUI_DEFAULT_MAX_CONTENT_SIZE.height,
            convergence: OpenGpuiNodeSizeConvergence::default(),
        }
    }
}

/// Bounds and convergence controls for fully intrinsic content sizing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiContentSizePolicy {
    min_size: CanvasSize,
    max_size: CanvasSize,
    convergence: OpenGpuiNodeSizeConvergence,
}

impl OpenGpuiContentSizePolicy {
    /// Creates validated two-axis bounds with default convergence controls.
    ///
    /// # Errors
    ///
    /// Returns an error unless both sizes are positive and finite and every minimum axis is no
    /// larger than its corresponding maximum axis.
    pub fn new(
        min_size: CanvasSize,
        max_size: CanvasSize,
    ) -> Result<Self, OpenGpuiNodeSizeConfigError> {
        if !valid_size_bounds(min_size, max_size) {
            return Err(OpenGpuiNodeSizeConfigError::InvalidContentBounds);
        }
        Ok(Self {
            min_size,
            max_size,
            convergence: OpenGpuiNodeSizeConvergence::default(),
        })
    }

    /// Replaces convergence controls with an already validated configuration.
    #[must_use]
    pub const fn with_convergence(mut self, convergence: OpenGpuiNodeSizeConvergence) -> Self {
        self.convergence = convergence;
        self
    }

    #[must_use]
    pub const fn min_size(self) -> CanvasSize {
        self.min_size
    }

    #[must_use]
    pub const fn max_size(self) -> CanvasSize {
        self.max_size
    }

    #[must_use]
    pub const fn convergence(self) -> OpenGpuiNodeSizeConvergence {
        self.convergence
    }
}

impl Default for OpenGpuiContentSizePolicy {
    fn default() -> Self {
        Self {
            min_size: OPEN_GPUI_DEFAULT_MIN_CONTENT_SIZE,
            max_size: OPEN_GPUI_DEFAULT_MAX_CONTENT_SIZE,
            convergence: OpenGpuiNodeSizeConvergence::default(),
        }
    }
}

/// Size authority selected for one registered Open GPUI node renderer.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum OpenGpuiNodeSizePolicy {
    /// The graph document is the sole size authority and content measurements are ignored.
    #[default]
    Document,
    /// Document width is fixed while measured 1:1 content controls bounded height.
    ContentHeight(OpenGpuiContentHeightPolicy),
    /// Measured 1:1 content controls both axes within explicit bounds.
    Content(OpenGpuiContentSizePolicy),
}

impl OpenGpuiNodeSizePolicy {
    /// Returns convergence controls for content-authoritative policies.
    #[must_use]
    pub const fn convergence(self) -> Option<OpenGpuiNodeSizeConvergence> {
        match self {
            Self::Document => None,
            Self::ContentHeight(policy) => Some(policy.convergence()),
            Self::Content(policy) => Some(policy.convergence()),
        }
    }

    /// Derives one bounded canvas target from a native 1:1 content measurement.
    ///
    /// `None` means the policy has no content authority or either input is invalid. The measured
    /// size must not be taken from a zoomed host wrapper.
    #[must_use]
    pub fn desired_size(
        self,
        document_size: CanvasSize,
        measured_at_native_scale: CanvasSize,
    ) -> Option<CanvasSize> {
        if !document_size.is_positive_finite() || !measured_at_native_scale.is_positive_finite() {
            return None;
        }
        match self {
            Self::Document => None,
            Self::ContentHeight(policy) => Some(CanvasSize {
                width: document_size.width,
                height: measured_at_native_scale
                    .height
                    .clamp(policy.min_height(), policy.max_height()),
            }),
            Self::Content(policy) => Some(CanvasSize {
                width: measured_at_native_scale
                    .width
                    .clamp(policy.min_size().width, policy.max_size().width),
                height: measured_at_native_scale
                    .height
                    .clamp(policy.min_size().height, policy.max_size().height),
            }),
        }
    }
}

/// Invalid node-size policy or reducer configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenGpuiNodeSizeConfigError {
    /// Height bounds are non-finite, non-positive, or reversed.
    #[error("content-height bounds must be positive, finite, and ordered")]
    InvalidHeightBounds,
    /// Two-axis content bounds are non-finite, non-positive, or reversed.
    #[error("content size bounds must be positive, finite, and ordered per axis")]
    InvalidContentBounds,
    /// Epsilon is not positive and finite.
    #[error("size epsilon must be positive and finite")]
    InvalidEpsilon,
    /// Fewer than two samples were requested.
    #[error("content sizing requires at least two stable samples")]
    TooFewStableSamples,
    /// No convergence attempts were permitted.
    #[error("size attempt bound must be greater than zero")]
    InvalidAttemptBound,
    /// Initial or reset document geometry is invalid.
    #[error("authoritative node size must be positive and finite")]
    InvalidAuthoritativeSize,
}

/// Independent content and environment identity for one size-convergence session.
///
/// Neither field should be derived from graph or binding revision: a size command itself advances
/// those revisions and would otherwise reset convergence indefinitely.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OpenGpuiNodeSizeEpoch {
    content: u64,
    environment: u64,
}

impl OpenGpuiNodeSizeEpoch {
    #[must_use]
    pub const fn new(content: u64, environment: u64) -> Self {
        Self {
            content,
            environment,
        }
    }

    #[must_use]
    pub const fn content(self) -> u64 {
        self.content
    }

    #[must_use]
    pub const fn environment(self) -> u64 {
        self.environment
    }
}

/// Adapter-local identity for one pending size command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OpenGpuiNodeSizeTicket(u64);

impl OpenGpuiNodeSizeTicket {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One command intent emitted by the pure convergence reducer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiNodeSizeCommand {
    ticket: OpenGpuiNodeSizeTicket,
    size: CanvasSize,
}

impl OpenGpuiNodeSizeCommand {
    #[must_use]
    pub const fn ticket(self) -> OpenGpuiNodeSizeTicket {
        self.ticket
    }

    #[must_use]
    pub const fn size(self) -> CanvasSize {
        self.size
    }
}

/// Terminal outcome for one adapter-local size ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiNodeSizeSettlement {
    /// The requested size became authoritative, including a semantic no-op receipt.
    Applied,
    /// The graph rejected the command for a non-conflict reason.
    Rejected,
    /// The command lost its expected-revision race.
    Conflict,
    /// The owning node instance retired before the command could settle normally.
    Retired,
}

/// Deterministic reason that content-driven sizing stopped for the current epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiNodeSizeFreezeReason {
    /// The latest desired size continued an alternating A-B command sequence at the bound.
    AlternatingCycle,
    /// The epoch exhausted its configured command-attempt bound.
    AttemptBound,
}

/// One structured, edge-triggered non-convergence diagnostic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiNodeSizeDiagnostic {
    epoch: OpenGpuiNodeSizeEpoch,
    reason: OpenGpuiNodeSizeFreezeReason,
    attempts: u16,
    last_applied_size: CanvasSize,
    suppressed_size: CanvasSize,
}

impl OpenGpuiNodeSizeDiagnostic {
    #[must_use]
    pub const fn epoch(self) -> OpenGpuiNodeSizeEpoch {
        self.epoch
    }

    #[must_use]
    pub const fn reason(self) -> OpenGpuiNodeSizeFreezeReason {
        self.reason
    }

    #[must_use]
    pub const fn attempts(self) -> u16 {
        self.attempts
    }

    #[must_use]
    pub const fn last_applied_size(self) -> CanvasSize {
        self.last_applied_size
    }

    #[must_use]
    pub const fn suppressed_size(self) -> CanvasSize {
        self.suppressed_size
    }
}

/// Effects produced by one pure size-state transition.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OpenGpuiNodeSizeUpdate {
    command: Option<OpenGpuiNodeSizeCommand>,
    diagnostic: Option<OpenGpuiNodeSizeDiagnostic>,
    matched_ticket: bool,
}

impl OpenGpuiNodeSizeUpdate {
    #[must_use]
    pub const fn command(self) -> Option<OpenGpuiNodeSizeCommand> {
        self.command
    }

    #[must_use]
    pub const fn diagnostic(self) -> Option<OpenGpuiNodeSizeDiagnostic> {
        self.diagnostic
    }

    /// Returns whether a settlement input matched and cleared the current pending ticket.
    #[must_use]
    pub const fn matched_ticket(self) -> bool {
        self.matched_ticket
    }

    #[must_use]
    pub const fn is_idle(self) -> bool {
        self.command.is_none() && self.diagnostic.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingSizeCommand {
    ticket: OpenGpuiNodeSizeTicket,
    desired_size: CanvasSize,
}

/// Pure convergence state for one retained node instance.
///
/// The state owns no graph, host, or framework handle. Callers map the local ticket to their
/// command journal, then feed one terminal settlement back through [`Self::settle`].
#[derive(Debug, Clone)]
pub struct OpenGpuiNodeSizeState {
    epoch: OpenGpuiNodeSizeEpoch,
    last_applied_size: CanvasSize,
    stable_candidate: Option<CanvasSize>,
    stable_sample_count: u16,
    pending: Option<PendingSizeCommand>,
    latest_desired_size: Option<CanvasSize>,
    suppressed_size: Option<CanvasSize>,
    attempts: u16,
    previous_attempt_size: Option<CanvasSize>,
    last_attempt_size: Option<CanvasSize>,
    next_ticket: Option<u64>,
    frozen: Option<OpenGpuiNodeSizeFreezeReason>,
}

impl OpenGpuiNodeSizeState {
    /// Starts convergence from one authoritative graph size.
    ///
    /// # Errors
    ///
    /// Returns an error when the authoritative size is not positive and finite.
    pub fn new(
        epoch: OpenGpuiNodeSizeEpoch,
        authoritative_size: CanvasSize,
    ) -> Result<Self, OpenGpuiNodeSizeConfigError> {
        if !authoritative_size.is_positive_finite() {
            return Err(OpenGpuiNodeSizeConfigError::InvalidAuthoritativeSize);
        }
        Ok(Self {
            epoch,
            last_applied_size: authoritative_size,
            stable_candidate: None,
            stable_sample_count: 0,
            pending: None,
            latest_desired_size: None,
            suppressed_size: None,
            attempts: 0,
            previous_attempt_size: None,
            last_attempt_size: None,
            next_ticket: Some(1),
            frozen: None,
        })
    }

    #[must_use]
    pub const fn epoch(&self) -> OpenGpuiNodeSizeEpoch {
        self.epoch
    }

    #[must_use]
    pub const fn last_applied_size(&self) -> CanvasSize {
        self.last_applied_size
    }

    #[must_use]
    pub fn pending_ticket(&self) -> Option<OpenGpuiNodeSizeTicket> {
        self.pending.map(|pending| pending.ticket)
    }

    #[must_use]
    pub fn pending_size(&self) -> Option<CanvasSize> {
        self.pending.map(|pending| pending.desired_size)
    }

    #[must_use]
    pub const fn latest_desired_size(&self) -> Option<CanvasSize> {
        self.latest_desired_size
    }

    #[must_use]
    pub const fn attempts(&self) -> u16 {
        self.attempts
    }

    #[must_use]
    pub const fn freeze_reason(&self) -> Option<OpenGpuiNodeSizeFreezeReason> {
        self.frozen
    }

    #[must_use]
    pub const fn is_frozen(&self) -> bool {
        self.frozen.is_some()
    }

    /// Begins a new content/environment epoch while preserving any in-flight ticket.
    ///
    /// Preserving the pending ticket prevents two graph size commands from becoming in flight at
    /// once. New stable measurements may coalesce behind it and dispatch only after settlement.
    ///
    /// # Errors
    ///
    /// Returns an error when the new authoritative size is not positive and finite.
    pub fn reset_epoch(
        &mut self,
        epoch: OpenGpuiNodeSizeEpoch,
        authoritative_size: CanvasSize,
    ) -> Result<(), OpenGpuiNodeSizeConfigError> {
        if !authoritative_size.is_positive_finite() {
            return Err(OpenGpuiNodeSizeConfigError::InvalidAuthoritativeSize);
        }
        self.epoch = epoch;
        self.last_applied_size = authoritative_size;
        self.stable_candidate = None;
        self.stable_sample_count = 0;
        self.latest_desired_size = None;
        self.suppressed_size = None;
        self.attempts = 0;
        self.previous_attempt_size = None;
        self.last_attempt_size = None;
        self.frozen = None;
        Ok(())
    }

    /// Observes one content measurement taken at native 1:1 scale.
    ///
    /// Material targets require the configured number of consecutive epsilon-equivalent samples.
    /// While a command is pending, only the latest stable target is retained.
    #[must_use]
    pub fn observe(
        &mut self,
        policy: OpenGpuiNodeSizePolicy,
        epoch: OpenGpuiNodeSizeEpoch,
        document_size: CanvasSize,
        measured_at_native_scale: CanvasSize,
    ) -> OpenGpuiNodeSizeUpdate {
        if epoch != self.epoch && self.reset_epoch(epoch, document_size).is_err() {
            return OpenGpuiNodeSizeUpdate::default();
        }
        if self.frozen.is_some() {
            return OpenGpuiNodeSizeUpdate::default();
        }
        let Some(convergence) = policy.convergence() else {
            self.reset_samples();
            self.latest_desired_size = None;
            return OpenGpuiNodeSizeUpdate::default();
        };
        let Some(desired_size) = policy.desired_size(document_size, measured_at_native_scale)
        else {
            self.reset_samples();
            return OpenGpuiNodeSizeUpdate::default();
        };
        if self.suppressed_size.is_some_and(|suppressed| {
            sizes_are_equivalent(suppressed, desired_size, convergence.epsilon_in_canvas())
        }) {
            self.reset_samples();
            return OpenGpuiNodeSizeUpdate::default();
        }
        self.suppressed_size = None;

        if self.stable_candidate.is_some_and(|candidate| {
            sizes_are_equivalent(candidate, desired_size, convergence.epsilon_in_canvas())
        }) {
            self.stable_sample_count = self.stable_sample_count.saturating_add(1);
        } else {
            self.stable_sample_count = 1;
        }
        self.stable_candidate = Some(desired_size);

        if self.stable_sample_count < convergence.stable_samples() {
            return OpenGpuiNodeSizeUpdate::default();
        }

        if let Some(pending) = self.pending {
            self.latest_desired_size = (!sizes_are_equivalent(
                desired_size,
                pending.desired_size,
                convergence.epsilon_in_canvas(),
            ))
            .then_some(desired_size);
            return OpenGpuiNodeSizeUpdate::default();
        }

        self.latest_desired_size = None;
        if sizes_are_equivalent(
            desired_size,
            self.last_applied_size,
            convergence.epsilon_in_canvas(),
        ) {
            return OpenGpuiNodeSizeUpdate::default();
        }
        self.issue(desired_size, convergence)
    }

    /// Applies one terminal command outcome and optionally dispatches the latest coalesced target.
    ///
    /// Unknown or stale local tickets are inert. Rejected and conflicting attempts are never
    /// replayed automatically; only a distinct stable target observed while pending can dispatch
    /// immediately after those outcomes.
    #[must_use]
    pub fn settle(
        &mut self,
        policy: OpenGpuiNodeSizePolicy,
        ticket: OpenGpuiNodeSizeTicket,
        settlement: OpenGpuiNodeSizeSettlement,
    ) -> OpenGpuiNodeSizeUpdate {
        let Some(pending) = self.pending.filter(|pending| pending.ticket == ticket) else {
            return OpenGpuiNodeSizeUpdate::default();
        };
        self.pending = None;

        if settlement == OpenGpuiNodeSizeSettlement::Applied {
            self.last_applied_size = pending.desired_size;
            self.suppressed_size = None;
        } else if matches!(
            settlement,
            OpenGpuiNodeSizeSettlement::Rejected | OpenGpuiNodeSizeSettlement::Conflict
        ) {
            self.suppressed_size = Some(pending.desired_size);
        }
        if settlement == OpenGpuiNodeSizeSettlement::Retired {
            self.reset_samples();
            self.latest_desired_size = None;
            return OpenGpuiNodeSizeUpdate {
                matched_ticket: true,
                ..OpenGpuiNodeSizeUpdate::default()
            };
        }

        let Some(desired_size) = self.latest_desired_size.take() else {
            return OpenGpuiNodeSizeUpdate {
                matched_ticket: true,
                ..OpenGpuiNodeSizeUpdate::default()
            };
        };
        let Some(convergence) = policy.convergence() else {
            return OpenGpuiNodeSizeUpdate {
                matched_ticket: true,
                ..OpenGpuiNodeSizeUpdate::default()
            };
        };
        if sizes_are_equivalent(
            desired_size,
            self.last_applied_size,
            convergence.epsilon_in_canvas(),
        ) {
            return OpenGpuiNodeSizeUpdate {
                matched_ticket: true,
                ..OpenGpuiNodeSizeUpdate::default()
            };
        }

        self.suppressed_size = None;
        let mut update = self.issue(desired_size, convergence);
        update.matched_ticket = true;
        update
    }

    fn issue(
        &mut self,
        desired_size: CanvasSize,
        convergence: OpenGpuiNodeSizeConvergence,
    ) -> OpenGpuiNodeSizeUpdate {
        if self.frozen.is_some() {
            return OpenGpuiNodeSizeUpdate::default();
        }
        if let Some(pending) = self.pending {
            self.latest_desired_size = (!sizes_are_equivalent(
                desired_size,
                pending.desired_size,
                convergence.epsilon_in_canvas(),
            ))
            .then_some(desired_size);
            return OpenGpuiNodeSizeUpdate::default();
        }

        if self.attempts >= convergence.attempt_bound() || self.next_ticket.is_none() {
            let reason = if self
                .continues_alternating_cycle(desired_size, convergence.epsilon_in_canvas())
            {
                OpenGpuiNodeSizeFreezeReason::AlternatingCycle
            } else {
                OpenGpuiNodeSizeFreezeReason::AttemptBound
            };
            self.frozen = Some(reason);
            self.latest_desired_size = None;
            return OpenGpuiNodeSizeUpdate {
                diagnostic: Some(OpenGpuiNodeSizeDiagnostic {
                    epoch: self.epoch,
                    reason,
                    attempts: self.attempts,
                    last_applied_size: self.last_applied_size,
                    suppressed_size: desired_size,
                }),
                ..OpenGpuiNodeSizeUpdate::default()
            };
        }

        let Some(ticket_value) = self.next_ticket else {
            return OpenGpuiNodeSizeUpdate::default();
        };
        self.next_ticket = ticket_value.checked_add(1);
        let ticket = OpenGpuiNodeSizeTicket(ticket_value);
        self.attempts += 1;
        self.previous_attempt_size = self.last_attempt_size;
        self.last_attempt_size = Some(desired_size);
        self.pending = Some(PendingSizeCommand {
            ticket,
            desired_size,
        });
        OpenGpuiNodeSizeUpdate {
            command: Some(OpenGpuiNodeSizeCommand {
                ticket,
                size: desired_size,
            }),
            ..OpenGpuiNodeSizeUpdate::default()
        }
    }

    fn continues_alternating_cycle(&self, desired_size: CanvasSize, epsilon: f32) -> bool {
        self.previous_attempt_size
            .zip(self.last_attempt_size)
            .is_some_and(|(previous, last)| {
                sizes_are_equivalent(desired_size, previous, epsilon)
                    && !sizes_are_equivalent(desired_size, last, epsilon)
            })
    }

    fn reset_samples(&mut self) {
        self.stable_candidate = None;
        self.stable_sample_count = 0;
    }
}

fn valid_height_bounds(min_height: f32, max_height: f32) -> bool {
    min_height.is_finite() && max_height.is_finite() && min_height > 0.0 && min_height <= max_height
}

fn valid_size_bounds(min_size: CanvasSize, max_size: CanvasSize) -> bool {
    min_size.is_positive_finite()
        && max_size.is_positive_finite()
        && min_size.width <= max_size.width
        && min_size.height <= max_size.height
}

fn sizes_are_equivalent(left: CanvasSize, right: CanvasSize, epsilon: f32) -> bool {
    (left.width - right.width).abs() <= epsilon && (left.height - right.height).abs() <= epsilon
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPOCH: OpenGpuiNodeSizeEpoch = OpenGpuiNodeSizeEpoch::new(3, 5);

    fn size(width: f32, height: f32) -> CanvasSize {
        CanvasSize { width, height }
    }

    fn state(applied: CanvasSize) -> OpenGpuiNodeSizeState {
        OpenGpuiNodeSizeState::new(EPOCH, applied).expect("valid authoritative size")
    }

    fn observe_twice(
        state: &mut OpenGpuiNodeSizeState,
        policy: OpenGpuiNodeSizePolicy,
        document: CanvasSize,
        measured: CanvasSize,
    ) -> OpenGpuiNodeSizeUpdate {
        assert!(state.observe(policy, EPOCH, document, measured).is_idle());
        state.observe(policy, EPOCH, document, measured)
    }

    #[test]
    fn document_is_the_default_size_authority() {
        assert_eq!(
            OpenGpuiNodeSizePolicy::default(),
            OpenGpuiNodeSizePolicy::Document
        );
    }

    #[test]
    fn policies_validate_every_convergence_and_bound_input() {
        assert_eq!(
            OpenGpuiNodeSizeConvergence::new(0.0, 2, 4),
            Err(OpenGpuiNodeSizeConfigError::InvalidEpsilon)
        );
        assert_eq!(
            OpenGpuiNodeSizeConvergence::new(0.5, 1, 4),
            Err(OpenGpuiNodeSizeConfigError::TooFewStableSamples)
        );
        assert_eq!(
            OpenGpuiNodeSizeConvergence::new(0.5, 2, 0),
            Err(OpenGpuiNodeSizeConfigError::InvalidAttemptBound)
        );
        assert_eq!(
            OpenGpuiContentHeightPolicy::new(100.0, 50.0),
            Err(OpenGpuiNodeSizeConfigError::InvalidHeightBounds)
        );
        assert_eq!(
            OpenGpuiContentSizePolicy::new(size(100.0, 20.0), size(90.0, 30.0)),
            Err(OpenGpuiNodeSizeConfigError::InvalidContentBounds)
        );
    }

    #[test]
    fn document_policy_never_emits_a_content_size_command() {
        let mut state = state(size(240.0, 100.0));
        for measured in [size(500.0, 800.0), size(20.0, 10.0)] {
            for _ in 0..4 {
                assert!(
                    state
                        .observe(
                            OpenGpuiNodeSizePolicy::Document,
                            EPOCH,
                            size(240.0, 100.0),
                            measured,
                        )
                        .is_idle()
                );
            }
        }
        assert_eq!(state.attempts(), 0);
        assert_eq!(state.last_applied_size(), size(240.0, 100.0));
    }

    #[test]
    fn content_height_preserves_document_width_and_clamps_height() {
        let policy = OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(80.0, 320.0).expect("valid bounds"),
        );
        let document = size(240.0, 100.0);
        let mut state = state(document);

        let command = observe_twice(&mut state, policy, document, size(900.0, 500.0))
            .command()
            .expect("stable command");
        assert_eq!(command.size(), size(240.0, 320.0));
    }

    #[test]
    fn bounded_content_controls_both_axes_after_two_stable_samples() {
        let policy = OpenGpuiNodeSizePolicy::Content(
            OpenGpuiContentSizePolicy::new(size(100.0, 80.0), size(400.0, 300.0))
                .expect("valid bounds"),
        );
        let document = size(200.0, 120.0);
        let mut state = state(document);

        let command = observe_twice(&mut state, policy, document, size(600.0, 40.0))
            .command()
            .expect("stable command");
        assert_eq!(command.size(), size(400.0, 80.0));
    }

    #[test]
    fn pending_command_coalesces_only_the_latest_stable_target() {
        let policy = OpenGpuiNodeSizePolicy::Content(OpenGpuiContentSizePolicy::default());
        let document = size(100.0, 100.0);
        let mut state = state(document);
        let first = observe_twice(&mut state, policy, document, size(120.0, 120.0))
            .command()
            .expect("first command");

        assert!(observe_twice(&mut state, policy, document, size(130.0, 130.0)).is_idle());
        assert!(observe_twice(&mut state, policy, document, size(140.0, 140.0)).is_idle());
        assert_eq!(state.latest_desired_size(), Some(size(140.0, 140.0)));

        let settled = state.settle(policy, first.ticket(), OpenGpuiNodeSizeSettlement::Applied);
        assert!(settled.matched_ticket());
        assert_eq!(
            settled.command().map(OpenGpuiNodeSizeCommand::size),
            Some(size(140.0, 140.0))
        );
        assert_eq!(state.attempts(), 2);
    }

    #[test]
    fn every_terminal_settlement_clears_the_matching_pending_ticket() {
        let policy = OpenGpuiNodeSizePolicy::Content(OpenGpuiContentSizePolicy::default());
        let document = size(100.0, 100.0);

        for settlement in [
            OpenGpuiNodeSizeSettlement::Applied,
            OpenGpuiNodeSizeSettlement::Rejected,
            OpenGpuiNodeSizeSettlement::Conflict,
            OpenGpuiNodeSizeSettlement::Retired,
        ] {
            let mut state = state(document);
            let command = observe_twice(&mut state, policy, document, size(120.0, 120.0))
                .command()
                .expect("pending command");
            let update = state.settle(policy, command.ticket(), settlement);
            assert!(update.matched_ticket());
            assert!(update.command().is_none());
            assert_eq!(state.pending_ticket(), None);
        }
    }

    #[test]
    fn rejected_and_conflicting_targets_do_not_retry_until_the_target_or_epoch_changes() {
        let policy = OpenGpuiNodeSizePolicy::Content(OpenGpuiContentSizePolicy::default());
        let document = size(100.0, 100.0);

        for settlement in [
            OpenGpuiNodeSizeSettlement::Rejected,
            OpenGpuiNodeSizeSettlement::Conflict,
        ] {
            let mut state = state(document);
            let rejected = observe_twice(&mut state, policy, document, size(120.0, 120.0))
                .command()
                .expect("pending command");
            assert!(
                state
                    .settle(policy, rejected.ticket(), settlement)
                    .matched_ticket()
            );
            let epoch = state.epoch();

            for _ in 0..120 {
                assert!(
                    state
                        .observe(policy, epoch, document, size(120.0, 120.0),)
                        .is_idle()
                );
            }
            assert!(
                state
                    .observe(policy, epoch, document, size(140.0, 140.0),)
                    .is_idle()
            );
            assert!(
                state
                    .observe(policy, epoch, document, size(140.0, 140.0),)
                    .command()
                    .is_some()
            );
        }
    }

    #[test]
    fn epoch_reset_preserves_one_pending_ticket_and_unfreezes_convergence() {
        let convergence = OpenGpuiNodeSizeConvergence::new(0.5, 2, 1).expect("valid convergence");
        let policy = OpenGpuiNodeSizePolicy::Content(
            OpenGpuiContentSizePolicy::default().with_convergence(convergence),
        );
        let document = size(100.0, 100.0);
        let mut state = state(document);
        let first = observe_twice(&mut state, policy, document, size(120.0, 120.0))
            .command()
            .expect("first command");
        let next_epoch = OpenGpuiNodeSizeEpoch::new(4, 5);

        assert!(
            state
                .observe(policy, next_epoch, document, size(140.0, 140.0))
                .is_idle()
        );
        assert!(
            state
                .observe(policy, next_epoch, document, size(140.0, 140.0))
                .is_idle()
        );
        assert_eq!(state.pending_ticket(), Some(first.ticket()));
        assert_eq!(state.attempts(), 0);

        let update = state.settle(policy, first.ticket(), OpenGpuiNodeSizeSettlement::Applied);
        assert!(update.matched_ticket());
        assert_eq!(
            update.command().map(OpenGpuiNodeSizeCommand::size),
            Some(size(140.0, 140.0))
        );
        assert_eq!(state.attempts(), 1);
    }

    #[test]
    fn one_hundred_twenty_equivalent_samples_remain_quiescent() {
        let policy = OpenGpuiNodeSizePolicy::Content(OpenGpuiContentSizePolicy::default());
        let document = size(200.0, 120.0);
        let mut state = state(document);

        for index in 0..120 {
            let delta = if index % 2 == 0 { 0.2 } else { -0.2 };
            let update = state.observe(
                policy,
                EPOCH,
                document,
                size(document.width + delta, document.height - delta),
            );
            assert!(update.is_idle(), "sample {index} emitted an effect");
        }

        assert_eq!(state.attempts(), 0);
        assert_eq!(state.pending_ticket(), None);
        assert!(!state.is_frozen());
    }

    #[test]
    fn alternating_cycle_freezes_once_at_the_attempt_bound_until_epoch_reset() {
        let convergence = OpenGpuiNodeSizeConvergence::new(0.1, 2, 4).expect("valid convergence");
        let policy = OpenGpuiNodeSizePolicy::Content(
            OpenGpuiContentSizePolicy::default().with_convergence(convergence),
        );
        let document = size(100.0, 100.0);
        let a = size(120.0, 120.0);
        let b = size(140.0, 140.0);
        let mut state = state(document);

        for target in [a, b, a, b] {
            let command = observe_twice(&mut state, policy, document, target)
                .command()
                .expect("bounded command");
            assert!(
                state
                    .settle(
                        policy,
                        command.ticket(),
                        OpenGpuiNodeSizeSettlement::Applied
                    )
                    .matched_ticket()
            );
        }
        let frozen = observe_twice(&mut state, policy, document, a);
        let diagnostic = frozen.diagnostic().expect("one freeze diagnostic");
        assert_eq!(
            diagnostic.reason(),
            OpenGpuiNodeSizeFreezeReason::AlternatingCycle
        );
        assert_eq!(diagnostic.attempts(), 4);
        assert_eq!(diagnostic.last_applied_size(), b);
        assert_eq!(diagnostic.suppressed_size(), a);
        assert!(state.is_frozen());

        for _ in 0..120 {
            assert!(state.observe(policy, EPOCH, document, a).is_idle());
        }
        assert_eq!(state.attempts(), 4);

        let next_epoch = OpenGpuiNodeSizeEpoch::new(4, 6);
        assert!(state.observe(policy, next_epoch, b, a).is_idle());
        let resumed = state.observe(policy, next_epoch, b, a);
        assert!(resumed.command().is_some());
        assert!(!state.is_frozen());
        assert_eq!(state.attempts(), 1);
    }

    #[test]
    fn non_alternating_changes_report_the_general_attempt_bound() {
        let convergence = OpenGpuiNodeSizeConvergence::new(0.1, 2, 2).expect("valid convergence");
        let policy = OpenGpuiNodeSizePolicy::Content(
            OpenGpuiContentSizePolicy::default().with_convergence(convergence),
        );
        let document = size(100.0, 100.0);
        let mut state = state(document);

        for target in [size(120.0, 120.0), size(140.0, 140.0)] {
            let command = observe_twice(&mut state, policy, document, target)
                .command()
                .expect("bounded command");
            assert!(
                state
                    .settle(
                        policy,
                        command.ticket(),
                        OpenGpuiNodeSizeSettlement::Applied
                    )
                    .matched_ticket()
            );
        }

        let diagnostic = observe_twice(&mut state, policy, document, size(160.0, 160.0))
            .diagnostic()
            .expect("attempt-bound diagnostic");
        assert_eq!(
            diagnostic.reason(),
            OpenGpuiNodeSizeFreezeReason::AttemptBound
        );
    }
}
