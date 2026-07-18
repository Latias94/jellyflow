use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId, NodeId, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::runtime::adapter_protocol::{ViewPoint, ViewSize};
use crate::runtime::geometry::HandlePosition;
use crate::runtime::graph_runtime::GraphRevision;
use crate::runtime::view_session::{
    ConnectionProbeQuery, ConnectionProbeQueryOutcome, EdgePickQuery, EdgePickQueryOutcome,
    ViewGeneration, ViewId,
};

use super::intent::{FreshGraphMutation, RelativeNodeMoveIntent};
use super::presentation::{
    AlignmentGuide, CanvasInteractionPhase, CanvasInteractionPresentation, ConnectionPresentation,
    ConnectionSearchStatus, InteractionPosition, InteractionTerminalReason, NodeDragPresentation,
};
use super::{ConnectionGestureIntent, ConnectionProbe};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasInteractionConfig {
    pub drag_threshold_in_view: f32,
    pub connection_radius_in_view: f32,
    pub edge_pick_radius_in_view: f32,
    pub grid_size_in_canvas: Option<CanvasSize>,
    pub auto_pan_band_in_view: f32,
    pub auto_pan_speed_in_view_per_second: f32,
    pub query_entry_budget: usize,
}

impl Default for CanvasInteractionConfig {
    fn default() -> Self {
        Self {
            drag_threshold_in_view: 4.0,
            connection_radius_in_view: 12.0,
            edge_pick_radius_in_view: 10.0,
            grid_size_in_canvas: None,
            auto_pan_band_in_view: 36.0,
            auto_pan_speed_in_view_per_second: 720.0,
            query_entry_budget: 256,
        }
    }
}

impl CanvasInteractionConfig {
    pub fn is_valid(self) -> bool {
        self.drag_threshold_in_view.is_finite()
            && self.drag_threshold_in_view >= 0.0
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
    nodes: Box<[NodeId]>,
    snap_origin: CanvasPoint,
    selection_mode: SelectionMode,
}

impl NodeDragStart {
    pub fn new(
        clicked: NodeId,
        nodes: impl IntoIterator<Item = NodeId>,
        snap_origin: CanvasPoint,
        selection_mode: SelectionMode,
    ) -> Option<Self> {
        if !snap_origin.is_finite() {
            return None;
        }
        let mut nodes = nodes.into_iter().collect::<Vec<_>>();
        nodes.sort_unstable();
        nodes.dedup();
        if !nodes.contains(&clicked) {
            nodes.push(clicked);
            nodes.sort_unstable();
        }
        Some(Self {
            clicked,
            nodes: nodes.into_boxed_slice(),
            snap_origin,
            selection_mode,
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
    pub fn new(
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
    },
    Connection(ConnectionInteractionStart),
    Edge {
        edge: EdgeId,
        selection_mode: SelectionMode,
    },
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
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewportMutation {
    PanByView(CanvasPoint),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionSearchRequest {
    generation: InteractionGeneration,
    source: PortId,
    mode: NodeGraphConnectionMode,
    position: InteractionPosition,
}

impl ConnectionSearchRequest {
    pub fn generation(self) -> InteractionGeneration {
        self.generation
    }

    pub fn source(self) -> PortId {
        self.source
    }

    pub fn mode(self) -> NodeGraphConnectionMode {
        self.mode
    }

    pub fn position(self) -> InteractionPosition {
        self.position
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

impl CanvasInteractionUpdate {
    pub fn queries(&self) -> &[CanvasInteractionQuery] {
        &self.queries
    }

    pub fn effects(&self) -> &[CanvasInteractionEffect] {
        &self.effects
    }

    pub fn into_parts(
        self,
    ) -> (
        Vec<CanvasInteractionQuery>,
        Vec<CanvasInteractionEffect>,
        InteractionDamage,
        InteractionTickDemand,
    ) {
        (self.queries, self.effects, self.damage, self.tick_demand)
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
    pending_edge_pick: Option<InteractionQueryId>,
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
}

#[derive(Debug)]
struct ArmedInteraction {
    generation: InteractionGeneration,
    target: ArmedTarget,
    start: InteractionPosition,
}

#[derive(Debug)]
enum ArmedTarget {
    NodeDrag(NodeDragStart),
    Pan,
    Marquee(SelectionMode),
    Connection(ConnectionInteractionStart),
}

#[derive(Debug)]
struct NodeDragInteraction {
    generation: InteractionGeneration,
    nodes: Box<[NodeId]>,
    snap_origin: CanvasPoint,
    start: InteractionPosition,
    pointer: InteractionPosition,
    delta: CanvasPoint,
    guides: Box<[AlignmentGuide]>,
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
}

#[derive(Debug)]
struct ConnectionCommitInteraction {
    generation: InteractionGeneration,
    start: ConnectionInteractionStart,
    pointer: InteractionPosition,
    probe: ConnectionProbe,
    ticket: InteractionTicketId,
    return_to_search: bool,
}

#[derive(Debug)]
struct NodeCommitInteraction {
    generation: InteractionGeneration,
    nodes: Box<[NodeId]>,
    delta: CanvasPoint,
    guides: Box<[AlignmentGuide]>,
    ticket: InteractionTicketId,
}

#[derive(Debug)]
struct EdgeInteraction {
    generation: InteractionGeneration,
    edge: EdgeId,
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
                &drag.nodes,
                drag.delta,
                &drag.guides,
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
                            &node.nodes,
                            node.delta,
                            &node.guides,
                            Some(node.ticket),
                        ))
                }
            },
            InteractionState::EdgeActive(edge) => {
                CanvasInteractionPresentation::new(CanvasInteractionPhase::EdgeActive)
                    .with_active_edge(edge.edge)
            }
        };
        presentation = presentation
            .with_generation(state_generation(&self.state))
            .with_hovered_edge(self.hovered_edge)
            .with_terminal(self.terminal);
        presentation
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
            CanvasPressTarget::NodeDrag(start) => {
                let generation = self.allocate_generation()?;
                update.effects.push(CanvasInteractionEffect::Selection(
                    SelectionMutation::Node {
                        node: start.clicked,
                        mode: start.selection_mode,
                    },
                ));
                update.damage.selection = true;
                self.state = InteractionState::Armed(ArmedInteraction {
                    generation,
                    target: ArmedTarget::NodeDrag(start),
                    start: position,
                });
                update.damage.merge(InteractionDamage::presentation());
            }
            CanvasPressTarget::Pan => {
                self.arm(ArmedTarget::Pan, position)?;
                update.damage = InteractionDamage::presentation();
            }
            CanvasPressTarget::Marquee { selection_mode } => {
                self.arm(ArmedTarget::Marquee(selection_mode), position)?;
                update.damage = InteractionDamage::presentation();
            }
            CanvasPressTarget::Connection(start) => {
                self.arm(ArmedTarget::Connection(start), position)?;
                update.damage = InteractionDamage::connection();
            }
        }
        Ok(update.active())
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
        self.pending_edge_pick = Some(id);
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

    pub fn resolve_edge_pick(
        &mut self,
        id: InteractionQueryId,
        outcome: EdgePickQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let mut update = CanvasInteractionUpdate::default();
        if self.pending_edge_pick != Some(id) {
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

    pub fn move_to(
        &mut self,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let state = std::mem::take(&mut self.state);
        let mut update = CanvasInteractionUpdate::default();
        self.state = match state {
            InteractionState::Armed(armed) => {
                if !threshold_crossed(
                    armed.start.view(),
                    position.view(),
                    self.config.drag_threshold_in_view,
                ) {
                    InteractionState::Armed(armed)
                } else {
                    self.activate_armed(armed, position, &mut update)?
                }
            }
            InteractionState::NodeDrag(mut drag) => {
                drag.pointer = position;
                drag.delta = snapped_delta(
                    drag.start.canvas(),
                    position.canvas(),
                    drag.snap_origin,
                    self.config.grid_size_in_canvas,
                );
                drag.guides = Box::default();
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
            other => other,
        };
        if is_tick_active(&self.state) {
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
            InteractionState::Armed(_) => {
                update.damage.merge(InteractionDamage::presentation());
                InteractionState::Idle
            }
            InteractionState::NodeDrag(drag) => {
                let Some(intent) =
                    RelativeNodeMoveIntent::new(drag.nodes.iter().copied(), drag.delta)
                else {
                    update.damage.merge(InteractionDamage::node_drag());
                    return Ok(update);
                };
                let Some(ticket) = self.allocate_ticket() else {
                    self.terminal = Some(InteractionTerminalReason::IdentityExhausted);
                    update.damage.merge(InteractionDamage::node_drag());
                    return Ok(update);
                };
                update.effects.push(CanvasInteractionEffect::SubmitGraph {
                    ticket,
                    mutation: FreshGraphMutation::MoveNodes(intent),
                });
                update.damage.merge(InteractionDamage::node_drag());
                InteractionState::Committing(CommitInteraction::NodeDrag(NodeCommitInteraction {
                    generation: drag.generation,
                    nodes: drag.nodes,
                    delta: drag.delta,
                    guides: drag.guides,
                    ticket,
                }))
            }
            InteractionState::Connection(ConnectionInteraction::Dragging(mut drag)) => {
                drag.pointer = position;
                drag.release_pending = true;
                self.start_connection_probe(&mut drag, &mut update)?;
                update.damage.merge(InteractionDamage::connection());
                InteractionState::Connection(ConnectionInteraction::Dragging(drag))
            }
            InteractionState::Pan(_)
            | InteractionState::Marquee(_)
            | InteractionState::EdgeActive(_) => {
                update.damage.merge(InteractionDamage::presentation());
                InteractionState::Idle
            }
            other => other,
        };
        if is_tick_active(&self.state) {
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
                        update = update.active();
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
            other => other,
        };
        if is_tick_active(&self.state) {
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
        let mut update = CanvasInteractionUpdate::default();
        if let InteractionState::Connection(ConnectionInteraction::Searching(search)) =
            &mut self.state
            && search.generation == generation
        {
            search.status = ConnectionSearchStatus::Loading;
            update
                .effects
                .push(CanvasInteractionEffect::Search(search_request(search)));
            update.damage.merge(InteractionDamage::connection());
            update = update.active();
        }
        Ok(update)
    }

    pub fn select_search_result(
        &mut self,
        generation: InteractionGeneration,
        target: PortId,
        validated_revision: GraphRevision,
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
                    target,
                    validated_revision,
                    true,
                    &mut update,
                )
            }
            other => other,
        };
        if is_tick_active(&self.state) {
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
                match settlement {
                    CommandSettlement::Applied | CommandSettlement::Noop => {
                        self.terminal = Some(settlement_terminal(settlement));
                        InteractionState::Idle
                    }
                    CommandSettlement::Rejected
                    | CommandSettlement::Conflict
                    | CommandSettlement::TransportFailure
                        if commit.return_to_search =>
                    {
                        self.terminal = Some(settlement_terminal(settlement));
                        InteractionState::Connection(ConnectionInteraction::Searching(
                            ConnectionSearchInteraction {
                                generation: commit.generation,
                                start: commit.start,
                                pointer: commit.pointer,
                                probe: commit.probe,
                                status: ConnectionSearchStatus::CommitRejected,
                            },
                        ))
                    }
                    CommandSettlement::Rejected
                    | CommandSettlement::Conflict
                    | CommandSettlement::TransportFailure => {
                        self.terminal = Some(settlement_terminal(settlement));
                        InteractionState::Idle
                    }
                }
            }
            InteractionState::Committing(CommitInteraction::NodeDrag(commit))
                if commit.ticket == ticket =>
            {
                self.terminal = Some(settlement_terminal(settlement));
                update.damage.merge(InteractionDamage::node_drag());
                InteractionState::Idle
            }
            other => other,
        };
        if is_tick_active(&self.state) {
            update = update.active();
        }
        Ok(update)
    }

    pub fn apply_alignment_snap(
        &mut self,
        generation: InteractionGeneration,
        delta: CanvasPoint,
        guides: impl IntoIterator<Item = AlignmentGuide>,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        if !delta.is_finite() {
            return Err(CanvasInteractionError::InvalidPosition);
        }
        let mut update = CanvasInteractionUpdate::default();
        if let InteractionState::NodeDrag(drag) = &mut self.state
            && drag.generation == generation
        {
            drag.delta = delta;
            drag.guides = guides.into_iter().collect::<Vec<_>>().into_boxed_slice();
            update.damage.merge(InteractionDamage::node_drag());
            update = update.active();
        }
        Ok(update)
    }

    pub fn tick(
        &mut self,
        elapsed_seconds: f32,
        viewport: ViewSize,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 || !viewport.is_positive_finite()
        {
            return Ok(CanvasInteractionUpdate::default());
        }
        let pointer = active_pointer(&self.state);
        let mut update = CanvasInteractionUpdate::default();
        if let Some(pointer) = pointer {
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
            }
            update = update.active();
        }
        Ok(update)
    }

    pub fn cancel(&mut self) -> CanvasInteractionUpdate {
        self.pending_edge_pick = None;
        if matches!(self.state, InteractionState::Idle) {
            return CanvasInteractionUpdate::default();
        }
        let damage = damage_for_state(&self.state);
        self.state = InteractionState::Idle;
        self.terminal = Some(InteractionTerminalReason::Cancelled);
        CanvasInteractionUpdate::default().with_damage(damage)
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
                let delta = snapped_delta(
                    armed.start.canvas(),
                    position.canvas(),
                    start.snap_origin,
                    self.config.grid_size_in_canvas,
                );
                update.damage.merge(InteractionDamage::node_drag());
                InteractionState::NodeDrag(NodeDragInteraction {
                    generation: armed.generation,
                    nodes: start.nodes,
                    snap_origin: start.snap_origin,
                    start: armed.start,
                    pointer: position,
                    delta,
                    guides: Box::default(),
                })
            }
            ArmedTarget::Pan => {
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
            ArmedTarget::Marquee(selection_mode) => {
                let bounds = rect_between(armed.start.canvas(), position.canvas());
                update.effects.push(CanvasInteractionEffect::Selection(
                    SelectionMutation::Marquee {
                        bounds,
                        mode: selection_mode,
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
                })
            }
            ArmedTarget::Connection(start) => {
                let mut drag = ConnectionDragInteraction {
                    generation: armed.generation,
                    start,
                    pointer: position,
                    probe: ConnectionProbe::None,
                    pending_query: None,
                    release_pending: false,
                };
                self.start_connection_probe(&mut drag, update)?;
                update.damage.merge(InteractionDamage::connection());
                InteractionState::Connection(ConnectionInteraction::Dragging(drag))
            }
        })
    }

    fn start_connection_probe(
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
            },
        });
        InteractionState::Committing(CommitInteraction::Connection(ConnectionCommitInteraction {
            generation,
            start,
            pointer,
            probe,
            ticket,
            return_to_search,
        }))
    }

    fn set_search_status(
        &mut self,
        generation: InteractionGeneration,
        status: ConnectionSearchStatus,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.ensure_open()?;
        let mut update = CanvasInteractionUpdate::default();
        if let InteractionState::Connection(ConnectionInteraction::Searching(search)) =
            &mut self.state
            && search.generation == generation
        {
            search.status = status;
            update.damage.merge(InteractionDamage::connection());
            update = update.active();
        }
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
    nodes: &[NodeId],
    delta: CanvasPoint,
    guides: &[AlignmentGuide],
    ticket: Option<InteractionTicketId>,
) -> NodeDragPresentation {
    NodeDragPresentation::new(
        generation,
        nodes.to_vec().into_boxed_slice(),
        delta,
        guides.to_vec().into_boxed_slice(),
        ticket,
    )
}

fn search_request(search: &ConnectionSearchInteraction) -> ConnectionSearchRequest {
    ConnectionSearchRequest {
        generation: search.generation,
        source: search.start.intent.fixed_port(),
        mode: search.start.intent.mode(),
        position: search.pointer,
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

fn threshold_crossed(start: ViewPoint, current: ViewPoint, threshold: f32) -> bool {
    let x = current.x - start.x;
    let y = current.y - start.y;
    x.mul_add(x, y * y) >= threshold * threshold
}

fn view_delta(start: ViewPoint, current: ViewPoint) -> CanvasPoint {
    CanvasPoint {
        x: current.x - start.x,
        y: current.y - start.y,
    }
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
) -> CanvasPoint {
    CanvasPoint {
        x: auto_pan_axis(pointer.x, viewport.width, band) * speed * elapsed,
        y: auto_pan_axis(pointer.y, viewport.height, band) * speed * elapsed,
    }
}

fn auto_pan_axis(position: f32, extent: f32, band: f32) -> f32 {
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
        InteractionState::NodeDrag(drag) => Some(drag.pointer),
        InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => Some(drag.pointer),
        InteractionState::Armed(_)
        | InteractionState::Idle
        | InteractionState::Pan(_)
        | InteractionState::Marquee(_)
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
        InteractionState::EdgeActive(edge) => Some(edge.generation),
    }
}

fn is_tick_active(state: &InteractionState) -> bool {
    matches!(
        state,
        InteractionState::Armed(_)
            | InteractionState::NodeDrag(_)
            | InteractionState::Pan(_)
            | InteractionState::Marquee(_)
            | InteractionState::Connection(_)
            | InteractionState::Committing(_)
            | InteractionState::EdgeActive(_)
    )
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
        InteractionState::Idle
        | InteractionState::Armed(_)
        | InteractionState::Pan(_)
        | InteractionState::Marquee(_)
        | InteractionState::EdgeActive(_) => InteractionDamage::presentation(),
    }
}

fn state_uses_owner(state: &InteractionState, owner: InteractionOwner) -> bool {
    match state {
        InteractionState::Armed(armed) => match &armed.target {
            ArmedTarget::NodeDrag(start) => owner == InteractionOwner::Node(start.clicked),
            ArmedTarget::Connection(start) => connection_uses_owner(*start, owner),
            ArmedTarget::Pan | ArmedTarget::Marquee(_) => false,
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
        InteractionState::EdgeActive(edge) => owner == InteractionOwner::Edge(edge.edge),
        InteractionState::Idle | InteractionState::Pan(_) | InteractionState::Marquee(_) => false,
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
    use jellyflow_core::core::{EdgeId, NodeId, PortId};
    use jellyflow_core::interaction::NodeGraphConnectionMode;

    use super::*;
    use crate::rules::EdgeEndpoint;
    use crate::runtime::geometry::HandlePosition;
    use crate::runtime::interaction::ConnectionProbeTarget;
    use crate::runtime::view_session::ConnectionProbeWork;

    #[test]
    fn connection_stays_armed_below_threshold_and_release_is_a_noop() {
        let mut session = test_session(CanvasInteractionConfig::default());
        session
            .press(connection_target(false), position(0.0, 0.0))
            .unwrap();

        let moved = session.move_to(position(3.0, 0.0)).unwrap();
        assert_eq!(
            session.presentation().phase(),
            CanvasInteractionPhase::Armed
        );
        assert!(moved.queries().is_empty());

        let released = session.release(position(3.0, 0.0)).unwrap();
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert!(released.effects().is_empty());
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
            .select_search_result(generation, target_port(), 0)
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
        let (_, mut effects, _, _) = released.into_parts();
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
        let tick = session.tick(0.1, ViewSize::new(200.0, 120.0)).unwrap();
        let [CanvasInteractionEffect::Viewport(ViewportMutation::PanByView(delta))] =
            tick.effects()
        else {
            panic!("edge-band drag must request auto-pan");
        };
        assert!(delta.x > 0.0 && delta.y > 0.0);
        assert!(delta.x <= 72.0 && delta.y <= 72.0);

        session.owner_retired(InteractionOwner::Node(node));
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::OwnerRetired)
        );
    }

    fn test_session(config: CanvasInteractionConfig) -> CanvasInteractionSession {
        CanvasInteractionSession::new(
            ViewId::new(crate::runtime::graph_runtime::RuntimeId::UNAVAILABLE, 0),
            ViewGeneration::initial(),
            config,
        )
        .unwrap()
    }

    fn connection_target(allow_search: bool) -> CanvasPressTarget {
        CanvasPressTarget::Connection(
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
            .unwrap(),
        )
    }

    fn position(x: f32, y: f32) -> InteractionPosition {
        InteractionPosition::new(ViewPoint::new(x, y), CanvasPoint { x, y }).unwrap()
    }

    fn valid_probe(port: PortId) -> ConnectionProbe {
        ConnectionProbe::Valid {
            revision: 0,
            target: ConnectionProbeTarget::new(
                port,
                NodeId::from_u128(2),
                CanvasPoint { x: 24.0, y: 8.0 },
                HandlePosition::Left,
                0.0,
            ),
        }
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
