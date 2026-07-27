use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, EdgeId, NodeId, PortId};

use crate::rules::EdgeEndpoint;
use crate::runtime::adapter_protocol::ViewPoint;
use crate::runtime::geometry::{EdgeEndpointPosition, EdgePosition, HandlePosition};
use crate::runtime::graph_runtime::{EdgeInstanceKey, NodeLifetimeKey};
use crate::runtime::view_session::ViewProvenance;
#[cfg(test)]
use crate::runtime::viewport::ViewportTransform;
use crate::runtime::viewport::{ViewportPointerSample, ViewportRevision, ViewportSnapshot};

use super::intent::NodeMoveCohort;
use super::{
    ConnectionGestureBinding, ConnectionProbe, InteractionGeneration, InteractionTicketId,
};

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
pub(crate) struct InteractionPosition {
    provenance: ViewProvenance,
    viewport_revision: ViewportRevision,
    view: ViewPoint,
    canvas: CanvasPoint,
}

impl InteractionPosition {
    pub(super) fn from_sample(sample: ViewportPointerSample) -> Option<Self> {
        let snapshot = sample.snapshot();
        let view = sample.raw();
        let canvas = view.to_canvas_point(snapshot.transform())?;
        Some(Self {
            provenance: snapshot.provenance(),
            viewport_revision: snapshot.revision(),
            view,
            canvas,
        })
    }

    #[cfg(test)]
    pub(crate) fn from_view_for_test(
        view: ViewPoint,
        transform: ViewportTransform,
    ) -> Option<Self> {
        let snapshot = ViewportSnapshot::new(
            ViewProvenance::unavailable(),
            ViewportRevision::initial(),
            transform,
        );
        Self::from_sample(ViewportPointerSample::new(snapshot, view)?)
    }

    pub(super) fn reproject(self, snapshot: ViewportSnapshot) -> Option<Self> {
        if self.provenance != snapshot.provenance() {
            return None;
        }
        let canvas = self.view.to_canvas_point(snapshot.transform())?;
        Some(Self {
            provenance: snapshot.provenance(),
            viewport_revision: snapshot.revision(),
            view: self.view,
            canvas,
        })
    }

    pub(super) const fn provenance(self) -> ViewProvenance {
        self.provenance
    }

    pub(super) const fn viewport_revision(self) -> ViewportRevision {
        self.viewport_revision
    }

    pub(crate) fn view(self) -> ViewPoint {
        self.view
    }

    pub(crate) fn canvas(self) -> CanvasPoint {
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

/// Probe result captured by one immutable interaction-wire projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionProbeValidity {
    Unresolved,
    Invalid,
    Valid,
}

impl ConnectionProbeValidity {
    fn from_probe(probe: &ConnectionProbe) -> Self {
        match probe {
            ConnectionProbe::None => Self::Unresolved,
            ConnectionProbe::Invalid { .. } => Self::Invalid,
            ConnectionProbe::Valid { .. } => Self::Valid,
        }
    }

    pub const fn is_invalid(self) -> bool {
        matches!(self, Self::Invalid)
    }
}

/// Stable renderer-neutral wire geometry for one connection interaction generation.
///
/// Both endpoint directions are captured when the interaction begins. A valid probe contributes
/// its exact probe-time route anchor and outward direction; unresolved and invalid probes use the
/// current semantic pointer with the captured moving direction. Native adapters must paint this
/// projection rather than resolving the fixed endpoint from a later geometry snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionWireProjection {
    generation: InteractionGeneration,
    endpoints: EdgePosition,
    fixed_endpoint: EdgeEndpoint,
    probe_validity: ConnectionProbeValidity,
    suppressed_edge: Option<EdgeInstanceKey>,
}

impl ConnectionWireProjection {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        generation: InteractionGeneration,
        fixed_endpoint: EdgeEndpoint,
        fixed_anchor: CanvasPoint,
        fixed_outward: HandlePosition,
        moving_outward: HandlePosition,
        pointer: InteractionPosition,
        probe: &ConnectionProbe,
        suppressed_edge: Option<EdgeInstanceKey>,
    ) -> Self {
        let fixed = EdgeEndpointPosition {
            point: fixed_anchor,
            position: fixed_outward,
        };
        let moving = match probe {
            ConnectionProbe::Valid { target, .. } => EdgeEndpointPosition {
                point: target.route_anchor(),
                position: target.outward(),
            },
            ConnectionProbe::None | ConnectionProbe::Invalid { .. } => EdgeEndpointPosition {
                point: pointer.canvas(),
                position: moving_outward,
            },
        };
        let endpoints = match fixed_endpoint {
            EdgeEndpoint::From => EdgePosition {
                source: fixed,
                target: moving,
            },
            EdgeEndpoint::To => EdgePosition {
                source: moving,
                target: fixed,
            },
        };
        Self {
            generation,
            endpoints,
            fixed_endpoint,
            probe_validity: ConnectionProbeValidity::from_probe(probe),
            suppressed_edge,
        }
    }

    pub const fn generation(self) -> InteractionGeneration {
        self.generation
    }

    pub const fn endpoints(self) -> EdgePosition {
        self.endpoints
    }

    pub const fn fixed_endpoint(self) -> EdgeEndpointPosition {
        match self.fixed_endpoint {
            EdgeEndpoint::From => self.endpoints.source,
            EdgeEndpoint::To => self.endpoints.target,
        }
    }

    pub const fn moving_endpoint(self) -> EdgeEndpointPosition {
        match self.fixed_endpoint {
            EdgeEndpoint::From => self.endpoints.target,
            EdgeEndpoint::To => self.endpoints.source,
        }
    }

    pub const fn probe_validity(self) -> ConnectionProbeValidity {
        self.probe_validity
    }

    pub const fn suppressed_edge(self) -> Option<EdgeInstanceKey> {
        self.suppressed_edge
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionPresentation {
    generation: InteractionGeneration,
    binding: ConnectionGestureBinding,
    wire: ConnectionWireProjection,
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
        binding: ConnectionGestureBinding,
        fixed_endpoint: EdgeEndpoint,
        fixed_anchor: CanvasPoint,
        fixed_outward: HandlePosition,
        moving_outward: HandlePosition,
        pointer: InteractionPosition,
        probe: ConnectionProbe,
        suppressed_edge: Option<EdgeInstanceKey>,
        search: Option<ConnectionSearchStatus>,
        release_pending: bool,
        ticket: Option<InteractionTicketId>,
    ) -> Self {
        let wire = ConnectionWireProjection::new(
            generation,
            fixed_endpoint,
            fixed_anchor,
            fixed_outward,
            moving_outward,
            pointer,
            &probe,
            suppressed_edge,
        );
        Self {
            generation,
            binding,
            wire,
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

    pub fn binding(&self) -> ConnectionGestureBinding {
        self.binding.clone()
    }

    pub fn fixed_port(&self) -> PortId {
        self.binding.fixed_port_instance().port()
    }

    pub fn fixed_anchor(&self) -> CanvasPoint {
        self.wire.fixed_endpoint().point
    }

    pub fn fixed_outward(&self) -> HandlePosition {
        self.wire.fixed_endpoint().position
    }

    pub fn pointer_view(&self) -> ViewPoint {
        self.pointer.view()
    }

    pub fn pointer_canvas(&self) -> CanvasPoint {
        self.pointer.canvas()
    }

    pub fn probe(&self) -> &ConnectionProbe {
        &self.probe
    }

    pub fn preview_target(&self) -> CanvasPoint {
        self.wire.moving_endpoint().point
    }

    pub fn suppressed_edge(&self) -> Option<EdgeId> {
        self.wire.suppressed_edge().map(EdgeInstanceKey::edge)
    }

    pub fn wire(&self) -> ConnectionWireProjection {
        self.wire
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
    cohort: NodeMoveCohort,
    delta: CanvasPoint,
    guides: Arc<[AlignmentGuide]>,
    release_pending: bool,
    ticket: Option<InteractionTicketId>,
}

impl NodeDragPresentation {
    pub(super) fn new(
        generation: InteractionGeneration,
        cohort: NodeMoveCohort,
        delta: CanvasPoint,
        guides: Arc<[AlignmentGuide]>,
        release_pending: bool,
        ticket: Option<InteractionTicketId>,
    ) -> Self {
        Self {
            generation,
            cohort,
            delta,
            guides,
            release_pending,
            ticket,
        }
    }

    pub fn generation(&self) -> InteractionGeneration {
        self.generation
    }

    pub fn nodes(&self) -> &[NodeId] {
        self.cohort.nodes()
    }

    pub fn members(&self) -> &[NodeLifetimeKey] {
        self.cohort.members()
    }

    pub fn delta(&self) -> CanvasPoint {
        self.delta
    }

    pub fn guides(&self) -> &[AlignmentGuide] {
        &self.guides
    }

    pub fn is_release_pending(&self) -> bool {
        self.release_pending
    }

    pub fn ticket(&self) -> Option<InteractionTicketId> {
        self.ticket
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignmentGuideAxis {
    Vertical,
    Horizontal,
}

/// One runtime-issued alignment guide in canvas space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlignmentGuide {
    axis: AlignmentGuideAxis,
    canvas_coordinate: f32,
}

impl AlignmentGuide {
    pub(super) fn new(axis: AlignmentGuideAxis, canvas_coordinate: f32) -> Option<Self> {
        canvas_coordinate.is_finite().then_some(Self {
            axis,
            canvas_coordinate,
        })
    }

    pub fn axis(self) -> AlignmentGuideAxis {
        self.axis
    }

    pub fn canvas_coordinate(self) -> f32 {
        self.canvas_coordinate
    }
}

#[derive(Debug, Clone)]
pub struct CanvasInteractionPresentation {
    phase: CanvasInteractionPhase,
    generation: Option<InteractionGeneration>,
    connection_intent: Option<ConnectionGestureBinding>,
    connection: Option<ConnectionPresentation>,
    node_drag: Option<NodeDragPresentation>,
    marquee: Option<CanvasRect>,
    active_edge: Option<EdgeInstanceKey>,
    hovered_edge: Option<EdgeInstanceKey>,
    terminal: Option<InteractionTerminalReason>,
}

impl CanvasInteractionPresentation {
    pub(super) fn new(phase: CanvasInteractionPhase) -> Self {
        Self {
            phase,
            generation: None,
            connection_intent: None,
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

    pub(super) fn with_connection_intent(
        mut self,
        intent: Option<ConnectionGestureBinding>,
    ) -> Self {
        self.connection_intent = intent;
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

    pub(super) fn with_active_edge(mut self, edge: EdgeInstanceKey) -> Self {
        self.active_edge = Some(edge);
        self
    }

    pub(super) fn with_hovered_edge(mut self, edge: Option<EdgeInstanceKey>) -> Self {
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

    /// Connection intent owned by the current interaction, including the armed pre-drag phase.
    pub fn connection_intent(&self) -> Option<ConnectionGestureBinding> {
        self.connection_intent.clone()
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

    pub fn active_edge(&self) -> Option<EdgeInstanceKey> {
        self.active_edge
    }

    pub fn hovered_edge(&self) -> Option<EdgeInstanceKey> {
        self.hovered_edge
    }

    pub fn terminal(&self) -> Option<InteractionTerminalReason> {
        self.terminal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interaction_position_normalizes_view_space_at_common_zoom_levels() {
        let view = ViewPoint::new(40.0, 20.0);
        for (zoom, expected) in [
            (0.5, CanvasPoint { x: 80.0, y: 40.0 }),
            (1.0, CanvasPoint { x: 40.0, y: 20.0 }),
            (2.0, CanvasPoint { x: 20.0, y: 10.0 }),
        ] {
            let transform = ViewportTransform::new(CanvasPoint::default(), zoom).unwrap();
            let position = InteractionPosition::from_view_for_test(view, transform).unwrap();
            assert_eq!(position.view(), view);
            assert_eq!(position.canvas(), expected);
        }
    }
}
