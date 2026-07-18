use jellyflow_core::core::{CanvasPoint, CanvasRect, EdgeId, NodeId, PortId};

use crate::runtime::adapter_protocol::ViewPoint;
use crate::runtime::geometry::HandlePosition;

use super::{ConnectionGestureIntent, ConnectionProbe, InteractionGeneration, InteractionTicketId};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CanvasInteractionPhase {
    #[default]
    Idle,
    Armed,
    NodeDrag,
    Pan,
    Marquee,
    ConnectionDrag,
    ConnectionSearch,
    Committing,
    EdgeActive,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionTerminalReason {
    Applied,
    Noop,
    Rejected,
    Conflict,
    TransportFailure,
    Cancelled,
    OwnerRetired,
    ViewClosed,
    BudgetExhausted,
    GeometryIndeterminate,
    IdentityExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InteractionPosition {
    view: ViewPoint,
    canvas: CanvasPoint,
}

impl InteractionPosition {
    pub fn new(view: ViewPoint, canvas: CanvasPoint) -> Option<Self> {
        (view.is_finite() && canvas.is_finite()).then_some(Self { view, canvas })
    }

    pub fn view(self) -> ViewPoint {
        self.view
    }

    pub fn canvas(self) -> CanvasPoint {
        self.canvas
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionSearchStatus {
    Loading,
    Ready { result_count: usize },
    Empty,
    Error,
    CommitRejected,
}

#[derive(Debug, Clone)]
pub struct ConnectionPresentation {
    generation: InteractionGeneration,
    intent: ConnectionGestureIntent,
    fixed_anchor: CanvasPoint,
    fixed_outward: HandlePosition,
    pointer: InteractionPosition,
    probe: ConnectionProbe,
    search: Option<ConnectionSearchStatus>,
    release_pending: bool,
    ticket: Option<InteractionTicketId>,
}

impl ConnectionPresentation {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        generation: InteractionGeneration,
        intent: ConnectionGestureIntent,
        fixed_anchor: CanvasPoint,
        fixed_outward: HandlePosition,
        pointer: InteractionPosition,
        probe: ConnectionProbe,
        search: Option<ConnectionSearchStatus>,
        release_pending: bool,
        ticket: Option<InteractionTicketId>,
    ) -> Self {
        Self {
            generation,
            intent,
            fixed_anchor,
            fixed_outward,
            pointer,
            probe,
            search,
            release_pending,
            ticket,
        }
    }

    pub fn generation(&self) -> InteractionGeneration {
        self.generation
    }

    pub fn intent(&self) -> ConnectionGestureIntent {
        self.intent
    }

    pub fn fixed_port(&self) -> PortId {
        self.intent.fixed_port()
    }

    pub fn fixed_anchor(&self) -> CanvasPoint {
        self.fixed_anchor
    }

    pub fn fixed_outward(&self) -> HandlePosition {
        self.fixed_outward
    }

    pub fn pointer(&self) -> InteractionPosition {
        self.pointer
    }

    pub fn probe(&self) -> &ConnectionProbe {
        &self.probe
    }

    pub fn preview_target(&self) -> CanvasPoint {
        match &self.probe {
            ConnectionProbe::Valid { target, .. } => target.route_anchor(),
            ConnectionProbe::None | ConnectionProbe::Invalid { .. } => self.pointer.canvas(),
        }
    }

    pub fn suppressed_edge(&self) -> Option<EdgeId> {
        self.intent.detached_port().map(|_| self.intent.edge())
    }

    pub fn search(&self) -> Option<ConnectionSearchStatus> {
        self.search
    }

    pub fn is_release_pending(&self) -> bool {
        self.release_pending
    }

    pub fn ticket(&self) -> Option<InteractionTicketId> {
        self.ticket
    }
}

#[derive(Debug, Clone)]
pub struct NodeDragPresentation {
    generation: InteractionGeneration,
    nodes: Box<[NodeId]>,
    delta: CanvasPoint,
    guides: Box<[AlignmentGuide]>,
    ticket: Option<InteractionTicketId>,
}

impl NodeDragPresentation {
    pub(super) fn new(
        generation: InteractionGeneration,
        nodes: Box<[NodeId]>,
        delta: CanvasPoint,
        guides: Box<[AlignmentGuide]>,
        ticket: Option<InteractionTicketId>,
    ) -> Self {
        Self {
            generation,
            nodes,
            delta,
            guides,
            ticket,
        }
    }

    pub fn generation(&self) -> InteractionGeneration {
        self.generation
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    pub fn delta(&self) -> CanvasPoint {
        self.delta
    }

    pub fn guides(&self) -> &[AlignmentGuide] {
        &self.guides
    }

    pub fn ticket(&self) -> Option<InteractionTicketId> {
        self.ticket
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlignmentGuide {
    Vertical { canvas_x: f32 },
    Horizontal { canvas_y: f32 },
}

#[derive(Debug, Clone)]
pub struct CanvasInteractionPresentation {
    phase: CanvasInteractionPhase,
    generation: Option<InteractionGeneration>,
    connection: Option<ConnectionPresentation>,
    node_drag: Option<NodeDragPresentation>,
    marquee: Option<CanvasRect>,
    active_edge: Option<EdgeId>,
    hovered_edge: Option<EdgeId>,
    terminal: Option<InteractionTerminalReason>,
}

impl CanvasInteractionPresentation {
    pub(super) fn new(phase: CanvasInteractionPhase) -> Self {
        Self {
            phase,
            generation: None,
            connection: None,
            node_drag: None,
            marquee: None,
            active_edge: None,
            hovered_edge: None,
            terminal: None,
        }
    }

    pub(super) fn with_generation(mut self, generation: Option<InteractionGeneration>) -> Self {
        self.generation = generation;
        self
    }

    pub(super) fn with_connection(mut self, connection: ConnectionPresentation) -> Self {
        self.connection = Some(connection);
        self
    }

    pub(super) fn with_node_drag(mut self, node_drag: NodeDragPresentation) -> Self {
        self.node_drag = Some(node_drag);
        self
    }

    pub(super) fn with_marquee(mut self, marquee: CanvasRect) -> Self {
        self.marquee = Some(marquee);
        self
    }

    pub(super) fn with_active_edge(mut self, edge: EdgeId) -> Self {
        self.active_edge = Some(edge);
        self
    }

    pub(super) fn with_hovered_edge(mut self, edge: Option<EdgeId>) -> Self {
        self.hovered_edge = edge;
        self
    }

    pub(super) fn with_terminal(mut self, terminal: Option<InteractionTerminalReason>) -> Self {
        self.terminal = terminal;
        self
    }

    pub fn phase(&self) -> CanvasInteractionPhase {
        self.phase
    }

    pub fn generation(&self) -> Option<InteractionGeneration> {
        self.generation
    }

    pub fn connection(&self) -> Option<&ConnectionPresentation> {
        self.connection.as_ref()
    }

    pub fn node_drag(&self) -> Option<&NodeDragPresentation> {
        self.node_drag.as_ref()
    }

    pub fn marquee(&self) -> Option<CanvasRect> {
        self.marquee
    }

    pub fn active_edge(&self) -> Option<EdgeId> {
        self.active_edge
    }

    pub fn hovered_edge(&self) -> Option<EdgeId> {
        self.hovered_edge
    }

    pub fn terminal(&self) -> Option<InteractionTerminalReason> {
        self.terminal
    }
}
