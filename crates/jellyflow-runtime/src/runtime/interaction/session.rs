use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, NodeId, PortDirection, PortId,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::rules::{Diagnostic, EdgeEndpoint};
use crate::runtime::adapter_protocol::{PublishedEdgeRoute, ViewDelta, ViewPoint, ViewSize};
use crate::runtime::commands::{
    CommandExecutionCompletion, CommandExecutionKind, GraphCommandIdentity,
};
use crate::runtime::geometry::{EdgeEndpointPosition, HandlePosition};
use crate::runtime::graph_runtime::{
    CreateFromWireCandidates, CreateFromWireQueryError, EdgeInstanceKey, GraphRuntimeError,
    GraphRuntimePolicy, GraphSnapshot, GraphVersion, NodeLifetimeKey, PortConnectionDragIntent,
    PortInstanceKey,
};
use crate::runtime::unwind::{PanicPayload, resume};
use crate::runtime::view_session::{
    AlignmentSnapQueryReceipt, ConnectionProbeCursorStaleReason, ConnectionProbeQuery,
    ConnectionProbeQueryError, ConnectionProbeQueryOutcome, ConnectionProbeQueryReceipt,
    ConnectionProbeTargeting, EdgePickCursorStaleReason, EdgePickQuery, EdgePickQueryError,
    EdgePickQueryOutcome, EdgePickQueryReceipt, InteractionQueryBinding,
    NodeIntersectionCursorStaleReason, NodeIntersectionQuery, NodeIntersectionQueryError,
    NodeIntersectionQueryOutcome, NodeIntersectionQueryReceipt, ViewCreateFromWireQueryError,
    ViewGeneration, ViewId, ViewProvenance, ViewSession, ViewSessionError, WeakViewSession,
};
use crate::runtime::viewport::{
    ViewportIntent, ViewportPanRequest, ViewportPointerSample, ViewportSnapshot,
};
#[cfg(test)]
use crate::runtime::viewport::{ViewportRevision, ViewportTransform};
use crate::schema::NodeCreation;

use super::intent::{
    ConnectionCommitTarget, EdgeDeletionIntent, FreshGraphMutation, FreshGraphMutationBindError,
    NodeMoveCohort, RelativeNodeMoveIntent, SelectionBaseline, SelectionDeletionIntent,
    feasible_cohort_delta,
};
use super::navigation::{AlignmentSnapLocks, AlignmentSnapQuery, AlignmentSnapQueryOutcome};
use super::presentation::{
    AlignmentGuide, CanvasInteractionPhase, CanvasInteractionPresentation, ConnectionPresentation,
    ConnectionSearchStatus, InteractionPosition, InteractionTerminalReason, NodeDragPresentation,
};
use super::query_work::{PendingInteractionQuery, PendingInteractionQueryKind, QueryWork};
use super::{ConnectionGestureBinding, ConnectionProbe, ConnectionProbeTargetIdentity};

const MAX_AUTO_PAN_ELAPSED_SECONDS: f32 = 0.05;
// A mutable view may invalidate a cursor, but one logical query must still terminate.
const MAX_STALE_QUERY_RESTARTS: u8 = 8;

/// Screen-pixel acquire/release bands for hysteretic node alignment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlignmentSnapConfig {
    acquire_tolerance_in_view: f32,
    release_tolerance_in_view: f32,
}

impl AlignmentSnapConfig {
    pub fn new(acquire_tolerance_in_view: f32, release_tolerance_in_view: f32) -> Option<Self> {
        (acquire_tolerance_in_view.is_finite()
            && acquire_tolerance_in_view > 0.0
            && release_tolerance_in_view.is_finite()
            && release_tolerance_in_view >= acquire_tolerance_in_view)
            .then_some(Self {
                acquire_tolerance_in_view,
                release_tolerance_in_view,
            })
    }

    pub fn acquire_tolerance_in_view(self) -> f32 {
        self.acquire_tolerance_in_view
    }

    pub fn release_tolerance_in_view(self) -> f32 {
        self.release_tolerance_in_view
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasInteractionConfig {
    pub node_click_tolerance_in_view: f32,
    pub node_drag_threshold_in_view: f32,
    pub connection_drag_threshold_in_view: f32,
    pub canvas_click_tolerance_in_view: f32,
    pub canvas_drag_threshold_in_view: f32,
    pub connection_acquire_radius_in_view: f32,
    pub connection_release_radius_in_view: f32,
    pub edge_pick_radius_in_view: f32,
    pub keyboard_nudge_step_in_canvas: f32,
    pub keyboard_nudge_accelerated_multiplier: f32,
    pub grid_size_in_canvas: Option<CanvasSize>,
    pub alignment_snap: Option<AlignmentSnapConfig>,
    pub auto_pan_band_in_view: f32,
    pub auto_pan_speed_in_view_per_second: f32,
    pub query_entry_budget: usize,
}

impl Default for CanvasInteractionConfig {
    fn default() -> Self {
        Self {
            node_click_tolerance_in_view: 2.0,
            node_drag_threshold_in_view: 4.0,
            connection_drag_threshold_in_view: 3.0,
            canvas_click_tolerance_in_view: 2.0,
            canvas_drag_threshold_in_view: 3.0,
            connection_acquire_radius_in_view: 12.0,
            connection_release_radius_in_view: 18.0,
            edge_pick_radius_in_view: 10.0,
            keyboard_nudge_step_in_canvas: 10.0,
            keyboard_nudge_accelerated_multiplier: 5.0,
            grid_size_in_canvas: None,
            alignment_snap: None,
            auto_pan_band_in_view: 36.0,
            auto_pan_speed_in_view_per_second: 720.0,
            query_entry_budget: 256,
        }
    }
}

impl CanvasInteractionConfig {
    pub fn is_valid(self) -> bool {
        self.node_click_tolerance_in_view.is_finite()
            && self.node_click_tolerance_in_view >= 0.0
            && self.node_drag_threshold_in_view.is_finite()
            && self.node_drag_threshold_in_view >= 0.0
            && self.connection_drag_threshold_in_view.is_finite()
            && self.connection_drag_threshold_in_view >= 0.0
            && self.canvas_click_tolerance_in_view.is_finite()
            && self.canvas_click_tolerance_in_view >= 0.0
            && self.canvas_drag_threshold_in_view.is_finite()
            && self.canvas_drag_threshold_in_view >= 0.0
            && self.connection_acquire_radius_in_view.is_finite()
            && self.connection_acquire_radius_in_view > 0.0
            && self.connection_release_radius_in_view.is_finite()
            && self.connection_release_radius_in_view >= self.connection_acquire_radius_in_view
            && self.edge_pick_radius_in_view.is_finite()
            && self.edge_pick_radius_in_view > 0.0
            && self.keyboard_nudge_step_in_canvas.is_finite()
            && self.keyboard_nudge_step_in_canvas > 0.0
            && self.keyboard_nudge_accelerated_multiplier.is_finite()
            && self.keyboard_nudge_accelerated_multiplier >= 1.0
            && self.auto_pan_band_in_view.is_finite()
            && self.auto_pan_band_in_view > 0.0
            && self.auto_pan_speed_in_view_per_second.is_finite()
            && self.auto_pan_speed_in_view_per_second >= 0.0
            && self.query_entry_budget > 0
            && self
                .grid_size_in_canvas
                .is_none_or(CanvasSize::is_positive_finite)
            && self.alignment_snap.is_none_or(|snap| {
                AlignmentSnapConfig::new(
                    snap.acquire_tolerance_in_view,
                    snap.release_tolerance_in_view,
                ) == Some(snap)
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InteractionSessionId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct InteractionIdentity {
    session: InteractionSessionId,
    sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConnectionSearchAttemptId(InteractionIdentity);

impl InteractionIdentity {
    const fn new(session: InteractionSessionId, sequence: u64) -> Self {
        Self { session, sequence }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InteractionGeneration(InteractionIdentity);

impl InteractionGeneration {
    pub const fn session(self) -> InteractionSessionId {
        self.0.session
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InteractionQueryId(InteractionIdentity);

impl InteractionQueryId {
    pub const fn session(self) -> InteractionSessionId {
        self.0.session
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InteractionTicketId(InteractionIdentity);

impl InteractionTicketId {
    pub const fn session(self) -> InteractionSessionId {
        self.0.session
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    Replace,
    Add,
    Toggle,
}

/// Direction for one runtime-owned keyboard movement of the exact selected node cohort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeNudgeDirection {
    Left,
    Right,
    Up,
    Down,
}

impl NodeNudgeDirection {
    fn delta(self, step: f32) -> CanvasPoint {
        match self {
            Self::Left => CanvasPoint { x: -step, y: 0.0 },
            Self::Right => CanvasPoint { x: step, y: 0.0 },
            Self::Up => CanvasPoint { x: 0.0, y: -step },
            Self::Down => CanvasPoint { x: 0.0, y: step },
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeDragStart {
    provenance: ViewProvenance,
    clicked: NodeLifetimeKey,
    cohort: NodeMoveCohort,
    snap_origin: CanvasPoint,
    selection_mode: SelectionMode,
    clicked_was_selected: bool,
}

impl NodeDragStart {
    pub(crate) fn new(
        provenance: ViewProvenance,
        clicked: NodeLifetimeKey,
        members: impl IntoIterator<Item = NodeLifetimeKey>,
        snap_origin: CanvasPoint,
        selection_mode: SelectionMode,
    ) -> Option<Self> {
        if !snap_origin.is_finite() {
            return None;
        }
        let mut members = members.into_iter().collect::<Vec<_>>();
        let clicked_was_selected = members.contains(&clicked);
        if !clicked_was_selected {
            members.push(clicked);
        }
        Some(Self {
            provenance,
            clicked,
            cohort: NodeMoveCohort::new(members)?,
            snap_origin,
            selection_mode,
            clicked_was_selected,
        })
    }

    pub fn provenance(&self) -> ViewProvenance {
        self.provenance
    }

    pub fn clicked_lifetime(&self) -> NodeLifetimeKey {
        self.clicked
    }

    pub fn members(&self) -> &[NodeLifetimeKey] {
        self.cohort.members()
    }

    pub fn snap_origin(&self) -> CanvasPoint {
        self.snap_origin
    }

    pub fn selection_mode(&self) -> SelectionMode {
        self.selection_mode
    }

    pub fn clicked_was_selected(&self) -> bool {
        self.clicked_was_selected
    }
}

const fn opposite_edge_endpoint(endpoint: EdgeEndpoint) -> EdgeEndpoint {
    match endpoint {
        EdgeEndpoint::From => EdgeEndpoint::To,
        EdgeEndpoint::To => EdgeEndpoint::From,
    }
}

#[cfg(test)]
const fn opposite_handle_position(position: HandlePosition) -> HandlePosition {
    match position {
        HandlePosition::Top => HandlePosition::Bottom,
        HandlePosition::Right => HandlePosition::Left,
        HandlePosition::Bottom => HandlePosition::Top,
        HandlePosition::Left => HandlePosition::Right,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionInteractionStart {
    provenance: ViewProvenance,
    validated_version: GraphVersion,
    binding: ConnectionGestureBinding,
    fixed_endpoint: EdgeEndpoint,
    fixed_anchor: CanvasPoint,
    fixed_outward: HandlePosition,
    moving_outward: HandlePosition,
    allow_create_from_wire: bool,
}

/// Exact view-scoped capability for one real graph port used by keyboard connection flows.
///
/// Native adapters obtain this capability from the owning [`ViewSession`] and can only hand it
/// back to the bound [`CanvasInteractionSession`]. It intentionally carries no pointer position
/// or published geometry: keyboard connection admission is entirely graph- and policy-driven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyboardConnectionPort {
    provenance: ViewProvenance,
    instance: PortInstanceKey,
}

impl KeyboardConnectionPort {
    fn new(provenance: ViewProvenance, instance: PortInstanceKey) -> Self {
        Self {
            provenance,
            instance,
        }
    }

    /// Returns the stable document port identifier for semantic labels and focus restoration.
    pub fn port(self) -> PortId {
        self.instance.port()
    }

    /// Returns the owning node for semantic labels and focus restoration.
    pub fn node(self) -> NodeId {
        self.instance.node()
    }

    fn belongs_to(self, provenance: ViewProvenance) -> bool {
        self.provenance == provenance
    }

    fn instance(self) -> PortInstanceKey {
        self.instance
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyboardConnectionStart {
    source: KeyboardConnectionPort,
    binding: ConnectionGestureBinding,
}

impl KeyboardConnectionStart {
    fn new(source: KeyboardConnectionPort, binding: ConnectionGestureBinding) -> Self {
        Self { source, binding }
    }
}

/// Structured policy feedback retained after a keyboard target activation is rejected.
///
/// The active source remains armed so focus can continue through real ports. Calling
/// [`CanvasInteractionSession::cancel`] releases that source without a graph mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardConnectionFeedback {
    source: PortId,
    target: PortId,
    diagnostics: Arc<[Diagnostic]>,
}

impl KeyboardConnectionFeedback {
    fn rejected(source: PortId, target: PortId, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            source,
            target,
            diagnostics: diagnostics.into(),
        }
    }

    pub fn source(&self) -> PortId {
        self.source
    }

    pub fn target(&self) -> PortId {
        self.target
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

impl ConnectionInteractionStart {
    #[cfg(test)]
    pub(crate) fn new(
        binding: ConnectionGestureBinding,
        validated_version: GraphVersion,
        fixed_anchor: CanvasPoint,
        fixed_outward: HandlePosition,
        allow_create_from_wire: bool,
    ) -> Option<Self> {
        let start = Self::new_with_provenance(
            ViewProvenance::unavailable(),
            validated_version,
            binding,
            EdgeEndpoint::From,
            fixed_anchor,
            fixed_outward,
            opposite_handle_position(fixed_outward),
            allow_create_from_wire,
        )?;
        Some(start)
    }

    fn new_with_provenance(
        provenance: ViewProvenance,
        validated_version: GraphVersion,
        binding: ConnectionGestureBinding,
        fixed_endpoint: EdgeEndpoint,
        fixed_anchor: CanvasPoint,
        fixed_outward: HandlePosition,
        moving_outward: HandlePosition,
        allow_create_from_wire: bool,
    ) -> Option<Self> {
        fixed_anchor.is_finite().then_some(Self {
            provenance,
            validated_version,
            binding,
            fixed_endpoint,
            fixed_anchor,
            fixed_outward,
            moving_outward,
            allow_create_from_wire,
        })
    }

    /// Resolves a connection start from revision-bound runtime port policy facts.
    fn from_port_drag_intent(
        provenance: ViewProvenance,
        drag_intent: PortConnectionDragIntent,
        new_edge: EdgeId,
        mode: NodeGraphConnectionMode,
        fixed_anchor: CanvasPoint,
        fixed_outward: HandlePosition,
        moving_outward: HandlePosition,
        allow_create_from_wire: bool,
    ) -> Option<Self> {
        let binding = Self::binding_from_port_drag(drag_intent, new_edge, mode)?;
        let fixed_endpoint = match drag_intent {
            PortConnectionDragIntent::Connect { .. } => match drag_intent.direction() {
                PortDirection::Out => EdgeEndpoint::From,
                PortDirection::In => EdgeEndpoint::To,
            },
            PortConnectionDragIntent::Reconnect { candidate, .. } => {
                opposite_edge_endpoint(candidate.endpoint())
            }
            PortConnectionDragIntent::BlockedOccupied { .. } => return None,
        };
        Self::new_with_provenance(
            provenance,
            drag_intent.version(),
            binding,
            fixed_endpoint,
            fixed_anchor,
            fixed_outward,
            moving_outward,
            allow_create_from_wire,
        )
    }

    /// Resolves the renderer-neutral gesture intent carried by runtime-issued port drag facts.
    pub fn binding_from_port_drag(
        drag_intent: PortConnectionDragIntent,
        new_edge: EdgeId,
        mode: NodeGraphConnectionMode,
    ) -> Option<ConnectionGestureBinding> {
        if !drag_intent.can_begin_connection() {
            return None;
        }
        match drag_intent {
            PortConnectionDragIntent::Connect { facts } => {
                let endpoint = match facts.direction() {
                    PortDirection::Out => EdgeEndpoint::From,
                    PortDirection::In => EdgeEndpoint::To,
                };
                Some(ConnectionGestureBinding::connect(
                    facts.instance(),
                    endpoint,
                    new_edge,
                    mode,
                ))
            }
            PortConnectionDragIntent::Reconnect { facts, candidate } => {
                ConnectionGestureBinding::reconnect(
                    candidate.edge_instance(),
                    candidate.endpoint(),
                    facts.instance(),
                    candidate.opposite_port_instance(),
                    mode,
                )
            }
            PortConnectionDragIntent::BlockedOccupied { .. } => None,
        }
    }

    /// Resolves an endpoint reconnect from authoritative graph topology and published route
    /// policy, using geometry selected by the adapter for the endpoint that remains fixed.
    fn reconnect_from_graph_route(
        provenance: ViewProvenance,
        snapshot: &GraphSnapshot,
        route: &PublishedEdgeRoute,
        endpoint: EdgeEndpoint,
        fixed_endpoint: EdgeEndpointPosition,
        mode: NodeGraphConnectionMode,
        allow_create_from_wire: bool,
    ) -> Option<Self> {
        let edge = route.edge();
        let interaction = route.facts().interaction;
        let endpoint_allowed = match endpoint {
            EdgeEndpoint::From => interaction.reconnect_source,
            EdgeEndpoint::To => interaction.reconnect_target,
        };
        if !endpoint_allowed {
            return None;
        }
        let graph = snapshot.graph();
        let graph_edge = graph.edge(&edge)?;
        let (detached_port, fixed_port) = match endpoint {
            EdgeEndpoint::From => (graph_edge.from, graph_edge.to),
            EdgeEndpoint::To => (graph_edge.to, graph_edge.from),
        };
        let detached = snapshot.port_instance_key(detached_port)?;
        let fixed = snapshot.port_instance_key(fixed_port)?;
        let moving_endpoint = match endpoint {
            EdgeEndpoint::From => route.facts().endpoints.source,
            EdgeEndpoint::To => route.facts().endpoints.target,
        };
        let binding =
            ConnectionGestureBinding::reconnect(route.instance(), endpoint, detached, fixed, mode)?;
        Self::new_with_provenance(
            provenance,
            snapshot.version(),
            binding,
            opposite_edge_endpoint(endpoint),
            fixed_endpoint.point,
            fixed_endpoint.position,
            moving_endpoint.position,
            allow_create_from_wire,
        )
    }

    pub fn owner_node(&self) -> NodeId {
        self.binding.owner().node()
    }

    pub fn binding(&self) -> ConnectionGestureBinding {
        self.binding.clone()
    }

    fn belongs_to(&self, provenance: ViewProvenance) -> bool {
        self.provenance == provenance
    }
}

impl ViewSession {
    /// Validates one runtime-issued connection capability against current graph and policy facts.
    pub fn validate_connection_binding(
        &self,
        binding: &ConnectionGestureBinding,
    ) -> Result<(), ViewSessionError> {
        let snapshot = self.graph_snapshot()?;
        if binding.provenance() != snapshot.provenance() {
            return Err(ViewSessionError::ForeignInteractionFacts);
        }
        let policy = self.graph_policy()?;
        if !binding.is_interactive(&snapshot, policy) {
            return Err(ViewSessionError::StaleInteractionFacts);
        }
        Ok(())
    }

    /// Issues a connection start only when port facts came from this view's runtime.
    pub fn connection_start_from_port_drag_intent(
        &self,
        drag_intent: PortConnectionDragIntent,
        new_edge: EdgeId,
        mode: NodeGraphConnectionMode,
        fixed_anchor: CanvasPoint,
        fixed_outward: HandlePosition,
        moving_outward: HandlePosition,
        allow_create_from_wire: bool,
    ) -> Result<Option<ConnectionInteractionStart>, ViewSessionError> {
        let provenance = self.provenance()?;
        let snapshot = self.graph_snapshot()?;
        let policy = self.graph_policy()?;
        connection_start_from_port_drag_facts(
            provenance,
            &snapshot,
            policy,
            drag_intent,
            new_edge,
            mode,
            fixed_anchor,
            fixed_outward,
            moving_outward,
            allow_create_from_wire,
        )
    }

    /// Resolves reconnect topology from this runtime instead of accepting a caller-supplied graph.
    pub fn connection_start_from_route(
        &self,
        route: &PublishedEdgeRoute,
        endpoint: EdgeEndpoint,
        fixed_endpoint: EdgeEndpointPosition,
        mode: NodeGraphConnectionMode,
        allow_create_from_wire: bool,
    ) -> Result<Option<ConnectionInteractionStart>, ViewSessionError> {
        let provenance = self.provenance()?;
        if route.provenance() != provenance {
            return Err(ViewSessionError::ForeignInteractionFacts);
        }
        let snapshot = self.validate_published_edge_route(route)?;
        let start = ConnectionInteractionStart::reconnect_from_graph_route(
            provenance,
            &snapshot,
            route,
            endpoint,
            fixed_endpoint,
            mode,
            allow_create_from_wire,
        );
        if let Some(start) = start {
            self.validate_connection_binding(&start.binding)?;
            return Ok(Some(start));
        }
        Ok(None)
    }

    fn validate_published_edge_route(
        &self,
        route: &PublishedEdgeRoute,
    ) -> Result<GraphSnapshot, ViewSessionError> {
        let provenance = self.provenance()?;
        if route.provenance() != provenance {
            return Err(ViewSessionError::ForeignInteractionFacts);
        }
        let geometry = self.geometry_snapshot()?;
        let current = geometry
            .route(route.edge())
            .ok_or(ViewSessionError::StaleInteractionFacts)?;
        if !current.same_publication(route) {
            return Err(ViewSessionError::StaleInteractionFacts);
        }
        let snapshot = self.graph_snapshot()?;
        if !snapshot.edge_instance_is_current(route.instance()) {
            return Err(ViewSessionError::StaleInteractionFacts);
        }
        Ok(snapshot)
    }

    /// Binds one exact real-port instance to this view generation for keyboard connection input.
    ///
    /// This is the only keyboard connection input accepted by
    /// [`CanvasInteractionSession::activate_keyboard_connection`] and
    /// [`CanvasInteractionSession::activate_keyboard_connection_target`].
    ///
    /// ```compile_fail
    /// use jellyflow_core::core::PortId;
    /// use jellyflow_runtime::view::ViewSession;
    ///
    /// fn bind_stale_frame_id(view: &ViewSession, port: PortId) {
    ///     let _ = view.keyboard_connection_port(port);
    /// }
    /// ```
    pub fn keyboard_connection_port(
        &self,
        port: PortInstanceKey,
    ) -> Result<KeyboardConnectionPort, ViewSessionError> {
        let provenance = self.provenance()?;
        let snapshot = self.graph_snapshot()?;
        ensure_keyboard_port_is_current(&snapshot, port)?;
        Ok(KeyboardConnectionPort::new(provenance, port))
    }
}

impl WeakViewSession {
    /// Issues a non-owning view-scoped capability for one currently live real port.
    pub fn keyboard_connection_port(
        &self,
        port: PortInstanceKey,
    ) -> Result<KeyboardConnectionPort, ViewSessionError> {
        let provenance = self.provenance()?;
        let snapshot = self.graph_snapshot()?;
        ensure_keyboard_port_is_current(&snapshot, port)?;
        Ok(KeyboardConnectionPort::new(provenance, port))
    }

    fn validate_connection_binding(
        &self,
        binding: &ConnectionGestureBinding,
    ) -> Result<(), ViewSessionError> {
        let snapshot = self.graph_snapshot()?;
        if binding.provenance() != snapshot.provenance() {
            return Err(ViewSessionError::ForeignInteractionFacts);
        }
        let policy = self.graph_policy()?;
        if !binding.is_interactive(&snapshot, policy) {
            return Err(ViewSessionError::StaleInteractionFacts);
        }
        Ok(())
    }

    fn validate_published_edge_route(
        &self,
        route: &PublishedEdgeRoute,
    ) -> Result<GraphSnapshot, ViewSessionError> {
        let provenance = self.provenance()?;
        if route.provenance() != provenance {
            return Err(ViewSessionError::ForeignInteractionFacts);
        }
        let geometry = self.geometry_snapshot()?;
        let current = geometry
            .route(route.edge())
            .ok_or(ViewSessionError::StaleInteractionFacts)?;
        if !current.same_publication(route) {
            return Err(ViewSessionError::StaleInteractionFacts);
        }
        let snapshot = self.graph_snapshot()?;
        if !snapshot.edge_instance_is_current(route.instance()) {
            return Err(ViewSessionError::StaleInteractionFacts);
        }
        Ok(snapshot)
    }

    /// Issues a connection start without owning or reviving the view session.
    pub fn connection_start_from_port_drag_intent(
        &self,
        drag_intent: PortConnectionDragIntent,
        new_edge: EdgeId,
        mode: NodeGraphConnectionMode,
        fixed_anchor: CanvasPoint,
        fixed_outward: HandlePosition,
        moving_outward: HandlePosition,
        allow_create_from_wire: bool,
    ) -> Result<Option<ConnectionInteractionStart>, ViewSessionError> {
        let provenance = self.provenance()?;
        let snapshot = self.graph_snapshot()?;
        let policy = self.graph_policy()?;
        connection_start_from_port_drag_facts(
            provenance,
            &snapshot,
            policy,
            drag_intent,
            new_edge,
            mode,
            fixed_anchor,
            fixed_outward,
            moving_outward,
            allow_create_from_wire,
        )
    }

    fn keyboard_connection_start(
        &self,
        source: KeyboardConnectionPort,
        prospective_edge: EdgeId,
        mode: NodeGraphConnectionMode,
    ) -> Result<Option<KeyboardConnectionStart>, ViewSessionError> {
        let provenance = self.provenance()?;
        let snapshot = self.graph_snapshot()?;
        let policy = self.graph_policy()?;
        let drag_intent = self.port_connection_drag_intent(source.instance())?;
        keyboard_connection_start_from_facts(
            provenance,
            &snapshot,
            policy,
            source,
            drag_intent,
            prospective_edge,
            mode,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn connection_start_from_port_drag_facts(
    provenance: ViewProvenance,
    snapshot: &GraphSnapshot,
    policy: GraphRuntimePolicy,
    drag_intent: PortConnectionDragIntent,
    new_edge: EdgeId,
    mode: NodeGraphConnectionMode,
    fixed_anchor: CanvasPoint,
    fixed_outward: HandlePosition,
    moving_outward: HandlePosition,
    allow_create_from_wire: bool,
) -> Result<Option<ConnectionInteractionStart>, ViewSessionError> {
    if drag_intent.provenance() != provenance.runtime() {
        return Err(ViewSessionError::ForeignInteractionFacts);
    }
    if drag_intent.revision() != snapshot.revision()
        || !snapshot.port_instance_is_current(drag_intent.instance())
    {
        return Err(ViewSessionError::StaleInteractionFacts);
    }
    let start = ConnectionInteractionStart::from_port_drag_intent(
        provenance,
        drag_intent,
        new_edge,
        mode,
        fixed_anchor,
        fixed_outward,
        moving_outward,
        allow_create_from_wire,
    );
    if let Some(start) = start {
        if !start.binding.is_interactive(snapshot, policy) {
            return Err(ViewSessionError::StaleInteractionFacts);
        }
        return Ok(Some(start));
    }
    Ok(None)
}

fn keyboard_connection_start_from_facts(
    provenance: ViewProvenance,
    snapshot: &GraphSnapshot,
    policy: GraphRuntimePolicy,
    source: KeyboardConnectionPort,
    drag_intent: PortConnectionDragIntent,
    prospective_edge: EdgeId,
    mode: NodeGraphConnectionMode,
) -> Result<Option<KeyboardConnectionStart>, ViewSessionError> {
    if !source.belongs_to(provenance) || source.instance().provenance() != snapshot.provenance() {
        return Err(ViewSessionError::ForeignInteractionFacts);
    }
    if !snapshot.port_instance_is_current(source.instance()) {
        return Err(ViewSessionError::StaleInteractionFacts);
    }
    if drag_intent.version() != snapshot.version() || drag_intent.instance() != source.instance() {
        return Err(ViewSessionError::StaleInteractionFacts);
    }
    let Some(binding) =
        ConnectionInteractionStart::binding_from_port_drag(drag_intent, prospective_edge, mode)
    else {
        return Ok(None);
    };
    if !binding.is_interactive(snapshot, policy) {
        return Err(ViewSessionError::StaleInteractionFacts);
    }
    Ok(Some(KeyboardConnectionStart::new(source, binding)))
}

fn ensure_keyboard_port_is_current(
    snapshot: &GraphSnapshot,
    port: PortInstanceKey,
) -> Result<(), ViewSessionError> {
    if port.provenance() != snapshot.provenance() {
        return Err(ViewSessionError::ForeignInteractionFacts);
    }
    if !snapshot.port_instance_is_current(port) {
        return Err(ViewSessionError::StaleInteractionFacts);
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub enum CanvasPressTarget {
    NativeControl,
    NodeDrag(NodeDragStart),
    Pan,
    Marquee {
        selection_mode: SelectionMode,
        baseline: SelectionBaseline,
    },
    Connection(ConnectionInteractionStart),
    Edge {
        instance: EdgeInstanceKey,
        selection_mode: SelectionMode,
    },
    /// Resolves an exact edge at the press position before activating the canvas fallback.
    ///
    /// Pointer adapters should use this target for blank-scene input instead of reusing a prior
    /// hover result. Keyboard and accessibility adapters may continue to use [`Self::Edge`] when
    /// they already own an authoritative edge identity.
    Canvas(CanvasPressFallback),
}

/// Owned canvas behavior retained while an exact press-position edge pick is pending.
#[derive(Debug, Clone, PartialEq)]
pub enum CanvasPressFallback {
    Pan {
        selection_mode: SelectionMode,
    },
    Marquee {
        selection_mode: SelectionMode,
        baseline: SelectionBaseline,
    },
}

impl CanvasPressFallback {
    fn selection_mode(&self) -> SelectionMode {
        match self {
            Self::Pan { selection_mode } | Self::Marquee { selection_mode, .. } => *selection_mode,
        }
    }

    fn into_armed_target(self) -> ArmedTarget {
        match self {
            Self::Pan { selection_mode } => ArmedTarget::Pan {
                selection_mode: Some(selection_mode),
            },
            Self::Marquee {
                selection_mode,
                baseline,
            } => ArmedTarget::Marquee {
                selection_mode,
                baseline,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionOwner {
    Node(NodeLifetimeKey),
    Port(PortInstanceKey),
    Edge(EdgeInstanceKey),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SelectionMutation {
    Node {
        provenance: ViewProvenance,
        node: NodeLifetimeKey,
        mode: SelectionMode,
    },
    Edge {
        provenance: ViewProvenance,
        instance: EdgeInstanceKey,
        mode: SelectionMode,
    },
    Marquee {
        provenance: ViewProvenance,
        nodes: Arc<[NodeLifetimeKey]>,
        mode: SelectionMode,
        baseline: SelectionBaseline,
    },
    Clear {
        provenance: ViewProvenance,
    },
    Restore(SelectionBaseline),
}

#[derive(Debug, Clone, PartialEq)]
struct ConnectionSearchToken {
    provenance: ViewProvenance,
    session: InteractionSessionId,
    generation: InteractionGeneration,
    attempt: ConnectionSearchAttemptId,
    binding: ConnectionGestureBinding,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionSearchRequest {
    token: ConnectionSearchToken,
    position: InteractionPosition,
}

impl ConnectionSearchRequest {
    pub fn generation(&self) -> InteractionGeneration {
        self.token.generation
    }

    pub fn view_position(&self) -> ViewPoint {
        self.position.view()
    }

    pub fn canvas_position(&self) -> CanvasPoint {
        self.position.canvas()
    }
}

/// One create-from-wire selection that must add topology and its connection atomically.
#[derive(Debug)]
pub struct ConnectionSearchSelection {
    token: ConnectionSearchToken,
    validated_version: GraphVersion,
    target: ConnectionCommitTarget,
}

impl ConnectionSearchSelection {
    fn from_snapshot(
        token: ConnectionSearchToken,
        snapshot: &GraphSnapshot,
        target: PortId,
        creation: NodeCreation,
    ) -> Option<Self> {
        let target = ConnectionCommitTarget::from_node_creation(snapshot, target, creation)?;
        Some(Self {
            token,
            validated_version: snapshot.version(),
            target,
        })
    }
}

/// Opaque, single-use authority to bind one interaction mutation to a fresh graph snapshot.
///
/// The ticket and mutation deliberately cannot be separated. Adapters may wait for their command
/// prefix, return the submission to [`CanvasInteractionSession::resume_submission_after_prefix`],
/// and finally return a re-emitted submission to
/// [`CanvasInteractionSession::bind_submission`]. Mutation semantics, session authority, and
/// revalidation policy remain owned by the interaction session.
#[derive(Debug)]
pub struct InteractionGraphSubmission {
    ticket: InteractionTicketId,
    mutation: FreshGraphMutation,
}

impl InteractionGraphSubmission {
    fn new(ticket: InteractionTicketId, mutation: FreshGraphMutation) -> Self {
        Self { ticket, mutation }
    }

    pub const fn ticket(&self) -> InteractionTicketId {
        self.ticket
    }

    fn requires_connection_revalidation(&self) -> bool {
        self.mutation.requires_connection_revalidation()
    }

    #[cfg(test)]
    fn connection_facts_for_test(
        &self,
    ) -> Option<(ConnectionGestureBinding, PortId, GraphVersion, bool)> {
        self.mutation.connection_facts_for_test()
    }

    #[cfg(test)]
    fn move_nodes_for_test(&self) -> Option<&RelativeNodeMoveIntent> {
        self.mutation.move_nodes_for_test()
    }

    #[cfg(test)]
    fn delete_edge_for_test(&self) -> Option<EdgeDeletionIntent> {
        self.mutation.delete_edge_for_test()
    }
}

/// Why an opaque interaction submission could not cross its transport-acceptance boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InteractionGraphSubmissionBindError {
    #[error("the interaction session is closed")]
    Closed,
    #[error("the graph submission belongs to another canvas interaction session")]
    ForeignInteractionSession {
        expected: InteractionSessionId,
        actual: InteractionSessionId,
    },
    #[error("graph submission ticket {ticket:?} is no longer current; current ticket: {current:?}")]
    Superseded {
        ticket: InteractionTicketId,
        current: Option<InteractionTicketId>,
    },
    #[error("the interaction view cannot validate graph submission authority")]
    ViewUnavailable,
    #[error("the interaction graph submission is missing or was already bound")]
    SubmissionAlreadyBound,
    #[error("the interaction session belongs to another runtime or view generation")]
    ForeignInteractionFacts,
    #[error(transparent)]
    Mutation(#[from] FreshGraphMutationBindError),
}

#[derive(Debug)]
pub enum CanvasInteractionEffect {
    SubmitGraph {
        submission: InteractionGraphSubmission,
    },
    Search(ConnectionSearchRequest),
}

/// Runtime proof returned for one successfully executed interaction-query page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasInteractionQueryPageReceipt {
    AlignmentSnap(AlignmentSnapQueryReceipt),
    ConnectionProbe(ConnectionProbeQueryReceipt),
    EdgePick(EdgePickQueryReceipt),
    NodeIntersection(NodeIntersectionQueryReceipt),
}

/// Bounded work evidence for one native query advancement grant.
#[must_use = "query advancement receipts and terminal errors must be handled"]
#[derive(Debug, Default)]
pub struct CanvasInteractionQueryAdvanceReceipt {
    pages_granted: usize,
    pages_attempted: usize,
    page_receipts: Vec<CanvasInteractionQueryPageReceipt>,
}

impl CanvasInteractionQueryAdvanceReceipt {
    pub fn pages_granted(&self) -> usize {
        self.pages_granted
    }

    pub fn pages_attempted(&self) -> usize {
        self.pages_attempted
    }

    pub fn page_receipts(&self) -> &[CanvasInteractionQueryPageReceipt] {
        &self.page_receipts
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InteractionDamage {
    pub presentation: bool,
    pub nodes: bool,
    pub ports: bool,
    pub edges: bool,
    pub viewport: bool,
    pub selection: bool,
}

impl InteractionDamage {
    fn presentation() -> Self {
        Self {
            presentation: true,
            ..Self::default()
        }
    }

    fn connection() -> Self {
        Self {
            presentation: true,
            ports: true,
            edges: true,
            ..Self::default()
        }
    }

    fn node_drag() -> Self {
        Self {
            presentation: true,
            nodes: true,
            ports: true,
            edges: true,
            ..Self::default()
        }
    }

    fn merge(&mut self, other: Self) {
        self.presentation |= other.presentation;
        self.nodes |= other.nodes;
        self.ports |= other.ports;
        self.edges |= other.edges;
        self.viewport |= other.viewport;
        self.selection |= other.selection;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InteractionTickDemand {
    #[default]
    None,
    WhileActive,
}

#[derive(Debug, Default)]
pub struct CanvasInteractionUpdate {
    effects: Vec<CanvasInteractionEffect>,
    pending_selection: Option<SelectionMutation>,
    #[cfg(test)]
    selection_mutations: Vec<SelectionMutation>,
    damage: InteractionDamage,
    tick_demand: InteractionTickDemand,
    query_work_pending: bool,
}

/// Adapter work emitted by one semantic interaction transition.
///
/// View-owned selection and viewport changes are already committed before this dispatch is
/// returned. Effects are application work such as graph submission or native search; query work
/// remains inside the interaction session and is exposed only as pending scheduling demand.
#[derive(Debug)]
pub struct CanvasInteractionDispatch {
    pub effects: Vec<CanvasInteractionEffect>,
    pub damage: InteractionDamage,
    pub tick_demand: InteractionTickDemand,
    pub query_work_pending: bool,
}

impl CanvasInteractionUpdate {
    pub fn effects(&self) -> &[CanvasInteractionEffect] {
        &self.effects
    }

    pub fn into_dispatch(self) -> CanvasInteractionDispatch {
        CanvasInteractionDispatch {
            effects: self.effects,
            damage: self.damage,
            tick_demand: self.tick_demand,
            query_work_pending: self.query_work_pending,
        }
    }

    #[cfg(test)]
    pub(crate) fn selection_mutations_for_test(&self) -> &[SelectionMutation] {
        &self.selection_mutations
    }

    pub fn damage(&self) -> InteractionDamage {
        self.damage
    }

    pub fn tick_demand(&self) -> InteractionTickDemand {
        self.tick_demand
    }

    pub fn query_work_pending(&self) -> bool {
        self.query_work_pending
    }

    fn with_damage(mut self, damage: InteractionDamage) -> Self {
        self.damage.merge(damage);
        self
    }

    fn merge_internal(&mut self, other: Self) {
        debug_assert!(other.pending_selection.is_none());
        self.effects.extend(other.effects);
        #[cfg(test)]
        self.selection_mutations.extend(other.selection_mutations);
        self.damage.merge(other.damage);
    }
}

/// Result of advancing runtime-owned interaction query work with a native page grant.
#[must_use = "query advancement damage, scheduling demand, receipts, and errors must be handled"]
#[derive(Debug)]
pub struct CanvasInteractionQueryAdvance {
    update: CanvasInteractionUpdate,
    needs_tick: bool,
    receipt: CanvasInteractionQueryAdvanceReceipt,
    terminal_error: Option<CanvasInteractionError>,
}

impl CanvasInteractionQueryAdvance {
    pub fn update(&self) -> &CanvasInteractionUpdate {
        &self.update
    }

    pub fn needs_tick(&self) -> bool {
        self.needs_tick
    }

    pub fn receipt(&self) -> &CanvasInteractionQueryAdvanceReceipt {
        &self.receipt
    }

    pub fn terminal_error(&self) -> Option<CanvasInteractionError> {
        self.terminal_error
    }

    pub fn into_parts(
        self,
    ) -> (
        CanvasInteractionUpdate,
        bool,
        CanvasInteractionQueryAdvanceReceipt,
        Option<CanvasInteractionError>,
    ) {
        (
            self.update,
            self.needs_tick,
            self.receipt,
            self.terminal_error,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CanvasInteractionError {
    #[error("canvas interaction configuration is invalid")]
    InvalidConfiguration,
    #[error("the normalized interaction position is invalid")]
    InvalidPosition,
    #[error("the reveal viewport or padding is invalid")]
    InvalidRevealRequest,
    #[error("no keyboard connection source is active")]
    KeyboardConnectionInactive,
    #[error("another semantic canvas interaction is active")]
    AlreadyActive,
    #[error("canvas interaction identity space is exhausted")]
    IdentityExhausted,
    #[error("the interaction session is closed")]
    Closed,
    #[error("the interaction identity belongs to another canvas session")]
    ForeignInteractionSession {
        expected: InteractionSessionId,
        actual: InteractionSessionId,
    },
    #[error("the interaction submission has not been bound to a graph command")]
    SubmissionNotBound,
    #[error("the interaction completion protocol does not match the submission binding state")]
    SubmissionCompletionStateMismatch,
    #[error("the command completion identity does not match the bound interaction submission")]
    CommandCompletionIdentityMismatch,
    #[error("interaction query {query:?} has no runtime receipt")]
    MissingInteractionQueryReceipt { query: InteractionQueryId },
    #[error("interaction query receipt belongs to {actual:?}, expected {expected:?}")]
    MismatchedInteractionQueryReceipt {
        expected: InteractionQueryId,
        actual: InteractionQueryId,
    },
    #[error("ordinary connection commits require a fresh target probe")]
    ConnectionCommitRequiresProbe,
    #[error("create-from-wire commits require a fresh graph revision")]
    ConnectionCommitRequiresFreshRevision,
    #[error("create-from-wire target {target:?} already exists in the fresh graph snapshot")]
    ConnectionCommitTargetNoLongerProspective { target: PortId },
    #[error("this connection interaction does not allow create-from-wire search")]
    ConnectionSearchUnavailable,
    #[error("no connection search is active")]
    ConnectionSearchInactive,
    #[error("the connection search generation is stale")]
    ConnectionSearchStaleGeneration,
    #[error("the interaction operation belongs to another view")]
    ViewMismatch,
    #[error("the interaction operation belongs to another view generation")]
    ViewGenerationMismatch,
    #[error("interaction facts belong to another runtime or view generation")]
    ForeignInteractionFacts,
    #[error("interaction facts have been superseded in the bound view")]
    StaleInteractionFacts,
    #[error("the interaction view cannot provide an authoritative graph snapshot")]
    ViewUnavailable,
}

static NEXT_INTERACTION_SESSION_ID: AtomicU64 = AtomicU64::new(1);

fn allocate_interaction_session_id() -> Option<InteractionSessionId> {
    NEXT_INTERACTION_SESSION_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .ok()
        .map(InteractionSessionId)
}

pub struct CanvasInteractionSession {
    bound_view: WeakViewSession,
    provenance: ViewProvenance,
    id: InteractionSessionId,
    config: CanvasInteractionConfig,
    next_generation: Option<u64>,
    next_query: Option<u64>,
    next_ticket: Option<u64>,
    next_search_attempt: Option<u64>,
    state: InteractionState,
    query_work: QueryWork,
    pending_edge_pick: Option<PendingHoverEdgePick>,
    hovered_edge: Option<EdgeInstanceKey>,
    terminal: Option<InteractionTerminalReason>,
    submission_binding: Option<SubmissionBinding>,
    closed: bool,
    #[cfg(test)]
    test_authority_bypass: bool,
    #[cfg(test)]
    test_viewport: ViewportSnapshot,
    #[cfg(test)]
    selection_commit_failure: Option<CanvasInteractionError>,
}

struct SubmissionBinding {
    ticket: InteractionTicketId,
    phase: SubmissionBindingPhase,
}

struct SelectionCommitFailure {
    error: CanvasInteractionError,
    retryable: bool,
}

enum SubmissionBindingPhase {
    Pending,
    NoCommand,
    Command(GraphCommandIdentity),
}

impl std::fmt::Debug for CanvasInteractionSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanvasInteractionSession")
            .field("provenance", &self.provenance)
            .field("id", &self.id)
            .field("config", &self.config)
            .field("state", &self.state)
            .field("terminal", &self.terminal)
            .field("closed", &self.closed)
            .finish()
    }
}

#[derive(Debug, Default)]
enum InteractionState {
    #[default]
    Idle,
    Armed(ArmedInteraction),
    NodeDrag(NodeDragInteraction),
    Pan(PanInteraction),
    Marquee(MarqueeInteraction),
    Connection(ConnectionInteraction),
    KeyboardConnection(KeyboardConnectionInteraction),
    Committing(CommitInteraction),
    EdgeActive(EdgeInteraction),
    EdgePickPending(CanvasEdgePickInteraction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallbackAuthority {
    Current,
    Stale,
}

#[derive(Debug, Clone)]
struct ArmedInteraction {
    generation: InteractionGeneration,
    target: ArmedTarget,
    start: InteractionPosition,
    max_view_distance_squared: f32,
}

#[derive(Debug, Clone)]
enum ArmedTarget {
    NodeDrag(NodeDragStart),
    Pan {
        selection_mode: Option<SelectionMode>,
    },
    Marquee {
        selection_mode: SelectionMode,
        baseline: SelectionBaseline,
    },
    Connection(ConnectionInteractionStart),
}

#[derive(Debug)]
struct NodeDragInteraction {
    generation: InteractionGeneration,
    cohort: NodeMoveCohort,
    snap_origin: CanvasPoint,
    start: InteractionPosition,
    pointer: InteractionPosition,
    raw_delta: CanvasPoint,
    delta: CanvasPoint,
    guides: Arc<[AlignmentGuide]>,
    alignment_locks: AlignmentSnapLocks,
    pending_alignment_query: Option<InteractionQueryId>,
    stale_query_restarts: u8,
    release_pending: bool,
}

#[derive(Debug)]
struct PanInteraction {
    generation: InteractionGeneration,
    pointer: InteractionPosition,
}

#[derive(Debug, Clone)]
struct MarqueeInteraction {
    generation: InteractionGeneration,
    start: InteractionPosition,
    pointer: InteractionPosition,
    selection_mode: SelectionMode,
    baseline: SelectionBaseline,
    pending_query: Option<InteractionQueryId>,
    stale_query_restarts: u8,
    release_pending: bool,
}

#[derive(Debug)]
enum ConnectionInteraction {
    Dragging(ConnectionDragInteraction),
    Searching(ConnectionSearchInteraction),
}

#[derive(Debug)]
struct KeyboardConnectionInteraction {
    generation: InteractionGeneration,
    start: KeyboardConnectionStart,
    feedback: Option<KeyboardConnectionFeedback>,
}

#[derive(Debug)]
struct ConnectionDragInteraction {
    generation: InteractionGeneration,
    start: ConnectionInteractionStart,
    pointer: InteractionPosition,
    probe: ConnectionProbe,
    target_lock: Option<ConnectionProbeTargetIdentity>,
    pending_query: Option<InteractionQueryId>,
    stale_query_restarts: u8,
    release_pending: bool,
}

#[derive(Debug)]
struct ConnectionSearchInteraction {
    generation: InteractionGeneration,
    attempt: ConnectionSearchAttemptId,
    start: ConnectionInteractionStart,
    pointer: InteractionPosition,
    probe: ConnectionProbe,
    status: ConnectionSearchStatus,
}

#[derive(Debug)]
enum CommitInteraction {
    Connection(Box<ConnectionCommitInteraction>),
    KeyboardConnection(KeyboardConnectionCommitInteraction),
    NodeMove(NodeMoveCommitInteraction),
    EdgeDelete(EdgeDeleteCommitInteraction),
    SelectionDelete(SelectionDeleteCommitInteraction),
}

#[derive(Debug)]
struct ConnectionCommitInteraction {
    generation: InteractionGeneration,
    search_attempt: Option<ConnectionSearchAttemptId>,
    start: ConnectionInteractionStart,
    pointer: InteractionPosition,
    probe: ConnectionProbe,
    ticket: InteractionTicketId,
    target: ConnectionCommitTarget,
    return_to_search: bool,
    freshness: ConnectionCommitFreshness,
    stale_query_restarts: u8,
}

#[derive(Debug)]
struct KeyboardConnectionCommitInteraction {
    generation: InteractionGeneration,
    start: KeyboardConnectionStart,
    target: KeyboardConnectionPort,
    ticket: InteractionTicketId,
}

#[derive(Debug, Default)]
enum ConnectionCommitFreshness {
    #[default]
    Initial,
    Probing(InteractionQueryId),
    Revalidated,
}

#[derive(Debug)]
struct NodeMoveCommitInteraction {
    generation: InteractionGeneration,
    cohort: NodeMoveCohort,
    delta: CanvasPoint,
    guides: Arc<[AlignmentGuide]>,
    ticket: InteractionTicketId,
}

#[derive(Debug)]
struct EdgeDeleteCommitInteraction {
    generation: InteractionGeneration,
    instance: EdgeInstanceKey,
    ticket: InteractionTicketId,
}

#[derive(Debug)]
struct SelectionDeleteCommitInteraction {
    generation: InteractionGeneration,
    ticket: InteractionTicketId,
}

#[derive(Debug)]
struct EdgeInteraction {
    generation: InteractionGeneration,
    instance: EdgeInstanceKey,
}

#[derive(Debug, Clone, Copy)]
struct PendingHoverEdgePick {
    id: InteractionQueryId,
    position: InteractionPosition,
    stale_query_restarts: u8,
}

#[derive(Debug)]
struct CanvasEdgePickInteraction {
    generation: InteractionGeneration,
    query: InteractionQueryId,
    press: InteractionPosition,
    pointer: InteractionPosition,
    max_view_distance_squared: f32,
    fallback: CanvasPressFallback,
    stale_query_restarts: u8,
    release_pending: bool,
}

#[derive(Debug)]
enum CanvasEdgePickResolution {
    Continue(EdgePickQuery),
    Complete(Option<EdgeInstanceKey>),
    BudgetExhausted,
    GeometryIndeterminate,
}

impl CanvasInteractionSession {
    pub fn attach(
        view: &ViewSession,
        config: CanvasInteractionConfig,
    ) -> Result<Self, CanvasInteractionError> {
        let provenance = view
            .provenance()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        Self::from_binding(view.downgrade(), provenance, config)
    }

    fn from_binding(
        bound_view: WeakViewSession,
        provenance: ViewProvenance,
        config: CanvasInteractionConfig,
    ) -> Result<Self, CanvasInteractionError> {
        if !config.is_valid() {
            return Err(CanvasInteractionError::InvalidConfiguration);
        }
        let id =
            allocate_interaction_session_id().ok_or(CanvasInteractionError::IdentityExhausted)?;
        Ok(Self {
            bound_view,
            provenance,
            id,
            config,
            next_generation: Some(1),
            next_query: Some(1),
            next_ticket: Some(1),
            next_search_attempt: Some(1),
            state: InteractionState::Idle,
            query_work: QueryWork::default(),
            pending_edge_pick: None,
            hovered_edge: None,
            terminal: None,
            submission_binding: None,
            closed: false,
            #[cfg(test)]
            test_authority_bypass: false,
            #[cfg(test)]
            test_viewport: ViewportSnapshot::new(
                provenance,
                ViewportRevision::initial(),
                ViewportTransform::new(CanvasPoint::default(), 1.0)
                    .expect("the identity viewport is valid"),
            ),
            #[cfg(test)]
            selection_commit_failure: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        view: ViewId,
        view_generation: ViewGeneration,
        config: CanvasInteractionConfig,
    ) -> Result<Self, CanvasInteractionError> {
        let mut session = Self::from_binding(
            WeakViewSession::unavailable_for_test(),
            ViewProvenance::new(view, view_generation),
            config,
        )?;
        session.test_authority_bypass = true;
        Ok(session)
    }

    #[cfg(test)]
    pub(crate) fn bind_completion_for_test(
        &mut self,
        ticket: InteractionTicketId,
        completion: &CommandExecutionCompletion,
    ) {
        assert_eq!(
            interaction_state_submission_ticket(&self.state),
            Some(ticket),
            "test completion must bind the current interaction ticket"
        );
        self.submission_binding = Some(SubmissionBinding {
            ticket,
            phase: SubmissionBindingPhase::Command(completion.identity().clone()),
        });
    }

    pub fn view(&self) -> ViewId {
        self.provenance.view()
    }

    pub fn id(&self) -> InteractionSessionId {
        self.id
    }

    pub fn view_generation(&self) -> ViewGeneration {
        self.provenance.generation()
    }

    pub fn config(&self) -> CanvasInteractionConfig {
        self.config
    }

    /// Reports whether the current semantic interaction needs continuous native frame ticks.
    ///
    /// Adapters use this as the scheduling authority so duplicate callbacks and stale redraws cannot
    /// keep an idle, release-pending, or terminal interaction alive.
    pub fn tick_demand(&self) -> InteractionTickDemand {
        if self.closed || self.terminal.is_some() {
            InteractionTickDemand::None
        } else if state_needs_tick(&self.state) {
            InteractionTickDemand::WhileActive
        } else {
            InteractionTickDemand::None
        }
    }

    /// Replaces interaction thresholds and renderer-neutral navigation policy while idle.
    pub fn set_config(
        &mut self,
        config: CanvasInteractionConfig,
    ) -> Result<bool, CanvasInteractionError> {
        self.ensure_open()?;
        if !config.is_valid() {
            return Err(CanvasInteractionError::InvalidConfiguration);
        }
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        if self.config == config {
            return Ok(false);
        }
        self.config = config;
        Ok(true)
    }

    pub fn presentation(&self) -> CanvasInteractionPresentation {
        let mut presentation = match &self.state {
            InteractionState::Idle => CanvasInteractionPresentation::new(if self.closed {
                CanvasInteractionPhase::Closed
            } else {
                CanvasInteractionPhase::Idle
            }),
            InteractionState::Armed(_) => {
                CanvasInteractionPresentation::new(CanvasInteractionPhase::Armed)
            }
            InteractionState::NodeDrag(drag) => CanvasInteractionPresentation::new(
                CanvasInteractionPhase::NodeDrag,
            )
            .with_node_drag(node_drag_presentation(
                drag.generation,
                drag.cohort.clone(),
                drag.delta,
                Arc::clone(&drag.guides),
                drag.release_pending,
                None,
            )),
            InteractionState::Pan(_) => {
                CanvasInteractionPresentation::new(CanvasInteractionPhase::Pan)
            }
            InteractionState::Marquee(marquee) => {
                CanvasInteractionPresentation::new(CanvasInteractionPhase::Marquee).with_marquee(
                    rect_between(marquee.start.canvas(), marquee.pointer.canvas()),
                )
            }
            InteractionState::Connection(connection) => match connection {
                ConnectionInteraction::Dragging(drag) => {
                    CanvasInteractionPresentation::new(CanvasInteractionPhase::ConnectionDrag)
                        .with_connection(connection_presentation_from_drag(drag))
                }
                ConnectionInteraction::Searching(search) => {
                    CanvasInteractionPresentation::new(CanvasInteractionPhase::ConnectionSearch)
                        .with_connection(connection_presentation_from_search(search))
                }
            },
            InteractionState::KeyboardConnection(_) => {
                CanvasInteractionPresentation::new(CanvasInteractionPhase::ConnectionDrag)
            }
            InteractionState::Committing(commit) => match commit {
                CommitInteraction::Connection(connection) => {
                    CanvasInteractionPresentation::new(CanvasInteractionPhase::Committing)
                        .with_connection(connection_presentation_from_commit(connection))
                }
                CommitInteraction::KeyboardConnection(_) => {
                    CanvasInteractionPresentation::new(CanvasInteractionPhase::Committing)
                }
                CommitInteraction::NodeMove(node) => {
                    CanvasInteractionPresentation::new(CanvasInteractionPhase::Committing)
                        .with_node_drag(node_drag_presentation(
                            node.generation,
                            node.cohort.clone(),
                            node.delta,
                            Arc::clone(&node.guides),
                            false,
                            Some(node.ticket),
                        ))
                }
                CommitInteraction::EdgeDelete(edge) => {
                    CanvasInteractionPresentation::new(CanvasInteractionPhase::Committing)
                        .with_active_edge(edge.instance)
                }
                CommitInteraction::SelectionDelete(_) => {
                    CanvasInteractionPresentation::new(CanvasInteractionPhase::Committing)
                }
            },
            InteractionState::EdgeActive(edge) => {
                CanvasInteractionPresentation::new(CanvasInteractionPhase::EdgeActive)
                    .with_active_edge(edge.instance)
            }
            InteractionState::EdgePickPending(_) => {
                CanvasInteractionPresentation::new(CanvasInteractionPhase::Armed)
            }
        };
        presentation = presentation
            .with_generation(state_generation(&self.state))
            .with_connection_intent(state_connection_intent(&self.state))
            .with_hovered_edge(self.hovered_edge)
            .with_terminal(self.terminal);
        presentation
    }

    /// Returns the exact source capability while a keyboard connection is active or committing.
    pub fn keyboard_connection_source(&self) -> Option<KeyboardConnectionPort> {
        match &self.state {
            InteractionState::KeyboardConnection(connection) => Some(connection.start.source),
            InteractionState::Committing(CommitInteraction::KeyboardConnection(commit)) => {
                Some(commit.start.source)
            }
            _ => None,
        }
    }

    /// Returns the latest structured invalid-target feedback for the active keyboard source.
    pub fn keyboard_connection_feedback(&self) -> Option<&KeyboardConnectionFeedback> {
        match &self.state {
            InteractionState::KeyboardConnection(connection) => connection.feedback.as_ref(),
            _ => None,
        }
    }

    /// Returns the authoritative create-from-wire request for the active search generation.
    pub fn search_request(&self) -> Option<ConnectionSearchRequest> {
        match &self.state {
            InteractionState::Connection(ConnectionInteraction::Searching(search)) => {
                Some(search_request(self.id, self.provenance, search))
            }
            InteractionState::Committing(CommitInteraction::Connection(commit))
                if commit.return_to_search =>
            {
                Some(connection_search_request(
                    self.id,
                    self.provenance,
                    commit.generation,
                    commit
                        .search_attempt
                        .expect("return-to-search commits retain their request attempt"),
                    &commit.start,
                    commit.pointer,
                ))
            }
            _ => None,
        }
    }

    /// Resolves create-from-wire candidates through this session's exact active search request.
    pub fn search_candidates(
        &self,
        request: &ConnectionSearchRequest,
    ) -> Result<CreateFromWireCandidates, CanvasInteractionError> {
        self.validate_search_request(request)?;
        self.bound_view
            .create_from_wire_candidates(&request.token.binding)
            .map_err(map_create_from_wire_query_error)
    }

    /// Seals application-owned topology to this session's exact active search request.
    pub fn create_search_selection(
        &self,
        request: &ConnectionSearchRequest,
        target: PortId,
        creation: NodeCreation,
    ) -> Result<Option<ConnectionSearchSelection>, CanvasInteractionError> {
        self.validate_search_request(request)?;
        self.bound_view
            .validate_connection_binding(&request.token.binding)
            .map_err(map_connection_binding_error)?;
        let snapshot = self
            .bound_view
            .graph_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        Ok(ConnectionSearchSelection::from_snapshot(
            request.token.clone(),
            &snapshot,
            target,
            creation,
        ))
    }

    pub fn press(
        &mut self,
        target: CanvasPressTarget,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let (position, _) = self.normalize_pointer_sample(sample)?;
        let view = self.authoritative_view()?;
        self.validate_press_target(&view, &target)?;
        self.press_current(target, position)
    }

    #[cfg(test)]
    fn press_for_test(
        &mut self,
        target: CanvasPressTarget,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let (position, _) = self.normalize_pointer_sample(sample)?;
        self.validate_press_target_provenance(&target)?;
        self.press_current(target, position)
    }

    fn validate_press_target(
        &self,
        view: &WeakViewSession,
        target: &CanvasPressTarget,
    ) -> Result<(), CanvasInteractionError> {
        self.validate_press_target_provenance(target)?;
        match target {
            CanvasPressTarget::Connection(start) => {
                let snapshot = view
                    .graph_snapshot()
                    .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
                if start.validated_version != snapshot.version() {
                    return Err(CanvasInteractionError::StaleInteractionFacts);
                }
                view.validate_connection_binding(&start.binding)
                    .map_err(map_connection_binding_error)
            }
            CanvasPressTarget::NodeDrag(start) => self.validate_node_drag_start(view, start),
            CanvasPressTarget::Edge { instance, .. } => {
                let snapshot = view
                    .graph_snapshot()
                    .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
                match snapshot.edge_instance_key(instance.edge()) {
                    Some(current) if current == *instance => Ok(()),
                    _ => Err(CanvasInteractionError::StaleInteractionFacts),
                }
            }
            _ => Ok(()),
        }
    }

    fn validate_node_drag_start(
        &self,
        view: &WeakViewSession,
        start: &NodeDragStart,
    ) -> Result<(), CanvasInteractionError> {
        if start.provenance() != self.provenance {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        let snapshot = view
            .graph_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        if start
            .members()
            .iter()
            .any(|member| member.provenance() != snapshot.provenance())
        {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        if !start.members().contains(&start.clicked_lifetime())
            || start.members().iter().any(|member| {
                !snapshot.node_lifetime_is_current(*member)
                    || snapshot.graph().node(&member.node()).is_none_or(|node| {
                        node.hidden
                            || !node.selectable.unwrap_or(true)
                            || !node.draggable.unwrap_or(true)
                    })
            })
        {
            return Err(CanvasInteractionError::StaleInteractionFacts);
        }
        Ok(())
    }

    fn validate_node_drag_activation(
        &self,
        start: &NodeDragStart,
    ) -> Result<(), CanvasInteractionError> {
        #[cfg(test)]
        if self.test_authority_bypass {
            return Ok(());
        }
        let view = self.authoritative_view()?;
        self.validate_node_drag_start(&view, start)
    }

    fn validate_press_target_provenance(
        &self,
        target: &CanvasPressTarget,
    ) -> Result<(), CanvasInteractionError> {
        self.ensure_available()?;
        if let CanvasPressTarget::Connection(start) = &target
            && !start.belongs_to(self.provenance)
        {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        if let CanvasPressTarget::NodeDrag(start) = &target
            && start.provenance() != self.provenance
        {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        if let CanvasPressTarget::Edge { instance, .. } = &target
            && instance.provenance() != self.provenance.runtime()
        {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        if let CanvasPressTarget::Marquee { baseline, .. } = &target
            && baseline.provenance() != self.provenance
        {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        Ok(())
    }

    fn press_current(
        &mut self,
        target: CanvasPressTarget,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_available()?;
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        self.terminal = None;
        self.clear_pending_edge_pick_work();
        let mut update = CanvasInteractionUpdate::default();
        match target {
            CanvasPressTarget::NativeControl => return Ok(self.finalize_update(update)),
            CanvasPressTarget::Edge {
                instance,
                selection_mode,
            } => {
                let generation = self.allocate_generation()?;
                self.state = InteractionState::EdgeActive(EdgeInteraction {
                    generation,
                    instance,
                });
                update.pending_selection = Some(SelectionMutation::Edge {
                    provenance: self.provenance,
                    instance,
                    mode: selection_mode,
                });
                update.damage = InteractionDamage {
                    presentation: true,
                    edges: true,
                    selection: true,
                    ..InteractionDamage::default()
                };
            }
            CanvasPressTarget::Canvas(fallback) => {
                let generation = self.allocate_generation()?;
                let Some(query) = self.allocate_query() else {
                    self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
                    return Ok(
                        self.finalize_update(update.with_damage(InteractionDamage::presentation()))
                    );
                };
                if self.hovered_edge.take().is_some() {
                    update.damage.edges = true;
                }
                self.query_work.replace(PendingInteractionQuery::edge_pick(
                    query,
                    EdgePickQuery::new(
                        position.view(),
                        self.config.edge_pick_radius_in_view,
                        self.config.query_entry_budget,
                    )
                    .bind_interaction(self.interaction_query_binding(query)),
                ));
                update.damage.presentation = true;
                self.state = InteractionState::EdgePickPending(CanvasEdgePickInteraction {
                    generation,
                    query,
                    press: position,
                    pointer: position,
                    max_view_distance_squared: 0.0,
                    fallback,
                    stale_query_restarts: 0,
                    release_pending: false,
                });
            }
            CanvasPressTarget::NodeDrag(start) => {
                let generation = self.allocate_generation()?;
                let promoted = self.promote_node_interaction(start.clicked_lifetime())?;
                self.state = InteractionState::Armed(ArmedInteraction {
                    generation,
                    target: ArmedTarget::NodeDrag(start),
                    start: position,
                    max_view_distance_squared: 0.0,
                });
                update.damage.merge(InteractionDamage::presentation());
                if promoted {
                    update.damage.nodes = true;
                    update.damage.ports = true;
                }
            }
            CanvasPressTarget::Pan => {
                self.arm(
                    ArmedTarget::Pan {
                        selection_mode: None,
                    },
                    position,
                )?;
                update.damage = InteractionDamage::presentation();
            }
            CanvasPressTarget::Marquee {
                selection_mode,
                baseline,
            } => {
                self.arm(
                    ArmedTarget::Marquee {
                        selection_mode,
                        baseline,
                    },
                    position,
                )?;
                update.damage = InteractionDamage::presentation();
            }
            CanvasPressTarget::Connection(start) => {
                self.arm(ArmedTarget::Connection(start), position)?;
                update.damage = InteractionDamage::connection();
            }
        }
        self.commit_selection_transition(&mut update, None)?;
        Ok(self.finalize_update(update))
    }

    /// Selects one actionable node without synthesizing a pointer drag or arming movement.
    ///
    /// Keyboard and accessibility adapters use this path so a selectable but non-draggable node
    /// remains activatable and native child controls do not inherit drag semantics.
    pub fn activate_node(
        &mut self,
        node: NodeLifetimeKey,
        selection_mode: SelectionMode,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let view = self.authoritative_view()?;
        let snapshot = view
            .graph_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        if node.provenance() != snapshot.provenance() {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        if !snapshot.node_lifetime_is_current(node)
            || snapshot
                .graph()
                .node(&node.node())
                .is_none_or(|record| record.hidden || !record.selectable.unwrap_or(true))
        {
            return Err(CanvasInteractionError::StaleInteractionFacts);
        }
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        let promoted = self.promote_node_interaction(node)?;
        self.terminal = None;
        self.clear_pending_edge_pick_work();
        let mut update = CanvasInteractionUpdate {
            pending_selection: Some(SelectionMutation::Node {
                provenance: self.provenance,
                node,
                mode: selection_mode,
            }),
            damage: InteractionDamage {
                presentation: true,
                nodes: promoted,
                ports: promoted,
                selection: true,
                ..InteractionDamage::default()
            },
            ..CanvasInteractionUpdate::default()
        };
        self.commit_selection_transition(&mut update, None)?;
        Ok(self.finalize_update(update))
    }

    /// Moves the exact current node selection through the ordinary graph-command pipeline.
    ///
    /// This deliberately accepts only a direction and a modifier state. The session derives the
    /// selected lifetime cohort, grid-adjusted step, shared extent-clamped delta, and fresh command
    /// authority; native adapters do not synthesize pointer motion or geometry mutations.
    pub fn nudge_selection(
        &mut self,
        direction: NodeNudgeDirection,
        accelerated: bool,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let view = self.authoritative_view()?;
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        let (selected_nodes, _, _) = view
            .live_selection()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        let Some(cohort) = NodeMoveCohort::new(selected_nodes) else {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        };
        let snapshot = view
            .graph_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        let multiplier = if accelerated {
            self.config.keyboard_nudge_accelerated_multiplier
        } else {
            1.0
        };
        let requested = direction.delta(self.config.keyboard_nudge_step_in_canvas * multiplier);
        let requested = nudge_delta_for_grid(
            &snapshot,
            &cohort,
            requested,
            self.config.grid_size_in_canvas,
        )
        .ok_or(CanvasInteractionError::StaleInteractionFacts)?;
        let Some(delta) = feasible_cohort_delta(&snapshot, &cohort, requested)
            .map_err(map_node_move_bind_error)?
        else {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        };
        let Some(intent) = RelativeNodeMoveIntent::from_cohort(cohort.clone(), delta) else {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        };
        let generation = self.allocate_generation()?;
        let Some(ticket) = self.allocate_ticket() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return Ok(self.finalize_update(
                CanvasInteractionUpdate::default().with_damage(InteractionDamage::node_drag()),
            ));
        };
        self.terminal = None;
        self.clear_pending_edge_pick_work();
        let mut update =
            CanvasInteractionUpdate::default().with_damage(InteractionDamage::node_drag());
        self.push_graph_mutation(&mut update, ticket, FreshGraphMutation::move_nodes(intent));
        self.state =
            InteractionState::Committing(CommitInteraction::NodeMove(NodeMoveCommitInteraction {
                generation,
                cohort,
                delta,
                guides: Arc::default(),
                ticket,
            }));
        Ok(self.finalize_update(update))
    }

    /// Minimally pans this view's owned viewport until one exact live node is visible.
    ///
    /// The caller supplies only the focused lifetime capability, actual native view dimensions,
    /// and a screen-space padding. Zoom is preserved exactly; stale or unmeasured nodes fail
    /// closed instead of guessing an adapter-local rectangle.
    pub fn reveal_node(
        &mut self,
        node: NodeLifetimeKey,
        viewport: ViewSize,
        padding_in_view: f32,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let view = self.authoritative_view()?;
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        if !viewport.is_positive_finite()
            || !padding_in_view.is_finite()
            || padding_in_view < 0.0
            || padding_in_view * 2.0 >= viewport.width
            || padding_in_view * 2.0 >= viewport.height
        {
            return Err(CanvasInteractionError::InvalidRevealRequest);
        }
        let snapshot = view
            .graph_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        if node.provenance() != snapshot.provenance() {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        if !snapshot.node_lifetime_is_current(node) {
            return Err(CanvasInteractionError::StaleInteractionFacts);
        }
        let geometry = view
            .geometry_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        if geometry.provenance() != self.provenance {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        let Some(geometry) = geometry.node(node.node()) else {
            return Err(CanvasInteractionError::StaleInteractionFacts);
        };
        if !geometry.is_fresh() || geometry.lifetime() != node {
            return Err(CanvasInteractionError::StaleInteractionFacts);
        }
        let current = self.current_viewport_snapshot()?;
        let delta = reveal_pan_delta(
            geometry.bounds(),
            current.transform(),
            viewport,
            padding_in_view,
        )
        .ok_or(CanvasInteractionError::InvalidRevealRequest)?;
        if delta.is_zero() {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        let (committed, panic) = self.commit_pan(delta)?;
        let mut update = CanvasInteractionUpdate::default();
        if committed.revision() != current.revision() {
            update.damage.viewport = true;
        }
        let update = self.finalize_update(update);
        resume(panic);
        Ok(update)
    }

    /// Activates one exact real port through the renderer-neutral non-drag connection authority.
    ///
    /// The first activation arms `port` as the source. While that source remains armed, the next
    /// activation is evaluated as a target. Adapters therefore do not need to mirror source/target
    /// state when mapping keyboard, accessibility, or click-to-connect input.
    pub fn activate_connection_port(
        &mut self,
        port: KeyboardConnectionPort,
        prospective_edge: EdgeId,
        mode: NodeGraphConnectionMode,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        if matches!(self.state, InteractionState::KeyboardConnection(_)) {
            self.activate_keyboard_connection_target(port)
        } else {
            self.activate_keyboard_connection(port, prospective_edge, mode)
        }
    }

    /// Releases an armed pointer connection, promoting only a true click to non-drag activation.
    ///
    /// The final release sample participates in the runtime drag-threshold decision even when the
    /// adapter did not observe an intermediate move event. Adapters therefore cannot accidentally
    /// turn a fast drag release into click-to-connect.
    pub fn release_connection_or_activate_port(
        &mut self,
        source: KeyboardConnectionPort,
        prospective_edge: EdgeId,
        mode: NodeGraphConnectionMode,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let (position, current) = self.normalize_pointer_sample(sample)?;
        self.rebase_interaction_positions(current)?;
        let is_click = match &self.state {
            InteractionState::Armed(armed)
                if matches!(armed.target, ArmedTarget::Connection(_)) =>
            {
                let distance = armed
                    .max_view_distance_squared
                    .max(view_distance_squared(armed.start.view(), position.view()));
                !threshold_crossed_distance(distance, self.config.connection_drag_threshold_in_view)
            }
            _ => false,
        };
        if !is_click {
            return self.release(sample);
        }
        let released = self.release(sample)?;
        debug_assert!(released.effects.is_empty());
        debug_assert!(!released.query_work_pending);
        self.activate_connection_port(source, prospective_edge, mode)
    }

    /// Arms one exact real port as the source of a keyboard connection interaction.
    ///
    /// The bound view resolves current port policy and reconnect semantics. Re-activating the
    /// already armed source is an intentional no-op; another active semantic interaction remains
    /// exclusive. No pointer sample or adapter-owned geometry participates in this transition.
    pub fn activate_keyboard_connection(
        &mut self,
        source: KeyboardConnectionPort,
        prospective_edge: EdgeId,
        mode: NodeGraphConnectionMode,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        if let InteractionState::KeyboardConnection(connection) = &self.state
            && connection.start.source == source
        {
            let binding = connection.start.binding.clone();
            self.authoritative_view()?
                .validate_connection_binding(&binding)
                .map_err(map_connection_binding_error)?;
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        let view = self.authoritative_view()?;
        let Some(start) = view
            .keyboard_connection_start(source, prospective_edge, mode)
            .map_err(map_connection_binding_error)?
        else {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        };
        let generation = self.allocate_generation()?;
        self.terminal = None;
        self.clear_pending_edge_pick_work();
        self.state = InteractionState::KeyboardConnection(KeyboardConnectionInteraction {
            generation,
            start,
            feedback: None,
        });
        Ok(self.finalize_update(
            CanvasInteractionUpdate::default().with_damage(InteractionDamage::connection()),
        ))
    }

    /// Activates one exact real target port for the currently armed keyboard source.
    ///
    /// Rejected policy decisions retain the active source and expose diagnostics through
    /// [`Self::keyboard_connection_feedback`]. Accepted decisions use the same fresh connection
    /// evaluation and `FreshGraphMutation` binder as pointer connection commits.
    pub fn activate_keyboard_connection_target(
        &mut self,
        target: KeyboardConnectionPort,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let (generation, start) = match &self.state {
            InteractionState::KeyboardConnection(connection) => {
                (connection.generation, connection.start.clone())
            }
            _ => return Err(CanvasInteractionError::KeyboardConnectionInactive),
        };
        if target == start.source {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        if !target.belongs_to(self.provenance) {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        let view = self.authoritative_view()?;
        let snapshot = view
            .graph_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        if target.instance().provenance() != snapshot.provenance() {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        if !snapshot.port_instance_is_current(target.instance()) {
            return Err(CanvasInteractionError::StaleInteractionFacts);
        }
        view.validate_connection_binding(&start.binding)
            .map_err(map_connection_binding_error)?;
        let evaluation = view
            .evaluate_connection(start.binding.edit_for_target(target.port()))
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        if !evaluation.is_accept() {
            let feedback = KeyboardConnectionFeedback::rejected(
                start.source.port(),
                target.port(),
                evaluation.diagnostics().to_vec(),
            );
            if let InteractionState::KeyboardConnection(connection) = &mut self.state {
                connection.feedback = Some(feedback);
            }
            return Ok(self.finalize_update(
                CanvasInteractionUpdate::default().with_damage(InteractionDamage::connection()),
            ));
        }
        let Some(ticket) = self.allocate_ticket() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            self.state = InteractionState::Idle;
            return Ok(self.finalize_update(
                CanvasInteractionUpdate::default().with_damage(InteractionDamage::connection()),
            ));
        };
        let mut update =
            CanvasInteractionUpdate::default().with_damage(InteractionDamage::connection());
        self.push_graph_mutation(
            &mut update,
            ticket,
            FreshGraphMutation::connection(
                start.binding.clone(),
                ConnectionCommitTarget::existing(target.instance()),
                evaluation.version(),
            ),
        );
        self.state = InteractionState::Committing(CommitInteraction::KeyboardConnection(
            KeyboardConnectionCommitInteraction {
                generation,
                start,
                target,
                ticket,
            },
        ));
        Ok(self.finalize_update(update))
    }

    /// Activates create-from-wire search without synthesizing a pointer drag.
    ///
    /// Keyboard and accessibility adapters use this path after resolving a real graph port. The
    /// resulting search owns the same generation, presentation, effects, and completion behavior
    /// as a pointer release on empty canvas.
    pub fn activate_connection_search(
        &mut self,
        start: ConnectionInteractionStart,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let (position, _) = self.normalize_pointer_sample(sample)?;
        let view = self.authoritative_view()?;
        if !start.belongs_to(self.provenance) {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        let snapshot = view
            .graph_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        if start.validated_version != snapshot.version() {
            return Err(CanvasInteractionError::StaleInteractionFacts);
        }
        view.validate_connection_binding(&start.binding)
            .map_err(map_connection_binding_error)?;
        self.activate_connection_search_current(start, position)
    }

    #[cfg(test)]
    fn activate_connection_search_for_test(
        &mut self,
        start: ConnectionInteractionStart,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let (position, _) = self.normalize_pointer_sample(sample)?;
        if !start.belongs_to(self.provenance) {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        self.activate_connection_search_current(start, position)
    }

    fn activate_connection_search_current(
        &mut self,
        start: ConnectionInteractionStart,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        if !start.allow_create_from_wire {
            return Err(CanvasInteractionError::ConnectionSearchUnavailable);
        }
        let generation = self.allocate_generation()?;
        let Some(attempt) = self.allocate_search_attempt() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return Err(CanvasInteractionError::IdentityExhausted);
        };
        self.terminal = None;
        self.clear_pending_edge_pick_work();
        let search = ConnectionSearchInteraction {
            generation,
            attempt,
            start,
            pointer: position,
            probe: ConnectionProbe::None,
            status: ConnectionSearchStatus::Loading,
        };
        let mut update = CanvasInteractionUpdate::default();
        update
            .effects
            .push(CanvasInteractionEffect::Search(search_request(
                self.id,
                self.provenance,
                &search,
            )));
        update.damage.merge(InteractionDamage::connection());
        self.state = InteractionState::Connection(ConnectionInteraction::Searching(search));
        Ok(self.finalize_update(update))
    }

    /// Requests deletion through the authoritative interaction and command-completion pipeline.
    ///
    /// A route whose effective policy denies deletion is a no-op and allocates no ticket. Once
    /// admitted, the mutation binds to a fresh graph snapshot before enqueueing; a retired edge
    /// therefore settles as a no-op instead of producing a stale disconnect command.
    pub fn delete_edge(
        &mut self,
        route: &PublishedEdgeRoute,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let view = self.authoritative_view()?;
        let snapshot = view
            .validate_published_edge_route(route)
            .map_err(|error| match error {
                ViewSessionError::ForeignInteractionFacts => {
                    CanvasInteractionError::ForeignInteractionFacts
                }
                ViewSessionError::StaleInteractionFacts => {
                    CanvasInteractionError::StaleInteractionFacts
                }
                _ => CanvasInteractionError::ViewUnavailable,
            })?;
        self.delete_edge_at_version(route, snapshot.version())
    }

    fn delete_edge_at_version(
        &mut self,
        route: &PublishedEdgeRoute,
        validated_version: GraphVersion,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_available()?;
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        let Some(intent) = EdgeDeletionIntent::from_route(route, validated_version) else {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        };
        self.terminal = None;
        self.clear_pending_edge_pick_work();
        let generation = self.allocate_generation()?;
        let Some(ticket) = self.allocate_ticket() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return Ok(self.finalize_update(
                CanvasInteractionUpdate::default().with_damage(edge_action_damage()),
            ));
        };
        if self
            .hovered_edge
            .is_some_and(|hovered| hovered.edge() == intent.edge())
        {
            self.hovered_edge = None;
        }
        let mut update = CanvasInteractionUpdate::default().with_damage(edge_action_damage());
        self.push_graph_mutation(&mut update, ticket, FreshGraphMutation::delete_edge(intent));
        self.state = InteractionState::Committing(CommitInteraction::EdgeDelete(
            EdgeDeleteCommitInteraction {
                generation,
                instance: route.instance(),
                ticket,
            },
        ));
        Ok(self.finalize_update(update))
    }

    /// Deletes every currently selected node and edge admitted by the effective runtime policy.
    ///
    /// The complete deletion is retained as one revision-bound intent and submitted as one topology
    /// command. Selected groups are preserved. Selected edges that will already be removed by a
    /// selected node's cascade are omitted from the explicit disconnect set.
    pub fn delete_selection(&mut self) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let view = self.authoritative_view()?;
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        let (selected_nodes, selected_edges, _) = view
            .live_selection()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        let snapshot = view
            .graph_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        let policy = view
            .graph_policy()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        let Some(intent) = SelectionDeletionIntent::from_selection(
            &snapshot,
            policy,
            &selected_nodes,
            &selected_edges,
        ) else {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        };

        self.terminal = None;
        self.clear_pending_edge_pick_work();
        let generation = self.allocate_generation()?;
        let Some(ticket) = self.allocate_ticket() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return Ok(self.finalize_update(
                CanvasInteractionUpdate::default().with_damage(selection_delete_damage()),
            ));
        };
        if self
            .hovered_edge
            .is_some_and(|hovered| intent.edges().any(|edge| edge == hovered.edge()))
        {
            self.hovered_edge = None;
        }
        let mut update = CanvasInteractionUpdate::default().with_damage(selection_delete_damage());
        self.push_graph_mutation(
            &mut update,
            ticket,
            FreshGraphMutation::delete_selection(intent),
        );
        self.state = InteractionState::Committing(CommitInteraction::SelectionDelete(
            SelectionDeleteCommitInteraction { generation, ticket },
        ));
        Ok(self.finalize_update(update))
    }

    pub fn request_edge_pick(
        &mut self,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let (position, _) = self.normalize_pointer_sample(sample)?;
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        self.clear_pending_edge_pick_work();
        let Some(id) = self.allocate_query() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            let damage = if self.hovered_edge.take().is_some() {
                InteractionDamage {
                    presentation: true,
                    edges: true,
                    ..InteractionDamage::default()
                }
            } else {
                InteractionDamage::default()
            };
            return Ok(self.finalize_update(CanvasInteractionUpdate::default().with_damage(damage)));
        };
        self.pending_edge_pick = Some(PendingHoverEdgePick {
            id,
            position,
            stale_query_restarts: 0,
        });
        let update = CanvasInteractionUpdate::default();
        self.query_work.replace(PendingInteractionQuery::edge_pick(
            id,
            EdgePickQuery::new(
                position.view(),
                self.config.edge_pick_radius_in_view,
                self.config.query_entry_budget,
            )
            .bind_interaction(self.interaction_query_binding(id)),
        ));
        Ok(self.finalize_update(update))
    }

    /// Clears adapter-local edge hover when the pointer leaves the graph-interactive surface.
    pub fn clear_edge_hover(&mut self) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        self.clear_pending_edge_pick_work();
        let update = if self.hovered_edge.take().is_none() {
            CanvasInteractionUpdate::default()
        } else {
            CanvasInteractionUpdate::default().with_damage(InteractionDamage {
                presentation: true,
                edges: true,
                ..InteractionDamage::default()
            })
        };
        Ok(self.finalize_update(update))
    }

    fn clear_pending_edge_pick_work(&mut self) {
        if let Some(pending) = self.pending_edge_pick.take() {
            self.query_work.clear_if_id(pending.id);
        }
    }

    /// Reports whether runtime owns a pending query continuation or its required terminal cleanup.
    pub fn query_work_pending(&self) -> bool {
        self.query_work.is_pending()
    }

    /// Advances at most `page_budget` runtime-owned interaction query pages.
    ///
    /// A zero budget never accesses the bound view or mutates the continuation. Native adapters
    /// own when this method runs, while query identity, cursors, stale restarts, target locks, and
    /// cleanup remain inside this session. A nonzero grant also retries terminal cleanup that was
    /// deferred by a transient view borrow; cleanup retries are not counted as query pages.
    pub fn advance_query_work(&mut self, page_budget: usize) -> CanvasInteractionQueryAdvance {
        let mut update = CanvasInteractionUpdate::default();
        let mut receipt = CanvasInteractionQueryAdvanceReceipt {
            pages_granted: page_budget,
            ..CanvasInteractionQueryAdvanceReceipt::default()
        };
        let mut terminal_error = self.ensure_session_open().err();

        if terminal_error.is_none() && page_budget > 0 && self.query_work.cleanup_pending() {
            let (cleanup_update, cleanup_error) = self.terminate_query_failure();
            update.merge_internal(cleanup_update);
            terminal_error = cleanup_error;
            let update = self.finalize_update(update);
            let needs_tick = update.query_work_pending();
            return CanvasInteractionQueryAdvance {
                update,
                needs_tick,
                receipt,
                terminal_error,
            };
        }

        for _ in 0..page_budget {
            if terminal_error.is_some() {
                break;
            }
            let Some(query) = self.query_work.take() else {
                break;
            };
            let failed_edge_pick =
                matches!(&query.kind, PendingInteractionQueryKind::EdgePick { .. });
            receipt.pages_attempted = receipt.pages_attempted.saturating_add(1);
            let (page_update, page_receipt) = match self.resolve_query_page(query) {
                Ok(page) => page,
                Err(error) => {
                    self.query_work.clear();
                    let (cleanup_update, _) = self.terminate_query_failure();
                    update.merge_internal(cleanup_update);
                    if failed_edge_pick {
                        self.clear_pending_edge_pick_work();
                        self.hovered_edge = None;
                        update.damage.merge(InteractionDamage {
                            presentation: true,
                            edges: true,
                            ..InteractionDamage::default()
                        });
                    }
                    terminal_error = Some(error);
                    break;
                }
            };
            if let Some(page_receipt) = page_receipt {
                receipt.page_receipts.push(page_receipt);
            }
            update.merge_internal(page_update);
        }

        let update = self.finalize_update(update);
        let needs_tick = update.query_work_pending();
        CanvasInteractionQueryAdvance {
            update,
            needs_tick,
            receipt,
            terminal_error,
        }
    }

    fn terminate_query_failure(
        &mut self,
    ) -> (CanvasInteractionUpdate, Option<CanvasInteractionError>) {
        let ticket = interaction_state_submission_ticket(&self.state);
        let pending_submission = ticket.filter(|ticket| {
            matches!(
                self.submission_binding.as_ref(),
                Some(SubmissionBinding {
                    ticket: bound_ticket,
                    phase: SubmissionBindingPhase::Pending,
                }) if bound_ticket == ticket
            )
        });
        if let Some(ticket) = pending_submission {
            return match self.submission_not_dispatched(ticket) {
                Ok(update) => (update, None),
                Err(error) => (self.force_terminate_query_failure(), Some(error)),
            };
        }
        match self.cancel_with_selection_failure() {
            Ok(update) => (update, None),
            Err(failure) if failure.retryable => {
                self.query_work.require_cleanup();
                (CanvasInteractionUpdate::default(), Some(failure.error))
            }
            Err(failure) => (self.force_terminate_query_failure(), Some(failure.error)),
        }
    }

    fn force_terminate_query_failure(&mut self) -> CanvasInteractionUpdate {
        let damage = damage_for_state(&self.state);
        self.query_work.clear();
        self.clear_pending_edge_pick_work();
        self.submission_binding = None;
        self.state = InteractionState::Idle;
        self.terminal = Some(InteractionTerminalReason::Cancelled);
        CanvasInteractionUpdate::default().with_damage(damage)
    }

    fn resolve_query_page(
        &mut self,
        query: PendingInteractionQuery,
    ) -> Result<
        (
            CanvasInteractionUpdate,
            Option<CanvasInteractionQueryPageReceipt>,
        ),
        CanvasInteractionError,
    > {
        let id = query.id();
        if self.callback_authority(id.session(), None, self.query_is_current(&query))?
            == CallbackAuthority::Stale
        {
            return Ok((
                self.finalize_update(CanvasInteractionUpdate::default()),
                None,
            ));
        }
        let view = self.bound_view.clone();
        match query.kind {
            PendingInteractionQueryKind::EdgePick { id, query } => match view.edge_pick(query) {
                Ok(outcome) => {
                    let receipt = outcome
                        .receipt()
                        .map(CanvasInteractionQueryPageReceipt::EdgePick);
                    self.resolve_edge_pick_outcome(id, outcome)
                        .map(|update| (update, receipt))
                }
                Err(error) => self
                    .resolve_edge_pick_query_error(id, error)
                    .map(|update| (update, None)),
            },
            PendingInteractionQueryKind::ConnectionProbe { id, query, .. } => {
                match view.connection_probe(query) {
                    Ok(outcome) => {
                        let receipt = outcome
                            .receipt()
                            .map(CanvasInteractionQueryPageReceipt::ConnectionProbe);
                        self.resolve_connection_probe_outcome(id, outcome)
                            .map(|update| (update, receipt))
                    }
                    Err(error) => self
                        .resolve_connection_probe_query_error(id, error)
                        .map(|update| (update, None)),
                }
            }
            PendingInteractionQueryKind::NodeIntersection { id, query, .. } => {
                match view.nodes_intersecting(query) {
                    Ok(outcome) => {
                        let receipt = outcome
                            .receipt()
                            .map(CanvasInteractionQueryPageReceipt::NodeIntersection);
                        self.resolve_node_intersection_outcome(id, outcome)
                            .map(|update| (update, receipt))
                    }
                    Err(error) => self
                        .resolve_node_intersection_query_error(id, error)
                        .map(|update| (update, None)),
                }
            }
            PendingInteractionQueryKind::AlignmentSnap { id, query, .. } => {
                match view.alignment_snap(query) {
                    Ok(outcome) => {
                        let receipt = outcome
                            .receipt()
                            .map(CanvasInteractionQueryPageReceipt::AlignmentSnap);
                        self.resolve_alignment_snap_outcome(id, outcome)
                            .map(|update| (update, receipt))
                    }
                    Err(error) => self
                        .resolve_alignment_query_error(id, error)
                        .map(|update| (update, None)),
                }
            }
        }
    }

    fn query_is_current(&self, query: &PendingInteractionQuery) -> bool {
        match &query.kind {
            PendingInteractionQueryKind::EdgePick { id, .. } => {
                matches!(&self.state, InteractionState::EdgePickPending(pending) if pending.query == *id)
                    || self
                        .pending_edge_pick
                        .is_some_and(|pending| pending.id == *id)
            }
            PendingInteractionQueryKind::ConnectionProbe { id, generation, .. } => {
                match &self.state {
                    InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => {
                        drag.generation == *generation && drag.pending_query == Some(*id)
                    }
                    InteractionState::Committing(CommitInteraction::Connection(commit)) => {
                        commit.generation == *generation
                            && matches!(
                                commit.freshness,
                                ConnectionCommitFreshness::Probing(pending) if pending == *id
                            )
                    }
                    _ => false,
                }
            }
            PendingInteractionQueryKind::NodeIntersection { id, generation, .. } => matches!(
                &self.state,
                InteractionState::Marquee(marquee)
                    if marquee.generation == *generation && marquee.pending_query == Some(*id)
            ),
            PendingInteractionQueryKind::AlignmentSnap { id, generation, .. } => matches!(
                &self.state,
                InteractionState::NodeDrag(drag)
                    if drag.generation == *generation
                        && drag.pending_alignment_query == Some(*id)
            ),
        }
    }

    fn resolve_edge_pick_query_error(
        &mut self,
        id: InteractionQueryId,
        error: EdgePickQueryError,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        match error {
            EdgePickQueryError::StaleCursor(reason) if reason.is_restartable() => {
                self.restart_stale_query(id)
            }
            EdgePickQueryError::StaleCursor(EdgePickCursorStaleReason::ForeignView) => {
                Err(CanvasInteractionError::ForeignInteractionFacts)
            }
            EdgePickQueryError::StaleCursor(EdgePickCursorStaleReason::ViewGeneration) => {
                Err(CanvasInteractionError::StaleInteractionFacts)
            }
            EdgePickQueryError::StaleCursor(_) => {
                Err(CanvasInteractionError::StaleInteractionFacts)
            }
            EdgePickQueryError::Session(error) => Err(map_connection_binding_error(error)),
            EdgePickQueryError::InvalidQuery => Err(CanvasInteractionError::InvalidPosition),
        }
    }

    fn resolve_connection_probe_query_error(
        &mut self,
        id: InteractionQueryId,
        error: ConnectionProbeQueryError,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        match error {
            ConnectionProbeQueryError::StaleCursor(reason) if reason.is_restartable() => {
                self.restart_stale_connection_query(id, reason)
            }
            ConnectionProbeQueryError::StaleCursor(
                ConnectionProbeCursorStaleReason::ForeignView,
            ) => Err(CanvasInteractionError::ForeignInteractionFacts),
            ConnectionProbeQueryError::StaleCursor(
                ConnectionProbeCursorStaleReason::ViewGeneration,
            ) => Err(CanvasInteractionError::StaleInteractionFacts),
            ConnectionProbeQueryError::StaleCursor(_) => {
                Err(CanvasInteractionError::StaleInteractionFacts)
            }
            ConnectionProbeQueryError::Session(error) => Err(map_connection_binding_error(error)),
            ConnectionProbeQueryError::InvalidQuery => Err(CanvasInteractionError::InvalidPosition),
        }
    }

    fn resolve_node_intersection_query_error(
        &mut self,
        id: InteractionQueryId,
        error: NodeIntersectionQueryError,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        match error {
            NodeIntersectionQueryError::StaleCursor(reason) if reason.is_restartable() => {
                self.restart_stale_query(id)
            }
            NodeIntersectionQueryError::StaleCursor(
                NodeIntersectionCursorStaleReason::ForeignView,
            ) => Err(CanvasInteractionError::ForeignInteractionFacts),
            NodeIntersectionQueryError::StaleCursor(
                NodeIntersectionCursorStaleReason::ViewGeneration,
            ) => Err(CanvasInteractionError::StaleInteractionFacts),
            NodeIntersectionQueryError::StaleCursor(_) => {
                Err(CanvasInteractionError::StaleInteractionFacts)
            }
            NodeIntersectionQueryError::View(error) => Err(map_connection_binding_error(error)),
            NodeIntersectionQueryError::InvalidQuery => {
                Err(CanvasInteractionError::InvalidPosition)
            }
        }
    }

    fn resolve_alignment_query_error(
        &mut self,
        id: InteractionQueryId,
        error: ViewSessionError,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        match error {
            ViewSessionError::StaleInteractionFacts => self.restart_stale_query(id),
            ViewSessionError::ForeignInteractionFacts => {
                Err(CanvasInteractionError::ForeignInteractionFacts)
            }
            error => Err(map_connection_binding_error(error)),
        }
    }

    fn resolve_edge_pick_outcome(
        &mut self,
        id: InteractionQueryId,
        outcome: EdgePickQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        self.ensure_interaction_session(id.session())?;
        let view = self.authoritative_view()?;
        let Some(receipt) = outcome.receipt() else {
            return Err(CanvasInteractionError::MissingInteractionQueryReceipt { query: id });
        };
        if receipt.query_id() != id {
            return Err(CanvasInteractionError::MismatchedInteractionQueryReceipt {
                expected: id,
                actual: receipt.query_id(),
            });
        }
        if !receipt.authenticates(self.interaction_query_binding(id)) {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        match view.edge_pick_receipt_is_current(receipt) {
            Ok(true) => self.resolve_edge_pick_current(id, outcome),
            Ok(false) | Err(ViewSessionError::StaleInteractionFacts) => {
                self.restart_stale_query(id)
            }
            Err(ViewSessionError::ForeignInteractionFacts) => {
                Err(CanvasInteractionError::ForeignInteractionFacts)
            }
            Err(_) => Err(CanvasInteractionError::ViewUnavailable),
        }
    }

    fn resolve_edge_pick_current(
        &mut self,
        id: InteractionQueryId,
        outcome: EdgePickQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        if matches!(
            &self.state,
            InteractionState::EdgePickPending(pending) if pending.query == id
        ) {
            return self.resolve_canvas_press_edge_pick(id, outcome);
        }

        let mut update = CanvasInteractionUpdate::default();
        if !self
            .pending_edge_pick
            .is_some_and(|pending| pending.id == id)
        {
            return Ok(self.finalize_update(update));
        }
        match outcome {
            EdgePickQueryOutcome::Incomplete { cursor, .. } => {
                self.query_work.replace(PendingInteractionQuery::edge_pick(
                    id,
                    EdgePickQuery::resume(cursor, self.config.query_entry_budget)
                        .bind_interaction(self.interaction_query_binding(id)),
                ));
            }
            EdgePickQueryOutcome::Complete { pick, .. } => {
                self.clear_pending_edge_pick_work();
                let edge = pick.map(|pick| pick.instance());
                if self.hovered_edge != edge {
                    self.hovered_edge = edge;
                    update.damage = InteractionDamage {
                        presentation: true,
                        edges: true,
                        ..InteractionDamage::default()
                    };
                }
            }
            EdgePickQueryOutcome::BudgetExhausted { .. } => {
                self.clear_pending_edge_pick_work();
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::BudgetExhausted);
                update.damage = InteractionDamage {
                    presentation: true,
                    edges: true,
                    ..InteractionDamage::default()
                };
            }
            EdgePickQueryOutcome::GeometryIndeterminate { .. } => {
                self.clear_pending_edge_pick_work();
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::GeometryIndeterminate);
                update.damage = InteractionDamage {
                    presentation: true,
                    edges: true,
                    ..InteractionDamage::default()
                };
            }
        }
        Ok(self.finalize_update(update))
    }

    fn resolve_canvas_press_edge_pick(
        &mut self,
        id: InteractionQueryId,
        outcome: EdgePickQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let resolution = match outcome {
            EdgePickQueryOutcome::Incomplete { cursor, .. } => CanvasEdgePickResolution::Continue(
                EdgePickQuery::resume(cursor, self.config.query_entry_budget)
                    .bind_interaction(self.interaction_query_binding(id)),
            ),
            EdgePickQueryOutcome::Complete { pick, .. } => {
                CanvasEdgePickResolution::Complete(pick.map(|pick| pick.instance()))
            }
            EdgePickQueryOutcome::BudgetExhausted { .. } => {
                CanvasEdgePickResolution::BudgetExhausted
            }
            EdgePickQueryOutcome::GeometryIndeterminate { .. } => {
                CanvasEdgePickResolution::GeometryIndeterminate
            }
        };
        self.resolve_canvas_press_edge_resolution(id, resolution)
    }

    fn resolve_canvas_press_edge_resolution(
        &mut self,
        id: InteractionQueryId,
        resolution: CanvasEdgePickResolution,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        if let Some(update) = self.try_finish_canvas_pan_fallback(id, &resolution)? {
            return Ok(update);
        }
        let state = std::mem::take(&mut self.state);
        let InteractionState::EdgePickPending(pending) = state else {
            self.state = state;
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        };
        if pending.query != id {
            self.state = InteractionState::EdgePickPending(pending);
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }

        let mut update = CanvasInteractionUpdate::default();
        self.state = match resolution {
            CanvasEdgePickResolution::Continue(query) => {
                self.query_work
                    .replace(PendingInteractionQuery::edge_pick(id, query));
                InteractionState::EdgePickPending(pending)
            }
            CanvasEdgePickResolution::Complete(edge) => {
                self.finish_canvas_press_edge_pick(pending, edge, &mut update)?
            }
            CanvasEdgePickResolution::BudgetExhausted => {
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::BudgetExhausted);
                update.damage = edge_pick_terminal_damage();
                InteractionState::Idle
            }
            CanvasEdgePickResolution::GeometryIndeterminate => {
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::GeometryIndeterminate);
                update.damage = edge_pick_terminal_damage();
                InteractionState::Idle
            }
        };
        self.commit_selection_transition(&mut update, None)?;
        Ok(self.finalize_update(update))
    }

    fn try_finish_canvas_pan_fallback(
        &mut self,
        id: InteractionQueryId,
        resolution: &CanvasEdgePickResolution,
    ) -> Result<Option<CanvasInteractionUpdate>, CanvasInteractionError> {
        if !matches!(resolution, CanvasEdgePickResolution::Complete(None)) {
            return Ok(None);
        }
        let Some((generation, press, pointer, release_pending)) = (match &self.state {
            InteractionState::EdgePickPending(pending)
                if pending.query == id
                    && matches!(pending.fallback, CanvasPressFallback::Pan { .. })
                    && threshold_crossed_distance(
                        pending.max_view_distance_squared,
                        self.config.canvas_drag_threshold_in_view,
                    ) =>
            {
                Some((
                    pending.generation,
                    pending.press,
                    pending.pointer,
                    pending.release_pending,
                ))
            }
            _ => None,
        }) else {
            return Ok(None);
        };

        let current = self.current_viewport_snapshot()?;
        if current.provenance() != self.provenance {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        validate_position_reprojection(press, current)?;
        validate_position_reprojection(pointer, current)?;
        let press = press
            .reproject(current)
            .expect("the canvas press was validated against this viewport");
        let pointer = pointer
            .reproject(current)
            .expect("the canvas pointer was validated against this viewport");
        let delta = view_delta(press.view(), pointer.view());

        let (committed, panic) = if delta.x == 0.0 && delta.y == 0.0 {
            (current, None)
        } else {
            let preview = ViewportIntent::pan(ViewportPanRequest::new(delta))
                .resolve(current.transform())
                .ok_or(CanvasInteractionError::InvalidPosition)?;
            let preview = ViewportSnapshot::new(self.provenance, current.revision(), preview);
            pointer
                .reproject(preview)
                .ok_or(CanvasInteractionError::InvalidPosition)?;
            self.commit_pan(delta)?
        };
        let pointer = pointer
            .reproject(committed)
            .expect("the committed canvas pan was previewed before mutation");

        let state = std::mem::take(&mut self.state);
        let InteractionState::EdgePickPending(pending) = state else {
            unreachable!("canvas pan fallback state cannot change during a synchronous commit");
        };
        debug_assert_eq!(pending.query, id);
        self.hovered_edge = None;
        self.state = if release_pending {
            InteractionState::Idle
        } else {
            InteractionState::Pan(PanInteraction {
                generation,
                pointer,
            })
        };
        let mut update = CanvasInteractionUpdate::default();
        update.damage.presentation = true;
        update.damage.viewport = committed.revision() != current.revision();
        let update = self.finalize_update(update);
        resume(panic);
        Ok(Some(update))
    }

    fn finish_canvas_press_edge_pick(
        &mut self,
        pending: CanvasEdgePickInteraction,
        instance: Option<EdgeInstanceKey>,
        update: &mut CanvasInteractionUpdate,
    ) -> Result<InteractionState, CanvasInteractionError> {
        self.hovered_edge = instance;
        let Some(instance) = instance else {
            return self.activate_canvas_press_fallback(pending, update);
        };

        if !within_click_tolerance(
            pending.max_view_distance_squared,
            self.config.canvas_click_tolerance_in_view,
        ) {
            update.damage = InteractionDamage {
                presentation: true,
                edges: true,
                ..InteractionDamage::default()
            };
            return Ok(InteractionState::Idle);
        }

        update.pending_selection = Some(SelectionMutation::Edge {
            provenance: self.provenance,
            instance,
            mode: pending.fallback.selection_mode(),
        });
        update.damage = InteractionDamage {
            presentation: true,
            edges: true,
            selection: true,
            ..InteractionDamage::default()
        };
        Ok(if pending.release_pending {
            InteractionState::Idle
        } else {
            InteractionState::EdgeActive(EdgeInteraction {
                generation: pending.generation,
                instance,
            })
        })
    }

    fn activate_canvas_press_fallback(
        &mut self,
        pending: CanvasEdgePickInteraction,
        update: &mut CanvasInteractionUpdate,
    ) -> Result<InteractionState, CanvasInteractionError> {
        let armed = ArmedInteraction {
            generation: pending.generation,
            target: pending.fallback.into_armed_target(),
            start: pending.press,
            max_view_distance_squared: pending.max_view_distance_squared,
        };
        let crossed = threshold_crossed_distance(
            pending.max_view_distance_squared,
            self.config.canvas_drag_threshold_in_view,
        );
        if pending.release_pending {
            if crossed {
                let activated = self.activate_armed(armed, pending.pointer, update)?;
                if let InteractionState::Marquee(mut marquee) = activated {
                    marquee.release_pending = true;
                    return Ok(InteractionState::Marquee(marquee));
                }
            } else {
                self.finish_armed_click(&armed, update);
            }
            update.damage.merge(InteractionDamage::presentation());
            return Ok(InteractionState::Idle);
        }
        if crossed {
            self.activate_armed(armed, pending.pointer, update)
        } else {
            Ok(InteractionState::Armed(armed))
        }
    }

    /// Restarts one still-current paged query after its opaque view cursor becomes stale.
    ///
    /// The session retains the normalized request facts and allocates a new query identity before
    /// emitting a first-page query. Runtime invokes this only for a restartable stale-cursor error
    /// reported by its bound view. Late errors for superseded query identities are ignored.
    fn restart_stale_query(
        &mut self,
        id: InteractionQueryId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        self.ensure_interaction_session(id.session())?;
        self.restart_stale_query_with_connection_lock(id, false)
    }

    /// Restarts one connection probe while applying invalidation-specific target-lock policy.
    ///
    /// Viewport and paint-order changes may retain the same target lifetime. Graph or published
    /// port-geometry changes clear it before the replacement first page is issued, forcing every
    /// target to satisfy the acquire radius again.
    fn restart_stale_connection_query(
        &mut self,
        id: InteractionQueryId,
        reason: ConnectionProbeCursorStaleReason,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        self.ensure_interaction_session(id.session())?;
        if !reason.is_restartable() {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        let preserve_lock = matches!(
            reason,
            ConnectionProbeCursorStaleReason::ViewportTransform
                | ConnectionProbeCursorStaleReason::PaintOrder
        );
        self.restart_stale_query_with_connection_lock(id, preserve_lock)
    }

    fn restart_stale_query_with_connection_lock(
        &mut self,
        id: InteractionQueryId,
        preserve_connection_lock: bool,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        if matches!(
            &self.state,
            InteractionState::EdgePickPending(pending) if pending.query == id
        ) {
            let state = std::mem::take(&mut self.state);
            let InteractionState::EdgePickPending(mut pending) = state else {
                unreachable!("the exact edge-pick state was matched immediately before mutation");
            };
            if !reserve_stale_query_restart(&mut pending.stale_query_restarts) {
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::BudgetExhausted);
                return Ok(self.finalize_update(
                    CanvasInteractionUpdate::default().with_damage(edge_pick_terminal_damage()),
                ));
            }
            let Some(next) = self.allocate_query() else {
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
                return Ok(self.finalize_update(
                    CanvasInteractionUpdate::default().with_damage(edge_pick_terminal_damage()),
                ));
            };
            pending.query = next;
            let update = CanvasInteractionUpdate::default();
            self.query_work.replace(PendingInteractionQuery::edge_pick(
                next,
                EdgePickQuery::new(
                    pending.press.view(),
                    self.config.edge_pick_radius_in_view,
                    self.config.query_entry_budget,
                )
                .bind_interaction(self.interaction_query_binding(next)),
            ));
            self.state = InteractionState::EdgePickPending(pending);
            return Ok(self.finalize_update(update));
        }

        if self
            .pending_edge_pick
            .is_some_and(|pending| pending.id == id)
        {
            let pending = self
                .pending_edge_pick
                .take()
                .expect("the matching edge-pick request remains pending");
            let mut stale_query_restarts = pending.stale_query_restarts;
            if !reserve_stale_query_restart(&mut stale_query_restarts) {
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::BudgetExhausted);
                return Ok(self.finalize_update(
                    CanvasInteractionUpdate::default().with_damage(edge_pick_terminal_damage()),
                ));
            }
            let Some(next) = self.allocate_query() else {
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
                return Ok(
                    self.finalize_update(CanvasInteractionUpdate::default().with_damage(
                        InteractionDamage {
                            presentation: true,
                            edges: true,
                            ..InteractionDamage::default()
                        },
                    )),
                );
            };
            self.pending_edge_pick = Some(PendingHoverEdgePick {
                id: next,
                position: pending.position,
                stale_query_restarts,
            });
            let update = CanvasInteractionUpdate::default();
            self.query_work.replace(PendingInteractionQuery::edge_pick(
                next,
                EdgePickQuery::new(
                    pending.position.view(),
                    self.config.edge_pick_radius_in_view,
                    self.config.query_entry_budget,
                )
                .bind_interaction(self.interaction_query_binding(next)),
            ));
            return Ok(self.finalize_update(update));
        }

        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        self.state = match state {
            InteractionState::NodeDrag(mut drag) if drag.pending_alignment_query == Some(id) => {
                drag.pending_alignment_query = None;
                if !reserve_stale_query_restart(&mut drag.stale_query_restarts) {
                    drag.delta = drag.raw_delta;
                    drag.guides = Arc::default();
                    drag.alignment_locks = AlignmentSnapLocks::default();
                    update.damage.merge(InteractionDamage::node_drag());
                    if drag.release_pending {
                        self.commit_node_drag(drag, &mut update)
                    } else {
                        InteractionState::NodeDrag(drag)
                    }
                } else if self.queue_alignment_snap(&mut drag, &mut update) {
                    InteractionState::NodeDrag(drag)
                } else if drag.release_pending {
                    self.commit_node_drag(drag, &mut update)
                } else {
                    InteractionState::NodeDrag(drag)
                }
            }
            InteractionState::Marquee(mut marquee) if marquee.pending_query == Some(id) => {
                if !reserve_stale_query_restart(&mut marquee.stale_query_restarts) {
                    self.terminal = Some(InteractionTerminalReason::BudgetExhausted);
                    update.damage.merge(InteractionDamage::presentation());
                    InteractionState::Idle
                } else {
                    marquee.pending_query = None;
                    if self.queue_marquee_query(&mut marquee, &mut update) {
                        InteractionState::Marquee(marquee)
                    } else {
                        update.damage.merge(InteractionDamage::presentation());
                        InteractionState::Idle
                    }
                }
            }
            InteractionState::Connection(ConnectionInteraction::Dragging(mut drag))
                if drag.pending_query == Some(id) =>
            {
                if !reserve_stale_query_restart(&mut drag.stale_query_restarts) {
                    self.terminal = Some(InteractionTerminalReason::BudgetExhausted);
                    update.damage.merge(InteractionDamage::connection());
                    InteractionState::Idle
                } else {
                    drag.pending_query = None;
                    if !preserve_connection_lock {
                        drag.target_lock = None;
                        drag.probe = ConnectionProbe::None;
                    }
                    if !self.queue_connection_probe(&mut drag, &mut update) {
                        update.damage.merge(InteractionDamage::connection());
                        InteractionState::Idle
                    } else {
                        InteractionState::Connection(ConnectionInteraction::Dragging(drag))
                    }
                }
            }
            InteractionState::Committing(CommitInteraction::Connection(mut commit)) if matches!(commit.freshness, ConnectionCommitFreshness::Probing(pending) if pending == id) => {
                if !reserve_stale_query_restart(&mut commit.stale_query_restarts) {
                    update.damage.merge(InteractionDamage::connection());
                    self.reject_connection_commit(
                        commit,
                        InteractionTerminalReason::BudgetExhausted,
                    )
                } else {
                    if !preserve_connection_lock {
                        commit.probe = ConnectionProbe::None;
                    }
                    let Some(next) = self.allocate_query() else {
                        update.damage.merge(InteractionDamage::connection());
                        self.state = self.reject_connection_commit(
                            commit,
                            InteractionTerminalReason::IdentityExhausted,
                        );
                        return Ok(self.finalize_update(update));
                    };
                    commit.freshness = ConnectionCommitFreshness::Probing(next);
                    self.query_work
                        .replace(PendingInteractionQuery::connection_probe(
                            next,
                            commit.generation,
                            ConnectionProbeQuery::new(
                                commit.start.binding.clone(),
                                commit.pointer.view(),
                                connection_probe_targeting(
                                    self.config,
                                    commit.probe.target().map(|target| target.identity()),
                                ),
                                self.config.query_entry_budget,
                            )
                            .bind_interaction(self.interaction_query_binding(next)),
                        ));
                    InteractionState::Committing(CommitInteraction::Connection(commit))
                }
            }
            other => other,
        };
        Ok(self.finalize_update(update))
    }

    pub fn move_to(
        &mut self,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let (position, current) = self.normalize_pointer_sample(sample)?;
        if let Some(update) = self.move_pan_if_active(position, current)? {
            return Ok(update);
        }
        self.rebase_interaction_positions(current)?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        let mut selection_rollback = None;
        self.state = match state {
            InteractionState::Armed(mut armed) => {
                armed.max_view_distance_squared = armed
                    .max_view_distance_squared
                    .max(view_distance_squared(armed.start.view(), position.view()));
                if !threshold_crossed_distance(
                    armed.max_view_distance_squared,
                    self.threshold_for(&armed.target),
                ) {
                    InteractionState::Armed(armed)
                } else {
                    selection_rollback = Some(InteractionState::Armed(armed.clone()));
                    self.activate_armed(armed, position, &mut update)?
                }
            }
            InteractionState::NodeDrag(mut drag) if !drag.release_pending => {
                drag.pointer = position;
                drag.release_pending = false;
                drag.raw_delta = snapped_delta(
                    drag.start.canvas(),
                    position.canvas(),
                    drag.snap_origin,
                    self.config.grid_size_in_canvas,
                );
                drag.delta = drag.raw_delta;
                drag.guides = Arc::default();
                self.start_alignment_snap(&mut drag, &mut update);
                update.damage.merge(InteractionDamage::node_drag());
                InteractionState::NodeDrag(drag)
            }
            InteractionState::Pan(mut pan) => {
                pan.pointer = position;
                InteractionState::Pan(pan)
            }
            InteractionState::Marquee(mut marquee) => {
                marquee.pointer = position;
                marquee.release_pending = false;
                update.damage.merge(InteractionDamage::presentation());
                if self.start_marquee_query(&mut marquee, &mut update) {
                    InteractionState::Marquee(marquee)
                } else {
                    InteractionState::Idle
                }
            }
            InteractionState::Connection(ConnectionInteraction::Dragging(mut drag)) => {
                drag.pointer = position;
                drag.release_pending = false;
                update.damage.merge(InteractionDamage::connection());
                if self.start_connection_probe(&mut drag, &mut update) {
                    InteractionState::Connection(ConnectionInteraction::Dragging(drag))
                } else {
                    InteractionState::Idle
                }
            }
            InteractionState::EdgePickPending(mut pending) => {
                pending.pointer = position;
                pending.max_view_distance_squared = pending
                    .max_view_distance_squared
                    .max(view_distance_squared(pending.press.view(), position.view()));
                InteractionState::EdgePickPending(pending)
            }
            other => other,
        };
        self.commit_selection_transition(&mut update, selection_rollback)?;
        Ok(self.finalize_update(update))
    }

    pub fn release(
        &mut self,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let (position, current) = self.normalize_pointer_sample(sample)?;
        if let Some(mut update) = self.move_pan_if_active(position, current)? {
            self.state = InteractionState::Idle;
            update.damage.merge(InteractionDamage::presentation());
            return Ok(self.finalize_update(update));
        }
        self.rebase_interaction_positions(current)?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        let mut selection_rollback = None;
        self.state = match state {
            InteractionState::Armed(mut armed) => {
                selection_rollback = Some(InteractionState::Armed(armed.clone()));
                armed.max_view_distance_squared = armed
                    .max_view_distance_squared
                    .max(view_distance_squared(armed.start.view(), position.view()));
                if threshold_crossed_distance(
                    armed.max_view_distance_squared,
                    self.threshold_for(&armed.target),
                ) {
                    let activated = self.activate_armed(armed, position, &mut update)?;
                    self.finish_activated_release(activated, &mut update)
                } else {
                    self.finish_armed_click(&armed, &mut update);
                    update.damage.merge(InteractionDamage::presentation());
                    InteractionState::Idle
                }
            }
            InteractionState::NodeDrag(mut drag) => {
                drag.pointer = position;
                drag.release_pending = true;
                drag.raw_delta = snapped_delta(
                    drag.start.canvas(),
                    position.canvas(),
                    drag.snap_origin,
                    self.config.grid_size_in_canvas,
                );
                drag.delta = drag.raw_delta;
                drag.guides = Arc::default();
                update.damage.merge(InteractionDamage::node_drag());
                if self.start_alignment_snap(&mut drag, &mut update) {
                    InteractionState::NodeDrag(drag)
                } else {
                    self.commit_node_drag(drag, &mut update)
                }
            }
            InteractionState::Connection(ConnectionInteraction::Dragging(mut drag)) => {
                drag.pointer = position;
                drag.release_pending = true;
                update.damage.merge(InteractionDamage::connection());
                if self.start_connection_probe(&mut drag, &mut update) {
                    InteractionState::Connection(ConnectionInteraction::Dragging(drag))
                } else {
                    InteractionState::Idle
                }
            }
            InteractionState::EdgePickPending(mut pending) => {
                pending.pointer = position;
                pending.max_view_distance_squared = pending
                    .max_view_distance_squared
                    .max(view_distance_squared(pending.press.view(), position.view()));
                pending.release_pending = true;
                InteractionState::EdgePickPending(pending)
            }
            InteractionState::Marquee(mut marquee) => {
                marquee.pointer = position;
                marquee.release_pending = true;
                update.damage.merge(InteractionDamage::presentation());
                if self.start_marquee_query(&mut marquee, &mut update) {
                    InteractionState::Marquee(marquee)
                } else {
                    InteractionState::Idle
                }
            }
            InteractionState::Pan(_) | InteractionState::EdgeActive(_) => {
                update.damage.merge(InteractionDamage::presentation());
                InteractionState::Idle
            }
            other => other,
        };
        self.commit_selection_transition(&mut update, selection_rollback)?;
        Ok(self.finalize_update(update))
    }

    fn resolve_connection_probe_outcome(
        &mut self,
        id: InteractionQueryId,
        outcome: ConnectionProbeQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        self.ensure_interaction_session(id.session())?;
        let view = self.authoritative_view()?;
        let Some(receipt) = outcome.receipt() else {
            return Err(CanvasInteractionError::MissingInteractionQueryReceipt { query: id });
        };
        if receipt.query_id() != id {
            return Err(CanvasInteractionError::MismatchedInteractionQueryReceipt {
                expected: id,
                actual: receipt.query_id(),
            });
        }
        if !receipt.authenticates(self.interaction_query_binding(id)) {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        match view.connection_probe_receipt_stale_reason(receipt) {
            Ok(None) => self.resolve_connection_probe_current(id, outcome),
            Ok(Some(reason)) if reason.is_restartable() => {
                self.restart_stale_connection_query(id, reason)
            }
            Ok(Some(ConnectionProbeCursorStaleReason::ForeignView)) => {
                Err(CanvasInteractionError::ForeignInteractionFacts)
            }
            Ok(Some(ConnectionProbeCursorStaleReason::ViewGeneration)) => {
                Err(CanvasInteractionError::StaleInteractionFacts)
            }
            Ok(Some(_)) => Err(CanvasInteractionError::StaleInteractionFacts),
            Err(ViewSessionError::ForeignInteractionFacts) => {
                Err(CanvasInteractionError::ForeignInteractionFacts)
            }
            Err(_) => Err(CanvasInteractionError::ViewUnavailable),
        }
    }

    fn resolve_connection_probe_current(
        &mut self,
        id: InteractionQueryId,
        outcome: ConnectionProbeQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        self.state = match state {
            InteractionState::Connection(ConnectionInteraction::Dragging(mut drag))
                if drag.pending_query == Some(id) =>
            {
                match outcome {
                    ConnectionProbeQueryOutcome::Incomplete { cursor, .. } => {
                        self.query_work
                            .replace(PendingInteractionQuery::connection_probe(
                                id,
                                drag.generation,
                                ConnectionProbeQuery::resume(
                                    cursor,
                                    self.config.query_entry_budget,
                                )
                                .bind_interaction(self.interaction_query_binding(id)),
                            ));
                        InteractionState::Connection(ConnectionInteraction::Dragging(drag))
                    }
                    ConnectionProbeQueryOutcome::BudgetExhausted { .. } => {
                        self.terminal = Some(InteractionTerminalReason::BudgetExhausted);
                        update.damage.merge(InteractionDamage::connection());
                        InteractionState::Idle
                    }
                    ConnectionProbeQueryOutcome::Complete { probe, .. } => {
                        drag.pending_query = None;
                        drag.target_lock = probe.target().map(|target| target.identity());
                        drag.probe = probe;
                        update.damage.merge(InteractionDamage::connection());
                        if drag.release_pending {
                            self.finish_connection_release(drag, &mut update)
                        } else {
                            InteractionState::Connection(ConnectionInteraction::Dragging(drag))
                        }
                    }
                }
            }
            InteractionState::Committing(CommitInteraction::Connection(mut commit)) if matches!(commit.freshness, ConnectionCommitFreshness::Probing(pending) if pending == id) => {
                match outcome {
                    ConnectionProbeQueryOutcome::Incomplete { cursor, .. } => {
                        self.query_work
                            .replace(PendingInteractionQuery::connection_probe(
                                id,
                                commit.generation,
                                ConnectionProbeQuery::resume(
                                    cursor,
                                    self.config.query_entry_budget,
                                )
                                .bind_interaction(self.interaction_query_binding(id)),
                            ));
                        InteractionState::Committing(CommitInteraction::Connection(commit))
                    }
                    ConnectionProbeQueryOutcome::BudgetExhausted { .. } => {
                        update.damage.merge(InteractionDamage::connection());
                        self.reject_connection_commit(
                            commit,
                            InteractionTerminalReason::BudgetExhausted,
                        )
                    }
                    ConnectionProbeQueryOutcome::Complete { probe, .. } => {
                        update.damage.merge(InteractionDamage::connection());
                        match probe {
                            valid @ ConnectionProbe::Valid { version, target } => {
                                commit.probe = valid;
                                commit.target =
                                    ConnectionCommitTarget::existing(target.identity().instance());
                                commit.freshness = ConnectionCommitFreshness::Revalidated;
                                self.push_graph_mutation(
                                    &mut update,
                                    commit.ticket,
                                    FreshGraphMutation::connection(
                                        commit.start.binding.clone(),
                                        commit.target.clone(),
                                        version,
                                    ),
                                );
                                InteractionState::Committing(CommitInteraction::Connection(commit))
                            }
                            invalid @ (ConnectionProbe::None | ConnectionProbe::Invalid { .. }) => {
                                commit.probe = invalid;
                                self.reject_connection_commit(
                                    commit,
                                    InteractionTerminalReason::Conflict,
                                )
                            }
                        }
                    }
                }
            }
            other => other,
        };
        Ok(self.finalize_update(update))
    }

    fn resolve_node_intersection_outcome(
        &mut self,
        id: InteractionQueryId,
        outcome: NodeIntersectionQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        self.ensure_interaction_session(id.session())?;
        let view = self.authoritative_view()?;
        let Some(receipt) = outcome.receipt() else {
            return Err(CanvasInteractionError::MissingInteractionQueryReceipt { query: id });
        };
        if receipt.query_id() != id {
            return Err(CanvasInteractionError::MismatchedInteractionQueryReceipt {
                expected: id,
                actual: receipt.query_id(),
            });
        }
        if !receipt.authenticates(self.interaction_query_binding(id)) {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        match view.node_intersection_receipt_is_current(receipt) {
            Ok(true) => {
                let snapshot = view
                    .graph_snapshot()
                    .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
                self.resolve_node_intersection_current(id, outcome, Some(&snapshot))
            }
            Ok(false) | Err(ViewSessionError::StaleInteractionFacts) => {
                self.restart_stale_query(id)
            }
            Err(ViewSessionError::ForeignInteractionFacts) => {
                Err(CanvasInteractionError::ForeignInteractionFacts)
            }
            Err(_) => Err(CanvasInteractionError::ViewUnavailable),
        }
    }

    fn resolve_node_intersection_current(
        &mut self,
        id: InteractionQueryId,
        outcome: NodeIntersectionQueryOutcome,
        snapshot: Option<&GraphSnapshot>,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        let mut selection_rollback = None;
        self.state = match state {
            InteractionState::Marquee(mut marquee) if marquee.pending_query == Some(id) => {
                selection_rollback = Some(InteractionState::Marquee(marquee.clone()));
                match outcome {
                    NodeIntersectionQueryOutcome::Incomplete { cursor, .. } => {
                        self.query_work
                            .replace(PendingInteractionQuery::node_intersection(
                                id,
                                marquee.generation,
                                NodeIntersectionQuery::resume(
                                    cursor,
                                    self.config.query_entry_budget,
                                )
                                .bind_interaction(self.interaction_query_binding(id)),
                            ));
                        InteractionState::Marquee(marquee)
                    }
                    NodeIntersectionQueryOutcome::BudgetExhausted { .. } => {
                        self.terminal = Some(InteractionTerminalReason::BudgetExhausted);
                        update.damage.merge(InteractionDamage::presentation());
                        InteractionState::Idle
                    }
                    NodeIntersectionQueryOutcome::Complete { nodes, .. } => {
                        marquee.pending_query = None;
                        let nodes = nodes
                            .into_iter()
                            .filter_map(|node| {
                                snapshot.map_or_else(
                                    || {
                                        #[cfg(test)]
                                        {
                                            Some(NodeLifetimeKey::unavailable(node))
                                        }
                                        #[cfg(not(test))]
                                        {
                                            None
                                        }
                                    },
                                    |snapshot| snapshot.node_lifetime_key(node),
                                )
                            })
                            .collect::<Arc<[_]>>();
                        update.pending_selection = Some(SelectionMutation::Marquee {
                            provenance: marquee.baseline.provenance(),
                            nodes,
                            mode: marquee.selection_mode,
                            baseline: marquee.baseline.clone(),
                        });
                        update.damage = InteractionDamage {
                            presentation: true,
                            selection: true,
                            ..InteractionDamage::default()
                        };
                        if marquee.release_pending {
                            InteractionState::Idle
                        } else {
                            InteractionState::Marquee(marquee)
                        }
                    }
                }
            }
            other => other,
        };
        self.commit_selection_transition(&mut update, selection_rollback)?;
        Ok(self.finalize_update(update))
    }

    pub fn search_ready(
        &mut self,
        request: &ConnectionSearchRequest,
        result_count: usize,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.set_search_status(
            request,
            if result_count == 0 {
                ConnectionSearchStatus::Empty
            } else {
                ConnectionSearchStatus::Ready { result_count }
            },
        )
    }

    pub fn search_failed(
        &mut self,
        request: &ConnectionSearchRequest,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.set_search_status(request, ConnectionSearchStatus::Error)
    }

    pub fn retry_search(
        &mut self,
        request: &ConnectionSearchRequest,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.validate_search_request(request)?;
        let session = self.id;
        let provenance = self.provenance;
        let Some(attempt) = self.allocate_search_attempt() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return Err(CanvasInteractionError::IdentityExhausted);
        };
        let search = match &mut self.state {
            InteractionState::Connection(ConnectionInteraction::Searching(search))
                if search_token_matches(&request.token, search) =>
            {
                search
            }
            InteractionState::Connection(ConnectionInteraction::Searching(_)) => {
                return Err(CanvasInteractionError::ConnectionSearchStaleGeneration);
            }
            _ => return Err(CanvasInteractionError::ConnectionSearchInactive),
        };
        let mut update = CanvasInteractionUpdate::default();
        search.attempt = attempt;
        search.status = ConnectionSearchStatus::Loading;
        update
            .effects
            .push(CanvasInteractionEffect::Search(search_request(
                session, provenance, search,
            )));
        update.damage.merge(InteractionDamage::connection());
        Ok(self.finalize_update(update))
    }

    pub fn select_search_result(
        &mut self,
        selection: ConnectionSearchSelection,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.validate_search_token(&selection.token)?;
        let ConnectionSearchSelection {
            token,
            validated_version,
            target,
        } = selection;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        self.state = match state {
            InteractionState::Connection(ConnectionInteraction::Searching(search))
                if search_token_matches(&token, &search) =>
            {
                self.begin_connection_commit(
                    search.generation,
                    search.start,
                    search.pointer,
                    ConnectionProbe::None,
                    target,
                    validated_version,
                    Some(token.attempt),
                    true,
                    &mut update,
                )
            }
            state @ InteractionState::Connection(ConnectionInteraction::Searching(_)) => {
                self.state = state;
                return Err(CanvasInteractionError::ConnectionSearchStaleGeneration);
            }
            other => {
                self.state = other;
                return Err(CanvasInteractionError::ConnectionSearchInactive);
            }
        };
        Ok(self.finalize_update(update))
    }

    /// Consumes one current submission at the transport-acceptance linearization point.
    ///
    /// Before this call succeeds, cancellation, close, ticket supersession, view retirement, and
    /// entity replacement revoke the submission. After it succeeds, the adapter owns the returned
    /// command and must enqueue or execute it immediately before reporting its completion.
    pub fn bind_submission(
        &mut self,
        submission: InteractionGraphSubmission,
    ) -> Result<Option<crate::runtime::commands::GraphCommand>, InteractionGraphSubmissionBindError>
    {
        if self.closed {
            return Err(InteractionGraphSubmissionBindError::Closed);
        }
        let ticket = submission.ticket;
        if ticket.session() != self.id {
            return Err(
                InteractionGraphSubmissionBindError::ForeignInteractionSession {
                    expected: self.id,
                    actual: ticket.session(),
                },
            );
        }
        let current = interaction_state_submission_ticket(&self.state);
        if current != Some(ticket) {
            return Err(InteractionGraphSubmissionBindError::Superseded { ticket, current });
        }
        if !matches!(
            self.submission_binding.as_ref(),
            Some(SubmissionBinding {
                ticket: bound_ticket,
                phase: SubmissionBindingPhase::Pending,
            }) if *bound_ticket == ticket
        ) {
            return Err(InteractionGraphSubmissionBindError::SubmissionAlreadyBound);
        }
        let actual = self
            .bound_view
            .provenance()
            .map_err(|_| InteractionGraphSubmissionBindError::ViewUnavailable)?;
        if actual != self.provenance {
            return Err(InteractionGraphSubmissionBindError::ForeignInteractionFacts);
        }
        let snapshot = self
            .bound_view
            .graph_snapshot()
            .map_err(|_| InteractionGraphSubmissionBindError::ViewUnavailable)?;
        let command = submission
            .mutation
            .into_command(&snapshot)
            .map_err(InteractionGraphSubmissionBindError::from)?;
        let phase = command
            .as_ref()
            .map_or(SubmissionBindingPhase::NoCommand, |command| {
                SubmissionBindingPhase::Command(command.identity().clone())
            });
        self.submission_binding = Some(SubmissionBinding { ticket, phase });
        Ok(command)
    }

    pub fn complete_submission(
        &mut self,
        ticket: InteractionTicketId,
        completion: CommandExecutionCompletion,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        if self.submission_callback_is_stale(ticket)? {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        match self.submission_binding.as_ref() {
            Some(SubmissionBinding {
                ticket: bound_ticket,
                phase: SubmissionBindingPhase::Command(expected),
            }) if *bound_ticket == ticket && expected == completion.identity() => {}
            Some(SubmissionBinding {
                ticket: bound_ticket,
                phase: SubmissionBindingPhase::Command(_),
            }) if *bound_ticket == ticket => {
                return Err(CanvasInteractionError::CommandCompletionIdentityMismatch);
            }
            Some(SubmissionBinding {
                ticket: bound_ticket,
                phase: SubmissionBindingPhase::Pending | SubmissionBindingPhase::NoCommand,
            }) if *bound_ticket == ticket => {
                return Err(CanvasInteractionError::SubmissionCompletionStateMismatch);
            }
            Some(_) | None => return Err(CanvasInteractionError::SubmissionNotBound),
        }
        self.submission_binding = None;
        let kind = completion.kind();
        self.complete_submission_with_terminal(ticket, completion_terminal(kind))
    }

    pub fn complete_submission_without_command(
        &mut self,
        ticket: InteractionTicketId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        if self.submission_callback_is_stale(ticket)? {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        if !matches!(
            self.submission_binding.as_ref(),
            Some(SubmissionBinding {
                ticket: bound_ticket,
                phase: SubmissionBindingPhase::NoCommand,
            }) if *bound_ticket == ticket
        ) {
            return Err(CanvasInteractionError::SubmissionCompletionStateMismatch);
        }
        self.submission_binding = None;
        self.complete_submission_with_terminal(ticket, InteractionTerminalReason::Noop)
    }

    pub fn submission_not_dispatched(
        &mut self,
        ticket: InteractionTicketId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        if self.submission_callback_is_stale(ticket)? {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        if !matches!(
            self.submission_binding.as_ref(),
            Some(SubmissionBinding {
                ticket: bound_ticket,
                phase: SubmissionBindingPhase::Pending | SubmissionBindingPhase::Command(_),
            }) if *bound_ticket == ticket
        ) {
            return Err(CanvasInteractionError::SubmissionCompletionStateMismatch);
        }
        self.submission_binding = None;
        self.complete_submission_with_terminal(ticket, InteractionTerminalReason::TransportFailure)
    }

    pub fn submission_outcome_unknown(
        &mut self,
        ticket: InteractionTicketId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        if self.submission_callback_is_stale(ticket)? {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        if !matches!(
            self.submission_binding.as_ref(),
            Some(SubmissionBinding {
                ticket: bound_ticket,
                phase: SubmissionBindingPhase::Command(_),
            }) if *bound_ticket == ticket
        ) {
            return Err(CanvasInteractionError::SubmissionCompletionStateMismatch);
        }
        self.submission_binding = None;
        self.complete_submission_with_terminal(ticket, InteractionTerminalReason::Uncertain)
    }

    fn submission_callback_is_stale(
        &self,
        ticket: InteractionTicketId,
    ) -> Result<bool, CanvasInteractionError> {
        Ok(self.callback_authority(
            ticket.session(),
            None,
            interaction_state_submission_ticket(&self.state) == Some(ticket),
        )? == CallbackAuthority::Stale)
    }

    fn complete_submission_with_terminal(
        &mut self,
        ticket: InteractionTicketId,
        terminal: InteractionTerminalReason,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        self.state = match state {
            InteractionState::Committing(CommitInteraction::Connection(commit))
                if commit.ticket == ticket =>
            {
                update.damage.merge(InteractionDamage::connection());
                self.finish_connection_commit(commit, terminal)
            }
            InteractionState::Committing(CommitInteraction::KeyboardConnection(commit))
                if commit.ticket == ticket =>
            {
                self.terminal = Some(terminal);
                update.damage.merge(InteractionDamage::connection());
                InteractionState::Idle
            }
            InteractionState::Committing(CommitInteraction::NodeMove(commit))
                if commit.ticket == ticket =>
            {
                self.terminal = Some(terminal);
                update.damage.merge(InteractionDamage::node_drag());
                InteractionState::Idle
            }
            InteractionState::Committing(CommitInteraction::EdgeDelete(commit))
                if commit.ticket == ticket =>
            {
                self.terminal = Some(terminal);
                update.damage.merge(edge_action_damage());
                InteractionState::Idle
            }
            InteractionState::Committing(CommitInteraction::SelectionDelete(commit))
                if commit.ticket == ticket =>
            {
                self.terminal = Some(terminal);
                update.damage.merge(selection_delete_damage());
                InteractionState::Idle
            }
            other => other,
        };
        Ok(self.finalize_update(update))
    }

    /// Resumes an opaque graph submission after an adapter-owned command prefix settles.
    ///
    /// Direct mutations are reconciled and re-emitted unchanged. Connection mutations are
    /// consumed and revalidated by the session before a replacement submission is emitted.
    /// Adapters neither inspect the mutation nor choose the continuation protocol.
    pub fn resume_submission_after_prefix(
        &mut self,
        submission: InteractionGraphSubmission,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let ticket = submission.ticket();
        if self.callback_authority(
            ticket.session(),
            None,
            interaction_state_submission_ticket(&self.state) == Some(ticket),
        )? == CallbackAuthority::Stale
        {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        if submission.requires_connection_revalidation() {
            return self.resume_connection_commit_after_prefix(ticket);
        }

        let mut update = self.reconcile_graph()?;
        if interaction_state_submission_ticket(&self.state) == Some(ticket) {
            self.push_graph_submission(&mut update, submission);
        }
        Ok(update)
    }

    fn resume_connection_commit_after_prefix(
        &mut self,
        ticket: InteractionTicketId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let view = self.authoritative_view()?;
        let is_current_connection_commit = matches!(
            &self.state,
            InteractionState::Committing(CommitInteraction::Connection(commit))
                if commit.ticket == ticket
        ) || matches!(
            &self.state,
            InteractionState::Committing(CommitInteraction::KeyboardConnection(commit))
                if commit.ticket == ticket
        );
        if !is_current_connection_commit {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        let mut update = self.reconcile_graph()?;
        let keyboard_connection = matches!(
            &self.state,
            InteractionState::Committing(CommitInteraction::KeyboardConnection(commit))
                if commit.ticket == ticket
        );
        let create_from_wire = matches!(
            &self.state,
            InteractionState::Committing(CommitInteraction::Connection(commit))
                if commit.ticket == ticket && commit.target.is_create_from_wire()
        );
        let resumed = if keyboard_connection {
            self.refresh_keyboard_connection_commit(ticket)?
        } else if create_from_wire {
            let snapshot = view
                .graph_snapshot()
                .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
            self.refresh_create_from_wire_commit_from_snapshot(ticket, &snapshot)?
        } else {
            self.request_connection_commit_revalidation(ticket)?
        };
        update.merge_internal(resumed);
        Ok(self.finalize_update(update))
    }

    fn refresh_keyboard_connection_commit(
        &mut self,
        ticket: InteractionTicketId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let (start, target) = match &self.state {
            InteractionState::Committing(CommitInteraction::KeyboardConnection(commit))
                if commit.ticket == ticket =>
            {
                (commit.start.clone(), commit.target)
            }
            _ => return Ok(self.finalize_update(CanvasInteractionUpdate::default())),
        };
        let view = self.authoritative_view()?;
        let snapshot = view
            .graph_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        if !snapshot.port_instance_is_current(target.instance()) {
            self.state = InteractionState::Idle;
            self.terminal = Some(InteractionTerminalReason::OwnerRetired);
            return Ok(self.finalize_update(
                CanvasInteractionUpdate::default().with_damage(InteractionDamage::connection()),
            ));
        }
        view.validate_connection_binding(&start.binding)
            .map_err(map_connection_binding_error)?;
        let evaluation = view
            .evaluate_connection(start.binding.edit_for_target(target.port()))
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        if !evaluation.is_accept() {
            self.state = InteractionState::Idle;
            self.terminal = Some(InteractionTerminalReason::Rejected);
            return Ok(self.finalize_update(
                CanvasInteractionUpdate::default().with_damage(InteractionDamage::connection()),
            ));
        }
        let mut update =
            CanvasInteractionUpdate::default().with_damage(InteractionDamage::connection());
        self.push_graph_mutation(
            &mut update,
            ticket,
            FreshGraphMutation::connection(
                start.binding,
                ConnectionCommitTarget::existing(target.instance()),
                evaluation.version(),
            ),
        );
        Ok(self.finalize_update(update))
    }

    /// Starts the fresh target probe for an ordinary connection held behind a command prefix.
    ///
    /// The session owns the query identity and resumes incomplete work through
    /// [`Self::resolve_connection_probe`]. A matching create-from-wire commit is rejected because
    /// its prospective target has no published view geometry yet. Stale tickets are ignored.
    fn request_connection_commit_revalidation(
        &mut self,
        ticket: InteractionTicketId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        let mut error = None;
        self.state = match state {
            InteractionState::Committing(CommitInteraction::Connection(mut commit))
                if commit.ticket == ticket =>
            {
                if !commit.target.is_existing() {
                    error = Some(CanvasInteractionError::ConnectionCommitRequiresFreshRevision);
                    InteractionState::Committing(CommitInteraction::Connection(commit))
                } else if !matches!(commit.freshness, ConnectionCommitFreshness::Initial) {
                    InteractionState::Committing(CommitInteraction::Connection(commit))
                } else if let Some(id) = self.allocate_query() {
                    commit.stale_query_restarts = 0;
                    commit.freshness = ConnectionCommitFreshness::Probing(id);
                    self.query_work
                        .replace(PendingInteractionQuery::connection_probe(
                            id,
                            commit.generation,
                            ConnectionProbeQuery::new(
                                commit.start.binding.clone(),
                                commit.pointer.view(),
                                connection_probe_targeting(
                                    self.config,
                                    commit.probe.target().map(|target| target.identity()),
                                ),
                                self.config.query_entry_budget,
                            )
                            .bind_interaction(self.interaction_query_binding(id)),
                        ));
                    InteractionState::Committing(CommitInteraction::Connection(commit))
                } else {
                    update.damage.merge(InteractionDamage::connection());
                    self.reject_connection_commit(
                        commit,
                        InteractionTerminalReason::IdentityExhausted,
                    )
                }
            }
            other => other,
        };
        if let Some(error) = error {
            return Err(error);
        }
        Ok(self.finalize_update(update))
    }

    fn refresh_create_from_wire_commit_from_snapshot(
        &mut self,
        ticket: InteractionTicketId,
        snapshot: &GraphSnapshot,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        let mut error = None;
        self.state = match state {
            InteractionState::Committing(CommitInteraction::Connection(mut commit))
                if commit.ticket == ticket =>
            {
                if commit.target.is_existing() {
                    error = Some(CanvasInteractionError::ConnectionCommitRequiresProbe);
                } else if matches!(commit.freshness, ConnectionCommitFreshness::Initial) {
                    if !commit.target.is_current(snapshot) {
                        error = Some(
                            CanvasInteractionError::ConnectionCommitTargetNoLongerProspective {
                                target: commit.target.port(),
                            },
                        );
                    } else {
                        commit.freshness = ConnectionCommitFreshness::Revalidated;
                        self.push_graph_mutation(
                            &mut update,
                            ticket,
                            FreshGraphMutation::connection(
                                commit.start.binding.clone(),
                                commit.target.clone(),
                                snapshot.version(),
                            ),
                        );
                    }
                }
                InteractionState::Committing(CommitInteraction::Connection(commit))
            }
            other => other,
        };
        if let Some(error) = error {
            return Err(error);
        }
        Ok(self.finalize_update(update))
    }

    fn resolve_alignment_snap_outcome(
        &mut self,
        id: InteractionQueryId,
        outcome: AlignmentSnapQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        self.ensure_interaction_session(id.session())?;
        let view = self.authoritative_view()?;
        let Some(receipt) = outcome.receipt() else {
            return Err(CanvasInteractionError::MissingInteractionQueryReceipt { query: id });
        };
        if receipt.query_id() != id {
            return Err(CanvasInteractionError::MismatchedInteractionQueryReceipt {
                expected: id,
                actual: receipt.query_id(),
            });
        }
        if !receipt.authenticates(self.interaction_query_binding(id)) {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        match view.alignment_snap_receipt_is_current(receipt) {
            Ok(true) => self.resolve_alignment_snap_current(id, outcome),
            Ok(false) | Err(ViewSessionError::StaleInteractionFacts) => {
                self.restart_stale_query(id)
            }
            Err(ViewSessionError::ForeignInteractionFacts) => {
                Err(CanvasInteractionError::ForeignInteractionFacts)
            }
            Err(_) => Err(CanvasInteractionError::ViewUnavailable),
        }
    }

    fn resolve_alignment_snap_current(
        &mut self,
        id: InteractionQueryId,
        outcome: AlignmentSnapQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        self.state = match state {
            InteractionState::NodeDrag(mut drag)
                if drag.pending_alignment_query == Some(id)
                    && outcome.generation() == drag.generation =>
            {
                drag.pending_alignment_query = None;
                match outcome {
                    AlignmentSnapQueryOutcome::Complete(result) => {
                        let (_, delta, guides, locks) = result.into_parts();
                        drag.delta = delta;
                        drag.guides = Arc::from(guides);
                        drag.alignment_locks = locks;
                    }
                    AlignmentSnapQueryOutcome::BudgetExhausted(_)
                    | AlignmentSnapQueryOutcome::GeometryIndeterminate(_) => {
                        drag.delta = drag.raw_delta;
                        drag.guides = Arc::default();
                        drag.alignment_locks = AlignmentSnapLocks::default();
                    }
                }
                update.damage.merge(InteractionDamage::node_drag());
                if drag.release_pending {
                    self.commit_node_drag(drag, &mut update)
                } else {
                    InteractionState::NodeDrag(drag)
                }
            }
            other => other,
        };
        Ok(self.finalize_update(update))
    }

    pub fn tick(
        &mut self,
        elapsed_seconds: f32,
        viewport: ViewSize,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 || !viewport.is_positive_finite()
        {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        let current = self.current_viewport_snapshot()?;
        if current.provenance() != self.provenance {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        self.validate_interaction_reprojection(current)?;
        let rebase_required =
            interaction_positions_need_rebase(&self.state, self.pending_edge_pick, current);
        let pointer = active_pointer(&self.state).map(|pointer| {
            pointer
                .reproject(current)
                .expect("the active pointer was validated against this viewport")
        });
        let mut update = CanvasInteractionUpdate::default();
        if let Some(pointer) = pointer {
            let elapsed_seconds = elapsed_seconds.min(MAX_AUTO_PAN_ELAPSED_SECONDS);
            let delta = auto_pan_delta(
                pointer.view(),
                viewport,
                self.config.auto_pan_band_in_view,
                self.config.auto_pan_speed_in_view_per_second,
                elapsed_seconds,
            );
            if delta.x != 0.0 || delta.y != 0.0 {
                let preview = ViewportIntent::pan(ViewportPanRequest::new(delta))
                    .resolve(current.transform())
                    .ok_or(CanvasInteractionError::InvalidPosition)?;
                let preview = ViewportSnapshot::new(self.provenance, current.revision(), preview);
                pointer
                    .reproject(preview)
                    .ok_or(CanvasInteractionError::InvalidPosition)?;

                let (snapshot, panic) = self.commit_pan(delta)?;
                // External drift rebases the complete gesture first. The session's own pan then
                // advances only the live pointer so drag anchors stay fixed in canvas space.
                self.rebase_interaction_positions(current)?;
                if snapshot.revision() != current.revision() {
                    update.damage.viewport = true;
                }
                self.reproject_active_pointer(snapshot, &mut update)?;
                let update = self.finalize_update(update);
                resume(panic);
                return Ok(update);
            }
        }
        if rebase_required {
            self.rebase_interaction_positions(current)?;
            self.reproject_active_pointer(current, &mut update)?;
        }
        Ok(self.finalize_update(update))
    }

    fn reproject_active_pointer(
        &mut self,
        snapshot: ViewportSnapshot,
        update: &mut CanvasInteractionUpdate,
    ) -> Result<(), CanvasInteractionError> {
        // A valid transform can still overflow during inverse projection. Validate it before
        // taking ownership of the active state so an error leaves the interaction untouched.
        let Some(pointer) = active_pointer(&self.state) else {
            return Ok(());
        };
        let pointer = pointer
            .reproject(snapshot)
            .ok_or(CanvasInteractionError::InvalidPosition)?;
        let state = std::mem::take(&mut self.state);
        self.state = match state {
            InteractionState::NodeDrag(mut drag) => {
                drag.pointer = pointer;
                drag.raw_delta = snapped_delta(
                    drag.start.canvas(),
                    drag.pointer.canvas(),
                    drag.snap_origin,
                    self.config.grid_size_in_canvas,
                );
                drag.delta = drag.raw_delta;
                drag.guides = Arc::default();
                self.start_alignment_snap(&mut drag, update);
                update.damage.merge(InteractionDamage::node_drag());
                InteractionState::NodeDrag(drag)
            }
            InteractionState::Connection(ConnectionInteraction::Dragging(mut drag))
                if !drag.release_pending =>
            {
                drag.pointer = pointer;
                update.damage.merge(InteractionDamage::connection());
                if self.start_connection_probe(&mut drag, update) {
                    InteractionState::Connection(ConnectionInteraction::Dragging(drag))
                } else {
                    InteractionState::Idle
                }
            }
            InteractionState::Marquee(mut marquee) if !marquee.release_pending => {
                marquee.pointer = pointer;
                update.damage.merge(InteractionDamage::presentation());
                if self.start_marquee_query(&mut marquee, update) {
                    InteractionState::Marquee(marquee)
                } else {
                    InteractionState::Idle
                }
            }
            other => other,
        };
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.cancel_with_selection_failure()
            .map_err(|failure| failure.error)
    }

    fn cancel_with_selection_failure(
        &mut self,
    ) -> Result<CanvasInteractionUpdate, SelectionCommitFailure> {
        if matches!(self.state, InteractionState::Idle) {
            self.clear_pending_edge_pick_work();
            self.query_work.clear();
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        let baseline = match &self.state {
            InteractionState::Marquee(marquee) => Some(marquee.baseline.clone()),
            _ => None,
        };
        let mut damage = damage_for_state(&self.state);
        let mut update = CanvasInteractionUpdate::default();
        if let Some(baseline) = baseline {
            update.pending_selection = Some(SelectionMutation::Restore(baseline));
            damage.selection = true;
        }
        let mut update = update.with_damage(damage);
        self.commit_update_selection_classified(&mut update)?;
        self.clear_pending_edge_pick_work();
        self.query_work.clear();
        self.submission_binding = None;
        self.state = InteractionState::Idle;
        self.terminal = Some(InteractionTerminalReason::Cancelled);
        Ok(self.finalize_update(update))
    }

    pub fn owner_retired(&mut self, owner: InteractionOwner) -> CanvasInteractionUpdate {
        let mut update = CanvasInteractionUpdate::default();
        if let InteractionOwner::Edge(edge) = owner
            && self.hovered_edge == Some(edge)
        {
            self.hovered_edge = None;
            update.damage = InteractionDamage {
                presentation: true,
                edges: true,
                ..InteractionDamage::default()
            };
        }
        if let InteractionOwner::Port(port) = owner
            && let InteractionState::Connection(ConnectionInteraction::Dragging(drag)) =
                &mut self.state
            && drag
                .target_lock
                .is_some_and(|target| target.instance() == port)
        {
            drag.target_lock = None;
            drag.probe = ConnectionProbe::None;
            update.damage.merge(InteractionDamage::connection());
        }
        if !state_uses_owner(&self.state, owner) {
            return self.finalize_update(update);
        }
        let damage = damage_for_state(&self.state);
        self.query_work.clear();
        self.submission_binding = None;
        self.state = InteractionState::Idle;
        self.terminal = Some(InteractionTerminalReason::OwnerRetired);
        self.finalize_update(update.with_damage(damage))
    }

    /// Reports whether the active semantic interaction still references an owner.
    ///
    /// Native adapters may keep pointer capture in a renderer-specific coordinator while
    /// the renderer-neutral session owns the interaction. This query lets those adapters
    /// validate lifecycle continuity without assuming which coordinator started the gesture.
    #[must_use]
    pub fn uses_owner(&self, owner: InteractionOwner) -> bool {
        state_uses_owner(&self.state, owner)
    }

    /// Reconciles retained interaction capabilities against the authoritative graph.
    ///
    /// Adapters should call this after applying a runtime patch. The session validates exact
    /// lifetimes itself, so adapters never need to translate raw changed IDs into retirement
    /// notifications and cannot accidentally retire a same-ID successor.
    pub fn reconcile_graph(&mut self) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let view = self.authoritative_view()?;
        let snapshot = view
            .graph_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        let policy = view
            .graph_policy()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        let mut update = CanvasInteractionUpdate::default();

        if self
            .hovered_edge
            .is_some_and(|edge| !snapshot.edge_instance_is_current(edge))
        {
            self.hovered_edge = None;
            update.damage.merge(InteractionDamage {
                presentation: true,
                edges: true,
                ..InteractionDamage::default()
            });
        }

        if let InteractionState::Connection(ConnectionInteraction::Dragging(drag)) = &mut self.state
            && drag
                .target_lock
                .is_some_and(|target| !snapshot.port_instance_is_current(target.instance()))
        {
            drag.target_lock = None;
            drag.probe = ConnectionProbe::None;
            update.damage.merge(InteractionDamage::connection());
        }

        if state_is_current(&self.state, &snapshot, policy) {
            return Ok(self.finalize_update(update));
        }

        let damage = damage_for_state(&self.state);
        self.query_work.clear();
        self.submission_binding = None;
        self.state = InteractionState::Idle;
        self.terminal = Some(InteractionTerminalReason::OwnerRetired);
        Ok(self.finalize_update(update.with_damage(damage)))
    }

    pub fn close(&mut self) -> CanvasInteractionUpdate {
        if self.closed {
            return self.finalize_update(CanvasInteractionUpdate::default());
        }
        let damage = damage_for_state(&self.state);
        self.clear_pending_edge_pick_work();
        self.hovered_edge = None;
        self.query_work.clear();
        self.submission_binding = None;
        self.state = InteractionState::Idle;
        self.closed = true;
        self.terminal = Some(InteractionTerminalReason::ViewClosed);
        self.finalize_update(CanvasInteractionUpdate::default().with_damage(damage))
    }

    fn ensure_available(&self) -> Result<(), CanvasInteractionError> {
        self.ensure_open()
    }

    fn ensure_session_open(&self) -> Result<(), CanvasInteractionError> {
        if self.closed {
            Err(CanvasInteractionError::Closed)
        } else {
            Ok(())
        }
    }

    fn ensure_open(&self) -> Result<(), CanvasInteractionError> {
        self.ensure_session_open()?;
        self.ensure_bound_view()
    }

    fn ensure_bound_view(&self) -> Result<(), CanvasInteractionError> {
        #[cfg(test)]
        if self.test_authority_bypass {
            return Ok(());
        }
        let actual = self
            .bound_view
            .provenance()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)?;
        if actual != self.provenance {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        Ok(())
    }

    fn ensure_interaction_session(
        &self,
        actual: InteractionSessionId,
    ) -> Result<(), CanvasInteractionError> {
        if actual != self.id {
            return Err(CanvasInteractionError::ForeignInteractionSession {
                expected: self.id,
                actual,
            });
        }
        Ok(())
    }

    fn callback_authority(
        &self,
        actual_session: InteractionSessionId,
        facts_provenance: Option<ViewProvenance>,
        current: bool,
    ) -> Result<CallbackAuthority, CanvasInteractionError> {
        self.ensure_interaction_session(actual_session)?;
        if facts_provenance.is_some_and(|actual| actual != self.provenance) {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        if !current {
            return Ok(CallbackAuthority::Stale);
        }
        self.ensure_session_open()?;
        self.ensure_bound_view()?;
        Ok(CallbackAuthority::Current)
    }

    fn authoritative_view(&self) -> Result<WeakViewSession, CanvasInteractionError> {
        self.ensure_open()?;
        Ok(self.bound_view.clone())
    }

    fn promote_node_interaction(
        &self,
        node: NodeLifetimeKey,
    ) -> Result<bool, CanvasInteractionError> {
        #[cfg(test)]
        if self.test_authority_bypass {
            return Ok(false);
        }
        self.bound_view
            .promote_node_interaction(node)
            .map_err(map_connection_binding_error)
    }

    fn current_viewport_snapshot(&self) -> Result<ViewportSnapshot, CanvasInteractionError> {
        #[cfg(test)]
        if self.test_authority_bypass {
            return Ok(self.test_viewport);
        }
        self.bound_view
            .viewport_snapshot()
            .map_err(|_| CanvasInteractionError::ViewUnavailable)
    }

    /// Converts a sample into the exact current viewport coordinate system.
    ///
    /// Provenance is checked before resolving the bound view so a foreign sample never observes
    /// or perturbs another view's state. Same-view samples may be queued behind a viewport change;
    /// their raw native point remains meaningful and is reprojected instead of rejected.
    fn normalize_pointer_sample(
        &self,
        sample: ViewportPointerSample,
    ) -> Result<(InteractionPosition, ViewportSnapshot), CanvasInteractionError> {
        self.ensure_session_open()?;
        if sample.snapshot().provenance() != self.provenance {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        let current = self.current_viewport_snapshot()?;
        if current.provenance() != self.provenance {
            return Err(CanvasInteractionError::ForeignInteractionFacts);
        }
        let position = InteractionPosition::from_sample(sample)
            .and_then(|position| position.reproject(current))
            .ok_or(CanvasInteractionError::InvalidPosition)?;
        Ok((position, current))
    }

    fn validate_interaction_reprojection(
        &self,
        snapshot: ViewportSnapshot,
    ) -> Result<(), CanvasInteractionError> {
        let mut error = None;
        visit_interaction_positions(&self.state, |position| {
            if error.is_none() {
                error = validate_position_reprojection(position, snapshot).err();
            }
        });
        if error.is_none()
            && let Some(pending) = self.pending_edge_pick
        {
            error = validate_position_reprojection(pending.position, snapshot).err();
        }
        error.map_or(Ok(()), Err)
    }

    /// Rebases retained pointer positions after an external viewport change. Callers validate
    /// first so inverse-projection failure cannot leave a half-rebased interaction behind.
    fn rebase_interaction_positions(
        &mut self,
        snapshot: ViewportSnapshot,
    ) -> Result<(), CanvasInteractionError> {
        self.validate_interaction_reprojection(snapshot)?;
        visit_interaction_positions_mut(&mut self.state, |position| {
            *position = position
                .reproject(snapshot)
                .expect("positions were validated against this exact viewport snapshot");
        });
        if let Some(pending) = &mut self.pending_edge_pick {
            pending.position = pending
                .position
                .reproject(snapshot)
                .expect("pending hover was validated against this exact viewport snapshot");
        }
        Ok(())
    }

    fn commit_pan(
        &mut self,
        delta: ViewDelta,
    ) -> Result<(ViewportSnapshot, Option<PanicPayload>), CanvasInteractionError> {
        let request = ViewportPanRequest::new(delta);
        #[cfg(test)]
        if self.test_authority_bypass {
            let current = self.test_viewport;
            let next = ViewportIntent::pan(request)
                .resolve(current.transform())
                .ok_or(CanvasInteractionError::InvalidPosition)?;
            let snapshot = if next == current.transform() {
                current
            } else {
                let revision = current
                    .revision()
                    .checked_next()
                    .ok_or(CanvasInteractionError::IdentityExhausted)?;
                ViewportSnapshot::new(self.provenance, revision, next)
            };
            self.test_viewport = snapshot;
            return Ok((snapshot, None));
        }

        match catch_unwind(AssertUnwindSafe(|| {
            self.bound_view
                .apply_viewport_intent(ViewportIntent::pan(request))
        })) {
            Ok(Ok(snapshot)) => Ok((snapshot, None)),
            Ok(Err(_)) => Err(CanvasInteractionError::ViewUnavailable),
            Err(panic) => match self.bound_view.viewport_snapshot() {
                Ok(snapshot) => Ok((snapshot, Some(panic))),
                Err(_) => {
                    resume(Some(panic));
                    Err(CanvasInteractionError::ViewUnavailable)
                }
            },
        }
    }

    fn move_pan_if_active(
        &mut self,
        position: InteractionPosition,
        current: ViewportSnapshot,
    ) -> Result<Option<CanvasInteractionUpdate>, CanvasInteractionError> {
        enum PanMove {
            Active {
                generation: InteractionGeneration,
                delta: ViewDelta,
            },
            Armed {
                generation: InteractionGeneration,
                delta: ViewDelta,
            },
        }

        self.validate_interaction_reprojection(current)?;
        let plan = match &self.state {
            InteractionState::Pan(pan) => {
                let pointer = pan
                    .pointer
                    .reproject(current)
                    .expect("the pan pointer was validated against this viewport");
                Some(PanMove::Active {
                    generation: pan.generation,
                    delta: view_delta(pointer.view(), position.view()),
                })
            }
            InteractionState::Armed(armed) if matches!(armed.target, ArmedTarget::Pan { .. }) => {
                let start = armed
                    .start
                    .reproject(current)
                    .expect("the armed press was validated against this viewport");
                let distance = armed
                    .max_view_distance_squared
                    .max(view_distance_squared(start.view(), position.view()));
                threshold_crossed_distance(distance, self.threshold_for(&armed.target)).then_some(
                    PanMove::Armed {
                        generation: armed.generation,
                        delta: view_delta(start.view(), position.view()),
                    },
                )
            }
            _ => None,
        };
        let Some(plan) = plan else {
            return Ok(None);
        };

        let (generation, delta) = match plan {
            PanMove::Active { generation, delta } | PanMove::Armed { generation, delta } => {
                (generation, delta)
            }
        };
        if delta.x == 0.0 && delta.y == 0.0 {
            self.rebase_interaction_positions(current)?;
            let InteractionState::Pan(pan) = &mut self.state else {
                unreachable!("only an active pan can retain a zero pan delta");
            };
            pan.pointer = position;
            return Ok(Some(
                self.finalize_update(CanvasInteractionUpdate::default()),
            ));
        }

        let preview = ViewportIntent::pan(ViewportPanRequest::new(delta))
            .resolve(current.transform())
            .ok_or(CanvasInteractionError::InvalidPosition)?;
        let preview = ViewportSnapshot::new(self.provenance, current.revision(), preview);
        position
            .reproject(preview)
            .ok_or(CanvasInteractionError::InvalidPosition)?;

        let (committed, panic) = self.commit_pan(delta)?;
        let pointer = position
            .reproject(committed)
            .expect("the committed pan transform was previewed before the atomic commit");
        self.state = InteractionState::Pan(PanInteraction {
            generation,
            pointer,
        });
        let mut update = CanvasInteractionUpdate::default();
        if committed.revision() != current.revision() {
            update.damage.viewport = true;
        }
        let update = self.finalize_update(update);
        resume(panic);
        Ok(Some(update))
    }

    fn push_graph_mutation(
        &mut self,
        update: &mut CanvasInteractionUpdate,
        ticket: InteractionTicketId,
        mutation: FreshGraphMutation,
    ) {
        self.push_graph_submission(update, InteractionGraphSubmission::new(ticket, mutation));
    }

    fn push_graph_submission(
        &mut self,
        update: &mut CanvasInteractionUpdate,
        submission: InteractionGraphSubmission,
    ) {
        self.submission_binding = Some(SubmissionBinding {
            ticket: submission.ticket(),
            phase: SubmissionBindingPhase::Pending,
        });
        update
            .effects
            .push(CanvasInteractionEffect::SubmitGraph { submission });
    }

    fn finalize_update(&mut self, mut update: CanvasInteractionUpdate) -> CanvasInteractionUpdate {
        if let Some(query) = self.query_work.take()
            && self.query_is_current(&query)
        {
            self.query_work.replace(query);
        }
        update.tick_demand = self.tick_demand();
        update.query_work_pending = self.query_work.is_pending();
        update
    }

    fn commit_update_selection(
        &mut self,
        update: &mut CanvasInteractionUpdate,
    ) -> Result<(), CanvasInteractionError> {
        self.commit_update_selection_classified(update)
            .map_err(|failure| failure.error)
    }

    fn commit_update_selection_classified(
        &mut self,
        update: &mut CanvasInteractionUpdate,
    ) -> Result<(), SelectionCommitFailure> {
        let Some(mutation) = update.pending_selection.as_ref().cloned() else {
            return Ok(());
        };
        #[cfg(test)]
        if let Some(error) = self.selection_commit_failure.take() {
            return Err(SelectionCommitFailure {
                retryable: error == CanvasInteractionError::ViewUnavailable,
                error,
            });
        }
        #[cfg(test)]
        if self.test_authority_bypass {
            update.selection_mutations.push(mutation);
            update.pending_selection = None;
            return Ok(());
        }
        self.bound_view
            .apply_selection_mutation(mutation)
            .map_err(|error| SelectionCommitFailure {
                retryable: matches!(
                    error,
                    ViewSessionError::Busy | ViewSessionError::Runtime(GraphRuntimeError::Busy)
                ),
                error: map_connection_binding_error(error),
            })?;
        update.pending_selection = None;
        Ok(())
    }

    fn commit_selection_transition(
        &mut self,
        update: &mut CanvasInteractionUpdate,
        rollback: Option<InteractionState>,
    ) -> Result<(), CanvasInteractionError> {
        if let Err(error) = self.commit_update_selection(update) {
            // Selection and the active interaction phase fail atomically. Monotonic identities and
            // view-owned effects from an earlier press (such as recent-node promotion) are not
            // rolled back; hover work is explicitly failed closed.
            self.state = rollback.unwrap_or_default();
            self.clear_pending_edge_pick_work();
            self.hovered_edge = None;
            self.query_work.clear();
            return Err(error);
        }
        Ok(())
    }

    fn allocate_generation(&mut self) -> Result<InteractionGeneration, CanvasInteractionError> {
        let Some(next) = self.next_generation else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return Err(CanvasInteractionError::IdentityExhausted);
        };
        self.next_generation = next.checked_add(1);
        Ok(InteractionGeneration(InteractionIdentity::new(
            self.id, next,
        )))
    }

    fn allocate_query(&mut self) -> Option<InteractionQueryId> {
        let next = self.next_query?;
        self.next_query = next.checked_add(1);
        Some(InteractionQueryId(InteractionIdentity::new(self.id, next)))
    }

    fn allocate_search_attempt(&mut self) -> Option<ConnectionSearchAttemptId> {
        let next = self.next_search_attempt?;
        self.next_search_attempt = next.checked_add(1);
        Some(ConnectionSearchAttemptId(InteractionIdentity::new(
            self.id, next,
        )))
    }

    fn interaction_query_binding(&self, id: InteractionQueryId) -> InteractionQueryBinding {
        InteractionQueryBinding::new(self.provenance, id)
    }

    #[cfg(test)]
    fn accepts_unsigned_query_outcomes_for_test(&self) -> bool {
        self.provenance.view()
            == ViewId::new(
                crate::runtime::graph_runtime::RuntimeId::UNAVAILABLE,
                self.provenance.view().get(),
            )
    }

    #[cfg(test)]
    fn resolve_edge_pick_for_test(
        &mut self,
        id: InteractionQueryId,
        outcome: EdgePickQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        if !outcome
            .receipt()
            .is_some_and(|receipt| receipt.authenticates(self.interaction_query_binding(id)))
            && !self.accepts_unsigned_query_outcomes_for_test()
        {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        self.resolve_edge_pick_current(id, outcome)
    }

    #[cfg(test)]
    fn resolve_connection_probe_for_test(
        &mut self,
        id: InteractionQueryId,
        outcome: ConnectionProbeQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        if !outcome
            .receipt()
            .is_some_and(|receipt| receipt.authenticates(self.interaction_query_binding(id)))
            && !self.accepts_unsigned_query_outcomes_for_test()
        {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        self.resolve_connection_probe_current(id, outcome)
    }

    #[cfg(test)]
    fn resolve_node_intersection_for_test(
        &mut self,
        id: InteractionQueryId,
        outcome: NodeIntersectionQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        if !outcome
            .receipt()
            .is_some_and(|receipt| receipt.authenticates(self.interaction_query_binding(id)))
            && !self.accepts_unsigned_query_outcomes_for_test()
        {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        self.resolve_node_intersection_current(id, outcome, None)
    }

    #[cfg(test)]
    fn resolve_alignment_snap_for_test(
        &mut self,
        id: InteractionQueryId,
        outcome: AlignmentSnapQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        if !outcome
            .receipt()
            .is_some_and(|receipt| receipt.authenticates(self.interaction_query_binding(id)))
            && !self.accepts_unsigned_query_outcomes_for_test()
        {
            return Ok(self.finalize_update(CanvasInteractionUpdate::default()));
        }
        self.resolve_alignment_snap_current(id, outcome)
    }

    fn allocate_ticket(&mut self) -> Option<InteractionTicketId> {
        let next = self.next_ticket?;
        self.next_ticket = next.checked_add(1);
        Some(InteractionTicketId(InteractionIdentity::new(self.id, next)))
    }

    fn arm(
        &mut self,
        target: ArmedTarget,
        position: InteractionPosition,
    ) -> Result<(), CanvasInteractionError> {
        let generation = self.allocate_generation()?;
        self.state = InteractionState::Armed(ArmedInteraction {
            generation,
            target,
            start: position,
            max_view_distance_squared: 0.0,
        });
        Ok(())
    }

    fn activate_armed(
        &mut self,
        armed: ArmedInteraction,
        position: InteractionPosition,
        update: &mut CanvasInteractionUpdate,
    ) -> Result<InteractionState, CanvasInteractionError> {
        Ok(match armed.target {
            ArmedTarget::NodeDrag(start) => {
                self.validate_node_drag_activation(&start)?;
                if !start.clicked_was_selected {
                    update.pending_selection = Some(SelectionMutation::Node {
                        provenance: start.provenance,
                        node: start.clicked,
                        mode: match start.selection_mode {
                            SelectionMode::Replace => SelectionMode::Replace,
                            SelectionMode::Add | SelectionMode::Toggle => SelectionMode::Add,
                        },
                    });
                    update.damage.selection = true;
                }
                let raw_delta = snapped_delta(
                    armed.start.canvas(),
                    position.canvas(),
                    start.snap_origin,
                    self.config.grid_size_in_canvas,
                );
                update.damage.merge(InteractionDamage::node_drag());
                let mut drag = NodeDragInteraction {
                    generation: armed.generation,
                    cohort: start.cohort,
                    snap_origin: start.snap_origin,
                    start: armed.start,
                    pointer: position,
                    raw_delta,
                    delta: raw_delta,
                    guides: Arc::default(),
                    alignment_locks: AlignmentSnapLocks::default(),
                    pending_alignment_query: None,
                    stale_query_restarts: 0,
                    release_pending: false,
                };
                self.start_alignment_snap(&mut drag, update);
                InteractionState::NodeDrag(drag)
            }
            ArmedTarget::Pan { .. } => unreachable!(
                "pan activation commits the bound viewport before interaction state is taken"
            ),
            ArmedTarget::Marquee {
                selection_mode,
                baseline,
            } => {
                let mut marquee = MarqueeInteraction {
                    generation: armed.generation,
                    start: armed.start,
                    pointer: position,
                    selection_mode,
                    baseline,
                    pending_query: None,
                    stale_query_restarts: 0,
                    release_pending: false,
                };
                update.damage.merge(InteractionDamage::presentation());
                if self.start_marquee_query(&mut marquee, update) {
                    InteractionState::Marquee(marquee)
                } else {
                    InteractionState::Idle
                }
            }
            ArmedTarget::Connection(start) => {
                let mut drag = ConnectionDragInteraction {
                    generation: armed.generation,
                    start,
                    pointer: position,
                    probe: ConnectionProbe::None,
                    target_lock: None,
                    pending_query: None,
                    stale_query_restarts: 0,
                    release_pending: false,
                };
                update.damage.merge(InteractionDamage::connection());
                if self.start_connection_probe(&mut drag, update) {
                    InteractionState::Connection(ConnectionInteraction::Dragging(drag))
                } else {
                    InteractionState::Idle
                }
            }
        })
    }

    fn finish_activated_release(
        &mut self,
        state: InteractionState,
        update: &mut CanvasInteractionUpdate,
    ) -> InteractionState {
        match state {
            InteractionState::NodeDrag(mut drag) => {
                drag.release_pending = true;
                if drag.pending_alignment_query.is_some() {
                    InteractionState::NodeDrag(drag)
                } else {
                    self.commit_node_drag(drag, update)
                }
            }
            InteractionState::Connection(ConnectionInteraction::Dragging(mut drag)) => {
                drag.release_pending = true;
                InteractionState::Connection(ConnectionInteraction::Dragging(drag))
            }
            InteractionState::Marquee(mut marquee) => {
                marquee.release_pending = true;
                InteractionState::Marquee(marquee)
            }
            other => other,
        }
    }

    fn finish_armed_click(&self, armed: &ArmedInteraction, update: &mut CanvasInteractionUpdate) {
        let tolerance = match &armed.target {
            ArmedTarget::NodeDrag(_) => self.config.node_click_tolerance_in_view,
            ArmedTarget::Pan { .. } | ArmedTarget::Marquee { .. } => {
                self.config.canvas_click_tolerance_in_view
            }
            ArmedTarget::Connection(_) => return,
        };
        if !within_click_tolerance(armed.max_view_distance_squared, tolerance) {
            return;
        }
        let mutation = match &armed.target {
            ArmedTarget::NodeDrag(start) => Some(SelectionMutation::Node {
                provenance: start.provenance,
                node: start.clicked,
                mode: start.selection_mode,
            }),
            ArmedTarget::Pan {
                selection_mode: Some(SelectionMode::Replace),
            }
            | ArmedTarget::Marquee {
                selection_mode: SelectionMode::Replace,
                ..
            } => Some(SelectionMutation::Clear {
                provenance: self.provenance,
            }),
            ArmedTarget::Pan { .. } | ArmedTarget::Marquee { .. } | ArmedTarget::Connection(_) => {
                None
            }
        };
        if let Some(mutation) = mutation {
            update.pending_selection = Some(mutation);
            update.damage.selection = true;
        }
    }

    fn threshold_for(&self, target: &ArmedTarget) -> f32 {
        match target {
            ArmedTarget::NodeDrag(_) => self.config.node_drag_threshold_in_view,
            ArmedTarget::Connection(_) => self.config.connection_drag_threshold_in_view,
            ArmedTarget::Pan { .. } | ArmedTarget::Marquee { .. } => {
                self.config.canvas_drag_threshold_in_view
            }
        }
    }

    fn start_alignment_snap(
        &mut self,
        drag: &mut NodeDragInteraction,
        update: &mut CanvasInteractionUpdate,
    ) -> bool {
        drag.stale_query_restarts = 0;
        self.queue_alignment_snap(drag, update)
    }

    fn queue_alignment_snap(
        &mut self,
        drag: &mut NodeDragInteraction,
        _update: &mut CanvasInteractionUpdate,
    ) -> bool {
        let Some(config) = self.config.alignment_snap else {
            drag.pending_alignment_query = None;
            drag.alignment_locks = AlignmentSnapLocks::default();
            return false;
        };
        let Some(id) = self.allocate_query() else {
            drag.pending_alignment_query = None;
            return false;
        };
        drag.pending_alignment_query = Some(id);
        self.query_work
            .replace(PendingInteractionQuery::alignment_snap(
                id,
                drag.generation,
                AlignmentSnapQuery::new(
                    self.provenance.view(),
                    self.provenance.generation(),
                    drag.generation,
                    drag.cohort.nodes_arc(),
                    drag.raw_delta,
                    config.acquire_tolerance_in_view(),
                    config.release_tolerance_in_view(),
                    self.config.query_entry_budget,
                    drag.alignment_locks,
                    self.interaction_query_binding(id),
                ),
            ));
        true
    }

    fn start_marquee_query(
        &mut self,
        marquee: &mut MarqueeInteraction,
        update: &mut CanvasInteractionUpdate,
    ) -> bool {
        marquee.stale_query_restarts = 0;
        self.queue_marquee_query(marquee, update)
    }

    fn queue_marquee_query(
        &mut self,
        marquee: &mut MarqueeInteraction,
        _update: &mut CanvasInteractionUpdate,
    ) -> bool {
        let Some(id) = self.allocate_query() else {
            marquee.pending_query = None;
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return false;
        };
        marquee.pending_query = Some(id);
        self.query_work
            .replace(PendingInteractionQuery::node_intersection(
                id,
                marquee.generation,
                NodeIntersectionQuery::new(
                    rect_between(marquee.start.canvas(), marquee.pointer.canvas()),
                    self.config.query_entry_budget,
                )
                .bind_interaction(self.interaction_query_binding(id)),
            ));
        true
    }

    fn commit_node_drag(
        &mut self,
        drag: NodeDragInteraction,
        update: &mut CanvasInteractionUpdate,
    ) -> InteractionState {
        let Some(intent) = RelativeNodeMoveIntent::from_cohort(drag.cohort.clone(), drag.delta)
        else {
            return InteractionState::Idle;
        };
        let Some(ticket) = self.allocate_ticket() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return InteractionState::Idle;
        };
        self.push_graph_mutation(update, ticket, FreshGraphMutation::move_nodes(intent));
        InteractionState::Committing(CommitInteraction::NodeMove(NodeMoveCommitInteraction {
            generation: drag.generation,
            cohort: drag.cohort,
            delta: drag.delta,
            guides: drag.guides,
            ticket,
        }))
    }

    fn start_connection_probe(
        &mut self,
        drag: &mut ConnectionDragInteraction,
        update: &mut CanvasInteractionUpdate,
    ) -> bool {
        drag.stale_query_restarts = 0;
        self.queue_connection_probe(drag, update)
    }

    fn queue_connection_probe(
        &mut self,
        drag: &mut ConnectionDragInteraction,
        _update: &mut CanvasInteractionUpdate,
    ) -> bool {
        let Some(id) = self.allocate_query() else {
            drag.pending_query = None;
            self.query_work.clear();
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return false;
        };
        drag.pending_query = Some(id);
        self.query_work
            .replace(PendingInteractionQuery::connection_probe(
                id,
                drag.generation,
                ConnectionProbeQuery::new(
                    drag.start.binding.clone(),
                    drag.pointer.view(),
                    connection_probe_targeting(self.config, drag.target_lock),
                    self.config.query_entry_budget,
                )
                .bind_interaction(self.interaction_query_binding(id)),
            ));
        true
    }

    fn finish_connection_release(
        &mut self,
        drag: ConnectionDragInteraction,
        update: &mut CanvasInteractionUpdate,
    ) -> InteractionState {
        match drag.probe.target() {
            Some(target) if drag.probe.is_valid() => {
                let validated_version = drag
                    .probe
                    .version()
                    .expect("a valid connection probe always carries its graph version");
                self.begin_connection_commit(
                    drag.generation,
                    drag.start,
                    drag.pointer,
                    drag.probe,
                    ConnectionCommitTarget::existing(target.identity().instance()),
                    validated_version,
                    None,
                    false,
                    update,
                )
            }
            None if drag.start.allow_create_from_wire => {
                let Some(attempt) = self.allocate_search_attempt() else {
                    self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
                    return InteractionState::Idle;
                };
                let search = ConnectionSearchInteraction {
                    generation: drag.generation,
                    attempt,
                    start: drag.start,
                    pointer: drag.pointer,
                    probe: drag.probe,
                    status: ConnectionSearchStatus::Loading,
                };
                update
                    .effects
                    .push(CanvasInteractionEffect::Search(search_request(
                        self.id,
                        self.provenance,
                        &search,
                    )));
                InteractionState::Connection(ConnectionInteraction::Searching(search))
            }
            Some(_) | None => {
                self.terminal = Some(InteractionTerminalReason::Cancelled);
                InteractionState::Idle
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn begin_connection_commit(
        &mut self,
        generation: InteractionGeneration,
        start: ConnectionInteractionStart,
        pointer: InteractionPosition,
        probe: ConnectionProbe,
        target: ConnectionCommitTarget,
        validated_version: GraphVersion,
        search_attempt: Option<ConnectionSearchAttemptId>,
        return_to_search: bool,
        update: &mut CanvasInteractionUpdate,
    ) -> InteractionState {
        let Some(ticket) = self.allocate_ticket() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return InteractionState::Idle;
        };
        self.push_graph_mutation(
            update,
            ticket,
            FreshGraphMutation::connection(
                start.binding.clone(),
                target.clone(),
                validated_version,
            ),
        );
        InteractionState::Committing(CommitInteraction::Connection(Box::new(
            ConnectionCommitInteraction {
                generation,
                search_attempt,
                start,
                pointer,
                probe,
                ticket,
                target,
                return_to_search,
                freshness: ConnectionCommitFreshness::Initial,
                stale_query_restarts: 0,
            },
        )))
    }

    fn finish_connection_commit(
        &mut self,
        commit: Box<ConnectionCommitInteraction>,
        terminal: InteractionTerminalReason,
    ) -> InteractionState {
        if matches!(
            terminal,
            InteractionTerminalReason::Applied | InteractionTerminalReason::Noop
        ) {
            self.terminal = Some(terminal);
            InteractionState::Idle
        } else {
            self.reject_connection_commit(commit, terminal)
        }
    }

    fn reject_connection_commit(
        &mut self,
        commit: Box<ConnectionCommitInteraction>,
        reason: InteractionTerminalReason,
    ) -> InteractionState {
        let commit = *commit;
        self.terminal = Some(reason);
        if commit.return_to_search {
            let Some(attempt) = self.allocate_search_attempt() else {
                self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
                return InteractionState::Idle;
            };
            InteractionState::Connection(ConnectionInteraction::Searching(
                ConnectionSearchInteraction {
                    generation: commit.generation,
                    attempt,
                    start: commit.start,
                    pointer: commit.pointer,
                    probe: commit.probe,
                    status: ConnectionSearchStatus::CommitRejected,
                },
            ))
        } else {
            InteractionState::Idle
        }
    }

    fn set_search_status(
        &mut self,
        request: &ConnectionSearchRequest,
        status: ConnectionSearchStatus,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.validate_search_request(request)?;
        let search = match &mut self.state {
            InteractionState::Connection(ConnectionInteraction::Searching(search))
                if search_token_matches(&request.token, search) =>
            {
                search
            }
            InteractionState::Connection(ConnectionInteraction::Searching(_)) => {
                return Err(CanvasInteractionError::ConnectionSearchStaleGeneration);
            }
            _ => return Err(CanvasInteractionError::ConnectionSearchInactive),
        };
        let mut update = CanvasInteractionUpdate::default();
        search.status = status;
        update.damage.merge(InteractionDamage::connection());
        Ok(self.finalize_update(update))
    }

    fn validate_search_request(
        &self,
        request: &ConnectionSearchRequest,
    ) -> Result<(), CanvasInteractionError> {
        self.validate_search_token(&request.token)
    }

    fn validate_search_token(
        &self,
        token: &ConnectionSearchToken,
    ) -> Result<(), CanvasInteractionError> {
        let (current, stale_error) = match &self.state {
            InteractionState::Connection(ConnectionInteraction::Searching(search))
                if search_token_matches(token, search) =>
            {
                (
                    true,
                    CanvasInteractionError::ConnectionSearchStaleGeneration,
                )
            }
            InteractionState::Connection(ConnectionInteraction::Searching(_)) => (
                false,
                CanvasInteractionError::ConnectionSearchStaleGeneration,
            ),
            _ => (false, CanvasInteractionError::ConnectionSearchInactive),
        };
        match self.callback_authority(token.session, Some(token.provenance), current)? {
            CallbackAuthority::Current => Ok(()),
            CallbackAuthority::Stale => Err(stale_error),
        }
    }
}

fn search_token_matches(
    token: &ConnectionSearchToken,
    search: &ConnectionSearchInteraction,
) -> bool {
    token.generation == search.generation
        && token.attempt == search.attempt
        && token.binding == search.start.binding
}

fn connection_presentation_from_drag(drag: &ConnectionDragInteraction) -> ConnectionPresentation {
    ConnectionPresentation::new(
        drag.generation,
        drag.start.binding.clone(),
        drag.start.fixed_endpoint,
        drag.start.fixed_anchor,
        drag.start.fixed_outward,
        drag.start.moving_outward,
        drag.pointer,
        drag.probe.clone(),
        drag.start.binding.reconnect_edge(),
        None,
        drag.release_pending,
        None,
    )
}

fn connection_presentation_from_search(
    search: &ConnectionSearchInteraction,
) -> ConnectionPresentation {
    ConnectionPresentation::new(
        search.generation,
        search.start.binding.clone(),
        search.start.fixed_endpoint,
        search.start.fixed_anchor,
        search.start.fixed_outward,
        search.start.moving_outward,
        search.pointer,
        search.probe.clone(),
        search.start.binding.reconnect_edge(),
        Some(search.status),
        false,
        None,
    )
}

fn connection_presentation_from_commit(
    commit: &ConnectionCommitInteraction,
) -> ConnectionPresentation {
    ConnectionPresentation::new(
        commit.generation,
        commit.start.binding.clone(),
        commit.start.fixed_endpoint,
        commit.start.fixed_anchor,
        commit.start.fixed_outward,
        commit.start.moving_outward,
        commit.pointer,
        commit.probe.clone(),
        commit.start.binding.reconnect_edge(),
        commit
            .return_to_search
            .then_some(ConnectionSearchStatus::Loading),
        false,
        Some(commit.ticket),
    )
}

fn node_drag_presentation(
    generation: InteractionGeneration,
    cohort: NodeMoveCohort,
    delta: CanvasPoint,
    guides: Arc<[AlignmentGuide]>,
    release_pending: bool,
    ticket: Option<InteractionTicketId>,
) -> NodeDragPresentation {
    NodeDragPresentation::new(generation, cohort, delta, guides, release_pending, ticket)
}

fn search_request(
    session: InteractionSessionId,
    provenance: ViewProvenance,
    search: &ConnectionSearchInteraction,
) -> ConnectionSearchRequest {
    connection_search_request(
        session,
        provenance,
        search.generation,
        search.attempt,
        &search.start,
        search.pointer,
    )
}

fn connection_search_request(
    session: InteractionSessionId,
    provenance: ViewProvenance,
    generation: InteractionGeneration,
    attempt: ConnectionSearchAttemptId,
    start: &ConnectionInteractionStart,
    position: InteractionPosition,
) -> ConnectionSearchRequest {
    ConnectionSearchRequest {
        token: ConnectionSearchToken {
            provenance,
            session,
            generation,
            attempt,
            binding: start.binding.clone(),
        },
        position,
    }
}

fn completion_terminal(kind: CommandExecutionKind) -> InteractionTerminalReason {
    match kind {
        CommandExecutionKind::Applied => InteractionTerminalReason::Applied,
        CommandExecutionKind::Noop => InteractionTerminalReason::Noop,
        CommandExecutionKind::Rejected => InteractionTerminalReason::Rejected,
        CommandExecutionKind::Conflict => InteractionTerminalReason::Conflict,
        CommandExecutionKind::Failed => InteractionTerminalReason::ExecutionFailed,
    }
}

fn connection_probe_targeting(
    config: CanvasInteractionConfig,
    preferred: Option<ConnectionProbeTargetIdentity>,
) -> ConnectionProbeTargeting {
    let targeting = ConnectionProbeTargeting::new(
        config.connection_acquire_radius_in_view,
        config.connection_release_radius_in_view,
    )
    .expect("a live interaction session retains valid connection target bands");
    preferred.map_or(targeting, |preferred| targeting.with_preferred(preferred))
}

fn map_connection_binding_error(error: ViewSessionError) -> CanvasInteractionError {
    match error {
        ViewSessionError::ForeignInteractionFacts => {
            CanvasInteractionError::ForeignInteractionFacts
        }
        ViewSessionError::StaleInteractionFacts => CanvasInteractionError::StaleInteractionFacts,
        _ => CanvasInteractionError::ViewUnavailable,
    }
}

fn map_create_from_wire_query_error(error: ViewCreateFromWireQueryError) -> CanvasInteractionError {
    match error {
        ViewCreateFromWireQueryError::Session(error) => map_connection_binding_error(error),
        ViewCreateFromWireQueryError::Query(CreateFromWireQueryError::Busy) => {
            CanvasInteractionError::ViewUnavailable
        }
        ViewCreateFromWireQueryError::Query(CreateFromWireQueryError::ForeignSourcePort {
            ..
        }) => CanvasInteractionError::ForeignInteractionFacts,
        ViewCreateFromWireQueryError::Query(
            CreateFromWireQueryError::MissingSourcePort { .. }
            | CreateFromWireQueryError::SourcePortRetired { .. }
            | CreateFromWireQueryError::MissingSourceNode { .. }
            | CreateFromWireQueryError::SourceNotConnectable { .. },
        ) => CanvasInteractionError::StaleInteractionFacts,
    }
}

fn map_node_move_bind_error(error: FreshGraphMutationBindError) -> CanvasInteractionError {
    match error {
        FreshGraphMutationBindError::NodeDragMemberForeignRuntime { .. } => {
            CanvasInteractionError::ForeignInteractionFacts
        }
        FreshGraphMutationBindError::NodeDragMemberPositionOverflow { .. } => {
            CanvasInteractionError::InvalidPosition
        }
        FreshGraphMutationBindError::NodeDragMemberUnavailable { .. }
        | FreshGraphMutationBindError::NodeDragMemberReplaced { .. }
        | FreshGraphMutationBindError::NodeDragMemberNotDraggable { .. }
        | FreshGraphMutationBindError::NodeMoveExtentUnavailable { .. } => {
            CanvasInteractionError::StaleInteractionFacts
        }
        _ => CanvasInteractionError::StaleInteractionFacts,
    }
}

fn threshold_crossed_distance(distance_squared: f32, threshold: f32) -> bool {
    distance_squared > threshold * threshold
}

fn within_click_tolerance(distance_squared: f32, tolerance: f32) -> bool {
    distance_squared <= tolerance * tolerance
}

fn view_distance_squared(start: ViewPoint, current: ViewPoint) -> f32 {
    let x = current.x - start.x;
    let y = current.y - start.y;
    x.mul_add(x, y * y)
}

fn view_delta(start: ViewPoint, current: ViewPoint) -> ViewDelta {
    ViewDelta::between(start, current)
}

fn snapped_delta(
    start: CanvasPoint,
    current: CanvasPoint,
    snap_origin: CanvasPoint,
    grid: Option<CanvasSize>,
) -> CanvasPoint {
    let raw = CanvasPoint {
        x: current.x - start.x,
        y: current.y - start.y,
    };
    let Some(grid) = grid else {
        return raw;
    };
    CanvasPoint {
        x: ((snap_origin.x + raw.x) / grid.width).round() * grid.width - snap_origin.x,
        y: ((snap_origin.y + raw.y) / grid.height).round() * grid.height - snap_origin.y,
    }
}

fn nudge_delta_for_grid(
    snapshot: &GraphSnapshot,
    cohort: &NodeMoveCohort,
    requested: CanvasPoint,
    grid: Option<CanvasSize>,
) -> Option<CanvasPoint> {
    let anchor = snapshot
        .graph()
        .node(&cohort.nodes().first().copied()?)?
        .pos;
    anchor.is_finite().then(|| {
        snapped_delta(
            anchor,
            CanvasPoint {
                x: anchor.x + requested.x,
                y: anchor.y + requested.y,
            },
            anchor,
            grid,
        )
    })
}

fn reveal_pan_delta(
    bounds: CanvasRect,
    transform: crate::runtime::viewport::ViewportTransform,
    viewport: ViewSize,
    padding: f32,
) -> Option<ViewDelta> {
    if !bounds.is_positive_finite() || !transform.is_valid() || !viewport.is_positive_finite() {
        return None;
    }
    let min = transform.screen_point_for_canvas(bounds.origin)?;
    let max = transform.screen_point_for_canvas(CanvasPoint {
        x: bounds.origin.x + bounds.size.width,
        y: bounds.origin.y + bounds.size.height,
    })?;
    Some(ViewDelta {
        x: reveal_axis_delta(min.x, max.x, viewport.width, padding),
        y: reveal_axis_delta(min.y, max.y, viewport.height, padding),
    })
}

fn reveal_axis_delta(min: f32, max: f32, extent: f32, padding: f32) -> f32 {
    let available = extent - padding * 2.0;
    if max - min > available {
        extent * 0.5 - (min + max) * 0.5
    } else if min < padding {
        padding - min
    } else if max > extent - padding {
        extent - padding - max
    } else {
        0.0
    }
}

fn rect_between(first: CanvasPoint, second: CanvasPoint) -> CanvasRect {
    let min_x = first.x.min(second.x);
    let min_y = first.y.min(second.y);
    CanvasRect {
        origin: CanvasPoint { x: min_x, y: min_y },
        size: CanvasSize {
            width: (first.x - second.x).abs(),
            height: (first.y - second.y).abs(),
        },
    }
}

fn auto_pan_delta(
    pointer: ViewPoint,
    viewport: ViewSize,
    band: f32,
    speed: f32,
    elapsed: f32,
) -> ViewDelta {
    ViewDelta {
        x: auto_pan_axis(pointer.x, viewport.width, band) * speed * elapsed,
        y: auto_pan_axis(pointer.y, viewport.height, band) * speed * elapsed,
    }
}

fn auto_pan_axis(position: f32, extent: f32, band: f32) -> f32 {
    let band = band.min(extent * 0.5);
    if band <= 0.0 {
        return 0.0;
    }
    if position < band {
        ((band - position) / band).clamp(0.0, 1.0)
    } else if position > extent - band {
        -((position - (extent - band)) / band).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn active_pointer(state: &InteractionState) -> Option<InteractionPosition> {
    match state {
        InteractionState::NodeDrag(drag) if !drag.release_pending => Some(drag.pointer),
        InteractionState::Connection(ConnectionInteraction::Dragging(drag))
            if !drag.release_pending =>
        {
            Some(drag.pointer)
        }
        InteractionState::Marquee(marquee) if !marquee.release_pending => Some(marquee.pointer),
        InteractionState::NodeDrag(_)
        | InteractionState::Armed(_)
        | InteractionState::EdgePickPending(_)
        | InteractionState::Idle
        | InteractionState::Pan(_)
        | InteractionState::Marquee(_)
        | InteractionState::Connection(ConnectionInteraction::Dragging(_))
        | InteractionState::Connection(ConnectionInteraction::Searching(_))
        | InteractionState::KeyboardConnection(_)
        | InteractionState::Committing(_)
        | InteractionState::EdgeActive(_) => None,
    }
}

fn validate_position_reprojection(
    position: InteractionPosition,
    snapshot: ViewportSnapshot,
) -> Result<(), CanvasInteractionError> {
    if position.provenance() != snapshot.provenance() {
        return Err(CanvasInteractionError::ForeignInteractionFacts);
    }
    position
        .reproject(snapshot)
        .map(|_| ())
        .ok_or(CanvasInteractionError::InvalidPosition)
}

fn visit_interaction_positions(
    state: &InteractionState,
    mut visit: impl FnMut(InteractionPosition),
) {
    match state {
        InteractionState::Idle
        | InteractionState::KeyboardConnection(_)
        | InteractionState::Committing(CommitInteraction::NodeMove(_))
        | InteractionState::Committing(CommitInteraction::KeyboardConnection(_))
        | InteractionState::Committing(CommitInteraction::EdgeDelete(_))
        | InteractionState::Committing(CommitInteraction::SelectionDelete(_))
        | InteractionState::EdgeActive(_) => {}
        InteractionState::Armed(armed) => visit(armed.start),
        InteractionState::NodeDrag(drag) => {
            visit(drag.start);
            visit(drag.pointer);
        }
        InteractionState::Pan(pan) => visit(pan.pointer),
        InteractionState::Marquee(marquee) => {
            visit(marquee.start);
            visit(marquee.pointer);
        }
        InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => visit(drag.pointer),
        InteractionState::Connection(ConnectionInteraction::Searching(search)) => {
            visit(search.pointer);
        }
        InteractionState::Committing(CommitInteraction::Connection(commit)) => {
            visit(commit.pointer);
        }
        InteractionState::EdgePickPending(pending) => {
            visit(pending.press);
            visit(pending.pointer);
        }
    }
}

fn visit_interaction_positions_mut(
    state: &mut InteractionState,
    mut visit: impl FnMut(&mut InteractionPosition),
) {
    match state {
        InteractionState::Idle
        | InteractionState::KeyboardConnection(_)
        | InteractionState::Committing(CommitInteraction::NodeMove(_))
        | InteractionState::Committing(CommitInteraction::KeyboardConnection(_))
        | InteractionState::Committing(CommitInteraction::EdgeDelete(_))
        | InteractionState::Committing(CommitInteraction::SelectionDelete(_))
        | InteractionState::EdgeActive(_) => {}
        InteractionState::Armed(armed) => visit(&mut armed.start),
        InteractionState::NodeDrag(drag) => {
            visit(&mut drag.start);
            visit(&mut drag.pointer);
        }
        InteractionState::Pan(pan) => visit(&mut pan.pointer),
        InteractionState::Marquee(marquee) => {
            visit(&mut marquee.start);
            visit(&mut marquee.pointer);
        }
        InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => {
            visit(&mut drag.pointer);
        }
        InteractionState::Connection(ConnectionInteraction::Searching(search)) => {
            visit(&mut search.pointer);
        }
        InteractionState::Committing(CommitInteraction::Connection(commit)) => {
            visit(&mut commit.pointer);
        }
        InteractionState::EdgePickPending(pending) => {
            visit(&mut pending.press);
            visit(&mut pending.pointer);
        }
    }
}

fn interaction_positions_need_rebase(
    state: &InteractionState,
    pending_hover: Option<PendingHoverEdgePick>,
    snapshot: ViewportSnapshot,
) -> bool {
    let mut stale = false;
    visit_interaction_positions(state, |position| {
        stale |= position.viewport_revision() != snapshot.revision();
    });
    stale
        || pending_hover
            .is_some_and(|pending| pending.position.viewport_revision() != snapshot.revision())
}

fn state_generation(state: &InteractionState) -> Option<InteractionGeneration> {
    match state {
        InteractionState::Idle => None,
        InteractionState::Armed(armed) => Some(armed.generation),
        InteractionState::NodeDrag(drag) => Some(drag.generation),
        InteractionState::Pan(pan) => Some(pan.generation),
        InteractionState::Marquee(marquee) => Some(marquee.generation),
        InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => {
            Some(drag.generation)
        }
        InteractionState::Connection(ConnectionInteraction::Searching(search)) => {
            Some(search.generation)
        }
        InteractionState::KeyboardConnection(connection) => Some(connection.generation),
        InteractionState::Committing(CommitInteraction::Connection(commit)) => {
            Some(commit.generation)
        }
        InteractionState::Committing(CommitInteraction::KeyboardConnection(commit)) => {
            Some(commit.generation)
        }
        InteractionState::Committing(CommitInteraction::NodeMove(commit)) => {
            Some(commit.generation)
        }
        InteractionState::Committing(CommitInteraction::EdgeDelete(commit)) => {
            Some(commit.generation)
        }
        InteractionState::Committing(CommitInteraction::SelectionDelete(commit)) => {
            Some(commit.generation)
        }
        InteractionState::EdgeActive(edge) => Some(edge.generation),
        InteractionState::EdgePickPending(pending) => Some(pending.generation),
    }
}

fn interaction_state_submission_ticket(state: &InteractionState) -> Option<InteractionTicketId> {
    match state {
        InteractionState::Committing(CommitInteraction::Connection(commit)) => Some(commit.ticket),
        InteractionState::Committing(CommitInteraction::KeyboardConnection(commit)) => {
            Some(commit.ticket)
        }
        InteractionState::Committing(CommitInteraction::NodeMove(commit)) => Some(commit.ticket),
        InteractionState::Committing(CommitInteraction::EdgeDelete(commit)) => Some(commit.ticket),
        InteractionState::Committing(CommitInteraction::SelectionDelete(commit)) => {
            Some(commit.ticket)
        }
        _ => None,
    }
}

fn state_connection_intent(state: &InteractionState) -> Option<ConnectionGestureBinding> {
    match state {
        InteractionState::Armed(ArmedInteraction {
            target: ArmedTarget::Connection(start),
            ..
        }) => Some(start.binding.clone()),
        InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => {
            Some(drag.start.binding.clone())
        }
        InteractionState::Connection(ConnectionInteraction::Searching(search)) => {
            Some(search.start.binding.clone())
        }
        InteractionState::KeyboardConnection(connection) => Some(connection.start.binding.clone()),
        InteractionState::Committing(CommitInteraction::Connection(commit)) => {
            Some(commit.start.binding.clone())
        }
        InteractionState::Committing(CommitInteraction::KeyboardConnection(commit)) => {
            Some(commit.start.binding.clone())
        }
        InteractionState::Idle
        | InteractionState::Armed(_)
        | InteractionState::NodeDrag(_)
        | InteractionState::Pan(_)
        | InteractionState::Marquee(_)
        | InteractionState::Committing(CommitInteraction::NodeMove(_))
        | InteractionState::Committing(CommitInteraction::EdgeDelete(_))
        | InteractionState::Committing(CommitInteraction::SelectionDelete(_))
        | InteractionState::EdgeActive(_)
        | InteractionState::EdgePickPending(_) => None,
    }
}

fn reserve_stale_query_restart(restarts: &mut u8) -> bool {
    if *restarts >= MAX_STALE_QUERY_RESTARTS {
        return false;
    }
    *restarts = (*restarts).saturating_add(1);
    true
}

fn state_needs_tick(state: &InteractionState) -> bool {
    match state {
        InteractionState::NodeDrag(drag) => !drag.release_pending,
        InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => {
            !drag.release_pending
        }
        InteractionState::Marquee(marquee) => !marquee.release_pending,
        InteractionState::Idle
        | InteractionState::Armed(_)
        | InteractionState::Pan(_)
        | InteractionState::Connection(ConnectionInteraction::Searching(_))
        | InteractionState::KeyboardConnection(_)
        | InteractionState::Committing(_)
        | InteractionState::EdgeActive(_)
        | InteractionState::EdgePickPending(_) => false,
    }
}

fn edge_pick_terminal_damage() -> InteractionDamage {
    InteractionDamage {
        presentation: true,
        edges: true,
        ..InteractionDamage::default()
    }
}

fn edge_action_damage() -> InteractionDamage {
    InteractionDamage {
        presentation: true,
        edges: true,
        ..InteractionDamage::default()
    }
}

fn selection_delete_damage() -> InteractionDamage {
    InteractionDamage {
        presentation: true,
        nodes: true,
        ports: true,
        edges: true,
        selection: true,
        ..InteractionDamage::default()
    }
}

fn damage_for_state(state: &InteractionState) -> InteractionDamage {
    match state {
        InteractionState::NodeDrag(_)
        | InteractionState::Committing(CommitInteraction::NodeMove(_)) => {
            InteractionDamage::node_drag()
        }
        InteractionState::Connection(_)
        | InteractionState::KeyboardConnection(_)
        | InteractionState::Committing(CommitInteraction::Connection(_))
        | InteractionState::Committing(CommitInteraction::KeyboardConnection(_)) => {
            InteractionDamage::connection()
        }
        InteractionState::Committing(CommitInteraction::EdgeDelete(_)) => edge_action_damage(),
        InteractionState::Committing(CommitInteraction::SelectionDelete(_)) => {
            selection_delete_damage()
        }
        InteractionState::Idle
        | InteractionState::Armed(_)
        | InteractionState::Pan(_)
        | InteractionState::Marquee(_)
        | InteractionState::EdgeActive(_) => InteractionDamage::presentation(),
        InteractionState::EdgePickPending(_) => edge_pick_terminal_damage(),
    }
}

fn state_uses_owner(state: &InteractionState, owner: InteractionOwner) -> bool {
    match state {
        InteractionState::Armed(armed) => match &armed.target {
            ArmedTarget::NodeDrag(start) => match owner {
                InteractionOwner::Node(node) => start.cohort.contains_member(node),
                InteractionOwner::Port(_) | InteractionOwner::Edge(_) => false,
            },
            ArmedTarget::Connection(start) => connection_uses_owner(start, owner),
            ArmedTarget::Pan { .. } | ArmedTarget::Marquee { .. } => false,
        },
        InteractionState::NodeDrag(drag) => match owner {
            InteractionOwner::Node(node) => drag.cohort.contains_member(node),
            InteractionOwner::Port(_) | InteractionOwner::Edge(_) => false,
        },
        InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => {
            connection_uses_owner(&drag.start, owner)
        }
        InteractionState::Connection(ConnectionInteraction::Searching(search)) => {
            connection_uses_owner(&search.start, owner)
        }
        InteractionState::KeyboardConnection(connection) => {
            connection_binding_uses_owner(&connection.start.binding, owner)
        }
        InteractionState::Committing(CommitInteraction::Connection(commit)) => {
            connection_uses_owner(&commit.start, owner)
        }
        InteractionState::Committing(CommitInteraction::KeyboardConnection(commit)) => {
            connection_binding_uses_owner(&commit.start.binding, owner)
                || owner == InteractionOwner::Port(commit.target.instance())
        }
        InteractionState::Committing(CommitInteraction::NodeMove(commit)) => match owner {
            InteractionOwner::Node(node) => commit.cohort.contains_member(node),
            InteractionOwner::Port(_) | InteractionOwner::Edge(_) => false,
        },
        InteractionState::Committing(CommitInteraction::EdgeDelete(_)) => false,
        InteractionState::Committing(CommitInteraction::SelectionDelete(_)) => false,
        InteractionState::EdgeActive(edge) => owner == InteractionOwner::Edge(edge.instance),
        InteractionState::Idle
        | InteractionState::Pan(_)
        | InteractionState::Marquee(_)
        | InteractionState::EdgePickPending(_) => false,
    }
}

fn state_is_current(
    state: &InteractionState,
    snapshot: &GraphSnapshot,
    policy: GraphRuntimePolicy,
) -> bool {
    match state {
        InteractionState::Armed(armed) => match &armed.target {
            ArmedTarget::NodeDrag(start) => cohort_is_current(&start.cohort, snapshot),
            ArmedTarget::Connection(start) => start.binding.is_interactive(snapshot, policy),
            ArmedTarget::Pan { .. } | ArmedTarget::Marquee { .. } => true,
        },
        InteractionState::NodeDrag(drag) => cohort_is_current(&drag.cohort, snapshot),
        InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => {
            drag.start.binding.is_interactive(snapshot, policy)
        }
        InteractionState::Connection(ConnectionInteraction::Searching(search)) => {
            search.start.binding.is_interactive(snapshot, policy)
        }
        InteractionState::KeyboardConnection(connection) => {
            connection.start.binding.is_interactive(snapshot, policy)
        }
        InteractionState::Committing(CommitInteraction::Connection(commit)) => {
            commit.start.binding.is_interactive(snapshot, policy)
                && commit.target.is_current(snapshot)
        }
        InteractionState::Committing(CommitInteraction::KeyboardConnection(commit)) => {
            commit.start.binding.is_interactive(snapshot, policy)
                && snapshot.port_instance_is_current(commit.target.instance())
        }
        InteractionState::Committing(CommitInteraction::NodeMove(commit)) => {
            cohort_is_current(&commit.cohort, snapshot)
        }
        InteractionState::Committing(CommitInteraction::EdgeDelete(commit)) => {
            snapshot.edge_instance_is_current(commit.instance)
        }
        InteractionState::EdgeActive(edge) => snapshot.edge_instance_is_current(edge.instance),
        InteractionState::Idle
        | InteractionState::Pan(_)
        | InteractionState::Marquee(_)
        | InteractionState::Committing(CommitInteraction::SelectionDelete(_))
        | InteractionState::EdgePickPending(_) => true,
    }
}

fn cohort_is_current(cohort: &NodeMoveCohort, snapshot: &GraphSnapshot) -> bool {
    cohort.members().iter().all(|member| {
        snapshot.node_lifetime_is_current(*member)
            && snapshot.graph().node(&member.node()).is_some_and(|node| {
                !node.hidden && node.selectable.unwrap_or(true) && node.draggable.unwrap_or(true)
            })
    })
}

fn connection_uses_owner(start: &ConnectionInteractionStart, owner: InteractionOwner) -> bool {
    connection_binding_uses_owner(&start.binding, owner)
}

fn connection_binding_uses_owner(
    binding: &ConnectionGestureBinding,
    owner: InteractionOwner,
) -> bool {
    match owner {
        InteractionOwner::Node(node) => binding.owner() == node,
        InteractionOwner::Port(port) => {
            binding.fixed_port_instance() == port || binding.detached_port_instance() == Some(port)
        }
        InteractionOwner::Edge(edge) => binding.reconnect_edge() == Some(edge),
    }
}

#[cfg(test)]
mod tests;
