#![allow(dead_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceReconnectEndpoint {
    Source,
    Target,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceTarget {
    Canvas,
    NativeControl(u128),
    NodeDragHandle(u128),
    Port(u128),
    Edge(u128),
    ReconnectHandle {
        edge: u128,
        endpoint: TraceReconnectEndpoint,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceCandidate {
    None,
    Invalid { port: u128, reason: &'static str },
    Valid { port: u128 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceReceipt {
    Applied,
    Noop,
    Rejected,
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceCancelReason {
    Escape,
    OutsideClick,
    SourceRetired,
    HostClosed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceInput {
    Press { target: TraceTarget },
    Move { x: i32, y: i32 },
    ThresholdCrossed,
    Candidate(TraceCandidate),
    Release,
    SearchSelected { port: u128 },
    Receipt(TraceReceipt),
    Cancel(TraceCancelReason),
    Retire,
    Close,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TracePhase {
    Idle,
    Armed,
    NodeDrag,
    ConnectionDrag,
    Candidate,
    Searching,
    Committing,
    EdgeActive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceEffect {
    None,
    ProbeConnection,
    PickEdge,
    OpenSearch,
    SubmitConnection,
    SubmitNodeMove,
    RestorePresentation,
    ReleaseNativeOwnership,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceStep {
    pub input: TraceInput,
    pub phase: TracePhase,
    pub effect: TraceEffect,
}

impl TraceStep {
    pub const fn new(input: TraceInput, phase: TracePhase, effect: TraceEffect) -> Self {
        Self {
            input,
            phase,
            effect,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalInteractionTrace {
    pub name: &'static str,
    pub steps: Vec<TraceStep>,
}

pub fn new_connection_trace(receipt: TraceReceipt) -> CanonicalInteractionTrace {
    CanonicalInteractionTrace {
        name: "new-connection",
        steps: vec![
            TraceStep::new(
                TraceInput::Press {
                    target: TraceTarget::Port(1),
                },
                TracePhase::Armed,
                TraceEffect::None,
            ),
            TraceStep::new(
                TraceInput::ThresholdCrossed,
                TracePhase::ConnectionDrag,
                TraceEffect::ProbeConnection,
            ),
            TraceStep::new(
                TraceInput::Move { x: 120, y: 80 },
                TracePhase::ConnectionDrag,
                TraceEffect::ProbeConnection,
            ),
            TraceStep::new(
                TraceInput::Candidate(TraceCandidate::Invalid {
                    port: 2,
                    reason: "direction",
                }),
                TracePhase::ConnectionDrag,
                TraceEffect::None,
            ),
            TraceStep::new(
                TraceInput::Candidate(TraceCandidate::Valid { port: 3 }),
                TracePhase::Candidate,
                TraceEffect::None,
            ),
            TraceStep::new(
                TraceInput::Release,
                TracePhase::Committing,
                TraceEffect::SubmitConnection,
            ),
            TraceStep::new(
                TraceInput::Receipt(receipt),
                TracePhase::Idle,
                match receipt {
                    TraceReceipt::Applied | TraceReceipt::Noop => TraceEffect::None,
                    TraceReceipt::Rejected | TraceReceipt::Conflict => {
                        TraceEffect::RestorePresentation
                    }
                },
            ),
        ],
    }
}

pub fn reconnect_trace(
    endpoint: TraceReconnectEndpoint,
    receipt: TraceReceipt,
) -> CanonicalInteractionTrace {
    let mut trace = new_connection_trace(receipt);
    trace.name = match endpoint {
        TraceReconnectEndpoint::Source => "reconnect-source",
        TraceReconnectEndpoint::Target => "reconnect-target",
    };
    trace.steps[0] = TraceStep::new(
        TraceInput::Press {
            target: TraceTarget::ReconnectHandle { edge: 9, endpoint },
        },
        TracePhase::Armed,
        TraceEffect::None,
    );
    trace
}

pub fn create_from_wire_trace(receipt: TraceReceipt) -> CanonicalInteractionTrace {
    CanonicalInteractionTrace {
        name: "create-from-wire",
        steps: vec![
            TraceStep::new(
                TraceInput::Press {
                    target: TraceTarget::Port(1),
                },
                TracePhase::Armed,
                TraceEffect::None,
            ),
            TraceStep::new(
                TraceInput::ThresholdCrossed,
                TracePhase::ConnectionDrag,
                TraceEffect::ProbeConnection,
            ),
            TraceStep::new(
                TraceInput::Candidate(TraceCandidate::None),
                TracePhase::ConnectionDrag,
                TraceEffect::None,
            ),
            TraceStep::new(
                TraceInput::Release,
                TracePhase::Searching,
                TraceEffect::OpenSearch,
            ),
            TraceStep::new(
                TraceInput::SearchSelected { port: 4 },
                TracePhase::Committing,
                TraceEffect::SubmitConnection,
            ),
            TraceStep::new(
                TraceInput::Receipt(receipt),
                match receipt {
                    TraceReceipt::Applied | TraceReceipt::Noop => TracePhase::Idle,
                    TraceReceipt::Rejected | TraceReceipt::Conflict => TracePhase::Searching,
                },
                match receipt {
                    TraceReceipt::Applied | TraceReceipt::Noop => TraceEffect::None,
                    TraceReceipt::Rejected | TraceReceipt::Conflict => {
                        TraceEffect::RestorePresentation
                    }
                },
            ),
        ],
    }
}

pub fn control_and_drag_handle_traces() -> [CanonicalInteractionTrace; 2] {
    [
        CanonicalInteractionTrace {
            name: "native-control",
            steps: vec![TraceStep::new(
                TraceInput::Press {
                    target: TraceTarget::NativeControl(7),
                },
                TracePhase::Idle,
                TraceEffect::None,
            )],
        },
        CanonicalInteractionTrace {
            name: "node-drag-handle",
            steps: vec![
                TraceStep::new(
                    TraceInput::Press {
                        target: TraceTarget::NodeDragHandle(7),
                    },
                    TracePhase::Armed,
                    TraceEffect::None,
                ),
                TraceStep::new(
                    TraceInput::ThresholdCrossed,
                    TracePhase::NodeDrag,
                    TraceEffect::None,
                ),
                TraceStep::new(
                    TraceInput::Release,
                    TracePhase::Committing,
                    TraceEffect::SubmitNodeMove,
                ),
            ],
        },
    ]
}

pub fn edge_pick_trace() -> CanonicalInteractionTrace {
    CanonicalInteractionTrace {
        name: "edge-pick",
        steps: vec![
            TraceStep::new(
                TraceInput::Press {
                    target: TraceTarget::Edge(9),
                },
                TracePhase::EdgeActive,
                TraceEffect::PickEdge,
            ),
            TraceStep::new(TraceInput::Release, TracePhase::Idle, TraceEffect::None),
        ],
    }
}

pub fn terminal_ownership_traces() -> [CanonicalInteractionTrace; 4] {
    [
        terminal_trace("escape", TraceInput::Cancel(TraceCancelReason::Escape)),
        terminal_trace("source-retired", TraceInput::Retire),
        terminal_trace("host-closed", TraceInput::Close),
        terminal_trace(
            "outside-click",
            TraceInput::Cancel(TraceCancelReason::OutsideClick),
        ),
    ]
}

fn terminal_trace(name: &'static str, terminal: TraceInput) -> CanonicalInteractionTrace {
    CanonicalInteractionTrace {
        name,
        steps: vec![
            TraceStep::new(
                TraceInput::Press {
                    target: TraceTarget::Port(1),
                },
                TracePhase::Armed,
                TraceEffect::None,
            ),
            TraceStep::new(
                TraceInput::ThresholdCrossed,
                TracePhase::ConnectionDrag,
                TraceEffect::ProbeConnection,
            ),
            TraceStep::new(
                terminal,
                TracePhase::Idle,
                TraceEffect::ReleaseNativeOwnership,
            ),
        ],
    }
}
