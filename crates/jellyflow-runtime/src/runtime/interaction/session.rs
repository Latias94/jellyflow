use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, NodeId, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::rules::EdgeEndpoint;
use crate::runtime::adapter_protocol::{PublishedEdgeRoute, ViewDelta, ViewPoint, ViewSize};
use crate::runtime::commands::TopologyEdit;
use crate::runtime::geometry::HandlePosition;
use crate::runtime::graph_runtime::{GraphRevision, PortConnectionDragIntent};
use crate::runtime::view_session::{
    ConnectionProbeQuery, ConnectionProbeQueryOutcome, EdgePickQuery, EdgePickQueryOutcome,
    ViewGeneration, ViewId,
};
use crate::runtime::viewport::{ViewportPanRequest, ViewportTransform, pan_viewport};

use super::intent::{
    ConnectionTopology, EdgeDeletionIntent, FreshGraphMutation, RelativeNodeMoveIntent,
    SelectionBaseline,
};
use super::navigation::{AlignmentSnapLocks, AlignmentSnapQuery, AlignmentSnapQueryOutcome};
use super::presentation::{
    AlignmentGuide, CanvasInteractionPhase, CanvasInteractionPresentation, ConnectionPresentation,
    ConnectionSearchStatus, InteractionPosition, InteractionTerminalReason, NodeDragPresentation,
};
use super::{ConnectionGestureIntent, ConnectionProbe};

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
    pub connection_radius_in_view: f32,
    pub edge_pick_radius_in_view: f32,
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
            connection_radius_in_view: 12.0,
            edge_pick_radius_in_view: 10.0,
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
            && self.connection_radius_in_view.is_finite()
            && self.connection_radius_in_view > 0.0
            && self.edge_pick_radius_in_view.is_finite()
            && self.edge_pick_radius_in_view > 0.0
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
pub struct InteractionGeneration(u64);

impl InteractionGeneration {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InteractionQueryId(u64);

impl InteractionQueryId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InteractionTicketId(u64);

impl InteractionTicketId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    Replace,
    Add,
    Toggle,
}

#[derive(Debug, Clone)]
pub struct NodeDragStart {
    clicked: NodeId,
    nodes: Arc<[NodeId]>,
    snap_origin: CanvasPoint,
    selection_mode: SelectionMode,
    clicked_was_selected: bool,
}

impl NodeDragStart {
    pub(crate) fn new(
        clicked: NodeId,
        nodes: impl IntoIterator<Item = NodeId>,
        snap_origin: CanvasPoint,
        selection_mode: SelectionMode,
    ) -> Option<Self> {
        if !snap_origin.is_finite() {
            return None;
        }
        let mut nodes = nodes.into_iter().collect::<Vec<_>>();
        let clicked_was_selected = nodes.contains(&clicked);
        nodes.sort_unstable();
        nodes.dedup();
        if !nodes.contains(&clicked) {
            nodes.push(clicked);
            nodes.sort_unstable();
        }
        Some(Self {
            clicked,
            nodes: nodes.into(),
            snap_origin,
            selection_mode,
            clicked_was_selected,
        })
    }

    pub fn clicked(&self) -> NodeId {
        self.clicked
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionInteractionStart {
    owner_node: NodeId,
    intent: ConnectionGestureIntent,
    fixed_anchor: CanvasPoint,
    fixed_outward: HandlePosition,
    allow_create_from_wire: bool,
}

impl ConnectionInteractionStart {
    pub(crate) fn new(
        owner_node: NodeId,
        intent: ConnectionGestureIntent,
        fixed_anchor: CanvasPoint,
        fixed_outward: HandlePosition,
        allow_create_from_wire: bool,
    ) -> Option<Self> {
        fixed_anchor.is_finite().then_some(Self {
            owner_node,
            intent,
            fixed_anchor,
            fixed_outward,
            allow_create_from_wire,
        })
    }

    /// Resolves a connection start from revision-bound runtime port policy facts.
    pub fn from_port_drag_intent(
        drag_intent: PortConnectionDragIntent,
        new_edge: EdgeId,
        mode: NodeGraphConnectionMode,
        fixed_anchor: CanvasPoint,
        fixed_outward: HandlePosition,
        allow_create_from_wire: bool,
    ) -> Option<Self> {
        let intent = Self::intent_from_port_drag(drag_intent, new_edge, mode)?;
        Self::new(
            drag_intent.node(),
            intent,
            fixed_anchor,
            fixed_outward,
            allow_create_from_wire,
        )
    }

    /// Resolves the renderer-neutral gesture intent carried by runtime-issued port drag facts.
    pub fn intent_from_port_drag(
        drag_intent: PortConnectionDragIntent,
        new_edge: EdgeId,
        mode: NodeGraphConnectionMode,
    ) -> Option<ConnectionGestureIntent> {
        if !drag_intent.can_begin_connection() {
            return None;
        }
        match drag_intent {
            PortConnectionDragIntent::Connect { facts } => Some(ConnectionGestureIntent::connect(
                new_edge,
                facts.port(),
                mode,
            )),
            PortConnectionDragIntent::Reconnect { facts, candidate } => {
                Some(ConnectionGestureIntent::reconnect(
                    candidate.edge(),
                    candidate.endpoint(),
                    facts.port(),
                    candidate.opposite_port(),
                    mode,
                ))
            }
            PortConnectionDragIntent::BlockedOccupied { .. } => None,
        }
    }

    /// Resolves an endpoint reconnect from authoritative graph topology and published route facts.
    pub fn reconnect_from_graph_route(
        graph: &Graph,
        route: &PublishedEdgeRoute,
        endpoint: EdgeEndpoint,
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
        let graph_edge = graph.edge(&edge)?;
        let (detached_port, fixed_port, fixed_endpoint) = match endpoint {
            EdgeEndpoint::From => (
                graph_edge.from,
                graph_edge.to,
                route.facts().endpoints.target,
            ),
            EdgeEndpoint::To => (
                graph_edge.to,
                graph_edge.from,
                route.facts().endpoints.source,
            ),
        };
        let owner_node = graph.port(&detached_port)?.node;
        let intent =
            ConnectionGestureIntent::reconnect(edge, endpoint, detached_port, fixed_port, mode);
        Self::new(
            owner_node,
            intent,
            fixed_endpoint.point,
            fixed_endpoint.position,
            allow_create_from_wire,
        )
    }

    pub fn owner_node(self) -> NodeId {
        self.owner_node
    }

    pub fn intent(self) -> ConnectionGestureIntent {
        self.intent
    }
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
        edge: EdgeId,
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
    Node(NodeId),
    Port(PortId),
    Edge(EdgeId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectionMutation {
    Node {
        node: NodeId,
        mode: SelectionMode,
    },
    Edge {
        edge: EdgeId,
        mode: SelectionMode,
    },
    Marquee {
        bounds: CanvasRect,
        mode: SelectionMode,
        baseline: SelectionBaseline,
    },
    Clear,
    Restore(SelectionBaseline),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewportMutation {
    PanByView(ViewDelta),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionSearchRequest {
    generation: InteractionGeneration,
    intent: ConnectionGestureIntent,
    position: InteractionPosition,
}

impl ConnectionSearchRequest {
    pub fn generation(self) -> InteractionGeneration {
        self.generation
    }

    pub fn source(self) -> PortId {
        self.intent.fixed_port()
    }

    pub fn mode(self) -> NodeGraphConnectionMode {
        self.intent.mode()
    }

    pub fn intent(self) -> ConnectionGestureIntent {
        self.intent
    }

    pub fn position(self) -> InteractionPosition {
        self.position
    }
}

/// One create-from-wire selection that must add topology and its connection atomically.
#[derive(Debug, Clone)]
pub struct ConnectionSearchSelection {
    target: PortId,
    validated_revision: GraphRevision,
    topology: ConnectionTopology,
}

impl ConnectionSearchSelection {
    /// Creates a selection from application-owned node topology.
    ///
    /// Empty topology is rejected because an existing graph port must use the ordinary connection
    /// probe path rather than bypassing geometry revalidation through create-from-wire.
    pub fn new(
        target: PortId,
        validated_revision: GraphRevision,
        topology: TopologyEdit,
    ) -> Option<Self> {
        (!topology.is_empty()).then(|| Self {
            target,
            validated_revision,
            topology: ConnectionTopology::new(topology),
        })
    }

    pub fn target(&self) -> PortId {
        self.target
    }

    pub fn validated_revision(&self) -> GraphRevision {
        self.validated_revision
    }

    pub fn topology(&self) -> &TopologyEdit {
        self.topology.edit()
    }
}

#[derive(Debug)]
pub enum CanvasInteractionEffect {
    SubmitGraph {
        ticket: InteractionTicketId,
        mutation: FreshGraphMutation,
    },
    Selection(SelectionMutation),
    Viewport(ViewportMutation),
    Search(ConnectionSearchRequest),
}

#[derive(Debug)]
pub enum CanvasInteractionQuery {
    AlignmentSnap {
        id: InteractionQueryId,
        generation: InteractionGeneration,
        query: AlignmentSnapQuery,
    },
    ConnectionProbe {
        id: InteractionQueryId,
        generation: InteractionGeneration,
        query: ConnectionProbeQuery,
    },
    EdgePick {
        id: InteractionQueryId,
        query: EdgePickQuery,
    },
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
    queries: Vec<CanvasInteractionQuery>,
    effects: Vec<CanvasInteractionEffect>,
    damage: InteractionDamage,
    tick_demand: InteractionTickDemand,
}

/// Ordered adapter work emitted by one semantic interaction transition.
///
/// Adapters must apply every `pre_query_effects` entry before executing `queries`. A query may be
/// normalized against the viewport produced by an earlier effect in the same transition.
#[derive(Debug)]
pub struct CanvasInteractionDispatch {
    pub pre_query_effects: Vec<CanvasInteractionEffect>,
    pub queries: Vec<CanvasInteractionQuery>,
    pub damage: InteractionDamage,
    pub tick_demand: InteractionTickDemand,
}

impl CanvasInteractionUpdate {
    pub fn queries(&self) -> &[CanvasInteractionQuery] {
        &self.queries
    }

    pub fn effects(&self) -> &[CanvasInteractionEffect] {
        &self.effects
    }

    pub fn into_dispatch(self) -> CanvasInteractionDispatch {
        CanvasInteractionDispatch {
            pre_query_effects: self.effects,
            queries: self.queries,
            damage: self.damage,
            tick_demand: self.tick_demand,
        }
    }

    pub fn damage(&self) -> InteractionDamage {
        self.damage
    }

    pub fn tick_demand(&self) -> InteractionTickDemand {
        self.tick_demand
    }

    fn active(mut self) -> Self {
        self.tick_demand = InteractionTickDemand::WhileActive;
        self
    }

    fn with_damage(mut self, damage: InteractionDamage) -> Self {
        self.damage.merge(damage);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSettlement {
    Applied,
    Noop,
    Rejected,
    Conflict,
    TransportFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CanvasInteractionError {
    #[error("canvas interaction configuration is invalid")]
    InvalidConfiguration,
    #[error("the normalized interaction position is invalid")]
    InvalidPosition,
    #[error("another semantic canvas interaction is active")]
    AlreadyActive,
    #[error("canvas interaction identity space is exhausted")]
    IdentityExhausted,
    #[error("the interaction session is closed")]
    Closed,
    #[error("ordinary connection commits require a fresh target probe")]
    ConnectionCommitRequiresProbe,
    #[error("create-from-wire commits require a fresh graph revision")]
    ConnectionCommitRequiresFreshRevision,
    #[error("this connection interaction does not allow create-from-wire search")]
    ConnectionSearchUnavailable,
    #[error("no connection search is active")]
    ConnectionSearchInactive,
    #[error("the connection search generation is stale")]
    ConnectionSearchStaleGeneration,
}

#[derive(Debug)]
pub struct CanvasInteractionSession {
    view: ViewId,
    view_generation: ViewGeneration,
    config: CanvasInteractionConfig,
    next_generation: Option<u64>,
    next_query: Option<u64>,
    next_ticket: Option<u64>,
    state: InteractionState,
    pending_edge_pick: Option<PendingHoverEdgePick>,
    hovered_edge: Option<EdgeId>,
    terminal: Option<InteractionTerminalReason>,
    closed: bool,
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
    Committing(CommitInteraction),
    EdgeActive(EdgeInteraction),
    EdgePickPending(CanvasEdgePickInteraction),
}

#[derive(Debug)]
struct ArmedInteraction {
    generation: InteractionGeneration,
    target: ArmedTarget,
    start: InteractionPosition,
    max_view_distance_squared: f32,
}

#[derive(Debug)]
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
    nodes: Arc<[NodeId]>,
    snap_origin: CanvasPoint,
    start: InteractionPosition,
    pointer: InteractionPosition,
    raw_delta: CanvasPoint,
    delta: CanvasPoint,
    guides: Arc<[AlignmentGuide]>,
    alignment_locks: AlignmentSnapLocks,
    pending_alignment_query: Option<InteractionQueryId>,
    release_pending: bool,
}

#[derive(Debug)]
struct PanInteraction {
    generation: InteractionGeneration,
    pointer: InteractionPosition,
}

#[derive(Debug)]
struct MarqueeInteraction {
    generation: InteractionGeneration,
    start: InteractionPosition,
    pointer: InteractionPosition,
    selection_mode: SelectionMode,
    baseline: SelectionBaseline,
}

#[derive(Debug)]
enum ConnectionInteraction {
    Dragging(ConnectionDragInteraction),
    Searching(ConnectionSearchInteraction),
}

#[derive(Debug)]
struct ConnectionDragInteraction {
    generation: InteractionGeneration,
    start: ConnectionInteractionStart,
    pointer: InteractionPosition,
    probe: ConnectionProbe,
    pending_query: Option<InteractionQueryId>,
    stale_query_restarts: u8,
    release_pending: bool,
}

#[derive(Debug)]
struct ConnectionSearchInteraction {
    generation: InteractionGeneration,
    start: ConnectionInteractionStart,
    pointer: InteractionPosition,
    probe: ConnectionProbe,
    status: ConnectionSearchStatus,
}

#[derive(Debug)]
enum CommitInteraction {
    Connection(ConnectionCommitInteraction),
    NodeDrag(NodeCommitInteraction),
    EdgeDelete(EdgeDeleteCommitInteraction),
}

#[derive(Debug)]
struct ConnectionCommitInteraction {
    generation: InteractionGeneration,
    start: ConnectionInteractionStart,
    pointer: InteractionPosition,
    probe: ConnectionProbe,
    ticket: InteractionTicketId,
    target: PortId,
    return_to_search: bool,
    topology: ConnectionTopology,
    freshness: ConnectionCommitFreshness,
    stale_query_restarts: u8,
}

#[derive(Debug, Default)]
enum ConnectionCommitFreshness {
    #[default]
    Initial,
    Probing(InteractionQueryId),
    Revalidated,
}

#[derive(Debug)]
struct NodeCommitInteraction {
    generation: InteractionGeneration,
    nodes: Arc<[NodeId]>,
    delta: CanvasPoint,
    guides: Arc<[AlignmentGuide]>,
    ticket: InteractionTicketId,
}

#[derive(Debug)]
struct EdgeDeleteCommitInteraction {
    generation: InteractionGeneration,
    edge: EdgeId,
    ticket: InteractionTicketId,
}

#[derive(Debug)]
struct EdgeInteraction {
    generation: InteractionGeneration,
    edge: EdgeId,
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
    Complete(Option<EdgeId>),
    BudgetExhausted,
    GeometryIndeterminate,
}

impl CanvasInteractionSession {
    pub fn new(
        view: ViewId,
        view_generation: ViewGeneration,
        config: CanvasInteractionConfig,
    ) -> Result<Self, CanvasInteractionError> {
        if !config.is_valid() {
            return Err(CanvasInteractionError::InvalidConfiguration);
        }
        Ok(Self {
            view,
            view_generation,
            config,
            next_generation: Some(1),
            next_query: Some(1),
            next_ticket: Some(1),
            state: InteractionState::Idle,
            pending_edge_pick: None,
            hovered_edge: None,
            terminal: None,
            closed: false,
        })
    }

    pub fn view(&self) -> ViewId {
        self.view
    }

    pub fn view_generation(&self) -> ViewGeneration {
        self.view_generation
    }

    pub fn config(&self) -> CanvasInteractionConfig {
        self.config
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
                Arc::clone(&drag.nodes),
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
            InteractionState::Committing(commit) => match commit {
                CommitInteraction::Connection(connection) => {
                    CanvasInteractionPresentation::new(CanvasInteractionPhase::Committing)
                        .with_connection(connection_presentation_from_commit(connection))
                }
                CommitInteraction::NodeDrag(node) => {
                    CanvasInteractionPresentation::new(CanvasInteractionPhase::Committing)
                        .with_node_drag(node_drag_presentation(
                            node.generation,
                            Arc::clone(&node.nodes),
                            node.delta,
                            Arc::clone(&node.guides),
                            false,
                            Some(node.ticket),
                        ))
                }
                CommitInteraction::EdgeDelete(edge) => {
                    CanvasInteractionPresentation::new(CanvasInteractionPhase::Committing)
                        .with_active_edge(edge.edge)
                }
            },
            InteractionState::EdgeActive(edge) => {
                CanvasInteractionPresentation::new(CanvasInteractionPhase::EdgeActive)
                    .with_active_edge(edge.edge)
            }
            InteractionState::EdgePickPending(_) => {
                CanvasInteractionPresentation::new(CanvasInteractionPhase::Armed)
            }
        };
        presentation = presentation
            .with_generation(state_generation(&self.state))
            .with_hovered_edge(self.hovered_edge)
            .with_terminal(self.terminal);
        presentation
    }

    /// Returns the authoritative create-from-wire request for the active search generation.
    pub fn search_request(&self) -> Option<ConnectionSearchRequest> {
        match &self.state {
            InteractionState::Connection(ConnectionInteraction::Searching(search)) => {
                Some(search_request(search))
            }
            InteractionState::Committing(CommitInteraction::Connection(commit))
                if commit.return_to_search =>
            {
                Some(connection_search_request(
                    commit.generation,
                    commit.start,
                    commit.pointer,
                ))
            }
            _ => None,
        }
    }

    pub fn press(
        &mut self,
        target: CanvasPressTarget,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_available()?;
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        self.terminal = None;
        self.pending_edge_pick = None;
        let mut update = CanvasInteractionUpdate::default();
        match target {
            CanvasPressTarget::NativeControl => return Ok(update),
            CanvasPressTarget::Edge {
                edge,
                selection_mode,
            } => {
                let generation = self.allocate_generation()?;
                self.state = InteractionState::EdgeActive(EdgeInteraction { generation, edge });
                update.effects.push(CanvasInteractionEffect::Selection(
                    SelectionMutation::Edge {
                        edge,
                        mode: selection_mode,
                    },
                ));
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
                    return Ok(update.with_damage(InteractionDamage::presentation()));
                };
                if self.hovered_edge.take().is_some() {
                    update.damage.edges = true;
                }
                update.queries.push(CanvasInteractionQuery::EdgePick {
                    id: query,
                    query: EdgePickQuery::new(
                        position.view(),
                        self.config.edge_pick_radius_in_view,
                        self.config.query_entry_budget,
                    ),
                });
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
                self.state = InteractionState::Armed(ArmedInteraction {
                    generation,
                    target: ArmedTarget::NodeDrag(start),
                    start: position,
                    max_view_distance_squared: 0.0,
                });
                update.damage.merge(InteractionDamage::presentation());
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
        Ok(update)
    }

    /// Selects one actionable node without synthesizing a pointer drag or arming movement.
    ///
    /// Keyboard and accessibility adapters use this path so a selectable but non-draggable node
    /// remains activatable and native child controls do not inherit drag semantics.
    pub fn activate_node(
        &mut self,
        node: NodeId,
        selection_mode: SelectionMode,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_available()?;
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        self.terminal = None;
        self.pending_edge_pick = None;
        Ok(CanvasInteractionUpdate {
            effects: vec![CanvasInteractionEffect::Selection(
                SelectionMutation::Node {
                    node,
                    mode: selection_mode,
                },
            )],
            damage: InteractionDamage {
                presentation: true,
                selection: true,
                ..InteractionDamage::default()
            },
            ..CanvasInteractionUpdate::default()
        })
    }

    /// Activates create-from-wire search without synthesizing a pointer drag.
    ///
    /// Keyboard and accessibility adapters use this path after resolving a real graph port. The
    /// resulting search owns the same generation, presentation, effects, and settlement behavior
    /// as a pointer release on empty canvas.
    pub fn activate_connection_search(
        &mut self,
        start: ConnectionInteractionStart,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_available()?;
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        if !start.allow_create_from_wire {
            return Err(CanvasInteractionError::ConnectionSearchUnavailable);
        }
        let generation = self.allocate_generation()?;
        self.terminal = None;
        self.pending_edge_pick = None;
        let search = ConnectionSearchInteraction {
            generation,
            start,
            pointer: position,
            probe: ConnectionProbe::None,
            status: ConnectionSearchStatus::Loading,
        };
        let mut update = CanvasInteractionUpdate::default();
        update
            .effects
            .push(CanvasInteractionEffect::Search(search_request(&search)));
        update.damage.merge(InteractionDamage::connection());
        self.state = InteractionState::Connection(ConnectionInteraction::Searching(search));
        Ok(update)
    }

    /// Requests deletion through the authoritative interaction and command-settlement pipeline.
    ///
    /// A route whose effective policy denies deletion is a no-op and allocates no ticket. Once
    /// admitted, the mutation binds to a fresh graph snapshot before enqueueing; a retired edge
    /// therefore settles as a no-op instead of producing a stale disconnect command.
    pub fn delete_edge(
        &mut self,
        route: &PublishedEdgeRoute,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_available()?;
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        let Some(intent) = EdgeDeletionIntent::from_route(route) else {
            return Ok(CanvasInteractionUpdate::default());
        };
        self.terminal = None;
        self.pending_edge_pick = None;
        let generation = self.allocate_generation()?;
        let Some(ticket) = self.allocate_ticket() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return Ok(CanvasInteractionUpdate::default().with_damage(edge_action_damage()));
        };
        if self.hovered_edge == Some(intent.edge()) {
            self.hovered_edge = None;
        }
        let mut update = CanvasInteractionUpdate::default().with_damage(edge_action_damage());
        update.effects.push(CanvasInteractionEffect::SubmitGraph {
            ticket,
            mutation: FreshGraphMutation::DeleteEdge(intent),
        });
        self.state = InteractionState::Committing(CommitInteraction::EdgeDelete(
            EdgeDeleteCommitInteraction {
                generation,
                edge: intent.edge(),
                ticket,
            },
        ));
        Ok(update)
    }

    pub fn request_edge_pick(
        &mut self,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        if !matches!(self.state, InteractionState::Idle) {
            return Err(CanvasInteractionError::AlreadyActive);
        }
        let Some(id) = self.allocate_query() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return Ok(CanvasInteractionUpdate::default());
        };
        self.pending_edge_pick = Some(PendingHoverEdgePick {
            id,
            position,
            stale_query_restarts: 0,
        });
        let mut update = CanvasInteractionUpdate::default();
        update.queries.push(CanvasInteractionQuery::EdgePick {
            id,
            query: EdgePickQuery::new(
                position.view(),
                self.config.edge_pick_radius_in_view,
                self.config.query_entry_budget,
            ),
        });
        Ok(update)
    }

    /// Clears adapter-local edge hover when the pointer leaves the graph-interactive surface.
    pub fn clear_edge_hover(&mut self) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        self.pending_edge_pick = None;
        if self.hovered_edge.take().is_none() {
            return Ok(CanvasInteractionUpdate::default());
        }
        Ok(
            CanvasInteractionUpdate::default().with_damage(InteractionDamage {
                presentation: true,
                edges: true,
                ..InteractionDamage::default()
            }),
        )
    }

    pub fn resolve_edge_pick(
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
            return Ok(update);
        }
        match outcome {
            EdgePickQueryOutcome::Incomplete { cursor, .. } => {
                update.queries.push(CanvasInteractionQuery::EdgePick {
                    id,
                    query: EdgePickQuery::resume(cursor, self.config.query_entry_budget),
                });
            }
            EdgePickQueryOutcome::Complete { pick, .. } => {
                self.pending_edge_pick = None;
                let edge = pick.map(|pick| pick.edge());
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
                self.pending_edge_pick = None;
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::BudgetExhausted);
                update.damage = InteractionDamage {
                    presentation: true,
                    edges: true,
                    ..InteractionDamage::default()
                };
            }
            EdgePickQueryOutcome::GeometryIndeterminate { .. } => {
                self.pending_edge_pick = None;
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::GeometryIndeterminate);
                update.damage = InteractionDamage {
                    presentation: true,
                    edges: true,
                    ..InteractionDamage::default()
                };
            }
        }
        Ok(update)
    }

    fn resolve_canvas_press_edge_pick(
        &mut self,
        id: InteractionQueryId,
        outcome: EdgePickQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let resolution = match outcome {
            EdgePickQueryOutcome::Incomplete { cursor, .. } => CanvasEdgePickResolution::Continue(
                EdgePickQuery::resume(cursor, self.config.query_entry_budget),
            ),
            EdgePickQueryOutcome::Complete { pick, .. } => {
                CanvasEdgePickResolution::Complete(pick.map(|pick| pick.edge()))
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
        let state = std::mem::take(&mut self.state);
        let InteractionState::EdgePickPending(pending) = state else {
            self.state = state;
            return Ok(CanvasInteractionUpdate::default());
        };
        if pending.query != id {
            self.state = InteractionState::EdgePickPending(pending);
            return Ok(CanvasInteractionUpdate::default());
        }

        let mut update = CanvasInteractionUpdate::default();
        self.state = match resolution {
            CanvasEdgePickResolution::Continue(query) => {
                update
                    .queries
                    .push(CanvasInteractionQuery::EdgePick { id, query });
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
        if state_needs_tick(&self.state) {
            update = update.active();
        }
        Ok(update)
    }

    fn finish_canvas_press_edge_pick(
        &mut self,
        pending: CanvasEdgePickInteraction,
        edge: Option<EdgeId>,
        update: &mut CanvasInteractionUpdate,
    ) -> Result<InteractionState, CanvasInteractionError> {
        self.hovered_edge = edge;
        let Some(edge) = edge else {
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

        update.effects.push(CanvasInteractionEffect::Selection(
            SelectionMutation::Edge {
                edge,
                mode: pending.fallback.selection_mode(),
            },
        ));
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
                edge,
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
                let _ = self.activate_armed(armed, pending.pointer, update)?;
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
    /// emitting a first-page query. Adapters must call this only for a restartable stale-cursor
    /// error reported by their view query transport. Late errors for superseded query identities
    /// are ignored.
    pub fn restart_stale_query(
        &mut self,
        id: InteractionQueryId,
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
                return Ok(
                    CanvasInteractionUpdate::default().with_damage(edge_pick_terminal_damage())
                );
            }
            let Some(next) = self.allocate_query() else {
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
                return Ok(
                    CanvasInteractionUpdate::default().with_damage(edge_pick_terminal_damage())
                );
            };
            pending.query = next;
            let mut update = CanvasInteractionUpdate::default();
            update.queries.push(CanvasInteractionQuery::EdgePick {
                id: next,
                query: EdgePickQuery::new(
                    pending.press.view(),
                    self.config.edge_pick_radius_in_view,
                    self.config.query_entry_budget,
                ),
            });
            self.state = InteractionState::EdgePickPending(pending);
            return Ok(update);
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
                return Ok(
                    CanvasInteractionUpdate::default().with_damage(edge_pick_terminal_damage())
                );
            }
            let Some(next) = self.allocate_query() else {
                self.hovered_edge = None;
                self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
                return Ok(
                    CanvasInteractionUpdate::default().with_damage(InteractionDamage {
                        presentation: true,
                        edges: true,
                        ..InteractionDamage::default()
                    }),
                );
            };
            self.pending_edge_pick = Some(PendingHoverEdgePick {
                id: next,
                position: pending.position,
                stale_query_restarts,
            });
            let mut update = CanvasInteractionUpdate::default();
            update.queries.push(CanvasInteractionQuery::EdgePick {
                id: next,
                query: EdgePickQuery::new(
                    pending.position.view(),
                    self.config.edge_pick_radius_in_view,
                    self.config.query_entry_budget,
                ),
            });
            return Ok(update);
        }

        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        self.state = match state {
            InteractionState::Connection(ConnectionInteraction::Dragging(mut drag))
                if drag.pending_query == Some(id) =>
            {
                if !reserve_stale_query_restart(&mut drag.stale_query_restarts) {
                    self.terminal = Some(InteractionTerminalReason::BudgetExhausted);
                    update.damage.merge(InteractionDamage::connection());
                    InteractionState::Idle
                } else {
                    drag.pending_query = None;
                    self.queue_connection_probe(&mut drag, &mut update)?;
                    if drag.pending_query.is_none() {
                        update.damage.merge(InteractionDamage::connection());
                        return Ok(update);
                    }
                    InteractionState::Connection(ConnectionInteraction::Dragging(drag))
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
                    let Some(next) = self.allocate_query() else {
                        update.damage.merge(InteractionDamage::connection());
                        self.state = self.reject_connection_commit(
                            commit,
                            InteractionTerminalReason::IdentityExhausted,
                        );
                        return Ok(update);
                    };
                    commit.freshness = ConnectionCommitFreshness::Probing(next);
                    update
                        .queries
                        .push(CanvasInteractionQuery::ConnectionProbe {
                            id: next,
                            generation: commit.generation,
                            query: ConnectionProbeQuery::new(
                                commit.start.intent,
                                commit.pointer.view(),
                                self.config.connection_radius_in_view,
                                self.config.query_entry_budget,
                            ),
                        });
                    InteractionState::Committing(CommitInteraction::Connection(commit))
                }
            }
            other => other,
        };
        if state_needs_tick(&self.state) {
            update = update.active();
        }
        Ok(update)
    }

    pub fn move_to(
        &mut self,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
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
                let delta = view_delta(pan.pointer.view(), position.view());
                pan.pointer = position;
                if delta.x != 0.0 || delta.y != 0.0 {
                    update.effects.push(CanvasInteractionEffect::Viewport(
                        ViewportMutation::PanByView(delta),
                    ));
                    update.damage.viewport = true;
                }
                InteractionState::Pan(pan)
            }
            InteractionState::Marquee(mut marquee) => {
                marquee.pointer = position;
                update.effects.push(CanvasInteractionEffect::Selection(
                    SelectionMutation::Marquee {
                        bounds: rect_between(marquee.start.canvas(), position.canvas()),
                        mode: marquee.selection_mode,
                        baseline: marquee.baseline.clone(),
                    },
                ));
                update.damage = InteractionDamage {
                    presentation: true,
                    selection: true,
                    ..InteractionDamage::default()
                };
                InteractionState::Marquee(marquee)
            }
            InteractionState::Connection(ConnectionInteraction::Dragging(mut drag)) => {
                drag.pointer = position;
                drag.release_pending = false;
                self.start_connection_probe(&mut drag, &mut update)?;
                update.damage.merge(InteractionDamage::connection());
                InteractionState::Connection(ConnectionInteraction::Dragging(drag))
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
        if state_needs_tick(&self.state) {
            update = update.active();
        }
        Ok(update)
    }

    pub fn release(
        &mut self,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        self.state = match state {
            InteractionState::Armed(mut armed) => {
                armed.max_view_distance_squared = armed
                    .max_view_distance_squared
                    .max(view_distance_squared(armed.start.view(), position.view()));
                self.finish_armed_click(&armed, &mut update);
                update.damage.merge(InteractionDamage::presentation());
                InteractionState::Idle
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
                self.start_connection_probe(&mut drag, &mut update)?;
                update.damage.merge(InteractionDamage::connection());
                InteractionState::Connection(ConnectionInteraction::Dragging(drag))
            }
            InteractionState::EdgePickPending(mut pending) => {
                pending.pointer = position;
                pending.max_view_distance_squared = pending
                    .max_view_distance_squared
                    .max(view_distance_squared(pending.press.view(), position.view()));
                pending.release_pending = true;
                InteractionState::EdgePickPending(pending)
            }
            InteractionState::Pan(_)
            | InteractionState::Marquee(_)
            | InteractionState::EdgeActive(_) => {
                update.damage.merge(InteractionDamage::presentation());
                InteractionState::Idle
            }
            other => other,
        };
        if state_needs_tick(&self.state) {
            update = update.active();
        }
        Ok(update)
    }

    pub fn resolve_connection_probe(
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
                        update
                            .queries
                            .push(CanvasInteractionQuery::ConnectionProbe {
                                id,
                                generation: drag.generation,
                                query: ConnectionProbeQuery::resume(
                                    cursor,
                                    self.config.query_entry_budget,
                                ),
                            });
                        InteractionState::Connection(ConnectionInteraction::Dragging(drag))
                    }
                    ConnectionProbeQueryOutcome::BudgetExhausted { .. } => {
                        self.terminal = Some(InteractionTerminalReason::BudgetExhausted);
                        update.damage.merge(InteractionDamage::connection());
                        InteractionState::Idle
                    }
                    ConnectionProbeQueryOutcome::Complete { probe, .. } => {
                        drag.pending_query = None;
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
                        update
                            .queries
                            .push(CanvasInteractionQuery::ConnectionProbe {
                                id,
                                generation: commit.generation,
                                query: ConnectionProbeQuery::resume(
                                    cursor,
                                    self.config.query_entry_budget,
                                ),
                            });
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
                            valid @ ConnectionProbe::Valid { revision, target } => {
                                commit.probe = valid;
                                commit.target = target.port();
                                commit.freshness = ConnectionCommitFreshness::Revalidated;
                                update.effects.push(CanvasInteractionEffect::SubmitGraph {
                                    ticket: commit.ticket,
                                    mutation: FreshGraphMutation::Connection {
                                        intent: commit.start.intent,
                                        target: commit.target,
                                        validated_revision: revision,
                                        topology: commit.topology.clone(),
                                    },
                                });
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
        if state_needs_tick(&self.state) {
            update = update.active();
        }
        Ok(update)
    }

    pub fn search_ready(
        &mut self,
        generation: InteractionGeneration,
        result_count: usize,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.set_search_status(
            generation,
            if result_count == 0 {
                ConnectionSearchStatus::Empty
            } else {
                ConnectionSearchStatus::Ready { result_count }
            },
        )
    }

    pub fn search_failed(
        &mut self,
        generation: InteractionGeneration,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.set_search_status(generation, ConnectionSearchStatus::Error)
    }

    pub fn retry_search(
        &mut self,
        generation: InteractionGeneration,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let search = match &mut self.state {
            InteractionState::Connection(ConnectionInteraction::Searching(search))
                if search.generation == generation =>
            {
                search
            }
            InteractionState::Connection(ConnectionInteraction::Searching(_)) => {
                return Err(CanvasInteractionError::ConnectionSearchStaleGeneration);
            }
            _ => return Err(CanvasInteractionError::ConnectionSearchInactive),
        };
        let mut update = CanvasInteractionUpdate::default();
        search.status = ConnectionSearchStatus::Loading;
        update
            .effects
            .push(CanvasInteractionEffect::Search(search_request(search)));
        update.damage.merge(InteractionDamage::connection());
        Ok(update)
    }

    pub fn select_search_result(
        &mut self,
        generation: InteractionGeneration,
        selection: ConnectionSearchSelection,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        self.state = match state {
            InteractionState::Connection(ConnectionInteraction::Searching(search))
                if search.generation == generation =>
            {
                self.begin_connection_commit(
                    search.generation,
                    search.start,
                    search.pointer,
                    ConnectionProbe::None,
                    selection.target,
                    selection.validated_revision,
                    selection.topology,
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
        if state_needs_tick(&self.state) {
            update = update.active();
        }
        Ok(update)
    }

    pub fn settle(
        &mut self,
        ticket: InteractionTicketId,
        settlement: CommandSettlement,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        self.state = match state {
            InteractionState::Committing(CommitInteraction::Connection(commit))
                if commit.ticket == ticket =>
            {
                update.damage.merge(InteractionDamage::connection());
                self.finish_connection_commit(commit, settlement)
            }
            InteractionState::Committing(CommitInteraction::NodeDrag(commit))
                if commit.ticket == ticket =>
            {
                self.terminal = Some(settlement_terminal(settlement));
                update.damage.merge(InteractionDamage::node_drag());
                InteractionState::Idle
            }
            InteractionState::Committing(CommitInteraction::EdgeDelete(commit))
                if commit.ticket == ticket =>
            {
                self.terminal = Some(settlement_terminal(settlement));
                update.damage.merge(edge_action_damage());
                InteractionState::Idle
            }
            other => other,
        };
        if state_needs_tick(&self.state) {
            update = update.active();
        }
        Ok(update)
    }

    /// Starts the fresh target probe for an ordinary connection held behind a command prefix.
    ///
    /// The session owns the query identity and resumes incomplete work through
    /// [`Self::resolve_connection_probe`]. A matching create-from-wire commit is rejected because
    /// its prospective target has no published view geometry yet. Stale tickets are ignored.
    pub fn request_connection_commit_revalidation(
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
                if !commit.topology.is_empty() {
                    error = Some(CanvasInteractionError::ConnectionCommitRequiresFreshRevision);
                    InteractionState::Committing(CommitInteraction::Connection(commit))
                } else if !matches!(commit.freshness, ConnectionCommitFreshness::Initial) {
                    InteractionState::Committing(CommitInteraction::Connection(commit))
                } else if let Some(id) = self.allocate_query() {
                    commit.stale_query_restarts = 0;
                    commit.freshness = ConnectionCommitFreshness::Probing(id);
                    update
                        .queries
                        .push(CanvasInteractionQuery::ConnectionProbe {
                            id,
                            generation: commit.generation,
                            query: ConnectionProbeQuery::new(
                                commit.start.intent,
                                commit.pointer.view(),
                                self.config.connection_radius_in_view,
                                self.config.query_entry_budget,
                            ),
                        });
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
        if state_needs_tick(&self.state) {
            update = update.active();
        }
        Ok(update)
    }

    /// Refreshes a create-from-wire commit after its adapter-owned command prefix settles.
    ///
    /// The selected port belongs to prospective topology and therefore cannot be re-probed through
    /// current view geometry. This method only accepts a matching commit that retained non-empty
    /// topology, updates its one fresh revision, and re-emits the same atomic topology plus
    /// connection under the original ticket. Ordinary connections must use the probe API.
    pub fn refresh_create_from_wire_commit_revision(
        &mut self,
        ticket: InteractionTicketId,
        revision: GraphRevision,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        let mut error = None;
        self.state = match state {
            InteractionState::Committing(CommitInteraction::Connection(mut commit))
                if commit.ticket == ticket =>
            {
                if commit.topology.is_empty() {
                    error = Some(CanvasInteractionError::ConnectionCommitRequiresProbe);
                } else if matches!(commit.freshness, ConnectionCommitFreshness::Initial) {
                    commit.freshness = ConnectionCommitFreshness::Revalidated;
                    update.effects.push(CanvasInteractionEffect::SubmitGraph {
                        ticket,
                        mutation: FreshGraphMutation::Connection {
                            intent: commit.start.intent,
                            target: commit.target,
                            validated_revision: revision,
                            topology: commit.topology.clone(),
                        },
                    });
                }
                InteractionState::Committing(CommitInteraction::Connection(commit))
            }
            other => other,
        };
        if let Some(error) = error {
            return Err(error);
        }
        if state_needs_tick(&self.state) {
            update = update.active();
        }
        Ok(update)
    }

    pub fn resolve_alignment_snap(
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
        if state_needs_tick(&self.state) {
            update = update.active();
        }
        Ok(update)
    }

    pub fn tick(
        &mut self,
        elapsed_seconds: f32,
        viewport: ViewSize,
        transform: ViewportTransform,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 || !viewport.is_positive_finite()
        {
            return Ok(CanvasInteractionUpdate::default());
        }
        let pointer = active_pointer(&self.state);
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
                update.effects.push(CanvasInteractionEffect::Viewport(
                    ViewportMutation::PanByView(delta),
                ));
                update.damage.viewport = true;
                if let Some(next_transform) =
                    pan_viewport(transform, ViewportPanRequest::new(delta))
                {
                    self.reproject_active_pointer(next_transform, &mut update)?;
                }
            }
            update = update.active();
        }
        Ok(update)
    }

    fn reproject_active_pointer(
        &mut self,
        transform: ViewportTransform,
        update: &mut CanvasInteractionUpdate,
    ) -> Result<(), CanvasInteractionError> {
        let state = std::mem::take(&mut self.state);
        self.state = match state {
            InteractionState::NodeDrag(mut drag) => {
                let view = drag.pointer.view();
                drag.pointer = InteractionPosition::from_view(view, transform)
                    .ok_or(CanvasInteractionError::InvalidPosition)?;
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
                let view = drag.pointer.view();
                drag.pointer = InteractionPosition::from_view(view, transform)
                    .ok_or(CanvasInteractionError::InvalidPosition)?;
                self.start_connection_probe(&mut drag, update)?;
                update.damage.merge(InteractionDamage::connection());
                InteractionState::Connection(ConnectionInteraction::Dragging(drag))
            }
            other => other,
        };
        Ok(())
    }

    pub fn cancel(&mut self) -> CanvasInteractionUpdate {
        self.pending_edge_pick = None;
        if matches!(self.state, InteractionState::Idle) {
            return CanvasInteractionUpdate::default();
        }
        let baseline = match &self.state {
            InteractionState::Marquee(marquee) => Some(marquee.baseline.clone()),
            _ => None,
        };
        let mut damage = damage_for_state(&self.state);
        self.state = InteractionState::Idle;
        self.terminal = Some(InteractionTerminalReason::Cancelled);
        let mut update = CanvasInteractionUpdate::default();
        if let Some(baseline) = baseline {
            update.effects.push(CanvasInteractionEffect::Selection(
                SelectionMutation::Restore(baseline),
            ));
            damage.selection = true;
        }
        update.with_damage(damage)
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
        if !state_uses_owner(&self.state, owner) {
            return update;
        }
        let damage = damage_for_state(&self.state);
        self.state = InteractionState::Idle;
        self.terminal = Some(InteractionTerminalReason::OwnerRetired);
        update.with_damage(damage)
    }

    pub fn close(&mut self) -> CanvasInteractionUpdate {
        if self.closed {
            return CanvasInteractionUpdate::default();
        }
        let damage = damage_for_state(&self.state);
        self.pending_edge_pick = None;
        self.hovered_edge = None;
        self.state = InteractionState::Idle;
        self.closed = true;
        self.terminal = Some(InteractionTerminalReason::ViewClosed);
        CanvasInteractionUpdate::default().with_damage(damage)
    }

    fn ensure_available(&self) -> Result<(), CanvasInteractionError> {
        self.ensure_open()
    }

    fn ensure_open(&self) -> Result<(), CanvasInteractionError> {
        if self.closed {
            Err(CanvasInteractionError::Closed)
        } else {
            Ok(())
        }
    }

    fn allocate_generation(&mut self) -> Result<InteractionGeneration, CanvasInteractionError> {
        let Some(next) = self.next_generation else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return Err(CanvasInteractionError::IdentityExhausted);
        };
        self.next_generation = next.checked_add(1);
        Ok(InteractionGeneration(next))
    }

    fn allocate_query(&mut self) -> Option<InteractionQueryId> {
        let next = self.next_query?;
        self.next_query = next.checked_add(1);
        Some(InteractionQueryId(next))
    }

    fn allocate_ticket(&mut self) -> Option<InteractionTicketId> {
        let next = self.next_ticket?;
        self.next_ticket = next.checked_add(1);
        Some(InteractionTicketId(next))
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
                if !start.clicked_was_selected {
                    update.effects.push(CanvasInteractionEffect::Selection(
                        SelectionMutation::Node {
                            node: start.clicked,
                            mode: match start.selection_mode {
                                SelectionMode::Replace => SelectionMode::Replace,
                                SelectionMode::Add | SelectionMode::Toggle => SelectionMode::Add,
                            },
                        },
                    ));
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
                    nodes: start.nodes,
                    snap_origin: start.snap_origin,
                    start: armed.start,
                    pointer: position,
                    raw_delta,
                    delta: raw_delta,
                    guides: Arc::default(),
                    alignment_locks: AlignmentSnapLocks::default(),
                    pending_alignment_query: None,
                    release_pending: false,
                };
                self.start_alignment_snap(&mut drag, update);
                InteractionState::NodeDrag(drag)
            }
            ArmedTarget::Pan { .. } => {
                let delta = view_delta(armed.start.view(), position.view());
                update.effects.push(CanvasInteractionEffect::Viewport(
                    ViewportMutation::PanByView(delta),
                ));
                update.damage.viewport = true;
                InteractionState::Pan(PanInteraction {
                    generation: armed.generation,
                    pointer: position,
                })
            }
            ArmedTarget::Marquee {
                selection_mode,
                baseline,
            } => {
                let bounds = rect_between(armed.start.canvas(), position.canvas());
                update.effects.push(CanvasInteractionEffect::Selection(
                    SelectionMutation::Marquee {
                        bounds,
                        mode: selection_mode,
                        baseline: baseline.clone(),
                    },
                ));
                update.damage = InteractionDamage {
                    presentation: true,
                    selection: true,
                    ..InteractionDamage::default()
                };
                InteractionState::Marquee(MarqueeInteraction {
                    generation: armed.generation,
                    start: armed.start,
                    pointer: position,
                    selection_mode,
                    baseline,
                })
            }
            ArmedTarget::Connection(start) => {
                let mut drag = ConnectionDragInteraction {
                    generation: armed.generation,
                    start,
                    pointer: position,
                    probe: ConnectionProbe::None,
                    pending_query: None,
                    stale_query_restarts: 0,
                    release_pending: false,
                };
                self.start_connection_probe(&mut drag, update)?;
                update.damage.merge(InteractionDamage::connection());
                InteractionState::Connection(ConnectionInteraction::Dragging(drag))
            }
        })
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
                node: start.clicked,
                mode: start.selection_mode,
            }),
            ArmedTarget::Pan {
                selection_mode: Some(SelectionMode::Replace),
            }
            | ArmedTarget::Marquee {
                selection_mode: SelectionMode::Replace,
                ..
            } => Some(SelectionMutation::Clear),
            ArmedTarget::Pan { .. } | ArmedTarget::Marquee { .. } | ArmedTarget::Connection(_) => {
                None
            }
        };
        if let Some(mutation) = mutation {
            update
                .effects
                .push(CanvasInteractionEffect::Selection(mutation));
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
        update.queries.push(CanvasInteractionQuery::AlignmentSnap {
            id,
            generation: drag.generation,
            query: AlignmentSnapQuery::new(
                self.view,
                self.view_generation,
                drag.generation,
                Arc::clone(&drag.nodes),
                drag.raw_delta,
                config.acquire_tolerance_in_view(),
                config.release_tolerance_in_view(),
                self.config.query_entry_budget,
                drag.alignment_locks,
            ),
        });
        true
    }

    fn commit_node_drag(
        &mut self,
        drag: NodeDragInteraction,
        update: &mut CanvasInteractionUpdate,
    ) -> InteractionState {
        let Some(intent) = RelativeNodeMoveIntent::new(drag.nodes.iter().copied(), drag.delta)
        else {
            return InteractionState::Idle;
        };
        let Some(ticket) = self.allocate_ticket() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return InteractionState::Idle;
        };
        update.effects.push(CanvasInteractionEffect::SubmitGraph {
            ticket,
            mutation: FreshGraphMutation::MoveNodes(intent),
        });
        InteractionState::Committing(CommitInteraction::NodeDrag(NodeCommitInteraction {
            generation: drag.generation,
            nodes: drag.nodes,
            delta: drag.delta,
            guides: drag.guides,
            ticket,
        }))
    }

    fn start_connection_probe(
        &mut self,
        drag: &mut ConnectionDragInteraction,
        update: &mut CanvasInteractionUpdate,
    ) -> Result<(), CanvasInteractionError> {
        drag.stale_query_restarts = 0;
        self.queue_connection_probe(drag, update)
    }

    fn queue_connection_probe(
        &mut self,
        drag: &mut ConnectionDragInteraction,
        update: &mut CanvasInteractionUpdate,
    ) -> Result<(), CanvasInteractionError> {
        let Some(id) = self.allocate_query() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return Ok(());
        };
        drag.pending_query = Some(id);
        update
            .queries
            .push(CanvasInteractionQuery::ConnectionProbe {
                id,
                generation: drag.generation,
                query: ConnectionProbeQuery::new(
                    drag.start.intent,
                    drag.pointer.view(),
                    self.config.connection_radius_in_view,
                    self.config.query_entry_budget,
                ),
            });
        Ok(())
    }

    fn finish_connection_release(
        &mut self,
        drag: ConnectionDragInteraction,
        update: &mut CanvasInteractionUpdate,
    ) -> InteractionState {
        match drag.probe.target() {
            Some(target) if drag.probe.is_valid() => {
                let validated_revision = drag
                    .probe
                    .revision()
                    .expect("a valid connection probe always carries its graph revision");
                self.begin_connection_commit(
                    drag.generation,
                    drag.start,
                    drag.pointer,
                    drag.probe,
                    target.port(),
                    validated_revision,
                    ConnectionTopology::ordinary(),
                    false,
                    update,
                )
            }
            None if drag.start.allow_create_from_wire => {
                let search = ConnectionSearchInteraction {
                    generation: drag.generation,
                    start: drag.start,
                    pointer: drag.pointer,
                    probe: drag.probe,
                    status: ConnectionSearchStatus::Loading,
                };
                update
                    .effects
                    .push(CanvasInteractionEffect::Search(search_request(&search)));
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
        target: PortId,
        validated_revision: GraphRevision,
        topology: ConnectionTopology,
        return_to_search: bool,
        update: &mut CanvasInteractionUpdate,
    ) -> InteractionState {
        let Some(ticket) = self.allocate_ticket() else {
            self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
            return InteractionState::Idle;
        };
        update.effects.push(CanvasInteractionEffect::SubmitGraph {
            ticket,
            mutation: FreshGraphMutation::Connection {
                intent: start.intent,
                target,
                validated_revision,
                topology: topology.clone(),
            },
        });
        InteractionState::Committing(CommitInteraction::Connection(ConnectionCommitInteraction {
            generation,
            start,
            pointer,
            probe,
            ticket,
            target,
            return_to_search,
            topology,
            freshness: ConnectionCommitFreshness::Initial,
            stale_query_restarts: 0,
        }))
    }

    fn finish_connection_commit(
        &mut self,
        commit: ConnectionCommitInteraction,
        settlement: CommandSettlement,
    ) -> InteractionState {
        match settlement {
            CommandSettlement::Applied | CommandSettlement::Noop => {
                self.terminal = Some(settlement_terminal(settlement));
                InteractionState::Idle
            }
            CommandSettlement::Rejected
            | CommandSettlement::Conflict
            | CommandSettlement::TransportFailure => {
                self.reject_connection_commit(commit, settlement_terminal(settlement))
            }
        }
    }

    fn reject_connection_commit(
        &mut self,
        commit: ConnectionCommitInteraction,
        reason: InteractionTerminalReason,
    ) -> InteractionState {
        self.terminal = Some(reason);
        if commit.return_to_search {
            InteractionState::Connection(ConnectionInteraction::Searching(
                ConnectionSearchInteraction {
                    generation: commit.generation,
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
        generation: InteractionGeneration,
        status: ConnectionSearchStatus,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let search = match &mut self.state {
            InteractionState::Connection(ConnectionInteraction::Searching(search))
                if search.generation == generation =>
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
        Ok(update)
    }
}

fn connection_presentation_from_drag(drag: &ConnectionDragInteraction) -> ConnectionPresentation {
    ConnectionPresentation::new(
        drag.generation,
        drag.start.intent,
        drag.start.fixed_anchor,
        drag.start.fixed_outward,
        drag.pointer,
        drag.probe.clone(),
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
        search.start.intent,
        search.start.fixed_anchor,
        search.start.fixed_outward,
        search.pointer,
        search.probe.clone(),
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
        commit.start.intent,
        commit.start.fixed_anchor,
        commit.start.fixed_outward,
        commit.pointer,
        commit.probe.clone(),
        commit
            .return_to_search
            .then_some(ConnectionSearchStatus::Loading),
        false,
        Some(commit.ticket),
    )
}

fn node_drag_presentation(
    generation: InteractionGeneration,
    nodes: Arc<[NodeId]>,
    delta: CanvasPoint,
    guides: Arc<[AlignmentGuide]>,
    release_pending: bool,
    ticket: Option<InteractionTicketId>,
) -> NodeDragPresentation {
    NodeDragPresentation::new(generation, nodes, delta, guides, release_pending, ticket)
}

fn search_request(search: &ConnectionSearchInteraction) -> ConnectionSearchRequest {
    connection_search_request(search.generation, search.start, search.pointer)
}

fn connection_search_request(
    generation: InteractionGeneration,
    start: ConnectionInteractionStart,
    position: InteractionPosition,
) -> ConnectionSearchRequest {
    ConnectionSearchRequest {
        generation,
        intent: start.intent,
        position,
    }
}

fn settlement_terminal(settlement: CommandSettlement) -> InteractionTerminalReason {
    match settlement {
        CommandSettlement::Applied => InteractionTerminalReason::Applied,
        CommandSettlement::Noop => InteractionTerminalReason::Noop,
        CommandSettlement::Rejected => InteractionTerminalReason::Rejected,
        CommandSettlement::Conflict => InteractionTerminalReason::Conflict,
        CommandSettlement::TransportFailure => InteractionTerminalReason::TransportFailure,
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
        InteractionState::NodeDrag(_)
        | InteractionState::Armed(_)
        | InteractionState::EdgePickPending(_)
        | InteractionState::Idle
        | InteractionState::Pan(_)
        | InteractionState::Marquee(_)
        | InteractionState::Connection(ConnectionInteraction::Dragging(_))
        | InteractionState::Connection(ConnectionInteraction::Searching(_))
        | InteractionState::Committing(_)
        | InteractionState::EdgeActive(_) => None,
    }
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
        InteractionState::Committing(CommitInteraction::Connection(commit)) => {
            Some(commit.generation)
        }
        InteractionState::Committing(CommitInteraction::NodeDrag(commit)) => {
            Some(commit.generation)
        }
        InteractionState::Committing(CommitInteraction::EdgeDelete(commit)) => {
            Some(commit.generation)
        }
        InteractionState::EdgeActive(edge) => Some(edge.generation),
        InteractionState::EdgePickPending(pending) => Some(pending.generation),
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
        InteractionState::Idle
        | InteractionState::Armed(_)
        | InteractionState::Pan(_)
        | InteractionState::Marquee(_)
        | InteractionState::Connection(ConnectionInteraction::Searching(_))
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

fn damage_for_state(state: &InteractionState) -> InteractionDamage {
    match state {
        InteractionState::NodeDrag(_)
        | InteractionState::Committing(CommitInteraction::NodeDrag(_)) => {
            InteractionDamage::node_drag()
        }
        InteractionState::Connection(_)
        | InteractionState::Committing(CommitInteraction::Connection(_)) => {
            InteractionDamage::connection()
        }
        InteractionState::Committing(CommitInteraction::EdgeDelete(_)) => edge_action_damage(),
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
            ArmedTarget::NodeDrag(start) => owner == InteractionOwner::Node(start.clicked),
            ArmedTarget::Connection(start) => connection_uses_owner(*start, owner),
            ArmedTarget::Pan { .. } | ArmedTarget::Marquee { .. } => false,
        },
        InteractionState::NodeDrag(drag) => match owner {
            InteractionOwner::Node(node) => drag.nodes.contains(&node),
            InteractionOwner::Port(_) | InteractionOwner::Edge(_) => false,
        },
        InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => {
            connection_uses_owner(drag.start, owner)
        }
        InteractionState::Connection(ConnectionInteraction::Searching(search)) => {
            connection_uses_owner(search.start, owner)
        }
        InteractionState::Committing(CommitInteraction::Connection(commit)) => {
            connection_uses_owner(commit.start, owner)
        }
        InteractionState::Committing(CommitInteraction::NodeDrag(commit)) => match owner {
            InteractionOwner::Node(node) => commit.nodes.contains(&node),
            InteractionOwner::Port(_) | InteractionOwner::Edge(_) => false,
        },
        InteractionState::Committing(CommitInteraction::EdgeDelete(_)) => false,
        InteractionState::EdgeActive(edge) => owner == InteractionOwner::Edge(edge.edge),
        InteractionState::Idle
        | InteractionState::Pan(_)
        | InteractionState::Marquee(_)
        | InteractionState::EdgePickPending(_) => false,
    }
}

fn connection_uses_owner(start: ConnectionInteractionStart, owner: InteractionOwner) -> bool {
    match owner {
        InteractionOwner::Node(node) => start.owner_node == node,
        InteractionOwner::Port(port) => {
            start.intent.fixed_port() == port || start.intent.detached_port() == Some(port)
        }
        InteractionOwner::Edge(edge) => start.intent.edge() == edge,
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{
        CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, EdgeReconnectable,
        EdgeReconnectableEndpoint, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
        PortCapacity, PortDirection, PortId, PortKey, PortKind,
    };
    use jellyflow_core::interaction::NodeGraphConnectionMode;

    use super::*;
    use crate::rules::EdgeEndpoint;
    use crate::runtime::geometry::{
        EdgeEndpointPosition, EdgePosition, HandlePosition, resolve_edge_route_path,
    };
    use crate::runtime::interaction::AlignmentSnapWork;
    use crate::runtime::interaction::ConnectionProbeTarget;
    use crate::runtime::view_session::{ConnectionProbeWork, EdgePickQueryOutcome, EdgePickWork};

    #[test]
    fn native_node_activation_selects_without_arming_drag_or_allocating_generation() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let node = NodeId::from_u128(10);

        let update = session
            .activate_node(node, SelectionMode::Replace)
            .expect("idle native activation");
        let [
            CanvasInteractionEffect::Selection(SelectionMutation::Node {
                node: selected,
                mode,
            }),
        ] = update.effects()
        else {
            panic!("native node activation must emit one node selection");
        };
        assert_eq!(*selected, node);
        assert_eq!(*mode, SelectionMode::Replace);
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(session.presentation().generation(), None);
        assert!(session.presentation().node_drag().is_none());

        session
            .press(CanvasPressTarget::Pan, position(0.0, 0.0))
            .expect("arm another interaction");
        assert!(matches!(
            session.activate_node(node, SelectionMode::Add),
            Err(CanvasInteractionError::AlreadyActive)
        ));
    }

    #[test]
    fn connection_stays_armed_below_threshold_and_release_is_a_noop() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let pressed = session
            .press(connection_target(false), position(0.0, 0.0))
            .unwrap();
        assert_eq!(pressed.tick_demand(), InteractionTickDemand::None);

        let moved = session.move_to(position(3.0, 0.0)).unwrap();
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Armed
        );
        assert!(moved.queries().is_empty());
        assert_eq!(moved.tick_demand(), InteractionTickDemand::None);

        let released = session.release(position(3.0, 0.0)).unwrap();
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert!(released.effects().is_empty());
    }

    #[test]
    fn independent_drag_thresholds_cross_only_beyond_the_exact_view_distance() {
        assert!(!threshold_crossed_distance(
            view_distance_squared(ViewPoint::new(0.0, 0.0), ViewPoint::new(4.0, 0.0)),
            4.0,
        ));
        assert!(threshold_crossed_distance(
            view_distance_squared(ViewPoint::new(0.0, 0.0), ViewPoint::new(4.01, 0.0)),
            4.0,
        ));

        let node = NodeId::from_u128(10);
        let start = NodeDragStart::new(
            node,
            [node],
            CanvasPoint { x: 0.0, y: 0.0 },
            SelectionMode::Replace,
        )
        .unwrap();
        let mut session = test_session(CanvasInteractionConfig::default());
        let pressed = session
            .press(CanvasPressTarget::NodeDrag(start), position(0.0, 0.0))
            .unwrap();
        assert_eq!(pressed.tick_demand(), InteractionTickDemand::None);
        let boundary = session.move_to(position(4.0, 0.0)).unwrap();
        assert_eq!(boundary.tick_demand(), InteractionTickDemand::None);
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Armed
        );
        let dragged = session.move_to(position(4.01, 0.0)).unwrap();
        assert_eq!(dragged.tick_demand(), InteractionTickDemand::WhileActive);
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::NodeDrag
        );
    }

    #[test]
    fn node_click_tolerance_is_independent_from_the_drag_threshold() {
        let node = NodeId::from_u128(10);
        let start = || {
            NodeDragStart::new(node, [], CanvasPoint::default(), SelectionMode::Replace).unwrap()
        };

        let mut click = test_session(CanvasInteractionConfig::default());
        click
            .press(CanvasPressTarget::NodeDrag(start()), position(0.0, 0.0))
            .unwrap();
        let selected = click.release(position(2.0, 0.0)).unwrap();
        assert!(matches!(
            selected.effects(),
            [CanvasInteractionEffect::Selection(SelectionMutation::Node {
                node: selected,
                mode: SelectionMode::Replace,
            })] if *selected == node
        ));

        let mut dead_zone = test_session(CanvasInteractionConfig::default());
        dead_zone
            .press(CanvasPressTarget::NodeDrag(start()), position(0.0, 0.0))
            .unwrap();
        let released = dead_zone.release(position(2.01, 0.0)).unwrap();
        assert!(released.effects().is_empty());
        assert_eq!(
            dead_zone.presentation().phase(),
            CanvasInteractionPhase::Idle
        );
    }

    #[test]
    fn blank_canvas_click_clears_only_for_replace_mode() {
        for mode in [
            SelectionMode::Replace,
            SelectionMode::Add,
            SelectionMode::Toggle,
        ] {
            let mut session = test_session(CanvasInteractionConfig::default());
            let pressed = session
                .press(canvas_marquee_target(mode), position(8.0, 8.0))
                .unwrap();
            session
                .resolve_canvas_press_edge_resolution(
                    edge_query_id(&pressed),
                    CanvasEdgePickResolution::Complete(None),
                )
                .unwrap();
            let released = session.release(position(8.0, 8.0)).unwrap();
            match mode {
                SelectionMode::Replace => assert!(matches!(
                    released.effects(),
                    [CanvasInteractionEffect::Selection(SelectionMutation::Clear)]
                )),
                SelectionMode::Add | SelectionMode::Toggle => {
                    assert!(released.effects().is_empty())
                }
            }
        }
    }

    #[test]
    fn toggle_node_click_toggles_but_toggle_drag_adds_without_deselecting() {
        let node = NodeId::from_u128(10);
        let start =
            || NodeDragStart::new(node, [], CanvasPoint::default(), SelectionMode::Toggle).unwrap();

        let mut click = test_session(CanvasInteractionConfig::default());
        click
            .press(CanvasPressTarget::NodeDrag(start()), position(0.0, 0.0))
            .unwrap();
        let selected = click.release(position(0.0, 0.0)).unwrap();
        assert!(matches!(
            selected.effects(),
            [CanvasInteractionEffect::Selection(
                SelectionMutation::Node {
                    mode: SelectionMode::Toggle,
                    ..
                }
            )]
        ));

        let mut drag = test_session(CanvasInteractionConfig::default());
        drag.press(CanvasPressTarget::NodeDrag(start()), position(0.0, 0.0))
            .unwrap();
        let activated = drag.move_to(position(8.0, 0.0)).unwrap();
        assert!(matches!(
            activated.effects(),
            [CanvasInteractionEffect::Selection(
                SelectionMutation::Node {
                    mode: SelectionMode::Add,
                    ..
                }
            )]
        ));
        assert_eq!(
            drag.presentation().phase(),
            CanvasInteractionPhase::NodeDrag
        );
    }

    #[test]
    fn asynchronous_search_and_commit_states_do_not_request_animation_ticks() {
        let mut search = test_session(CanvasInteractionConfig::default());
        let activated = search
            .activate_connection_search(connection_start(true), position(20.0, 10.0))
            .unwrap();
        assert_eq!(activated.tick_demand(), InteractionTickDemand::None);
        let ready = search
            .search_ready(
                search
                    .presentation()
                    .generation()
                    .expect("search generation"),
                1,
            )
            .unwrap();
        assert_eq!(ready.tick_demand(), InteractionTickDemand::None);

        let route =
            published_edge_route(EdgeId::from_u128(24), true, EdgeReconnectable::Bool(true));
        let mut deletion = test_session(CanvasInteractionConfig::default());
        let submitted = deletion.delete_edge(&route).unwrap();
        assert_eq!(submitted.tick_demand(), InteractionTickDemand::None);
    }

    #[test]
    fn direct_connection_reprobes_on_release_and_keeps_preview_until_terminal_receipt() {
        let mut session = test_session(CanvasInteractionConfig::default());
        session
            .press(connection_target(false), position(0.0, 0.0))
            .unwrap();
        let moved = session.move_to(position(8.0, 0.0)).unwrap();
        let move_query = connection_query_id(&moved);
        session
            .resolve_connection_probe(move_query, complete(valid_probe(target_port())))
            .unwrap();

        let released = session.release(position(20.0, 0.0)).unwrap();
        let release_query = connection_query_id(&released);
        assert_ne!(move_query, release_query);
        assert!(
            session
                .presentation()
                .connection()
                .expect("drag presentation")
                .is_release_pending()
        );

        let committing = session
            .resolve_connection_probe(release_query, complete(valid_probe(target_port())))
            .unwrap();
        let ticket = submit_ticket(&committing);
        let presentation = session.presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Committing);
        let connection = presentation.connection().expect("commit preview");
        assert_eq!(connection.ticket(), Some(ticket));
        assert_eq!(connection.preview_target(), CanvasPoint { x: 24.0, y: 8.0 });

        session.settle(ticket, CommandSettlement::Rejected).unwrap();
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Rejected)
        );

        let late = session.settle(ticket, CommandSettlement::Applied).unwrap();
        assert!(late.effects().is_empty());
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    }

    #[test]
    fn released_connection_at_auto_pan_edge_preserves_its_final_probe() {
        let mut session = test_session(CanvasInteractionConfig::default());
        session
            .press(connection_target(false), position(100.0, 40.0))
            .unwrap();
        let moved = session.move_to(position(190.0, 40.0)).unwrap();
        session
            .resolve_connection_probe(
                connection_query_id(&moved),
                complete(valid_probe(target_port())),
            )
            .unwrap();

        let released = session.release(position(195.0, 40.0)).unwrap();
        let release_query = connection_query_id(&released);
        let transform = ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();
        let tick = session
            .tick(0.1, ViewSize::new(200.0, 120.0), transform)
            .unwrap();

        assert!(tick.effects().is_empty());
        assert!(tick.queries().is_empty());
        assert!(
            session
                .presentation()
                .connection()
                .expect("released connection preview")
                .is_release_pending()
        );

        let committing = session
            .resolve_connection_probe(release_query, complete(valid_probe(target_port())))
            .unwrap();
        assert!(matches!(
            committing.effects(),
            [CanvasInteractionEffect::SubmitGraph { .. }]
        ));
    }

    #[test]
    fn stale_drag_probe_restart_replaces_identity_and_ignores_the_old_page() {
        let mut session = test_session(CanvasInteractionConfig {
            query_entry_budget: 1,
            ..CanvasInteractionConfig::default()
        });
        session
            .press(connection_target(false), position(0.0, 0.0))
            .unwrap();
        let started = session.move_to(position(12.0, 4.0)).unwrap();
        let stale_id = connection_query_id(&started);

        let restarted = session.restart_stale_query(stale_id).unwrap();
        let fresh_id = connection_query_id(&restarted);
        assert_ne!(fresh_id, stale_id);
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );

        let late = session
            .resolve_connection_probe(stale_id, complete(valid_probe(target_port())))
            .unwrap();
        assert!(late.queries().is_empty());
        assert!(late.effects().is_empty());
        let resolved = session
            .resolve_connection_probe(fresh_id, complete(valid_probe(target_port())))
            .unwrap();
        assert!(resolved.effects().is_empty());
        assert!(
            session
                .presentation()
                .connection()
                .expect("active connection preview")
                .probe()
                .is_valid()
        );
    }

    #[test]
    fn stale_release_probe_restart_preserves_release_pending_until_commit() {
        let mut session = test_session(CanvasInteractionConfig {
            query_entry_budget: 1,
            ..CanvasInteractionConfig::default()
        });
        session
            .press(connection_target(false), position(0.0, 0.0))
            .unwrap();
        let moved = session.move_to(position(8.0, 0.0)).unwrap();
        session
            .resolve_connection_probe(
                connection_query_id(&moved),
                complete(valid_probe(target_port())),
            )
            .unwrap();
        let released = session.release(position(20.0, 0.0)).unwrap();
        let stale_id = connection_query_id(&released);

        let restarted = session.restart_stale_query(stale_id).unwrap();
        let fresh_id = connection_query_id(&restarted);
        assert_ne!(fresh_id, stale_id);
        assert!(
            session
                .presentation()
                .connection()
                .expect("release preview")
                .is_release_pending()
        );

        let committing = session
            .resolve_connection_probe(fresh_id, complete(valid_probe(target_port())))
            .unwrap();
        assert!(matches!(
            committing.effects(),
            [CanvasInteractionEffect::SubmitGraph { .. }]
        ));
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Committing
        );
    }

    #[test]
    fn stale_commit_revalidation_restart_reuses_ticket_and_keeps_budget_terminal() {
        let mut session = test_session(CanvasInteractionConfig {
            query_entry_budget: 1,
            ..CanvasInteractionConfig::default()
        });
        let ticket = begin_direct_connection_commit(&mut session);
        let requested = session
            .request_connection_commit_revalidation(ticket)
            .unwrap();
        let stale_id = connection_query_id(&requested);
        let restarted = session.restart_stale_query(stale_id).unwrap();
        let fresh_id = connection_query_id(&restarted);
        assert_ne!(fresh_id, stale_id);
        assert_eq!(
            session
                .presentation()
                .connection()
                .expect("commit preview")
                .ticket(),
            Some(ticket)
        );

        let exhausted = session
            .resolve_connection_probe(
                fresh_id,
                ConnectionProbeQueryOutcome::BudgetExhausted {
                    work: ConnectionProbeWork::default(),
                },
            )
            .unwrap();
        assert!(exhausted.effects().is_empty());
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::BudgetExhausted)
        );
    }

    #[test]
    fn stale_edge_pick_restart_replaces_identity_without_relaxing_budget_failure() {
        let mut session = test_session(CanvasInteractionConfig {
            query_entry_budget: 1,
            ..CanvasInteractionConfig::default()
        });
        let requested = session.request_edge_pick(position(20.0, 10.0)).unwrap();
        let stale_id = edge_query_id(&requested);
        let restarted = session.restart_stale_query(stale_id).unwrap();
        let fresh_id = edge_query_id(&restarted);
        assert_ne!(fresh_id, stale_id);

        let late = session
            .resolve_edge_pick(
                stale_id,
                EdgePickQueryOutcome::Complete {
                    pick: None,
                    work: EdgePickWork::default(),
                },
            )
            .unwrap();
        assert!(late.queries().is_empty());
        let exhausted = session
            .resolve_edge_pick(
                fresh_id,
                EdgePickQueryOutcome::BudgetExhausted {
                    work: EdgePickWork::default(),
                },
            )
            .unwrap();
        assert!(exhausted.effects().is_empty());
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::BudgetExhausted)
        );
    }

    #[test]
    fn repeated_stale_drag_probe_restarts_are_bounded_per_pointer_query() {
        let mut session = test_session(CanvasInteractionConfig::default());
        session
            .press(connection_target(false), position(0.0, 0.0))
            .unwrap();
        let started = session.move_to(position(12.0, 4.0)).unwrap();
        let mut current = connection_query_id(&started);

        for _ in 0..MAX_STALE_QUERY_RESTARTS {
            let restarted = session.restart_stale_query(current).unwrap();
            current = connection_query_id(&restarted);
        }
        let exhausted = session.restart_stale_query(current).unwrap();

        assert!(exhausted.queries().is_empty());
        assert!(exhausted.effects().is_empty());
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::BudgetExhausted)
        );
    }

    #[test]
    fn repeated_stale_hover_edge_picks_are_bounded() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let requested = session.request_edge_pick(position(20.0, 10.0)).unwrap();
        let mut current = edge_query_id(&requested);

        for _ in 0..MAX_STALE_QUERY_RESTARTS {
            let restarted = session.restart_stale_query(current).unwrap();
            current = edge_query_id(&restarted);
        }
        let exhausted = session.restart_stale_query(current).unwrap();

        assert!(exhausted.queries().is_empty());
        assert!(exhausted.effects().is_empty());
        assert_eq!(session.presentation().hovered_edge(), None);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::BudgetExhausted)
        );
    }

    #[test]
    fn repeated_stale_commit_revalidation_rejects_without_graph_mutation() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let ticket = begin_direct_connection_commit(&mut session);
        let requested = session
            .request_connection_commit_revalidation(ticket)
            .unwrap();
        let mut current = connection_query_id(&requested);

        for _ in 0..MAX_STALE_QUERY_RESTARTS {
            let restarted = session.restart_stale_query(current).unwrap();
            current = connection_query_id(&restarted);
        }
        let exhausted = session.restart_stale_query(current).unwrap();

        assert!(exhausted.queries().is_empty());
        assert!(exhausted.effects().is_empty());
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::BudgetExhausted)
        );
    }

    #[test]
    fn exact_canvas_press_selects_its_resolved_edge_instead_of_a_previous_hover() {
        let stale_hover = EdgeId::from_u128(40);
        let pressed_edge = EdgeId::from_u128(41);
        let mut session = test_session(CanvasInteractionConfig::default());
        session.hovered_edge = Some(stale_hover);

        let pressed = session
            .press(
                canvas_marquee_target(SelectionMode::Toggle),
                position(20.0, 10.0),
            )
            .unwrap();
        let query = edge_query_id(&pressed);
        assert_eq!(session.presentation().hovered_edge(), None);
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Armed
        );

        let resolved = session
            .resolve_canvas_press_edge_resolution(
                query,
                CanvasEdgePickResolution::Complete(Some(pressed_edge)),
            )
            .unwrap();

        assert!(matches!(
            resolved.effects(),
            [CanvasInteractionEffect::Selection(SelectionMutation::Edge {
                edge,
                mode: SelectionMode::Toggle,
            })] if *edge == pressed_edge
        ));
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::EdgeActive
        );
        assert_eq!(session.presentation().active_edge(), Some(pressed_edge));
        assert_eq!(session.presentation().hovered_edge(), Some(pressed_edge));
    }

    #[test]
    fn empty_exact_pick_activates_marquee_from_press_to_latest_pointer() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let pressed = session
            .press(
                canvas_marquee_target(SelectionMode::Add),
                position(2.0, 3.0),
            )
            .unwrap();
        let query = edge_query_id(&pressed);
        let moved = session.move_to(position(14.0, 11.0)).unwrap();
        assert!(moved.effects().is_empty());

        let resolved = session
            .resolve_canvas_press_edge_resolution(query, CanvasEdgePickResolution::Complete(None))
            .unwrap();

        assert!(matches!(
            resolved.effects(),
            [CanvasInteractionEffect::Selection(SelectionMutation::Marquee {
                bounds,
                mode: SelectionMode::Add,
                ..
            })] if *bounds == CanvasRect {
                origin: CanvasPoint { x: 2.0, y: 3.0 },
                size: CanvasSize { width: 12.0, height: 8.0 },
            }
        ));
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Marquee
        );
        assert_eq!(
            session.presentation().marquee(),
            Some(CanvasRect {
                origin: CanvasPoint { x: 2.0, y: 3.0 },
                size: CanvasSize {
                    width: 12.0,
                    height: 8.0,
                },
            })
        );
    }

    #[test]
    fn release_before_exact_pick_completion_suppresses_delayed_edge_click_and_finishes_fallback() {
        let edge = EdgeId::from_u128(50);
        let mut edge_session = test_session(CanvasInteractionConfig::default());
        let pressed = edge_session
            .press(
                canvas_marquee_target(SelectionMode::Replace),
                position(0.0, 0.0),
            )
            .unwrap();
        let query = edge_query_id(&pressed);
        let released = edge_session.release(position(12.0, 0.0)).unwrap();
        assert!(released.effects().is_empty());
        let resolved = edge_session
            .resolve_canvas_press_edge_resolution(
                query,
                CanvasEdgePickResolution::Complete(Some(edge)),
            )
            .unwrap();
        assert!(resolved.effects().is_empty());
        assert_eq!(
            edge_session.presentation().phase(),
            CanvasInteractionPhase::Idle
        );

        let mut empty_session = test_session(CanvasInteractionConfig::default());
        let pressed = empty_session
            .press(
                canvas_marquee_target(SelectionMode::Replace),
                position(0.0, 0.0),
            )
            .unwrap();
        let query = edge_query_id(&pressed);
        empty_session.release(position(12.0, 8.0)).unwrap();
        let resolved = empty_session
            .resolve_canvas_press_edge_resolution(query, CanvasEdgePickResolution::Complete(None))
            .unwrap();
        assert!(matches!(
            resolved.effects(),
            [CanvasInteractionEffect::Selection(
                SelectionMutation::Marquee { .. }
            )]
        ));
        assert_eq!(
            empty_session.presentation().phase(),
            CanvasInteractionPhase::Idle
        );
    }

    #[test]
    fn exact_press_pagination_latches_maximum_displacement_until_completion() {
        let edge = EdgeId::from_u128(60);
        let mut session = test_session(CanvasInteractionConfig {
            query_entry_budget: 1,
            ..CanvasInteractionConfig::default()
        });
        let pressed = session
            .press(
                canvas_marquee_target(SelectionMode::Replace),
                position(4.0, 5.0),
            )
            .unwrap();
        let query = edge_query_id(&pressed);
        session.move_to(position(40.0, 50.0)).unwrap();
        let continued = session
            .resolve_canvas_press_edge_resolution(
                query,
                CanvasEdgePickResolution::Continue(EdgePickQuery::new(
                    ViewPoint::new(4.0, 5.0),
                    10.0,
                    1,
                )),
            )
            .unwrap();
        assert_eq!(edge_query_id(&continued), query);
        assert!(continued.effects().is_empty());
        let InteractionState::EdgePickPending(pending) = &session.state else {
            panic!("exact press must remain pending between query pages");
        };
        assert_eq!(pending.press, position(4.0, 5.0));
        assert_eq!(pending.pointer, position(40.0, 50.0));

        session.release(position(44.0, 55.0)).unwrap();
        let resolved = session
            .resolve_canvas_press_edge_resolution(
                query,
                CanvasEdgePickResolution::Complete(Some(edge)),
            )
            .unwrap();
        assert!(resolved.effects().is_empty());
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    }

    #[test]
    fn exact_press_latches_a_far_move_even_after_returning_to_the_press_point() {
        let edge = EdgeId::from_u128(60);
        let mut session = test_session(CanvasInteractionConfig::default());
        let pressed = session
            .press(
                canvas_marquee_target(SelectionMode::Replace),
                position(4.0, 5.0),
            )
            .unwrap();
        let query = edge_query_id(&pressed);
        session.move_to(position(40.0, 50.0)).unwrap();
        session.move_to(position(4.0, 5.0)).unwrap();
        session.release(position(4.0, 5.0)).unwrap();

        let resolved = session
            .resolve_canvas_press_edge_resolution(
                query,
                CanvasEdgePickResolution::Complete(Some(edge)),
            )
            .unwrap();
        assert!(resolved.effects().is_empty());
        assert_eq!(session.presentation().active_edge(), None);
    }

    #[test]
    fn stale_exact_press_restart_requeries_the_press_point_and_ignores_the_old_identity() {
        let edge = EdgeId::from_u128(61);
        let mut session = test_session(CanvasInteractionConfig::default());
        let pressed = session
            .press(
                canvas_marquee_target(SelectionMode::Replace),
                position(4.0, 5.0),
            )
            .unwrap();
        let stale = edge_query_id(&pressed);
        session.move_to(position(40.0, 50.0)).unwrap();

        let restarted = session.restart_stale_query(stale).unwrap();
        let fresh = edge_query_id(&restarted);
        assert_ne!(fresh, stale);
        let InteractionState::EdgePickPending(pending) = &session.state else {
            panic!("exact press restart must remain pending");
        };
        assert_eq!(pending.press, position(4.0, 5.0));
        assert_eq!(pending.pointer, position(40.0, 50.0));

        let late = session
            .resolve_canvas_press_edge_resolution(
                stale,
                CanvasEdgePickResolution::Complete(Some(edge)),
            )
            .unwrap();
        assert!(late.effects().is_empty());
        assert_eq!(session.presentation().active_edge(), None);

        let resolved = session
            .resolve_canvas_press_edge_resolution(
                fresh,
                CanvasEdgePickResolution::Complete(Some(edge)),
            )
            .unwrap();
        assert!(resolved.effects().is_empty());
    }

    #[test]
    fn exact_press_supersedes_fast_hover_without_accepting_its_late_result() {
        let pressed_edge = EdgeId::from_u128(63);
        let mut session = test_session(CanvasInteractionConfig::default());
        let hover = session.request_edge_pick(position(2.0, 3.0)).unwrap();
        let hover_query = edge_query_id(&hover);
        let pressed = session
            .press(
                canvas_marquee_target(SelectionMode::Replace),
                position(8.0, 9.0),
            )
            .unwrap();
        let press_query = edge_query_id(&pressed);
        assert_ne!(press_query, hover_query);

        let late_hover = session
            .resolve_edge_pick(
                hover_query,
                EdgePickQueryOutcome::BudgetExhausted {
                    work: EdgePickWork::default(),
                },
            )
            .unwrap();
        assert!(late_hover.effects().is_empty());
        assert_eq!(session.presentation().hovered_edge(), None);
        assert_eq!(session.presentation().terminal(), None);

        session
            .resolve_canvas_press_edge_resolution(
                press_query,
                CanvasEdgePickResolution::Complete(Some(pressed_edge)),
            )
            .unwrap();
        assert_eq!(session.presentation().active_edge(), Some(pressed_edge));
        assert_eq!(session.presentation().hovered_edge(), Some(pressed_edge));
    }

    #[test]
    fn exact_press_budget_geometry_cancel_and_late_results_never_select() {
        for (resolution, terminal) in [
            (
                CanvasEdgePickResolution::BudgetExhausted,
                InteractionTerminalReason::BudgetExhausted,
            ),
            (
                CanvasEdgePickResolution::GeometryIndeterminate,
                InteractionTerminalReason::GeometryIndeterminate,
            ),
        ] {
            let mut session = test_session(CanvasInteractionConfig::default());
            let pressed = session
                .press(
                    canvas_marquee_target(SelectionMode::Replace),
                    position(0.0, 0.0),
                )
                .unwrap();
            let resolved = session
                .resolve_canvas_press_edge_resolution(edge_query_id(&pressed), resolution)
                .unwrap();
            assert!(resolved.effects().is_empty());
            assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
            assert_eq!(session.presentation().terminal(), Some(terminal));
        }

        let edge = EdgeId::from_u128(70);
        let mut session = test_session(CanvasInteractionConfig::default());
        let pressed = session
            .press(
                canvas_marquee_target(SelectionMode::Replace),
                position(0.0, 0.0),
            )
            .unwrap();
        let query = edge_query_id(&pressed);
        let cancelled = session.cancel();
        assert!(cancelled.effects().is_empty());
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Cancelled)
        );
        let late = session
            .resolve_canvas_press_edge_resolution(
                query,
                CanvasEdgePickResolution::Complete(Some(edge)),
            )
            .unwrap();
        assert!(late.effects().is_empty());
        assert_eq!(session.presentation().active_edge(), None);

        let mut closed_session = test_session(CanvasInteractionConfig::default());
        let pressed = closed_session
            .press(
                canvas_marquee_target(SelectionMode::Replace),
                position(0.0, 0.0),
            )
            .unwrap();
        let query = edge_query_id(&pressed);
        let closed = closed_session.close();
        assert!(closed.effects().is_empty());
        assert_eq!(
            closed_session.presentation().terminal(),
            Some(InteractionTerminalReason::ViewClosed)
        );
        let late = closed_session
            .resolve_canvas_press_edge_resolution(
                query,
                CanvasEdgePickResolution::Complete(Some(edge)),
            )
            .unwrap();
        assert!(late.effects().is_empty());
        assert_eq!(closed_session.presentation().active_edge(), None);
    }

    #[test]
    fn empty_exact_pick_can_activate_pan_without_a_second_press() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let pressed = session
            .press(
                CanvasPressTarget::Canvas(CanvasPressFallback::Pan {
                    selection_mode: SelectionMode::Replace,
                }),
                position(1.0, 2.0),
            )
            .unwrap();
        let query = edge_query_id(&pressed);
        session.move_to(position(11.0, 14.0)).unwrap();
        let resolved = session
            .resolve_canvas_press_edge_resolution(query, CanvasEdgePickResolution::Complete(None))
            .unwrap();
        assert!(matches!(
            resolved.effects(),
            [CanvasInteractionEffect::Viewport(ViewportMutation::PanByView(delta))]
                if *delta == ViewDelta { x: 10.0, y: 12.0 }
        ));
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Pan);
    }

    #[test]
    fn reconnect_suppresses_old_edge_through_commit_and_restores_it_after_conflict() {
        let edge = EdgeId::from_u128(9);
        let start = ConnectionInteractionStart::new(
            source_node(),
            ConnectionGestureIntent::reconnect(
                edge,
                EdgeEndpoint::To,
                PortId::from_u128(4),
                source_port(),
                NodeGraphConnectionMode::Strict,
            ),
            CanvasPoint { x: 0.0, y: 8.0 },
            HandlePosition::Right,
            false,
        )
        .unwrap();
        let mut session = test_session(CanvasInteractionConfig::default());
        session
            .press(CanvasPressTarget::Connection(start), position(0.0, 0.0))
            .unwrap();
        let moved = session.move_to(position(8.0, 0.0)).unwrap();
        session
            .resolve_connection_probe(
                connection_query_id(&moved),
                complete(valid_probe(target_port())),
            )
            .unwrap();
        assert_eq!(
            session
                .presentation()
                .connection()
                .unwrap()
                .suppressed_edge(),
            Some(edge)
        );

        let released = session.release(position(20.0, 0.0)).unwrap();
        let committing = session
            .resolve_connection_probe(
                connection_query_id(&released),
                complete(valid_probe(target_port())),
            )
            .unwrap();
        let ticket = submit_ticket(&committing);
        assert_eq!(
            session
                .presentation()
                .connection()
                .unwrap()
                .suppressed_edge(),
            Some(edge)
        );

        session.settle(ticket, CommandSettlement::Conflict).unwrap();
        assert!(session.presentation().connection().is_none());
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Conflict)
        );
    }

    #[test]
    fn connection_commit_revalidation_reuses_ticket_and_refreshes_optimistic_preview() {
        let edge = EdgeId::from_u128(9);
        let start = ConnectionInteractionStart::new(
            source_node(),
            ConnectionGestureIntent::reconnect(
                edge,
                EdgeEndpoint::To,
                PortId::from_u128(4),
                source_port(),
                NodeGraphConnectionMode::Strict,
            ),
            CanvasPoint { x: 0.0, y: 8.0 },
            HandlePosition::Right,
            false,
        )
        .unwrap();
        let mut session = test_session(CanvasInteractionConfig::default());
        session
            .press(CanvasPressTarget::Connection(start), position(0.0, 0.0))
            .unwrap();
        let moved = session.move_to(position(8.0, 0.0)).unwrap();
        session
            .resolve_connection_probe(
                connection_query_id(&moved),
                complete(valid_probe(target_port())),
            )
            .unwrap();
        let released = session.release(position(20.0, 0.0)).unwrap();
        let committing = session
            .resolve_connection_probe(
                connection_query_id(&released),
                complete(valid_probe(target_port())),
            )
            .unwrap();
        let ticket = submit_ticket(&committing);
        let fresh_target = PortId::from_u128(3);
        let fresh_probe = valid_probe_at(fresh_target, 7, CanvasPoint { x: 48.0, y: 12.0 });

        let requested = session
            .request_connection_commit_revalidation(ticket)
            .unwrap();
        let query = connection_query_id(&requested);
        assert_eq!(
            session
                .presentation()
                .connection()
                .expect("commit preview")
                .suppressed_edge(),
            Some(edge)
        );
        let revalidated = session
            .resolve_connection_probe(query, complete(fresh_probe))
            .unwrap();
        let [
            CanvasInteractionEffect::SubmitGraph {
                ticket: emitted_ticket,
                mutation:
                    FreshGraphMutation::Connection {
                        intent,
                        target,
                        validated_revision,
                        topology,
                    },
            },
        ] = revalidated.effects()
        else {
            panic!("valid revalidation must emit one replacement connection mutation");
        };
        assert_eq!(*emitted_ticket, ticket);
        assert_eq!(*intent, start.intent());
        assert_eq!(*target, fresh_target);
        assert_eq!(*validated_revision, 7);
        assert!(topology.is_empty());

        let presentation = session.presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Committing);
        let connection = presentation.connection().expect("commit preview");
        assert_eq!(connection.ticket(), Some(ticket));
        assert_eq!(connection.suppressed_edge(), Some(edge));
        assert_eq!(
            connection.preview_target(),
            CanvasPoint { x: 48.0, y: 12.0 }
        );
    }

    #[test]
    fn invalid_commit_revalidation_conflicts_without_emitting_a_mutation() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let ticket = begin_direct_connection_commit(&mut session);
        let invalid = ConnectionProbe::Invalid {
            revision: 5,
            target: probe_target(target_port(), CanvasPoint { x: 32.0, y: 8.0 }),
            diagnostics: Vec::new(),
        };

        let requested = session
            .request_connection_commit_revalidation(ticket)
            .unwrap();
        let query = connection_query_id(&requested);
        let rejected = session
            .resolve_connection_probe(query, complete(invalid))
            .unwrap();
        assert!(rejected.effects().is_empty());
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Conflict)
        );

        let late = session
            .resolve_connection_probe(query, complete(valid_probe(target_port())))
            .unwrap();
        assert!(late.effects().is_empty());
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Conflict)
        );
    }

    #[test]
    fn create_from_wire_refresh_reuses_atomic_topology_and_returns_to_search_on_conflict() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let ticket = begin_search_connection_commit(&mut session);
        let request = session
            .search_request()
            .expect("authoritative search request");
        assert_eq!(
            request.intent(),
            ConnectionGestureIntent::connect(
                EdgeId::from_u128(9),
                source_port(),
                NodeGraphConnectionMode::Strict,
            )
        );

        let wrong_ticket = InteractionTicketId(ticket.get() + 1);
        let stale = session
            .refresh_create_from_wire_commit_revision(wrong_ticket, 6)
            .unwrap();
        assert!(stale.effects().is_empty());

        assert!(matches!(
            session.request_connection_commit_revalidation(ticket),
            Err(CanvasInteractionError::ConnectionCommitRequiresFreshRevision)
        ));
        let refreshed = session
            .refresh_create_from_wire_commit_revision(ticket, 7)
            .unwrap();
        let [
            CanvasInteractionEffect::SubmitGraph {
                ticket: emitted_ticket,
                mutation:
                    FreshGraphMutation::Connection {
                        target,
                        validated_revision,
                        topology,
                        ..
                    },
            },
        ] = refreshed.effects()
        else {
            panic!("fresh create-from-wire revision must resubmit one atomic mutation");
        };
        assert_eq!(*emitted_ticket, ticket);
        assert_eq!(*target, target_port());
        assert_eq!(*validated_revision, 7);
        assert!(!topology.is_empty());
        assert_eq!(
            session.search_request().unwrap().generation(),
            session.presentation().generation().unwrap()
        );

        let duplicate = session
            .refresh_create_from_wire_commit_revision(ticket, 8)
            .unwrap();
        assert!(duplicate.effects().is_empty());
        session.settle(ticket, CommandSettlement::Conflict).unwrap();
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        assert_eq!(
            session.presentation().connection().unwrap().search(),
            Some(ConnectionSearchStatus::CommitRejected)
        );
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Conflict)
        );
    }

    #[test]
    fn stale_connection_revalidation_ticket_does_not_change_pending_commit() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let ticket = begin_direct_connection_commit(&mut session);
        let original_preview = session
            .presentation()
            .connection()
            .expect("commit preview")
            .preview_target();
        let stale_ticket = InteractionTicketId(ticket.get() + 1);

        let stale = session
            .request_connection_commit_revalidation(stale_ticket)
            .unwrap();
        assert!(stale.queries().is_empty());
        assert!(stale.effects().is_empty());
        assert_eq!(stale.damage(), InteractionDamage::default());
        let presentation = session.presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Committing);
        let connection = presentation.connection().expect("commit preview");
        assert_eq!(connection.ticket(), Some(ticket));
        assert_eq!(connection.preview_target(), original_preview);

        let requested = session
            .request_connection_commit_revalidation(ticket)
            .unwrap();
        assert_eq!(requested.queries().len(), 1);
    }

    #[test]
    fn ordinary_connection_cannot_bypass_probe_with_revision_refresh() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let ticket = begin_direct_connection_commit(&mut session);

        assert!(matches!(
            session.refresh_create_from_wire_commit_revision(ticket, 9),
            Err(CanvasInteractionError::ConnectionCommitRequiresProbe)
        ));
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Committing
        );
        let requested = session
            .request_connection_commit_revalidation(ticket)
            .unwrap();
        assert_eq!(requested.queries().len(), 1);
    }

    #[test]
    fn connection_commit_revalidation_budget_exhaustion_is_terminal() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let ticket = begin_direct_connection_commit(&mut session);
        let requested = session
            .request_connection_commit_revalidation(ticket)
            .unwrap();
        let exhausted = session
            .resolve_connection_probe(
                connection_query_id(&requested),
                ConnectionProbeQueryOutcome::BudgetExhausted {
                    work: ConnectionProbeWork::default(),
                },
            )
            .unwrap();

        assert!(exhausted.effects().is_empty());
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::BudgetExhausted)
        );
    }

    #[test]
    fn release_budget_exhaustion_is_terminal_without_graph_mutation() {
        let mut session = test_session(CanvasInteractionConfig::default());
        session
            .press(connection_target(false), position(0.0, 0.0))
            .unwrap();
        let moved = session.move_to(position(8.0, 0.0)).unwrap();
        session
            .resolve_connection_probe(
                connection_query_id(&moved),
                complete(valid_probe(target_port())),
            )
            .unwrap();
        let released = session.release(position(20.0, 0.0)).unwrap();
        let exhausted = session
            .resolve_connection_probe(
                connection_query_id(&released),
                ConnectionProbeQueryOutcome::BudgetExhausted {
                    work: ConnectionProbeWork::default(),
                },
            )
            .unwrap();

        assert!(exhausted.effects().is_empty());
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::BudgetExhausted)
        );
    }

    #[test]
    fn search_failure_retry_and_rejected_selection_keep_one_search_owner() {
        let mut session = test_session(CanvasInteractionConfig::default());
        session
            .press(connection_target(true), position(0.0, 0.0))
            .unwrap();
        let moved = session.move_to(position(8.0, 0.0)).unwrap();
        session
            .resolve_connection_probe(connection_query_id(&moved), complete(ConnectionProbe::None))
            .unwrap();
        let released = session.release(position(20.0, 0.0)).unwrap();
        let searching = session
            .resolve_connection_probe(
                connection_query_id(&released),
                complete(ConnectionProbe::None),
            )
            .unwrap();
        let [CanvasInteractionEffect::Search(request)] = searching.effects() else {
            panic!("expected one connection search request");
        };
        assert_eq!(request.source(), source_port());
        assert_eq!(request.mode(), NodeGraphConnectionMode::Strict);
        let generation = session.presentation().generation().unwrap();
        assert_eq!(
            session.presentation().connection().unwrap().search(),
            Some(ConnectionSearchStatus::Loading)
        );

        session.search_failed(generation).unwrap();
        assert_eq!(
            session.presentation().connection().unwrap().search(),
            Some(ConnectionSearchStatus::Error)
        );
        let retry = session.retry_search(generation).unwrap();
        assert!(matches!(
            retry.effects(),
            [CanvasInteractionEffect::Search(_)]
        ));
        session.search_ready(generation, 2).unwrap();

        let selected = session
            .select_search_result(generation, search_selection(0))
            .unwrap();
        let ticket = submit_ticket(&selected);
        session.settle(ticket, CommandSettlement::Rejected).unwrap();
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        assert_eq!(
            session.presentation().connection().unwrap().search(),
            Some(ConnectionSearchStatus::CommitRejected)
        );

        session.cancel();
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    }

    #[test]
    fn search_callbacks_reject_inactive_and_stale_generations_without_state_changes() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let inactive_generation = InteractionGeneration(1);
        assert!(matches!(
            session.search_ready(inactive_generation, 1),
            Err(CanvasInteractionError::ConnectionSearchInactive)
        ));
        assert!(matches!(
            session.search_failed(inactive_generation),
            Err(CanvasInteractionError::ConnectionSearchInactive)
        ));
        assert!(matches!(
            session.retry_search(inactive_generation),
            Err(CanvasInteractionError::ConnectionSearchInactive)
        ));
        assert!(matches!(
            session.select_search_result(inactive_generation, search_selection(0)),
            Err(CanvasInteractionError::ConnectionSearchInactive)
        ));

        let CanvasPressTarget::Connection(start) = connection_target(true) else {
            unreachable!("connection fixture must produce a connection target");
        };
        session
            .activate_connection_search(start, position(12.0, 18.0))
            .unwrap();
        let generation = session.presentation().generation().unwrap();
        let stale_generation = InteractionGeneration(generation.get() + 1);
        for result in [
            session.search_ready(stale_generation, 1),
            session.search_failed(stale_generation),
            session.retry_search(stale_generation),
            session.select_search_result(stale_generation, search_selection(0)),
        ] {
            assert!(matches!(
                result,
                Err(CanvasInteractionError::ConnectionSearchStaleGeneration)
            ));
        }
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        assert_eq!(session.presentation().generation(), Some(generation));

        session.search_ready(generation, 1).unwrap();
        session
            .select_search_result(generation, search_selection(0))
            .unwrap();
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Committing
        );
        for result in [
            session.search_ready(generation, 1),
            session.search_failed(generation),
            session.retry_search(generation),
            session.select_search_result(generation, search_selection(0)),
        ] {
            assert!(matches!(
                result,
                Err(CanvasInteractionError::ConnectionSearchInactive)
            ));
        }
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Committing
        );
        assert_eq!(session.presentation().generation(), Some(generation));
    }

    #[test]
    fn native_activation_enters_the_same_connection_search_without_fake_pointer_motion() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let CanvasPressTarget::Connection(start) = connection_target(true) else {
            unreachable!("connection fixture must produce a connection target");
        };

        let activated = session
            .activate_connection_search(start, position(12.0, 18.0))
            .unwrap();
        let [CanvasInteractionEffect::Search(request)] = activated.effects() else {
            panic!("native activation must publish one search request");
        };
        assert_eq!(request.intent(), start.intent());
        assert_eq!(request.position(), position(12.0, 18.0));
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        assert_eq!(session.search_request(), Some(*request));

        let mut unavailable = test_session(CanvasInteractionConfig::default());
        let CanvasPressTarget::Connection(start) = connection_target(false) else {
            unreachable!("connection fixture must produce a connection target");
        };
        assert!(matches!(
            unavailable.activate_connection_search(start, position(0.0, 0.0)),
            Err(CanvasInteractionError::ConnectionSearchUnavailable)
        ));
        assert_eq!(
            unavailable.presentation().phase(),
            CanvasInteractionPhase::Idle
        );
    }

    #[test]
    fn reconnect_search_keeps_edge_identity_through_selection_and_revision_refresh() {
        let edge = EdgeId::from_u128(9);
        let intent = ConnectionGestureIntent::reconnect(
            edge,
            EdgeEndpoint::To,
            PortId::from_u128(4),
            source_port(),
            NodeGraphConnectionMode::Strict,
        );
        let start = ConnectionInteractionStart::new(
            source_node(),
            intent,
            CanvasPoint { x: 0.0, y: 8.0 },
            HandlePosition::Right,
            true,
        )
        .unwrap();
        let mut session = test_session(CanvasInteractionConfig::default());

        let activated = session
            .activate_connection_search(start, position(12.0, 18.0))
            .unwrap();
        let [CanvasInteractionEffect::Search(request)] = activated.effects() else {
            panic!("native reconnect activation must publish one search request");
        };
        assert_eq!(request.intent(), intent);
        let generation = request.generation();
        session.search_ready(generation, 1).unwrap();

        let selected = session
            .select_search_result(generation, search_selection(0))
            .unwrap();
        let [
            CanvasInteractionEffect::SubmitGraph {
                ticket,
                mutation:
                    FreshGraphMutation::Connection {
                        intent: selected_intent,
                        target,
                        topology,
                        ..
                    },
            },
        ] = selected.effects()
        else {
            panic!("reconnect search selection must submit one atomic connection mutation");
        };
        assert_eq!(*selected_intent, intent);
        assert_eq!(*target, target_port());
        assert!(!topology.is_empty());

        let refreshed = session
            .refresh_create_from_wire_commit_revision(*ticket, 7)
            .unwrap();
        let [
            CanvasInteractionEffect::SubmitGraph {
                ticket: refreshed_ticket,
                mutation:
                    FreshGraphMutation::Connection {
                        intent: refreshed_intent,
                        target: refreshed_target,
                        validated_revision,
                        topology: refreshed_topology,
                    },
            },
        ] = refreshed.effects()
        else {
            panic!("revision refresh must retain the atomic reconnect mutation");
        };
        assert_eq!(refreshed_ticket, ticket);
        assert_eq!(*refreshed_intent, intent);
        assert_eq!(*refreshed_target, target_port());
        assert_eq!(*validated_revision, 7);
        assert!(!refreshed_topology.is_empty());
    }

    #[test]
    fn generation_exhaustion_is_reported_without_arming_an_interaction() {
        let mut session = test_session(CanvasInteractionConfig::default());
        session.next_generation = None;

        assert!(matches!(
            session.press(CanvasPressTarget::Pan, position(0.0, 0.0)),
            Err(CanvasInteractionError::IdentityExhausted)
        ));
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::IdentityExhausted)
        );
    }

    #[test]
    fn repeated_toggle_marquee_moves_reuse_the_press_time_selection_baseline() {
        let selected_node = NodeId::from_u128(21);
        let selected_edge = EdgeId::from_u128(22);
        let baseline = SelectionBaseline::new([selected_node], [selected_edge], []);
        let mut session = test_session(CanvasInteractionConfig::default());
        session
            .press(
                CanvasPressTarget::Marquee {
                    selection_mode: SelectionMode::Toggle,
                    baseline: baseline.clone(),
                },
                position(0.0, 0.0),
            )
            .unwrap();

        let first = session.move_to(position(20.0, 16.0)).unwrap();
        let second = session.move_to(position(20.0, 16.0)).unwrap();
        let first_selection = marquee_selection_effect(&first);
        let second_selection = marquee_selection_effect(&second);

        assert_eq!(first_selection, second_selection);
        let SelectionMutation::Marquee {
            mode,
            baseline: emitted_baseline,
            ..
        } = first_selection
        else {
            panic!("marquee move must emit a marquee selection effect");
        };
        assert_eq!(*mode, SelectionMode::Toggle);
        assert_eq!(emitted_baseline, &baseline);

        let cancelled = session.cancel();
        assert!(matches!(
            cancelled.effects(),
            [CanvasInteractionEffect::Selection(SelectionMutation::Restore(restored))]
                if restored == &baseline
        ));
        assert!(cancelled.damage().selection);
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    }

    #[test]
    fn node_drag_snaps_relative_intent_and_conflict_is_not_retried() {
        let config = CanvasInteractionConfig {
            grid_size_in_canvas: Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            ..CanvasInteractionConfig::default()
        };
        let mut session = test_session(config);
        let first = NodeId::from_u128(10);
        let second = NodeId::from_u128(11);
        let start = NodeDragStart::new(
            first,
            [first, second],
            CanvasPoint { x: 0.0, y: 0.0 },
            SelectionMode::Replace,
        )
        .unwrap();
        session
            .press(CanvasPressTarget::NodeDrag(start), position(0.0, 0.0))
            .unwrap();
        session.move_to(position(6.0, 14.0)).unwrap();
        assert_eq!(
            session.presentation().node_drag().unwrap().delta(),
            CanvasPoint { x: 10.0, y: 10.0 }
        );

        let released = session.release(position(6.0, 14.0)).unwrap();
        let mut effects = released.into_dispatch().pre_query_effects;
        let CanvasInteractionEffect::SubmitGraph { ticket, mutation } = effects.remove(0) else {
            panic!("node release must emit one graph intent");
        };
        let FreshGraphMutation::MoveNodes(intent) = mutation else {
            panic!("node release emitted the wrong graph intent");
        };
        assert_eq!(intent.nodes(), &[first, second]);
        assert_eq!(intent.delta(), CanvasPoint { x: 10.0, y: 10.0 });
        assert!(effects.is_empty());

        session.settle(ticket, CommandSettlement::Conflict).unwrap();
        let late = session.settle(ticket, CommandSettlement::Applied).unwrap();
        assert!(late.effects().is_empty());
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Conflict)
        );
    }

    #[test]
    fn node_release_waits_for_the_final_alignment_query_without_requesting_ticks() {
        let mut session = test_session(CanvasInteractionConfig {
            alignment_snap: AlignmentSnapConfig::new(6.0, 9.0),
            ..CanvasInteractionConfig::default()
        });
        let node = NodeId::from_u128(10);
        let start =
            NodeDragStart::new(node, [node], CanvasPoint::default(), SelectionMode::Replace)
                .unwrap();
        session
            .press(CanvasPressTarget::NodeDrag(start), position(0.0, 0.0))
            .unwrap();
        session.move_to(position(12.0, 0.0)).unwrap();

        let released = session.release(position(16.0, 0.0)).unwrap();
        let [CanvasInteractionQuery::AlignmentSnap { id, generation, .. }] = released.queries()
        else {
            panic!("release must request one final alignment result");
        };
        assert!(released.effects().is_empty());
        assert_eq!(released.tick_demand(), InteractionTickDemand::None);
        assert!(
            session
                .presentation()
                .node_drag()
                .expect("release preview")
                .is_release_pending()
        );

        let committed = session
            .resolve_alignment_snap(
                *id,
                AlignmentSnapQueryOutcome::GeometryIndeterminate(AlignmentSnapWork::new(
                    *generation,
                    0,
                )),
            )
            .unwrap();
        assert!(matches!(
            committed.effects(),
            [CanvasInteractionEffect::SubmitGraph { .. }]
        ));
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Committing
        );
    }

    #[test]
    fn auto_pan_dispatch_orders_viewport_effect_before_alignment_query() {
        let mut session = test_session(CanvasInteractionConfig {
            alignment_snap: AlignmentSnapConfig::new(6.0, 9.0),
            ..CanvasInteractionConfig::default()
        });
        let node = NodeId::from_u128(10);
        let start =
            NodeDragStart::new(node, [node], CanvasPoint::default(), SelectionMode::Replace)
                .unwrap();
        session
            .press(CanvasPressTarget::NodeDrag(start), position(0.0, 0.0))
            .unwrap();
        session.move_to(position(195.0, 40.0)).unwrap();

        let dispatch = session
            .tick(
                1.0 / 60.0,
                ViewSize::new(200.0, 120.0),
                ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap(),
            )
            .unwrap()
            .into_dispatch();
        assert!(matches!(
            dispatch.pre_query_effects.as_slice(),
            [CanvasInteractionEffect::Viewport(
                ViewportMutation::PanByView(_)
            )]
        ));
        assert!(matches!(
            dispatch.queries.as_slice(),
            [CanvasInteractionQuery::AlignmentSnap { .. }]
        ));
    }

    #[test]
    fn edge_delete_requires_authoritative_permission_and_owns_one_ticket_until_settlement() {
        let edge = EdgeId::from_u128(21);
        let denied = published_edge_route(edge, false, EdgeReconnectable::Bool(true));
        let allowed = published_edge_route(edge, true, EdgeReconnectable::Bool(true));
        let mut session = test_session(CanvasInteractionConfig::default());
        session
            .press(CanvasPressTarget::Pan, position(0.0, 0.0))
            .unwrap();
        session.cancel();

        let ignored = session.delete_edge(&denied).unwrap();
        assert!(ignored.effects().is_empty());
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(session.presentation().generation(), None);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Cancelled)
        );

        let submitted = session.delete_edge(&allowed).unwrap();
        let [CanvasInteractionEffect::SubmitGraph { ticket, mutation }] = submitted.effects()
        else {
            panic!("one permitted edge delete must submit exactly one mutation");
        };
        let FreshGraphMutation::DeleteEdge(intent) = mutation else {
            panic!("edge delete emitted the wrong graph mutation");
        };
        assert_eq!(ticket.get(), 1);
        assert_eq!(intent.edge(), edge);
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Committing
        );
        assert_eq!(session.presentation().active_edge(), Some(edge));
        assert!(submitted.damage().edges);
        assert!(matches!(
            session.delete_edge(&allowed),
            Err(CanvasInteractionError::AlreadyActive)
        ));

        let settled = session
            .settle(*ticket, CommandSettlement::Rejected)
            .unwrap();
        assert!(settled.effects().is_empty());
        assert!(settled.damage().edges);
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Rejected)
        );
        let late = session.settle(*ticket, CommandSettlement::Applied).unwrap();
        assert!(late.effects().is_empty());
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Rejected)
        );
    }

    #[test]
    fn reconnect_start_resolves_graph_topology_and_endpoint_specific_route_facts() {
        let edge = EdgeId::from_u128(22);
        let graph = reconnect_graph(edge);
        let route = published_edge_route(
            edge,
            true,
            EdgeReconnectable::Endpoint(EdgeReconnectableEndpoint::Source),
        );
        let source = ConnectionInteractionStart::reconnect_from_graph_route(
            &graph,
            &route,
            EdgeEndpoint::From,
            NodeGraphConnectionMode::Loose,
            false,
        )
        .expect("source reconnect is permitted");
        assert_eq!(source.owner_node(), source_node());
        assert_eq!(source.intent().fixed_port(), target_port());
        assert_eq!(source.intent().detached_port(), Some(source_port()));
        assert_eq!(source.intent().mode(), NodeGraphConnectionMode::Loose);

        assert!(
            ConnectionInteractionStart::reconnect_from_graph_route(
                &graph,
                &route,
                EdgeEndpoint::To,
                NodeGraphConnectionMode::Strict,
                false,
            )
            .is_none()
        );
        let missing =
            published_edge_route(EdgeId::from_u128(23), true, EdgeReconnectable::Bool(true));
        assert!(
            ConnectionInteractionStart::reconnect_from_graph_route(
                &graph,
                &missing,
                EdgeEndpoint::From,
                NodeGraphConnectionMode::Strict,
                false,
            )
            .is_none()
        );
    }

    #[test]
    fn retiring_an_edge_during_delete_preserves_the_ticket_for_fresh_bind_noop() {
        let edge = EdgeId::from_u128(24);
        let route = published_edge_route(edge, true, EdgeReconnectable::Bool(true));
        let mut session = test_session(CanvasInteractionConfig::default());
        let submitted = session.delete_edge(&route).unwrap();
        let ticket = submit_ticket(&submitted);

        let retired = session.owner_retired(InteractionOwner::Edge(edge));
        assert!(retired.effects().is_empty());
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Committing
        );
        assert_eq!(session.presentation().active_edge(), Some(edge));

        session.settle(ticket, CommandSettlement::Noop).unwrap();
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Noop)
        );
    }

    #[test]
    fn active_drag_requests_bounded_auto_pan_and_retirement_cancels_late_receipts() {
        let mut session = test_session(CanvasInteractionConfig::default());
        let node = NodeId::from_u128(10);
        let start = NodeDragStart::new(
            node,
            [node],
            CanvasPoint { x: 0.0, y: 0.0 },
            SelectionMode::Replace,
        )
        .unwrap();
        session
            .press(CanvasPressTarget::NodeDrag(start), position(10.0, 10.0))
            .unwrap();
        session.move_to(position(5.0, 5.0)).unwrap();
        let transform = ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();
        let tick = session
            .tick(0.1, ViewSize::new(200.0, 120.0), transform)
            .unwrap();
        let [CanvasInteractionEffect::Viewport(ViewportMutation::PanByView(delta))] =
            tick.effects()
        else {
            panic!("edge-band drag must request auto-pan");
        };
        assert!(delta.x > 0.0 && delta.y > 0.0);
        assert!(delta.x <= 36.0 && delta.y <= 36.0);

        session.owner_retired(InteractionOwner::Node(node));
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::OwnerRetired)
        );
    }

    #[test]
    fn auto_pan_caps_long_frames_and_does_not_pan_from_a_narrow_viewport_center() {
        let long_frame = auto_pan_delta(
            ViewPoint::new(0.0, 0.0),
            ViewSize::new(200.0, 120.0),
            36.0,
            720.0,
            MAX_AUTO_PAN_ELAPSED_SECONDS,
        );
        assert_eq!(long_frame, ViewDelta { x: 36.0, y: 36.0 });
        assert_eq!(auto_pan_axis(10.0, 20.0, 36.0), 0.0);
        assert_eq!(auto_pan_axis(0.0, 20.0, 36.0), 1.0);
        assert_eq!(auto_pan_axis(20.0, 20.0, 36.0), -1.0);
    }

    fn test_session(config: CanvasInteractionConfig) -> CanvasInteractionSession {
        CanvasInteractionSession::new(
            ViewId::new(crate::runtime::graph_runtime::RuntimeId::UNAVAILABLE, 0),
            ViewGeneration::initial(),
            config,
        )
        .unwrap()
    }

    fn published_edge_route(
        edge: EdgeId,
        deletable: bool,
        reconnectable: EdgeReconnectable,
    ) -> PublishedEdgeRoute {
        let mut model = Edge::new(EdgeKind::Data, source_port(), target_port());
        model.deletable = Some(deletable);
        model.reconnectable = Some(reconnectable);
        let endpoints = EdgePosition {
            source: EdgeEndpointPosition {
                point: CanvasPoint { x: 0.0, y: 0.0 },
                position: HandlePosition::Right,
            },
            target: EdgeEndpointPosition {
                point: CanvasPoint { x: 100.0, y: 0.0 },
                position: HandlePosition::Left,
            },
        };
        let facts = resolve_edge_route_path(&model, endpoints).expect("test edge route");
        PublishedEdgeRoute::new(edge, facts)
    }

    fn reconnect_graph(edge: EdgeId) -> Graph {
        let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
        source.ports.push(source_port());
        let mut target = test_node(CanvasPoint { x: 100.0, y: 0.0 });
        target.ports.push(target_port());
        GraphBuilder::new(GraphId::from_u128(1))
            .with_node(source_node(), source)
            .with_port(
                source_port(),
                test_port(source_node(), "out", PortDirection::Out),
            )
            .with_node(NodeId::from_u128(2), target)
            .with_port(
                target_port(),
                test_port(NodeId::from_u128(2), "in", PortDirection::In),
            )
            .with_edge(
                edge,
                Edge::new(EdgeKind::Data, source_port(), target_port()),
            )
            .build()
            .expect("valid reconnect graph")
    }

    fn test_node(pos: CanvasPoint) -> Node {
        Node {
            kind: NodeKindKey::new("test.node"),
            kind_version: 1,
            pos,
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 160.0,
                height: 96.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::Value::Null,
        }
    }

    fn test_port(node: NodeId, key: &str, dir: PortDirection) -> Port {
        Port {
            node,
            key: PortKey::new(key),
            dir,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        }
    }

    fn connection_target(allow_search: bool) -> CanvasPressTarget {
        CanvasPressTarget::Connection(connection_start(allow_search))
    }

    fn connection_start(allow_search: bool) -> ConnectionInteractionStart {
        ConnectionInteractionStart::new(
            source_node(),
            ConnectionGestureIntent::connect(
                EdgeId::from_u128(9),
                source_port(),
                NodeGraphConnectionMode::Strict,
            ),
            CanvasPoint { x: 0.0, y: 8.0 },
            HandlePosition::Right,
            allow_search,
        )
        .unwrap()
    }

    fn canvas_marquee_target(selection_mode: SelectionMode) -> CanvasPressTarget {
        CanvasPressTarget::Canvas(CanvasPressFallback::Marquee {
            selection_mode,
            baseline: SelectionBaseline::default(),
        })
    }

    fn position(x: f32, y: f32) -> InteractionPosition {
        InteractionPosition::from_view(
            ViewPoint::new(x, y),
            ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap(),
        )
        .unwrap()
    }

    fn marquee_selection_effect(update: &CanvasInteractionUpdate) -> &SelectionMutation {
        let [CanvasInteractionEffect::Selection(selection)] = update.effects() else {
            panic!("marquee move must emit one selection effect");
        };
        selection
    }

    fn valid_probe(port: PortId) -> ConnectionProbe {
        valid_probe_at(port, 0, CanvasPoint { x: 24.0, y: 8.0 })
    }

    fn valid_probe_at(
        port: PortId,
        revision: GraphRevision,
        route_anchor: CanvasPoint,
    ) -> ConnectionProbe {
        ConnectionProbe::Valid {
            revision,
            target: probe_target(port, route_anchor),
        }
    }

    fn probe_target(port: PortId, route_anchor: CanvasPoint) -> ConnectionProbeTarget {
        ConnectionProbeTarget::new(
            port,
            NodeId::from_u128(2),
            route_anchor,
            HandlePosition::Left,
            0.0,
        )
    }

    fn begin_direct_connection_commit(
        session: &mut CanvasInteractionSession,
    ) -> InteractionTicketId {
        session
            .press(connection_target(false), position(0.0, 0.0))
            .unwrap();
        let moved = session.move_to(position(8.0, 0.0)).unwrap();
        session
            .resolve_connection_probe(
                connection_query_id(&moved),
                complete(valid_probe(target_port())),
            )
            .unwrap();
        let released = session.release(position(20.0, 0.0)).unwrap();
        let committing = session
            .resolve_connection_probe(
                connection_query_id(&released),
                complete(valid_probe(target_port())),
            )
            .unwrap();
        submit_ticket(&committing)
    }

    fn begin_search_connection_commit(
        session: &mut CanvasInteractionSession,
    ) -> InteractionTicketId {
        session
            .press(connection_target(true), position(0.0, 0.0))
            .unwrap();
        let moved = session.move_to(position(8.0, 0.0)).unwrap();
        session
            .resolve_connection_probe(connection_query_id(&moved), complete(ConnectionProbe::None))
            .unwrap();
        let released = session.release(position(20.0, 0.0)).unwrap();
        session
            .resolve_connection_probe(
                connection_query_id(&released),
                complete(ConnectionProbe::None),
            )
            .unwrap();
        let generation = session.presentation().generation().unwrap();
        session.search_ready(generation, 1).unwrap();
        let committing = session
            .select_search_result(generation, search_selection(0))
            .unwrap();
        submit_ticket(&committing)
    }

    fn search_selection(revision: GraphRevision) -> ConnectionSearchSelection {
        ConnectionSearchSelection::new(
            target_port(),
            revision,
            TopologyEdit::new().set_node_data(
                source_node(),
                serde_json::json!({ "created_from_wire": true }),
            ),
        )
        .expect("create-from-wire selection carries topology")
    }

    fn complete(probe: ConnectionProbe) -> ConnectionProbeQueryOutcome {
        ConnectionProbeQueryOutcome::Complete {
            probe,
            work: ConnectionProbeWork::default(),
        }
    }

    fn connection_query_id(update: &CanvasInteractionUpdate) -> InteractionQueryId {
        let [CanvasInteractionQuery::ConnectionProbe { id, .. }] = update.queries() else {
            panic!("expected one connection probe query");
        };
        *id
    }

    fn edge_query_id(update: &CanvasInteractionUpdate) -> InteractionQueryId {
        let [CanvasInteractionQuery::EdgePick { id, .. }] = update.queries() else {
            panic!("expected one edge-pick query");
        };
        *id
    }

    fn submit_ticket(update: &CanvasInteractionUpdate) -> InteractionTicketId {
        let [CanvasInteractionEffect::SubmitGraph { ticket, .. }] = update.effects() else {
            panic!("expected one graph submission");
        };
        *ticket
    }

    fn source_node() -> NodeId {
        NodeId::from_u128(1)
    }

    fn source_port() -> PortId {
        PortId::from_u128(1)
    }

    fn target_port() -> PortId {
        PortId::from_u128(2)
    }
}
