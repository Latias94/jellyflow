//! Reusable semantic scenarios for native adapter conformance tests.
//!
//! This module deliberately contains only data and comparison logic. Native adapters own event
//! synthesis, frame advancement, focus, capture, and command transport in their local test suites.

use std::fmt;

/// Stable semantic identity for a canonical adapter scenario.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConformanceScenarioId {
    NewConnection(ConformanceCommandEvidence),
    ReconnectSource(ConformanceCommandEvidence),
    ReconnectTarget(ConformanceCommandEvidence),
    ReconnectTargetNoop,
    InvalidConnectionRelease,
    ConnectionUncertainRetryCancel,
    CreateFromWire(ConformanceCommandEvidence),
    NodeDrag(ConformanceCommandEvidence),
    NewConnectionProbePrefix,
    NewConnectionPrefixWait,
    ReconnectSourcePrefixWait,
    ReconnectTargetPrefixWait,
    CreateFromWirePrefixWait,
    NodeDragPrefixWait,
    NodePressBelowThreshold,
    NativeControl,
    EdgePick,
    CaptureLost,
    OwnerReplaced,
    Escape,
    OutsideClick,
    SourceRetired,
    HostClosed,
}

impl ConformanceScenarioId {
    /// Returns the command evidence the scenario's native driver must supply, when applicable.
    #[must_use]
    pub const fn command_evidence(self) -> Option<ConformanceCommandEvidence> {
        match self {
            Self::NewConnection(evidence)
            | Self::ReconnectSource(evidence)
            | Self::ReconnectTarget(evidence)
            | Self::CreateFromWire(evidence)
            | Self::NodeDrag(evidence) => Some(evidence),
            Self::ReconnectTargetNoop => Some(ConformanceCommandEvidence::Completion(
                ConformanceCommandCompletion::Noop,
            )),
            Self::ConnectionUncertainRetryCancel => Some(ConformanceCommandEvidence::Transport(
                ConformanceAdapterTransportEvidence::OutcomeUnknown,
            )),
            Self::NewConnectionPrefixWait
            | Self::NewConnectionProbePrefix
            | Self::ReconnectSourcePrefixWait
            | Self::ReconnectTargetPrefixWait
            | Self::CreateFromWirePrefixWait
            | Self::NodeDragPrefixWait => Some(ConformanceCommandEvidence::Completion(
                ConformanceCommandCompletion::Applied,
            )),
            Self::InvalidConnectionRelease
            | Self::NodePressBelowThreshold
            | Self::NativeControl
            | Self::EdgePick
            | Self::CaptureLost
            | Self::OwnerReplaced
            | Self::Escape
            | Self::OutsideClick
            | Self::SourceRetired
            | Self::HostClosed => None,
        }
    }
}

/// A semantic target role that each native driver maps to its own widget or hit region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceTarget {
    NativeControl,
    NodeDragHandle,
    SourcePort,
    Edge,
    ReconnectSource,
    ReconnectTarget,
}

/// The endpoint detached by a reconnect scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceReconnectEndpoint {
    Source,
    Target,
}

/// A framework-independent connection candidate outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceCandidate {
    None,
    Invalid,
    Valid,
}

/// A command completion produced by the renderer-neutral runtime.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConformanceCommandCompletion {
    Applied,
    Noop,
    Rejected,
    Conflict,
    Failed,
}

/// Adapter-owned evidence about command transport, never a runtime command completion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConformanceAdapterTransportEvidence {
    NotDispatched,
    OutcomeUnknown,
}

/// The command evidence selected by a canonical mutation scenario.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConformanceCommandEvidence {
    Completion(ConformanceCommandCompletion),
    Transport(ConformanceAdapterTransportEvidence),
}

/// A semantic cancellation initiated outside command settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceCancelReason {
    Escape,
    OutsideClick,
}

/// One action that a native driver must realize through its own lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceAction {
    Press(ConformanceTarget),
    CrossDragThreshold,
    MoveToCandidate(ConformanceCandidate),
    Release,
    SelectSearchCandidate,
    QueuePrefix,
    DrainPrefix,
    AdvanceFrame,
    PublishAuthoritativeFrame,
    AdvanceAuthoritativeAbsence,
    RetryConnectionSettlement,
    CancelConnectionSettlement,
    CompleteCommand(ConformanceCommandCompletion),
    ReportTransportEvidence(ConformanceAdapterTransportEvidence),
    LoseCapture,
    ReplaceOwner,
    ReleasePreviousOwner,
    Cancel(ConformanceCancelReason),
    RetireSource,
    CloseHost,
}

/// Normalized renderer-neutral interaction phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformancePhase {
    Idle,
    Armed,
    NodeDrag,
    ConnectionDrag,
    Candidate,
    Searching,
    Committing,
    EdgeActive,
}

/// Observable work emitted by one semantic transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceEffect {
    None,
    ProbeConnection,
    PickEdge,
    OpenSearch,
    SubmitConnection,
    SubmitNodeMove,
    RestorePresentation,
    ReleaseNativeOwnership,
}

/// Terminal reason visible after a scenario settles or exits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceTerminal {
    Applied,
    Noop,
    Rejected,
    Conflict,
    ExecutionFailed,
    TransportFailure,
    Uncertain,
    Cancelled,
    InvalidTarget,
    OwnerRetired,
    HostClosed,
}

/// Whether the native adapter still owns pointer or gesture capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceOwnership {
    Released,
    HeldByAdapter,
    HeldBySuccessor,
}

/// The canonical graph selection visible through the native view capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceSelection {
    Empty,
    SourceNode,
    CanonicalEdge,
    Unavailable,
}

/// The terminal state of the native view capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceViewStatus {
    Open,
    HostClosed,
    RuntimeDropped,
}

/// Whether one renderer-neutral command ticket remains live.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceTicket {
    None,
    Pending,
}

/// Runtime-owned terminal connection projection visible after command settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceConnectionSettlement {
    None,
    AwaitingPublication,
    Uncertain { retryable: bool },
    Retryable { retryable: bool },
}

/// Connection intent retained by the renderer-neutral presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceIntent {
    None,
    Connect,
    ReconnectSource,
    ReconnectTarget,
}

/// Authoritative graph result of the canonical mutation under test.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceGraph {
    Baseline,
    SourceRetired,
    NewConnection,
    ReconnectSource,
    ReconnectTarget,
    CreateFromWire,
    NodeMoved,
}

/// Structured geometry result required by the active native surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceGeometry {
    Baseline,
    SourceRetired,
    NewConnectionRouted,
    ReconnectSourceRouted,
    ReconnectTargetRouted,
    CreateFromWireAwaitingLayout,
    CreateFromWireRouted,
    NodeMoved,
    Unavailable,
}

/// Normalized facts reported by a native driver after one action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalizedObservation {
    pub phase: ConformancePhase,
    pub effect: ConformanceEffect,
    pub terminal: Option<ConformanceTerminal>,
    pub ownership: ConformanceOwnership,
    pub ticket: ConformanceTicket,
    pub connection_settlement: ConformanceConnectionSettlement,
    pub intent: ConformanceIntent,
    pub optimistic_presentation: bool,
    pub graph_revision_delta: u64,
    pub graph: ConformanceGraph,
    pub geometry: ConformanceGeometry,
    pub selection: ConformanceSelection,
    pub view_status: ConformanceViewStatus,
}

impl NormalizedObservation {
    #[must_use]
    pub const fn new(
        phase: ConformancePhase,
        effect: ConformanceEffect,
        ownership: ConformanceOwnership,
        ticket: ConformanceTicket,
        graph_revision_delta: u64,
        geometry: ConformanceGeometry,
    ) -> Self {
        Self {
            phase,
            effect,
            terminal: None,
            ownership,
            ticket,
            connection_settlement: ConformanceConnectionSettlement::None,
            intent: ConformanceIntent::None,
            optimistic_presentation: false,
            graph_revision_delta,
            graph: ConformanceGraph::Baseline,
            geometry,
            selection: ConformanceSelection::Empty,
            view_status: ConformanceViewStatus::Open,
        }
    }

    #[must_use]
    pub const fn with_terminal(mut self, terminal: ConformanceTerminal) -> Self {
        self.terminal = Some(terminal);
        self
    }

    #[must_use]
    pub const fn with_optimistic_presentation(mut self) -> Self {
        self.optimistic_presentation = true;
        self
    }

    #[must_use]
    pub const fn with_graph(mut self, graph: ConformanceGraph) -> Self {
        self.graph = graph;
        self
    }

    #[must_use]
    pub const fn with_selection(mut self, selection: ConformanceSelection) -> Self {
        self.selection = selection;
        self
    }

    #[must_use]
    pub const fn with_connection_settlement(
        mut self,
        settlement: ConformanceConnectionSettlement,
    ) -> Self {
        self.connection_settlement = settlement;
        self
    }

    #[must_use]
    pub const fn with_view_status(mut self, view_status: ConformanceViewStatus) -> Self {
        self.view_status = view_status;
        self
    }
}

/// One canonical input and the observation required immediately after it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConformanceStep {
    action: ConformanceAction,
    expected: NormalizedObservation,
}

impl ConformanceStep {
    #[must_use]
    pub const fn new(action: ConformanceAction, expected: NormalizedObservation) -> Self {
        Self { action, expected }
    }

    #[must_use]
    pub const fn action(&self) -> ConformanceAction {
        self.action
    }

    #[must_use]
    pub const fn expected(&self) -> NormalizedObservation {
        self.expected
    }

    #[must_use]
    pub const fn with_selection(mut self, selection: ConformanceSelection) -> Self {
        self.expected.selection = selection;
        self
    }

    #[must_use]
    pub const fn with_intent(mut self, intent: ConformanceIntent) -> Self {
        self.expected.intent = intent;
        self
    }

    #[must_use]
    pub const fn with_terminal(mut self, terminal: ConformanceTerminal) -> Self {
        self.expected.terminal = Some(terminal);
        self
    }

    #[must_use]
    pub const fn with_connection_settlement(
        mut self,
        settlement: ConformanceConnectionSettlement,
    ) -> Self {
        self.expected.connection_settlement = settlement;
        self
    }

    #[must_use]
    pub const fn with_optimistic_presentation(mut self) -> Self {
        self.expected.optimistic_presentation = true;
        self
    }
}

/// A complete semantic scenario. Display names are diagnostic only; identity is typed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConformanceScenario {
    id: ConformanceScenarioId,
    display_name: &'static str,
    steps: Vec<ConformanceStep>,
}

impl ConformanceScenario {
    #[must_use]
    pub const fn id(&self) -> ConformanceScenarioId {
        self.id
    }

    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        self.display_name
    }

    #[must_use]
    pub fn steps(&self) -> &[ConformanceStep] {
        &self.steps
    }
}

/// One renderer-neutral field that diverged between a canonical and native observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceFieldMismatch {
    Phase {
        expected: ConformancePhase,
        actual: ConformancePhase,
    },
    Effect {
        expected: ConformanceEffect,
        actual: ConformanceEffect,
    },
    Terminal {
        expected: Option<ConformanceTerminal>,
        actual: Option<ConformanceTerminal>,
    },
    Ownership {
        expected: ConformanceOwnership,
        actual: ConformanceOwnership,
    },
    Ticket {
        expected: ConformanceTicket,
        actual: ConformanceTicket,
    },
    ConnectionSettlement {
        expected: ConformanceConnectionSettlement,
        actual: ConformanceConnectionSettlement,
    },
    Intent {
        expected: ConformanceIntent,
        actual: ConformanceIntent,
    },
    OptimisticPresentation {
        expected: bool,
        actual: bool,
    },
    GraphRevisionDelta {
        expected: u64,
        actual: u64,
    },
    Graph {
        expected: ConformanceGraph,
        actual: ConformanceGraph,
    },
    Geometry {
        expected: ConformanceGeometry,
        actual: ConformanceGeometry,
    },
    Selection {
        expected: ConformanceSelection,
        actual: ConformanceSelection,
    },
    ViewStatus {
        expected: ConformanceViewStatus,
        actual: ConformanceViewStatus,
    },
}

/// A structured difference between a canonical scenario and one native driver's observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConformanceMismatch {
    StepCount {
        scenario: ConformanceScenarioId,
        expected: usize,
        actual: usize,
    },
    Observation {
        scenario: ConformanceScenarioId,
        index: usize,
        action: ConformanceAction,
        fields: Vec<ConformanceFieldMismatch>,
    },
}

impl fmt::Display for ConformanceMismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StepCount {
                scenario,
                expected,
                actual,
            } => write!(
                formatter,
                "scenario {scenario:?} expected {expected} observations but received {actual}",
            ),
            Self::Observation {
                scenario,
                index,
                action,
                fields,
            } => write!(
                formatter,
                "scenario {scenario:?} diverged at step {index} after {action:?}: {fields:?}",
            ),
        }
    }
}

impl std::error::Error for ConformanceMismatch {}

/// Compares observations produced by a native driver with the canonical semantic contract.
pub fn compare_observations(
    scenario: &ConformanceScenario,
    actual: &[NormalizedObservation],
) -> Result<(), ConformanceMismatch> {
    if actual.len() != scenario.steps.len() {
        return Err(ConformanceMismatch::StepCount {
            scenario: scenario.id,
            expected: scenario.steps.len(),
            actual: actual.len(),
        });
    }
    for (index, (step, actual)) in scenario.steps.iter().zip(actual).enumerate() {
        let fields = observation_mismatches(step.expected, *actual);
        if !fields.is_empty() {
            return Err(ConformanceMismatch::Observation {
                scenario: scenario.id,
                index,
                action: step.action,
                fields,
            });
        }
    }
    Ok(())
}

fn observation_mismatches(
    expected: NormalizedObservation,
    actual: NormalizedObservation,
) -> Vec<ConformanceFieldMismatch> {
    let mut fields = Vec::new();
    if expected.phase != actual.phase {
        fields.push(ConformanceFieldMismatch::Phase {
            expected: expected.phase,
            actual: actual.phase,
        });
    }
    if expected.effect != actual.effect {
        fields.push(ConformanceFieldMismatch::Effect {
            expected: expected.effect,
            actual: actual.effect,
        });
    }
    if expected.terminal != actual.terminal {
        fields.push(ConformanceFieldMismatch::Terminal {
            expected: expected.terminal,
            actual: actual.terminal,
        });
    }
    if expected.ownership != actual.ownership {
        fields.push(ConformanceFieldMismatch::Ownership {
            expected: expected.ownership,
            actual: actual.ownership,
        });
    }
    if expected.ticket != actual.ticket {
        fields.push(ConformanceFieldMismatch::Ticket {
            expected: expected.ticket,
            actual: actual.ticket,
        });
    }
    if expected.connection_settlement != actual.connection_settlement {
        fields.push(ConformanceFieldMismatch::ConnectionSettlement {
            expected: expected.connection_settlement,
            actual: actual.connection_settlement,
        });
    }
    if expected.intent != actual.intent {
        fields.push(ConformanceFieldMismatch::Intent {
            expected: expected.intent,
            actual: actual.intent,
        });
    }
    if expected.optimistic_presentation != actual.optimistic_presentation {
        fields.push(ConformanceFieldMismatch::OptimisticPresentation {
            expected: expected.optimistic_presentation,
            actual: actual.optimistic_presentation,
        });
    }
    if expected.graph_revision_delta != actual.graph_revision_delta {
        fields.push(ConformanceFieldMismatch::GraphRevisionDelta {
            expected: expected.graph_revision_delta,
            actual: actual.graph_revision_delta,
        });
    }
    if expected.graph != actual.graph {
        fields.push(ConformanceFieldMismatch::Graph {
            expected: expected.graph,
            actual: actual.graph,
        });
    }
    if expected.geometry != actual.geometry {
        fields.push(ConformanceFieldMismatch::Geometry {
            expected: expected.geometry,
            actual: actual.geometry,
        });
    }
    if expected.selection != actual.selection {
        fields.push(ConformanceFieldMismatch::Selection {
            expected: expected.selection,
            actual: actual.selection,
        });
    }
    if expected.view_status != actual.view_status {
        fields.push(ConformanceFieldMismatch::ViewStatus {
            expected: expected.view_status,
            actual: actual.view_status,
        });
    }
    fields
}

/// Returns every terminal command-evidence case used by the canonical mutation matrix.
#[must_use]
pub const fn canonical_command_evidence() -> [ConformanceCommandEvidence; 4] {
    [
        ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Applied),
        ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Rejected),
        ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Conflict),
        ConformanceCommandEvidence::Transport(ConformanceAdapterTransportEvidence::NotDispatched),
    ]
}

/// Returns the complete renderer-neutral scenario set for native adapter tests.
#[must_use]
pub fn canonical_scenarios() -> Vec<ConformanceScenario> {
    let mut scenarios = Vec::new();
    for evidence in canonical_command_evidence() {
        scenarios.push(new_connection_scenario(evidence));
        scenarios.push(reconnect_scenario(
            ConformanceReconnectEndpoint::Source,
            evidence,
        ));
        scenarios.push(reconnect_scenario(
            ConformanceReconnectEndpoint::Target,
            evidence,
        ));
        scenarios.push(create_from_wire_scenario(evidence));
        scenarios.push(node_drag_scenario(evidence));
    }
    scenarios.push(reconnect_target_noop_scenario());
    scenarios.push(invalid_connection_release_scenario());
    scenarios.push(connection_uncertain_retry_cancel_scenario());
    scenarios.push(native_control_scenario());
    scenarios.push(edge_pick_scenario());
    scenarios.extend(connection_prefix_wait_scenarios());
    scenarios.push(node_drag_prefix_wait_scenario());
    scenarios.push(node_press_below_threshold_scenario());
    scenarios.extend(capture_ownership_scenarios());
    scenarios.extend(terminal_ownership_scenarios());
    scenarios
}

#[must_use]
pub fn new_connection_scenario(evidence: ConformanceCommandEvidence) -> ConformanceScenario {
    connection_scenario(
        ConformanceScenarioId::NewConnection(evidence),
        "new-connection",
        ConformanceTarget::SourcePort,
        evidence,
    )
}

/// Returns the connection-probe prefix without borrowing the identity of a full command trace.
///
/// The prefix is a distinct scenario because adapters use it to compare bounded probe work before
/// a release. It must not be represented by truncating a full mutation scenario in place.
#[must_use]
pub fn new_connection_probe_prefix_scenario() -> ConformanceScenario {
    let full = new_connection_scenario(ConformanceCommandEvidence::Completion(
        ConformanceCommandCompletion::Applied,
    ));
    ConformanceScenario {
        id: ConformanceScenarioId::NewConnectionProbePrefix,
        display_name: "new-connection-probe-prefix",
        steps: full.steps.into_iter().take(4).collect(),
    }
}

#[must_use]
pub fn reconnect_scenario(
    endpoint: ConformanceReconnectEndpoint,
    evidence: ConformanceCommandEvidence,
) -> ConformanceScenario {
    let (id, name, target) = match endpoint {
        ConformanceReconnectEndpoint::Source => (
            ConformanceScenarioId::ReconnectSource(evidence),
            "reconnect-source",
            ConformanceTarget::ReconnectSource,
        ),
        ConformanceReconnectEndpoint::Target => (
            ConformanceScenarioId::ReconnectTarget(evidence),
            "reconnect-target",
            ConformanceTarget::ReconnectTarget,
        ),
    };
    connection_scenario(id, name, target, evidence)
}

fn connection_scenario(
    id: ConformanceScenarioId,
    display_name: &'static str,
    target: ConformanceTarget,
    evidence: ConformanceCommandEvidence,
) -> ConformanceScenario {
    let (selection, intent, applied_graph) = match target {
        ConformanceTarget::ReconnectSource => (
            ConformanceSelection::CanonicalEdge,
            ConformanceIntent::ReconnectSource,
            ConformanceGraph::ReconnectSource,
        ),
        ConformanceTarget::ReconnectTarget => (
            ConformanceSelection::CanonicalEdge,
            ConformanceIntent::ReconnectTarget,
            ConformanceGraph::ReconnectTarget,
        ),
        ConformanceTarget::SourcePort => (
            ConformanceSelection::Empty,
            ConformanceIntent::Connect,
            ConformanceGraph::NewConnection,
        ),
        ConformanceTarget::NativeControl
        | ConformanceTarget::NodeDragHandle
        | ConformanceTarget::Edge => unreachable!("invalid connection scenario target"),
    };
    ConformanceScenario {
        id,
        display_name,
        steps: vec![
            held_step(
                ConformanceAction::Press(target),
                ConformancePhase::Armed,
                ConformanceEffect::None,
            )
            .with_intent(intent)
            .with_selection(selection),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            )
            .with_intent(intent)
            .with_selection(selection),
            held_step(
                ConformanceAction::MoveToCandidate(ConformanceCandidate::Invalid),
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            )
            .with_intent(intent)
            .with_selection(selection),
            held_step(
                ConformanceAction::MoveToCandidate(ConformanceCandidate::Valid),
                ConformancePhase::Candidate,
                ConformanceEffect::ProbeConnection,
            )
            .with_intent(intent)
            .with_selection(selection),
            pending_step(
                ConformanceAction::Release,
                ConformanceEffect::SubmitConnection,
            )
            .with_intent(intent)
            .with_selection(selection),
            pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None)
                .with_intent(intent)
                .with_selection(selection),
            {
                let phase = if matches!(
                    evidence,
                    ConformanceCommandEvidence::Transport(
                        ConformanceAdapterTransportEvidence::OutcomeUnknown
                    )
                ) {
                    ConformancePhase::Committing
                } else {
                    ConformancePhase::Idle
                };
                let step =
                    settlement_step(evidence, phase, applied_graph).with_selection(selection);
                if matches!(
                    evidence,
                    ConformanceCommandEvidence::Transport(
                        ConformanceAdapterTransportEvidence::OutcomeUnknown
                    )
                ) {
                    step.with_intent(intent).with_optimistic_presentation()
                } else {
                    step
                }
            },
        ],
    }
}

/// Reconnects the detached target to its original port and proves a real runtime no-op.
///
/// The completion may settle atomically because the unchanged authoritative route already proves
/// the final presentation at the no-op revision. Delayed layout publication is covered separately
/// by the create-from-wire scenarios.
#[must_use]
pub fn reconnect_target_noop_scenario() -> ConformanceScenario {
    let selection = ConformanceSelection::CanonicalEdge;
    let intent = ConformanceIntent::ReconnectTarget;
    ConformanceScenario {
        id: ConformanceScenarioId::ReconnectTargetNoop,
        display_name: "reconnect-target-noop",
        steps: vec![
            held_step(
                ConformanceAction::Press(ConformanceTarget::ReconnectTarget),
                ConformancePhase::Armed,
                ConformanceEffect::None,
            )
            .with_intent(intent)
            .with_selection(selection),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            )
            .with_intent(intent)
            .with_selection(selection),
            held_step(
                ConformanceAction::MoveToCandidate(ConformanceCandidate::Invalid),
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            )
            .with_intent(intent)
            .with_selection(selection),
            held_step(
                ConformanceAction::MoveToCandidate(ConformanceCandidate::Valid),
                ConformancePhase::Candidate,
                ConformanceEffect::ProbeConnection,
            )
            .with_intent(intent)
            .with_selection(selection),
            pending_step(
                ConformanceAction::Release,
                ConformanceEffect::SubmitConnection,
            )
            .with_intent(intent)
            .with_selection(selection),
            pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None)
                .with_intent(intent)
                .with_selection(selection),
            settlement_step(
                ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Noop),
                ConformancePhase::Idle,
                ConformanceGraph::Baseline,
            )
            .with_selection(selection),
        ],
    }
}

/// Releases a connection over an invalid target without manufacturing a command submission.
#[must_use]
pub fn invalid_connection_release_scenario() -> ConformanceScenario {
    ConformanceScenario {
        id: ConformanceScenarioId::InvalidConnectionRelease,
        display_name: "invalid-connection-release",
        steps: vec![
            held_step(
                ConformanceAction::Press(ConformanceTarget::SourcePort),
                ConformancePhase::Armed,
                ConformanceEffect::None,
            )
            .with_intent(ConformanceIntent::Connect),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            )
            .with_intent(ConformanceIntent::Connect),
            held_step(
                ConformanceAction::MoveToCandidate(ConformanceCandidate::Invalid),
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            )
            .with_intent(ConformanceIntent::Connect),
            released_step(
                ConformanceAction::Release,
                ConformancePhase::Idle,
                ConformanceEffect::None,
            )
            .with_terminal(ConformanceTerminal::InvalidTarget),
        ],
    }
}

/// Proves that post-admission uncertainty requires authoritative absence and a fresh retry ticket.
#[must_use]
pub fn connection_uncertain_retry_cancel_scenario() -> ConformanceScenario {
    let evidence =
        ConformanceCommandEvidence::Transport(ConformanceAdapterTransportEvidence::OutcomeUnknown);
    let mut scenario = new_connection_scenario(evidence);
    scenario.id = ConformanceScenarioId::ConnectionUncertainRetryCancel;
    scenario.display_name = "connection-uncertain-retry-cancel";
    scenario.steps.push(
        ConformanceStep::new(
            ConformanceAction::AdvanceAuthoritativeAbsence,
            NormalizedObservation::new(
                ConformancePhase::Committing,
                ConformanceEffect::None,
                ConformanceOwnership::Released,
                ConformanceTicket::None,
                0,
                ConformanceGeometry::Baseline,
            )
            .with_terminal(ConformanceTerminal::Uncertain)
            .with_connection_settlement(ConformanceConnectionSettlement::Uncertain {
                retryable: true,
            }),
        )
        .with_intent(ConformanceIntent::Connect)
        .with_optimistic_presentation(),
    );
    scenario.steps.push(
        pending_step(
            ConformanceAction::RetryConnectionSettlement,
            ConformanceEffect::SubmitConnection,
        )
        .with_intent(ConformanceIntent::Connect),
    );
    scenario.steps.push(
        settlement_step(
            evidence,
            ConformancePhase::Committing,
            ConformanceGraph::NewConnection,
        )
        .with_intent(ConformanceIntent::Connect)
        .with_optimistic_presentation(),
    );
    scenario.steps.push(
        released_step(
            ConformanceAction::CancelConnectionSettlement,
            ConformancePhase::Idle,
            ConformanceEffect::RestorePresentation,
        )
        .with_terminal(ConformanceTerminal::Cancelled),
    );
    scenario
}

#[must_use]
pub fn create_from_wire_scenario(evidence: ConformanceCommandEvidence) -> ConformanceScenario {
    let settlement_phase = match evidence {
        ConformanceCommandEvidence::Completion(
            ConformanceCommandCompletion::Applied | ConformanceCommandCompletion::Noop,
        )
        | ConformanceCommandEvidence::Transport(
            ConformanceAdapterTransportEvidence::OutcomeUnknown,
        ) => ConformancePhase::Committing,
        ConformanceCommandEvidence::Completion(
            ConformanceCommandCompletion::Rejected
            | ConformanceCommandCompletion::Conflict
            | ConformanceCommandCompletion::Failed,
        )
        | ConformanceCommandEvidence::Transport(
            ConformanceAdapterTransportEvidence::NotDispatched,
        ) => ConformancePhase::Searching,
    };
    let settlement_step =
        settlement_step(evidence, settlement_phase, ConformanceGraph::CreateFromWire);
    let settlement_step = match evidence {
        ConformanceCommandEvidence::Completion(
            ConformanceCommandCompletion::Applied | ConformanceCommandCompletion::Noop,
        )
        | ConformanceCommandEvidence::Transport(
            ConformanceAdapterTransportEvidence::OutcomeUnknown,
        ) => {
            let settlement_step = settlement_step.with_intent(ConformanceIntent::Connect);
            let settlement_step = if matches!(
                evidence,
                ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Applied)
            ) {
                settlement_step.with_connection_settlement(
                    ConformanceConnectionSettlement::AwaitingPublication,
                )
            } else {
                settlement_step
            };
            settlement_step.with_optimistic_presentation()
        }
        ConformanceCommandEvidence::Completion(
            ConformanceCommandCompletion::Rejected
            | ConformanceCommandCompletion::Conflict
            | ConformanceCommandCompletion::Failed,
        )
        | ConformanceCommandEvidence::Transport(
            ConformanceAdapterTransportEvidence::NotDispatched,
        ) => settlement_step.with_intent(ConformanceIntent::Connect),
    };
    let mut steps = vec![
        held_step(
            ConformanceAction::Press(ConformanceTarget::SourcePort),
            ConformancePhase::Armed,
            ConformanceEffect::None,
        )
        .with_intent(ConformanceIntent::Connect),
        held_step(
            ConformanceAction::CrossDragThreshold,
            ConformancePhase::ConnectionDrag,
            ConformanceEffect::ProbeConnection,
        )
        .with_intent(ConformanceIntent::Connect),
        held_step(
            ConformanceAction::MoveToCandidate(ConformanceCandidate::None),
            ConformancePhase::ConnectionDrag,
            ConformanceEffect::ProbeConnection,
        )
        .with_intent(ConformanceIntent::Connect),
        released_step(
            ConformanceAction::Release,
            ConformancePhase::Searching,
            ConformanceEffect::OpenSearch,
        )
        .with_intent(ConformanceIntent::Connect),
        pending_step(
            ConformanceAction::SelectSearchCandidate,
            ConformanceEffect::SubmitConnection,
        )
        .with_intent(ConformanceIntent::Connect),
        pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None)
            .with_intent(ConformanceIntent::Connect),
        settlement_step,
    ];
    if matches!(
        evidence,
        ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Applied)
    ) {
        steps.push(published_connection_step(
            ConformanceTerminal::Applied,
            1,
            ConformanceGraph::CreateFromWire,
            ConformanceGeometry::CreateFromWireRouted,
            ConformanceSelection::Empty,
        ));
    }
    ConformanceScenario {
        id: ConformanceScenarioId::CreateFromWire(evidence),
        display_name: "create-from-wire",
        steps,
    }
}

#[must_use]
pub fn node_drag_scenario(evidence: ConformanceCommandEvidence) -> ConformanceScenario {
    ConformanceScenario {
        id: ConformanceScenarioId::NodeDrag(evidence),
        display_name: "node-drag",
        steps: vec![
            held_step(
                ConformanceAction::Press(ConformanceTarget::NodeDragHandle),
                ConformancePhase::Armed,
                ConformanceEffect::None,
            ),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::NodeDrag,
                ConformanceEffect::None,
            )
            .with_selection(ConformanceSelection::SourceNode),
            pending_step(
                ConformanceAction::Release,
                ConformanceEffect::SubmitNodeMove,
            )
            .with_selection(ConformanceSelection::SourceNode),
            pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None)
                .with_selection(ConformanceSelection::SourceNode),
            settlement_step(
                evidence,
                ConformancePhase::Idle,
                ConformanceGraph::NodeMoved,
            )
            .with_selection(ConformanceSelection::SourceNode),
        ],
    }
}

/// Exercises the node-click path without allowing native movement to become a graph drag.
#[must_use]
pub fn node_press_below_threshold_scenario() -> ConformanceScenario {
    ConformanceScenario {
        id: ConformanceScenarioId::NodePressBelowThreshold,
        display_name: "node-press-below-threshold",
        steps: vec![
            held_step(
                ConformanceAction::Press(ConformanceTarget::NodeDragHandle),
                ConformancePhase::Armed,
                ConformanceEffect::None,
            ),
            released_step(
                ConformanceAction::Release,
                ConformancePhase::Idle,
                ConformanceEffect::None,
            )
            .with_selection(ConformanceSelection::SourceNode),
        ],
    }
}

/// Exercises application-owned command prefixes for every connection mutation family.
#[must_use]
pub fn connection_prefix_wait_scenarios() -> [ConformanceScenario; 4] {
    [
        with_prefix_wait(
            new_connection_scenario(ConformanceCommandEvidence::Completion(
                ConformanceCommandCompletion::Applied,
            )),
            ConformanceScenarioId::NewConnectionPrefixWait,
            "new-connection-prefix-wait",
            ConformanceSelection::Empty,
        ),
        with_prefix_wait(
            reconnect_scenario(
                ConformanceReconnectEndpoint::Source,
                ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Applied),
            ),
            ConformanceScenarioId::ReconnectSourcePrefixWait,
            "reconnect-source-prefix-wait",
            ConformanceSelection::CanonicalEdge,
        ),
        with_prefix_wait(
            reconnect_scenario(
                ConformanceReconnectEndpoint::Target,
                ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Applied),
            ),
            ConformanceScenarioId::ReconnectTargetPrefixWait,
            "reconnect-target-prefix-wait",
            ConformanceSelection::CanonicalEdge,
        ),
        with_prefix_wait(
            create_from_wire_scenario(ConformanceCommandEvidence::Completion(
                ConformanceCommandCompletion::Applied,
            )),
            ConformanceScenarioId::CreateFromWirePrefixWait,
            "create-from-wire-prefix-wait",
            ConformanceSelection::Empty,
        ),
    ]
}

fn with_prefix_wait(
    mut scenario: ConformanceScenario,
    id: ConformanceScenarioId,
    display_name: &'static str,
    initial_selection: ConformanceSelection,
) -> ConformanceScenario {
    scenario.id = id;
    scenario.display_name = display_name;
    scenario.steps.insert(
        0,
        released_step(
            ConformanceAction::QueuePrefix,
            ConformancePhase::Idle,
            ConformanceEffect::None,
        )
        .with_selection(initial_selection),
    );
    let settlement_index = scenario
        .steps
        .iter()
        .position(|step| {
            matches!(
                step.action,
                ConformanceAction::CompleteCommand(_)
                    | ConformanceAction::ReportTransportEvidence(_)
            )
        })
        .expect("a prefix-wait mutation scenario has one command evidence step");
    let active_selection = scenario.steps[settlement_index].expected.selection;
    let active_intent = scenario.steps[settlement_index - 1].expected.intent;
    scenario.steps.insert(
        settlement_index,
        pending_step(ConformanceAction::DrainPrefix, ConformanceEffect::None)
            .with_intent(active_intent)
            .with_selection(active_selection),
    );
    scenario.steps.insert(
        settlement_index + 1,
        pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None)
            .with_intent(active_intent)
            .with_selection(active_selection),
    );
    scenario
}

/// Exercises an application-owned command prefix without allowing the optimistic drag to jump.
#[must_use]
pub fn node_drag_prefix_wait_scenario() -> ConformanceScenario {
    ConformanceScenario {
        id: ConformanceScenarioId::NodeDragPrefixWait,
        display_name: "node-drag-prefix-wait",
        steps: vec![
            released_step(
                ConformanceAction::QueuePrefix,
                ConformancePhase::Idle,
                ConformanceEffect::None,
            ),
            held_step(
                ConformanceAction::Press(ConformanceTarget::NodeDragHandle),
                ConformancePhase::Armed,
                ConformanceEffect::None,
            ),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::NodeDrag,
                ConformanceEffect::None,
            )
            .with_selection(ConformanceSelection::SourceNode),
            pending_step(
                ConformanceAction::Release,
                ConformanceEffect::SubmitNodeMove,
            )
            .with_selection(ConformanceSelection::SourceNode),
            pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None)
                .with_selection(ConformanceSelection::SourceNode),
            pending_step(ConformanceAction::DrainPrefix, ConformanceEffect::None)
                .with_selection(ConformanceSelection::SourceNode),
            pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None)
                .with_selection(ConformanceSelection::SourceNode),
            settlement_step(
                ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Applied),
                ConformancePhase::Idle,
                ConformanceGraph::NodeMoved,
            )
            .with_selection(ConformanceSelection::SourceNode),
        ],
    }
}

#[must_use]
pub fn native_control_scenario() -> ConformanceScenario {
    ConformanceScenario {
        id: ConformanceScenarioId::NativeControl,
        display_name: "native-control",
        steps: vec![released_step(
            ConformanceAction::Press(ConformanceTarget::NativeControl),
            ConformancePhase::Idle,
            ConformanceEffect::None,
        )],
    }
}

#[must_use]
pub fn edge_pick_scenario() -> ConformanceScenario {
    ConformanceScenario {
        id: ConformanceScenarioId::EdgePick,
        display_name: "edge-pick",
        steps: vec![
            held_step(
                ConformanceAction::Press(ConformanceTarget::Edge),
                ConformancePhase::EdgeActive,
                ConformanceEffect::PickEdge,
            )
            .with_selection(ConformanceSelection::CanonicalEdge),
            released_step(
                ConformanceAction::Release,
                ConformancePhase::Idle,
                ConformanceEffect::None,
            )
            .with_selection(ConformanceSelection::CanonicalEdge),
        ],
    }
}

/// Returns native ownership replacement and loss scenarios shared by real adapters.
#[must_use]
pub fn capture_ownership_scenarios() -> [ConformanceScenario; 2] {
    [
        capture_lost_scenario(
            ConformanceScenarioId::CaptureLost,
            "capture-lost",
            ConformanceAction::LoseCapture,
        ),
        owner_replaced_scenario(),
    ]
}

fn capture_lost_scenario(
    id: ConformanceScenarioId,
    display_name: &'static str,
    terminal_action: ConformanceAction,
) -> ConformanceScenario {
    ConformanceScenario {
        id,
        display_name,
        steps: vec![
            held_step(
                ConformanceAction::Press(ConformanceTarget::SourcePort),
                ConformancePhase::Armed,
                ConformanceEffect::None,
            )
            .with_intent(ConformanceIntent::Connect),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            )
            .with_intent(ConformanceIntent::Connect),
            terminal_step(
                terminal_action,
                ConformanceTerminal::Cancelled,
                ConformanceGraph::Baseline,
                ConformanceGeometry::Baseline,
            ),
            ConformanceStep::new(
                ConformanceAction::AdvanceFrame,
                NormalizedObservation::new(
                    ConformancePhase::Idle,
                    ConformanceEffect::None,
                    ConformanceOwnership::Released,
                    ConformanceTicket::None,
                    0,
                    ConformanceGeometry::Baseline,
                )
                .with_terminal(ConformanceTerminal::Cancelled),
            ),
        ],
    }
}

fn owner_replaced_scenario() -> ConformanceScenario {
    let successor_observation = || {
        NormalizedObservation::new(
            ConformancePhase::Idle,
            ConformanceEffect::None,
            ConformanceOwnership::HeldBySuccessor,
            ConformanceTicket::None,
            0,
            ConformanceGeometry::Baseline,
        )
        .with_terminal(ConformanceTerminal::Cancelled)
    };
    ConformanceScenario {
        id: ConformanceScenarioId::OwnerReplaced,
        display_name: "owner-replaced",
        steps: vec![
            held_step(
                ConformanceAction::Press(ConformanceTarget::SourcePort),
                ConformancePhase::Armed,
                ConformanceEffect::None,
            )
            .with_intent(ConformanceIntent::Connect),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            )
            .with_intent(ConformanceIntent::Connect),
            ConformanceStep::new(
                ConformanceAction::ReplaceOwner,
                NormalizedObservation::new(
                    ConformancePhase::Idle,
                    ConformanceEffect::ReleaseNativeOwnership,
                    ConformanceOwnership::HeldBySuccessor,
                    ConformanceTicket::None,
                    0,
                    ConformanceGeometry::Baseline,
                )
                .with_terminal(ConformanceTerminal::Cancelled),
            ),
            ConformanceStep::new(
                ConformanceAction::ReleasePreviousOwner,
                successor_observation(),
            ),
            ConformanceStep::new(ConformanceAction::AdvanceFrame, successor_observation()),
        ],
    }
}

#[must_use]
pub fn terminal_ownership_scenarios() -> [ConformanceScenario; 4] {
    [
        terminal_scenario(
            ConformanceScenarioId::Escape,
            "escape",
            ConformanceAction::Cancel(ConformanceCancelReason::Escape),
            ConformanceTerminal::Cancelled,
            ConformanceGraph::Baseline,
            ConformanceGeometry::Baseline,
        ),
        terminal_scenario(
            ConformanceScenarioId::OutsideClick,
            "outside-click",
            ConformanceAction::Cancel(ConformanceCancelReason::OutsideClick),
            ConformanceTerminal::Cancelled,
            ConformanceGraph::Baseline,
            ConformanceGeometry::Baseline,
        ),
        terminal_scenario(
            ConformanceScenarioId::SourceRetired,
            "source-retired",
            ConformanceAction::RetireSource,
            ConformanceTerminal::OwnerRetired,
            ConformanceGraph::SourceRetired,
            ConformanceGeometry::SourceRetired,
        ),
        terminal_scenario(
            ConformanceScenarioId::HostClosed,
            "host-closed",
            ConformanceAction::CloseHost,
            ConformanceTerminal::HostClosed,
            ConformanceGraph::Baseline,
            ConformanceGeometry::Unavailable,
        ),
    ]
}

fn terminal_scenario(
    id: ConformanceScenarioId,
    display_name: &'static str,
    terminal_action: ConformanceAction,
    terminal: ConformanceTerminal,
    graph: ConformanceGraph,
    geometry: ConformanceGeometry,
) -> ConformanceScenario {
    ConformanceScenario {
        id,
        display_name,
        steps: vec![
            held_step(
                ConformanceAction::Press(ConformanceTarget::SourcePort),
                ConformancePhase::Armed,
                ConformanceEffect::None,
            )
            .with_intent(ConformanceIntent::Connect),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            )
            .with_intent(ConformanceIntent::Connect),
            terminal_step(terminal_action, terminal, graph, geometry),
        ],
    }
}

const fn terminal_step(
    action: ConformanceAction,
    terminal: ConformanceTerminal,
    graph: ConformanceGraph,
    geometry: ConformanceGeometry,
) -> ConformanceStep {
    let observation = NormalizedObservation::new(
        ConformancePhase::Idle,
        ConformanceEffect::ReleaseNativeOwnership,
        ConformanceOwnership::Released,
        ConformanceTicket::None,
        0,
        geometry,
    )
    .with_terminal(terminal)
    .with_graph(graph);
    let observation = match terminal {
        ConformanceTerminal::HostClosed => observation
            .with_selection(ConformanceSelection::Unavailable)
            .with_view_status(ConformanceViewStatus::HostClosed),
        ConformanceTerminal::Applied
        | ConformanceTerminal::Noop
        | ConformanceTerminal::Rejected
        | ConformanceTerminal::Conflict
        | ConformanceTerminal::ExecutionFailed
        | ConformanceTerminal::TransportFailure
        | ConformanceTerminal::Uncertain
        | ConformanceTerminal::Cancelled
        | ConformanceTerminal::InvalidTarget
        | ConformanceTerminal::OwnerRetired => observation,
    };
    ConformanceStep::new(action, observation)
}

const fn held_step(
    action: ConformanceAction,
    phase: ConformancePhase,
    effect: ConformanceEffect,
) -> ConformanceStep {
    ConformanceStep::new(
        action,
        NormalizedObservation::new(
            phase,
            effect,
            ConformanceOwnership::HeldByAdapter,
            ConformanceTicket::None,
            0,
            ConformanceGeometry::Baseline,
        ),
    )
}

const fn released_step(
    action: ConformanceAction,
    phase: ConformancePhase,
    effect: ConformanceEffect,
) -> ConformanceStep {
    ConformanceStep::new(
        action,
        NormalizedObservation::new(
            phase,
            effect,
            ConformanceOwnership::Released,
            ConformanceTicket::None,
            0,
            ConformanceGeometry::Baseline,
        ),
    )
}

const fn pending_step(action: ConformanceAction, effect: ConformanceEffect) -> ConformanceStep {
    ConformanceStep::new(
        action,
        NormalizedObservation::new(
            ConformancePhase::Committing,
            effect,
            ConformanceOwnership::Released,
            ConformanceTicket::Pending,
            0,
            ConformanceGeometry::Baseline,
        )
        .with_optimistic_presentation(),
    )
}

const fn settlement_step(
    evidence: ConformanceCommandEvidence,
    phase: ConformancePhase,
    applied_graph: ConformanceGraph,
) -> ConformanceStep {
    let (effect, terminal, revision_delta, graph, geometry) = match evidence {
        ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Applied) => (
            ConformanceEffect::None,
            ConformanceTerminal::Applied,
            1,
            applied_graph,
            geometry_for_graph(applied_graph),
        ),
        ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Noop) => (
            ConformanceEffect::None,
            ConformanceTerminal::Noop,
            0,
            ConformanceGraph::Baseline,
            ConformanceGeometry::Baseline,
        ),
        ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Rejected) => (
            ConformanceEffect::RestorePresentation,
            ConformanceTerminal::Rejected,
            0,
            ConformanceGraph::Baseline,
            ConformanceGeometry::Baseline,
        ),
        ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Conflict) => (
            ConformanceEffect::RestorePresentation,
            ConformanceTerminal::Conflict,
            0,
            ConformanceGraph::Baseline,
            ConformanceGeometry::Baseline,
        ),
        ConformanceCommandEvidence::Completion(ConformanceCommandCompletion::Failed) => (
            ConformanceEffect::RestorePresentation,
            ConformanceTerminal::ExecutionFailed,
            0,
            ConformanceGraph::Baseline,
            ConformanceGeometry::Baseline,
        ),
        ConformanceCommandEvidence::Transport(
            ConformanceAdapterTransportEvidence::NotDispatched,
        ) => (
            ConformanceEffect::RestorePresentation,
            ConformanceTerminal::TransportFailure,
            0,
            ConformanceGraph::Baseline,
            ConformanceGeometry::Baseline,
        ),
        ConformanceCommandEvidence::Transport(
            ConformanceAdapterTransportEvidence::OutcomeUnknown,
        ) => (
            ConformanceEffect::None,
            ConformanceTerminal::Uncertain,
            0,
            ConformanceGraph::Baseline,
            ConformanceGeometry::Baseline,
        ),
    };
    let action = match evidence {
        ConformanceCommandEvidence::Completion(completion) => {
            ConformanceAction::CompleteCommand(completion)
        }
        ConformanceCommandEvidence::Transport(evidence) => {
            ConformanceAction::ReportTransportEvidence(evidence)
        }
    };
    let observation = NormalizedObservation::new(
        phase,
        effect,
        ConformanceOwnership::Released,
        ConformanceTicket::None,
        revision_delta,
        geometry,
    )
    .with_terminal(terminal)
    .with_graph(graph);
    let observation = match (evidence, applied_graph) {
        (
            ConformanceCommandEvidence::Transport(
                ConformanceAdapterTransportEvidence::OutcomeUnknown,
            ),
            ConformanceGraph::NodeMoved,
        ) => observation,
        (
            ConformanceCommandEvidence::Transport(
                ConformanceAdapterTransportEvidence::OutcomeUnknown,
            ),
            ConformanceGraph::Baseline
            | ConformanceGraph::SourceRetired
            | ConformanceGraph::NewConnection
            | ConformanceGraph::ReconnectSource
            | ConformanceGraph::ReconnectTarget
            | ConformanceGraph::CreateFromWire,
        ) => observation.with_connection_settlement(ConformanceConnectionSettlement::Uncertain {
            retryable: false,
        }),
        (ConformanceCommandEvidence::Completion(_), _)
        | (
            ConformanceCommandEvidence::Transport(
                ConformanceAdapterTransportEvidence::NotDispatched,
            ),
            _,
        ) => observation,
    };
    ConformanceStep::new(action, observation)
}

const fn published_connection_step(
    terminal: ConformanceTerminal,
    revision_delta: u64,
    graph: ConformanceGraph,
    geometry: ConformanceGeometry,
    selection: ConformanceSelection,
) -> ConformanceStep {
    ConformanceStep::new(
        ConformanceAction::PublishAuthoritativeFrame,
        NormalizedObservation::new(
            ConformancePhase::Idle,
            ConformanceEffect::None,
            ConformanceOwnership::Released,
            ConformanceTicket::None,
            revision_delta,
            geometry,
        )
        .with_terminal(terminal)
        .with_graph(graph)
        .with_selection(selection),
    )
}

const fn geometry_for_graph(graph: ConformanceGraph) -> ConformanceGeometry {
    match graph {
        ConformanceGraph::Baseline => ConformanceGeometry::Baseline,
        ConformanceGraph::SourceRetired => ConformanceGeometry::SourceRetired,
        ConformanceGraph::NewConnection => ConformanceGeometry::NewConnectionRouted,
        ConformanceGraph::ReconnectSource => ConformanceGeometry::ReconnectSourceRouted,
        ConformanceGraph::ReconnectTarget => ConformanceGeometry::ReconnectTargetRouted,
        ConformanceGraph::CreateFromWire => ConformanceGeometry::CreateFromWireAwaitingLayout,
        ConformanceGraph::NodeMoved => ConformanceGeometry::NodeMoved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatch_reports_each_drifted_semantic_field() {
        let scenario = new_connection_scenario(ConformanceCommandEvidence::Completion(
            ConformanceCommandCompletion::Applied,
        ));
        let mut actual = scenario
            .steps()
            .iter()
            .map(ConformanceStep::expected)
            .collect::<Vec<_>>();
        actual[1].phase = ConformancePhase::Candidate;
        actual[1].effect = ConformanceEffect::None;
        actual[1].geometry = ConformanceGeometry::Unavailable;

        let Err(ConformanceMismatch::Observation { fields, .. }) =
            compare_observations(&scenario, &actual)
        else {
            panic!("semantic drift must report a field-level observation mismatch");
        };
        assert!(fields.iter().any(|field| matches!(
            field,
            ConformanceFieldMismatch::Phase {
                expected: ConformancePhase::ConnectionDrag,
                actual: ConformancePhase::Candidate,
            }
        )));
        assert!(fields.iter().any(|field| matches!(
            field,
            ConformanceFieldMismatch::Effect {
                expected: ConformanceEffect::ProbeConnection,
                actual: ConformanceEffect::None,
            }
        )));
        assert!(fields.iter().any(|field| matches!(
            field,
            ConformanceFieldMismatch::Geometry {
                expected: ConformanceGeometry::Baseline,
                actual: ConformanceGeometry::Unavailable,
            }
        )));
    }

    #[test]
    fn command_completion_and_transport_evidence_use_distinct_actions() {
        let applied = new_connection_scenario(ConformanceCommandEvidence::Completion(
            ConformanceCommandCompletion::Applied,
        ));
        assert!(applied.steps().iter().any(|step| {
            step.action()
                == ConformanceAction::CompleteCommand(ConformanceCommandCompletion::Applied)
        }));
        assert!(
            !applied
                .steps()
                .iter()
                .any(|step| matches!(step.action(), ConformanceAction::ReportTransportEvidence(_)))
        );

        let not_dispatched = new_connection_scenario(ConformanceCommandEvidence::Transport(
            ConformanceAdapterTransportEvidence::NotDispatched,
        ));
        assert!(not_dispatched.steps().iter().any(|step| {
            step.action()
                == ConformanceAction::ReportTransportEvidence(
                    ConformanceAdapterTransportEvidence::NotDispatched,
                )
        }));
        assert!(
            !not_dispatched
                .steps()
                .iter()
                .any(|step| matches!(step.action(), ConformanceAction::CompleteCommand(_)))
        );
    }

    #[test]
    fn p0_connection_exits_are_explicit_and_terminal() {
        let invalid = invalid_connection_release_scenario();
        let invalid_terminal = invalid.steps().last().expect("invalid release terminal");
        assert_eq!(invalid_terminal.action(), ConformanceAction::Release);
        assert_eq!(
            invalid_terminal.expected().terminal,
            Some(ConformanceTerminal::InvalidTarget)
        );
        assert!(!invalid.steps().iter().any(|step| matches!(
            step.action(),
            ConformanceAction::CompleteCommand(_) | ConformanceAction::ReportTransportEvidence(_)
        )));

        let noop = reconnect_target_noop_scenario();
        let published = noop.steps().last().expect("noop completion terminal");
        assert_eq!(
            published.action(),
            ConformanceAction::CompleteCommand(ConformanceCommandCompletion::Noop)
        );
        assert_eq!(published.expected().phase, ConformancePhase::Idle);
        assert_eq!(published.expected().graph_revision_delta, 0);
        assert_eq!(published.expected().graph, ConformanceGraph::Baseline);
        assert_eq!(published.expected().geometry, ConformanceGeometry::Baseline);

        let uncertain = connection_uncertain_retry_cancel_scenario();
        let evidence_steps = uncertain
            .steps()
            .iter()
            .filter(|step| {
                step.action()
                    == ConformanceAction::ReportTransportEvidence(
                        ConformanceAdapterTransportEvidence::OutcomeUnknown,
                    )
            })
            .collect::<Vec<_>>();
        assert_eq!(evidence_steps.len(), 2);
        for step in evidence_steps {
            assert_eq!(
                step.expected().connection_settlement,
                ConformanceConnectionSettlement::Uncertain { retryable: false }
            );
        }
        let retryable = uncertain
            .steps()
            .iter()
            .find(|step| step.action() == ConformanceAction::AdvanceAuthoritativeAbsence)
            .expect("uncertainty waits for authoritative absence");
        assert_eq!(
            retryable.expected().connection_settlement,
            ConformanceConnectionSettlement::Uncertain { retryable: true }
        );
        assert_eq!(
            uncertain
                .steps()
                .last()
                .expect("uncertainty terminal")
                .expected()
                .terminal,
            Some(ConformanceTerminal::Cancelled)
        );
    }
}
