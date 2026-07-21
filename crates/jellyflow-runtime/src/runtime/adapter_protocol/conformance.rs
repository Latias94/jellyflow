//! Reusable semantic scenarios for native adapter conformance tests.
//!
//! This module deliberately contains only data and comparison logic. Native adapters own event
//! synthesis, frame advancement, focus, capture, and command transport in their local test suites.

use std::fmt;

/// Stable semantic identity for a canonical adapter scenario.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConformanceScenarioId {
    NewConnection(ConformanceSettlement),
    ReconnectSource(ConformanceSettlement),
    ReconnectTarget(ConformanceSettlement),
    CreateFromWire(ConformanceSettlement),
    NodeDrag(ConformanceSettlement),
    NodeDragPrefixWait,
    NativeControl,
    EdgePick,
    CaptureLost,
    OwnerReplaced,
    Escape,
    OutsideClick,
    SourceRetired,
    HostClosed,
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

/// Every terminal command result understood by the renderer-neutral interaction session.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConformanceSettlement {
    Applied,
    Noop,
    Rejected,
    Conflict,
    TransportFailure,
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
    Settle(ConformanceSettlement),
    LoseCapture,
    ReplaceOwner,
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
    TransportFailure,
    Cancelled,
    OwnerRetired,
    HostClosed,
}

/// Whether the native adapter still owns pointer or gesture capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceOwnership {
    Released,
    Held,
}

/// Whether one renderer-neutral command ticket remains live.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceTicket {
    None,
    Pending,
}

/// Availability of the view geometry needed by the active native surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConformanceGeometry {
    Available,
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
    pub optimistic_presentation: bool,
    pub graph_revision_delta: u64,
    pub geometry: ConformanceGeometry,
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
            optimistic_presentation: false,
            graph_revision_delta,
            geometry,
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
}

/// One canonical input and the observation required immediately after it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConformanceStep {
    pub action: ConformanceAction,
    pub expected: NormalizedObservation,
}

impl ConformanceStep {
    #[must_use]
    pub const fn new(action: ConformanceAction, expected: NormalizedObservation) -> Self {
        Self { action, expected }
    }
}

/// A complete semantic scenario. Display names are diagnostic only; identity is typed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConformanceScenario {
    pub id: ConformanceScenarioId,
    pub display_name: &'static str,
    pub steps: Vec<ConformanceStep>,
}

/// A structured difference between a canonical scenario and one native driver's observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConformanceMismatch {
    StepCount {
        scenario: ConformanceScenarioId,
        expected: usize,
        actual: usize,
    },
    Step {
        scenario: ConformanceScenarioId,
        index: usize,
        action: ConformanceAction,
        expected: NormalizedObservation,
        actual: NormalizedObservation,
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
            Self::Step {
                scenario,
                index,
                action,
                expected,
                actual,
            } => write!(
                formatter,
                "scenario {scenario:?} diverged at step {index} after {action:?}: expected {expected:?}, received {actual:?}",
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
        if step.expected != *actual {
            return Err(ConformanceMismatch::Step {
                scenario: scenario.id,
                index,
                action: step.action,
                expected: step.expected,
                actual: *actual,
            });
        }
    }
    Ok(())
}

/// Returns every command settlement used by canonical mutation scenarios.
#[must_use]
pub const fn canonical_settlements() -> [ConformanceSettlement; 5] {
    [
        ConformanceSettlement::Applied,
        ConformanceSettlement::Noop,
        ConformanceSettlement::Rejected,
        ConformanceSettlement::Conflict,
        ConformanceSettlement::TransportFailure,
    ]
}

/// Returns the complete renderer-neutral scenario set for native adapter tests.
#[must_use]
pub fn canonical_scenarios() -> Vec<ConformanceScenario> {
    let mut scenarios = Vec::new();
    for settlement in canonical_settlements() {
        scenarios.push(new_connection_scenario(settlement));
        scenarios.push(reconnect_scenario(
            ConformanceReconnectEndpoint::Source,
            settlement,
        ));
        scenarios.push(reconnect_scenario(
            ConformanceReconnectEndpoint::Target,
            settlement,
        ));
        scenarios.push(create_from_wire_scenario(settlement));
        scenarios.push(node_drag_scenario(settlement));
    }
    scenarios.push(native_control_scenario());
    scenarios.push(edge_pick_scenario());
    scenarios.push(node_drag_prefix_wait_scenario());
    scenarios.extend(capture_ownership_scenarios());
    scenarios.extend(terminal_ownership_scenarios());
    scenarios
}

#[must_use]
pub fn new_connection_scenario(settlement: ConformanceSettlement) -> ConformanceScenario {
    connection_scenario(
        ConformanceScenarioId::NewConnection(settlement),
        "new-connection",
        ConformanceTarget::SourcePort,
        settlement,
    )
}

#[must_use]
pub fn reconnect_scenario(
    endpoint: ConformanceReconnectEndpoint,
    settlement: ConformanceSettlement,
) -> ConformanceScenario {
    let (id, name, target) = match endpoint {
        ConformanceReconnectEndpoint::Source => (
            ConformanceScenarioId::ReconnectSource(settlement),
            "reconnect-source",
            ConformanceTarget::ReconnectSource,
        ),
        ConformanceReconnectEndpoint::Target => (
            ConformanceScenarioId::ReconnectTarget(settlement),
            "reconnect-target",
            ConformanceTarget::ReconnectTarget,
        ),
    };
    connection_scenario(id, name, target, settlement)
}

fn connection_scenario(
    id: ConformanceScenarioId,
    display_name: &'static str,
    target: ConformanceTarget,
    settlement: ConformanceSettlement,
) -> ConformanceScenario {
    ConformanceScenario {
        id,
        display_name,
        steps: vec![
            held_step(
                ConformanceAction::Press(target),
                ConformancePhase::Armed,
                ConformanceEffect::None,
            ),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            ),
            held_step(
                ConformanceAction::MoveToCandidate(ConformanceCandidate::Invalid),
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            ),
            held_step(
                ConformanceAction::MoveToCandidate(ConformanceCandidate::Valid),
                ConformancePhase::Candidate,
                ConformanceEffect::ProbeConnection,
            ),
            pending_step(
                ConformanceAction::Release,
                ConformanceEffect::SubmitConnection,
            ),
            pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None),
            settlement_step(settlement, ConformancePhase::Idle),
        ],
    }
}

#[must_use]
pub fn create_from_wire_scenario(settlement: ConformanceSettlement) -> ConformanceScenario {
    ConformanceScenario {
        id: ConformanceScenarioId::CreateFromWire(settlement),
        display_name: "create-from-wire",
        steps: vec![
            held_step(
                ConformanceAction::Press(ConformanceTarget::SourcePort),
                ConformancePhase::Armed,
                ConformanceEffect::None,
            ),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            ),
            held_step(
                ConformanceAction::MoveToCandidate(ConformanceCandidate::None),
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            ),
            released_step(
                ConformanceAction::Release,
                ConformancePhase::Searching,
                ConformanceEffect::OpenSearch,
            ),
            pending_step(
                ConformanceAction::SelectSearchCandidate,
                ConformanceEffect::SubmitConnection,
            ),
            pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None),
            settlement_step(
                settlement,
                match settlement {
                    ConformanceSettlement::Applied | ConformanceSettlement::Noop => {
                        ConformancePhase::Idle
                    }
                    ConformanceSettlement::Rejected
                    | ConformanceSettlement::Conflict
                    | ConformanceSettlement::TransportFailure => ConformancePhase::Searching,
                },
            ),
        ],
    }
}

#[must_use]
pub fn node_drag_scenario(settlement: ConformanceSettlement) -> ConformanceScenario {
    ConformanceScenario {
        id: ConformanceScenarioId::NodeDrag(settlement),
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
            ),
            pending_step(
                ConformanceAction::Release,
                ConformanceEffect::SubmitNodeMove,
            ),
            pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None),
            settlement_step(settlement, ConformancePhase::Idle),
        ],
    }
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
            ),
            pending_step(
                ConformanceAction::Release,
                ConformanceEffect::SubmitNodeMove,
            ),
            pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None),
            pending_step(ConformanceAction::DrainPrefix, ConformanceEffect::None),
            pending_step(ConformanceAction::AdvanceFrame, ConformanceEffect::None),
            settlement_step(ConformanceSettlement::Applied, ConformancePhase::Idle),
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
            ),
            released_step(
                ConformanceAction::Release,
                ConformancePhase::Idle,
                ConformanceEffect::None,
            ),
        ],
    }
}

/// Returns native ownership replacement and loss scenarios shared by real adapters.
#[must_use]
pub fn capture_ownership_scenarios() -> [ConformanceScenario; 2] {
    [
        capture_terminal_scenario(
            ConformanceScenarioId::CaptureLost,
            "capture-lost",
            ConformanceAction::LoseCapture,
        ),
        capture_terminal_scenario(
            ConformanceScenarioId::OwnerReplaced,
            "owner-replaced",
            ConformanceAction::ReplaceOwner,
        ),
    ]
}

fn capture_terminal_scenario(
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
            ),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            ),
            terminal_step(
                terminal_action,
                ConformanceTerminal::Cancelled,
                ConformanceGeometry::Available,
            ),
            ConformanceStep::new(
                ConformanceAction::AdvanceFrame,
                NormalizedObservation::new(
                    ConformancePhase::Idle,
                    ConformanceEffect::None,
                    ConformanceOwnership::Released,
                    ConformanceTicket::None,
                    0,
                    ConformanceGeometry::Available,
                )
                .with_terminal(ConformanceTerminal::Cancelled),
            ),
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
            ConformanceGeometry::Available,
        ),
        terminal_scenario(
            ConformanceScenarioId::OutsideClick,
            "outside-click",
            ConformanceAction::Cancel(ConformanceCancelReason::OutsideClick),
            ConformanceTerminal::Cancelled,
            ConformanceGeometry::Available,
        ),
        terminal_scenario(
            ConformanceScenarioId::SourceRetired,
            "source-retired",
            ConformanceAction::RetireSource,
            ConformanceTerminal::OwnerRetired,
            ConformanceGeometry::Available,
        ),
        terminal_scenario(
            ConformanceScenarioId::HostClosed,
            "host-closed",
            ConformanceAction::CloseHost,
            ConformanceTerminal::HostClosed,
            ConformanceGeometry::Unavailable,
        ),
    ]
}

fn terminal_scenario(
    id: ConformanceScenarioId,
    display_name: &'static str,
    terminal_action: ConformanceAction,
    terminal: ConformanceTerminal,
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
            ),
            held_step(
                ConformanceAction::CrossDragThreshold,
                ConformancePhase::ConnectionDrag,
                ConformanceEffect::ProbeConnection,
            ),
            terminal_step(terminal_action, terminal, geometry),
        ],
    }
}

const fn terminal_step(
    action: ConformanceAction,
    terminal: ConformanceTerminal,
    geometry: ConformanceGeometry,
) -> ConformanceStep {
    ConformanceStep::new(
        action,
        NormalizedObservation::new(
            ConformancePhase::Idle,
            ConformanceEffect::ReleaseNativeOwnership,
            ConformanceOwnership::Released,
            ConformanceTicket::None,
            0,
            geometry,
        )
        .with_terminal(terminal),
    )
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
            ConformanceOwnership::Held,
            ConformanceTicket::None,
            0,
            ConformanceGeometry::Available,
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
            ConformanceGeometry::Available,
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
            ConformanceGeometry::Available,
        )
        .with_optimistic_presentation(),
    )
}

const fn settlement_step(
    settlement: ConformanceSettlement,
    phase: ConformancePhase,
) -> ConformanceStep {
    let (effect, terminal, revision_delta) = match settlement {
        ConformanceSettlement::Applied => {
            (ConformanceEffect::None, ConformanceTerminal::Applied, 1)
        }
        ConformanceSettlement::Noop => (ConformanceEffect::None, ConformanceTerminal::Noop, 0),
        ConformanceSettlement::Rejected => (
            ConformanceEffect::RestorePresentation,
            ConformanceTerminal::Rejected,
            0,
        ),
        ConformanceSettlement::Conflict => (
            ConformanceEffect::RestorePresentation,
            ConformanceTerminal::Conflict,
            0,
        ),
        ConformanceSettlement::TransportFailure => (
            ConformanceEffect::RestorePresentation,
            ConformanceTerminal::TransportFailure,
            0,
        ),
    };
    ConformanceStep::new(
        ConformanceAction::Settle(settlement),
        NormalizedObservation::new(
            phase,
            effect,
            ConformanceOwnership::Released,
            ConformanceTicket::None,
            revision_delta,
            ConformanceGeometry::Available,
        )
        .with_terminal(terminal),
    )
}
