use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node,
    NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_runtime::adapter::conformance::{
    ConformanceAction, ConformanceCancelReason, ConformanceCandidate, ConformanceEffect,
    ConformanceGeometry, ConformanceGraph, ConformanceIntent, ConformanceOwnership,
    ConformancePhase, ConformanceScenario, ConformanceScenarioId, ConformanceSelection,
    ConformanceCommandOutcome, ConformanceTarget, ConformanceTerminal, ConformanceTicket,
    ConformanceViewStatus, NormalizedObservation,
};
#[cfg(test)]
use jellyflow_runtime::adapter::conformance::{canonical_scenarios, compare_observations};
use jellyflow_runtime::commands::{
    GraphCommand, IncidentEdgePolicy, NodeGeometryEdit, TopologyEdit,
};
use jellyflow_runtime::geometry::{HandlePosition, ViewPoint, ViewRect, ViewSize};
use jellyflow_runtime::graph::{
    EdgeEndpoint, GraphRuntime, GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot,
};
use jellyflow_runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionPhase,
    CanvasInteractionPresentation, CanvasInteractionSession, CanvasInteractionUpdate,
    CanvasPressTarget, ConnectionGestureBinding, InteractionGraphSubmission, InteractionOwner,
    InteractionTerminalReason, SelectionMode,
};
use jellyflow_runtime::profile::ApplyPipelineError;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec, NodePortSpec};
use jellyflow_runtime::view::{
    FrameRequest, GeometrySnapshot, RendererStamp, ViewCloseReason, ViewSession, ViewSessionOptions,
};
use jellyflow_runtime::viewport::ViewportPointerSample;

const NEW_EDGE_ID: u128 = 100;
const CREATED_NODE_KIND: &str = "external.created-target";
const CREATED_PORT_KEY: &str = "input";

fn main() {}

struct RejectConformanceCommand;

impl GraphRuntimeMiddleware for RejectConformanceCommand {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        Err(ApplyPipelineError::Rejected {
            message: "external conformance command rejected".to_owned(),
            diagnostics: Vec::new(),
        })
    }
}

#[derive(Clone, Copy)]
struct FixtureIds {
    source_node: NodeId,
    source_port: PortId,
    invalid_node: NodeId,
    invalid_port: PortId,
    valid_node: NodeId,
    valid_port: PortId,
    created_node: NodeId,
    created_port: PortId,
    reconnect_source_node: NodeId,
    reconnect_source_port: PortId,
    reconnect_target_node: NodeId,
    reconnect_target_port: PortId,
    reconnect_edge: EdgeId,
}

impl FixtureIds {
    fn new() -> Self {
        Self {
            source_node: NodeId::from_u128(1),
            source_port: PortId::from_u128(11),
            invalid_node: NodeId::from_u128(2),
            invalid_port: PortId::from_u128(12),
            valid_node: NodeId::from_u128(3),
            valid_port: PortId::from_u128(13),
            created_node: NodeId::from_u128(6),
            created_port: PortId::from_u128(16),
            reconnect_source_node: NodeId::from_u128(4),
            reconnect_source_port: PortId::from_u128(14),
            reconnect_target_node: NodeId::from_u128(5),
            reconnect_target_port: PortId::from_u128(15),
            reconnect_edge: EdgeId::from_u128(9),
        }
    }
}

#[derive(Clone, Copy)]
struct FixtureAnchors {
    source: ViewPoint,
    invalid: ViewPoint,
    valid: ViewPoint,
    reconnect_source: ViewPoint,
    reconnect_target: ViewPoint,
    empty: ViewPoint,
}

#[derive(Clone, Copy)]
struct FixturePortSpec {
    node: NodeId,
    port: PortId,
    direction: PortDirection,
    anchor: ViewPoint,
}

impl FixtureAnchors {
    fn new() -> Self {
        Self {
            source: ViewPoint::new(80.0, 96.0),
            invalid: ViewPoint::new(240.0, 96.0),
            valid: ViewPoint::new(400.0, 96.0),
            reconnect_source: ViewPoint::new(560.0, 96.0),
            reconnect_target: ViewPoint::new(720.0, 96.0),
            empty: ViewPoint::new(700.0, 320.0),
        }
    }
}

#[derive(Clone, Copy)]
enum NativeHitRegion {
    Control,
    Node,
    SourcePort,
    Edge,
    ReconnectSource,
    ReconnectTarget,
}

#[derive(Clone, Copy)]
enum NativePointerDestination {
    Empty,
    InvalidCandidate,
    ValidCandidate,
}

#[derive(Clone, Copy)]
enum RawNativeEvent {
    PointerDown(NativeHitRegion),
    PointerMove(NativePointerDestination),
    PointerUp,
    SelectSearchCandidate,
    QueueCommandPrefix,
    DrainCommandPrefix,
    Advance,
    CommandSettled(ConformanceCommandOutcome),
    CaptureLost,
    CaptureReplaced,
    PreviousOwnerReleased,
    Cancel(ConformanceCancelReason),
    SourceRetired,
    HostClosed,
}

struct NativeFrame {
    sequence: u64,
    event: RawNativeEvent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeCaptureToken(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeCaptureOwner {
    Released,
    Adapter(NativeCaptureToken),
    Successor(NativeCaptureToken),
}

struct NativeSurface {
    next_frame: u64,
    next_capture: u64,
    capture: NativeCaptureOwner,
    previous_adapter: Option<NativeCaptureToken>,
}

impl NativeSurface {
    fn new() -> Self {
        Self {
            next_frame: 1,
            next_capture: 1,
            capture: NativeCaptureOwner::Released,
            previous_adapter: None,
        }
    }

    fn frame(&mut self, event: RawNativeEvent) -> NativeFrame {
        let sequence = self.next_frame;
        self.next_frame = self.next_frame.saturating_add(1);
        NativeFrame { sequence, event }
    }

    fn acquire_adapter(&mut self) {
        assert_eq!(self.capture, NativeCaptureOwner::Released);
        let token = NativeCaptureToken(self.next_capture);
        self.next_capture = self.next_capture.saturating_add(1);
        self.capture = NativeCaptureOwner::Adapter(token);
        self.previous_adapter = Some(token);
    }

    fn release_adapter(&mut self) -> bool {
        if matches!(self.capture, NativeCaptureOwner::Adapter(_)) {
            self.capture = NativeCaptureOwner::Released;
            true
        } else {
            false
        }
    }

    fn lose_capture(&mut self) {
        self.capture = NativeCaptureOwner::Released;
    }

    fn replace_capture(&mut self) {
        let NativeCaptureOwner::Adapter(previous) = self.capture else {
            panic!("capture replacement requires the adapter owner")
        };
        self.previous_adapter = Some(previous);
        let successor = NativeCaptureToken(self.next_capture);
        self.next_capture = self.next_capture.saturating_add(1);
        self.capture = NativeCaptureOwner::Successor(successor);
    }

    fn release_previous_owner(&mut self) {
        let previous = self
            .previous_adapter
            .expect("capture replacement remembers the adapter token");
        if self.capture == NativeCaptureOwner::Adapter(previous) {
            self.capture = NativeCaptureOwner::Released;
        }
    }

    fn normalized_ownership(&self) -> ConformanceOwnership {
        match self.capture {
            NativeCaptureOwner::Released => ConformanceOwnership::Released,
            NativeCaptureOwner::Adapter(_) => ConformanceOwnership::HeldByAdapter,
            NativeCaptureOwner::Successor(_) => ConformanceOwnership::HeldBySuccessor,
        }
    }
}

struct RuntimeInteractionBridge {
    session: CanvasInteractionSession,
    pointer: ViewportPointerSample,
    submission: Option<InteractionGraphSubmission>,
    node_intersection_pages: u64,
}

impl RuntimeInteractionBridge {
    fn new(view: &ViewSession, pointer: ViewportPointerSample) -> Self {
        Self::with_config(view, pointer, CanvasInteractionConfig::default())
    }

    fn with_config(
        view: &ViewSession,
        pointer: ViewportPointerSample,
        config: CanvasInteractionConfig,
    ) -> Self {
        let session = CanvasInteractionSession::attach(view, config)
            .expect("external interaction bridge starts");
        Self {
            session,
            pointer,
            submission: None,
            node_intersection_pages: 0,
        }
    }

    fn presentation(&self) -> CanvasInteractionPresentation {
        self.session.presentation()
    }

    fn press(
        &mut self,
        hit: NativeHitRegion,
        view: &ViewSession,
        ids: FixtureIds,
        anchors: FixtureAnchors,
    ) -> ConformanceEffect {
        match hit {
            NativeHitRegion::Control => {
                let update = self
                    .session
                    .press(CanvasPressTarget::NativeControl, self.pointer)
                    .expect("native control press");
                self.process_update(update)
            }
            NativeHitRegion::Node => {
                self.pointer = normalized_position(view, anchors.source);
                let node = view
                    .node_lifetime_key(ids.source_node)
                    .expect("source node identity query")
                    .expect("source node is current");
                let start = view
                    .node_drag_start(node, SelectionMode::Replace)
                    .expect("node drag facts")
                    .expect("source node is draggable");
                let update = self
                    .session
                    .press(CanvasPressTarget::NodeDrag(start), self.pointer)
                    .expect("node press reaches bridge");
                self.process_update(update)
            }
            NativeHitRegion::SourcePort => {
                self.pointer = normalized_position(view, anchors.source);
                let source = view
                    .port_instance_key(ids.source_port)
                    .expect("source port identity query")
                    .expect("source port is current");
                let drag_intent = view
                    .port_connection_drag_intent(source)
                    .expect("source port starts a connection")
                    .into_outcome()
                    .ready()
                    .expect("source port remains current");
                let start = view
                    .connection_start_from_port_drag_intent(
                        drag_intent,
                        EdgeId::from_u128(NEW_EDGE_ID),
                        NodeGraphConnectionMode::Strict,
                        canvas_anchor(view, anchors.source),
                        HandlePosition::Right,
                        HandlePosition::Left,
                        true,
                    )
                    .expect("source connection start belongs to this view")
                    .expect("source port produces a connection target");
                let update = self
                    .session
                    .press(CanvasPressTarget::Connection(start), self.pointer)
                    .expect("connection press reaches bridge");
                self.process_update(update)
            }
            NativeHitRegion::ReconnectSource | NativeHitRegion::ReconnectTarget => {
                let endpoint = match hit {
                    NativeHitRegion::ReconnectSource => EdgeEndpoint::From,
                    NativeHitRegion::ReconnectTarget => EdgeEndpoint::To,
                    NativeHitRegion::Control
                    | NativeHitRegion::Node
                    | NativeHitRegion::SourcePort
                    | NativeHitRegion::Edge => unreachable!(),
                };
                let geometry = view.geometry_snapshot().expect("reconnect geometry");
                let route = geometry
                    .route(ids.reconnect_edge)
                    .expect("published reconnect route");
                let fixed_endpoint = match endpoint {
                    EdgeEndpoint::From => route.facts().endpoints.target,
                    EdgeEndpoint::To => route.facts().endpoints.source,
                };
                let start = view
                    .connection_start_from_route(
                        route,
                        endpoint,
                        fixed_endpoint,
                        NodeGraphConnectionMode::Strict,
                        false,
                    )
                    .expect("reconnect route belongs to this view")
                    .expect("reconnect endpoint starts");
                self.pointer = normalized_position(
                    view,
                    match endpoint {
                        EdgeEndpoint::From => anchors.reconnect_target,
                        EdgeEndpoint::To => anchors.reconnect_source,
                    },
                );
                let update = self
                    .session
                    .press(CanvasPressTarget::Connection(start), self.pointer)
                    .expect("reconnect press reaches bridge");
                self.process_update(update)
            }
            NativeHitRegion::Edge => self.press_edge(view, ids, anchors),
        }
    }

    fn press_edge(
        &mut self,
        view: &ViewSession,
        ids: FixtureIds,
        anchors: FixtureAnchors,
    ) -> ConformanceEffect {
        self.pointer = normalized_position(
            view,
            ViewPoint::new(
                (anchors.reconnect_source.x + anchors.reconnect_target.x) * 0.5,
                anchors.reconnect_source.y,
            ),
        );
        let update = self
            .session
            .request_edge_pick(self.pointer)
            .expect("edge press requests a real pick");
        assert!(update.effects().is_empty());
        assert!(update.query_work_pending());
        self.process_update(update);
        let picked = self
            .session
            .presentation()
            .hovered_edge()
            .expect("canonical edge was picked");
        assert_eq!(picked.edge(), ids.reconnect_edge);
        let update = self
            .session
            .press(
                CanvasPressTarget::Edge {
                    instance: picked,
                    selection_mode: SelectionMode::Replace,
                },
                self.pointer,
            )
            .expect("picked edge activates");
        self.process_update(update);
        ConformanceEffect::PickEdge
    }

    fn move_to(
        &mut self,
        destination: NativePointerDestination,
        view: &ViewSession,
        anchors: FixtureAnchors,
        scenario: ConformanceScenarioId,
    ) -> ConformanceEffect {
        let point = match destination {
            NativePointerDestination::Empty => anchors.empty,
            NativePointerDestination::InvalidCandidate
                if matches!(
                    scenario,
                    ConformanceScenarioId::ReconnectSource(_)
                        | ConformanceScenarioId::ReconnectSourcePrefixWait
                ) =>
            {
                anchors.valid
            }
            NativePointerDestination::InvalidCandidate => anchors.invalid,
            NativePointerDestination::ValidCandidate
                if matches!(
                    scenario,
                    ConformanceScenarioId::ReconnectSource(_)
                        | ConformanceScenarioId::ReconnectSourcePrefixWait
                ) =>
            {
                anchors.source
            }
            NativePointerDestination::ValidCandidate => anchors.valid,
        };
        self.pointer = normalized_position(view, point);
        let update = self
            .session
            .move_to(self.pointer)
            .expect("native pointer move reaches bridge");
        self.process_update(update)
    }

    fn release(&mut self) -> ConformanceEffect {
        let update = self
            .session
            .release(self.pointer)
            .expect("native pointer release reaches bridge");
        self.process_update(update)
    }

    fn select_search_candidate(
        &mut self,
        view: &ViewSession,
        ids: FixtureIds,
    ) -> ConformanceEffect {
        let request = self
            .session
            .search_request()
            .expect("search exposes its active request capability");
        let candidates = self
            .session
            .search_candidates(&request)
            .expect("catalog candidates resolve through the active request");
        let created_kind = NodeKindKey::new(CREATED_NODE_KIND);
        let candidate = candidates
            .candidates()
            .iter()
            .find(|candidate| candidate.node_kind() == &created_kind)
            .expect("the fixture catalog exposes its create-from-wire target");
        let ready = self
            .session
            .search_ready(&request, candidates.candidates().len())
            .expect("one external search candidate is ready");
        self.process_update(ready);
        let creation = view
            .catalog()
            .instantiate_node_with_ids(
                candidate.node_kind(),
                ids.created_node,
                CanvasRect {
                    origin: canvas_anchor(view, FixtureAnchors::new().empty),
                    size: CanvasSize {
                        width: 60.0,
                        height: 60.0,
                    },
                },
                [ids.created_port],
            )
            .expect("the catalog instantiates the selected node kind");
        let target = creation
            .port_id(candidate.port())
            .expect("the catalog creation owns the selected target port");
        let selection = self
            .session
            .create_search_selection(&request, target, creation)
            .expect("the active request seals the catalog creation")
            .expect("the catalog creation remains prospective");
        let update = self
            .session
            .select_search_result(selection)
            .expect("search selection reaches bridge");
        self.process_update(update)
    }

    fn settle(
        &mut self,
        settlement: ConformanceCommandOutcome,
        view: &ViewSession,
        runtime: &GraphRuntime,
        excluded_revision_delta: &mut u64,
    ) -> ConformanceEffect {
        let submission = self
            .submission
            .take()
            .expect("one graph submission is pending");
        let ticket = submission.ticket();
        let update = match settlement {
            ConformanceCommandOutcome::NotDispatched => {
                drop(submission);
                self.session
                    .submission_not_dispatched(ticket)
                    .expect("external undispatched submission settles")
            }
            ConformanceCommandOutcome::Applied
            | ConformanceCommandOutcome::Rejected
            | ConformanceCommandOutcome::Conflict => {
                let command = self
                    .session
                    .bind_submission(submission)
                    .expect("external mutation remains fresh")
                    .expect("external mutation is not a no-op");
                if settlement == ConformanceCommandOutcome::Conflict {
                    let snapshot = runtime
                        .snapshot()
                        .expect("external conflict snapshot remains available");
                    runtime
                        .execute(GraphCommand::set_node_geometry(
                            snapshot.version(),
                            [NodeGeometryEdit::new(FixtureIds::new().invalid_node)
                                .with_position(CanvasPoint { x: 241.0, y: 96.0 })],
                        ))
                        .into_result()
                        .expect("external conflict prefix executes")
                        .applied()
                        .expect("external conflict prefix mutates the graph");
                    *excluded_revision_delta = excluded_revision_delta.saturating_add(1);
                }
                let completion = view
                    .execute(command)
                    .expect("external command transport remains available");
                self.session
                    .complete_submission(ticket, completion)
                    .expect("external command completion settles")
            }
        };
        let effect = self.process_update(update);
        if effect != ConformanceEffect::None {
            return effect;
        }
        match settlement {
            ConformanceCommandOutcome::Applied => ConformanceEffect::None,
            ConformanceCommandOutcome::Rejected
            | ConformanceCommandOutcome::Conflict
            | ConformanceCommandOutcome::NotDispatched => ConformanceEffect::RestorePresentation,
        }
    }

    fn revalidate_after_prefix(&mut self) {
        let Some(submission) = self.submission.take() else {
            return;
        };
        let update = self
            .session
            .resume_submission_after_prefix(submission)
            .expect("session-owned submission validation resumes after the prefix");
        self.process_update(update);
    }

    fn cancel(&mut self) {
        let update = self
            .session
            .cancel()
            .expect("external cancellation restores session-owned view state");
        self.process_update(update);
    }

    fn owner_retired(&mut self, owner: InteractionOwner) {
        let update = self.session.owner_retired(owner);
        self.process_update(update);
    }

    fn close(&mut self) {
        let update = self.session.close();
        self.process_update(update);
    }

    fn process_update(&mut self, update: CanvasInteractionUpdate) -> ConformanceEffect {
        let mut update = update;
        let mut observed = ConformanceEffect::None;
        let mut resolved_probe = false;
        loop {
            let dispatch = update.into_dispatch();
            for effect in dispatch.effects {
                match effect {
                    CanvasInteractionEffect::SubmitGraph { submission } => {
                        let presentation = self.session.presentation();
                        observed = if presentation.connection().is_some() {
                            ConformanceEffect::SubmitConnection
                        } else if presentation.node_drag().is_some() {
                            ConformanceEffect::SubmitNodeMove
                        } else {
                            panic!("canonical external driver received an unsupported submission")
                        };
                        self.submission = Some(submission);
                    }
                    CanvasInteractionEffect::Search(_) => {
                        observed = ConformanceEffect::OpenSearch;
                    }
                }
            }
            if !dispatch.query_work_pending {
                break;
            }
            match self.session.presentation().phase() {
                CanvasInteractionPhase::ConnectionDrag | CanvasInteractionPhase::Committing => {
                    resolved_probe = true;
                }
                CanvasInteractionPhase::Marquee => {
                    self.node_intersection_pages = self.node_intersection_pages.saturating_add(1);
                }
                CanvasInteractionPhase::Idle
                | CanvasInteractionPhase::Closed
                | CanvasInteractionPhase::Armed
                | CanvasInteractionPhase::NodeDrag
                | CanvasInteractionPhase::ConnectionSearch
                | CanvasInteractionPhase::EdgeActive
                | CanvasInteractionPhase::Pan => {}
            }
            let (next, needs_tick, receipt, terminal_error) =
                self.session.advance_query_work(1).into_parts();
            assert_eq!(receipt.pages_granted(), 1);
            assert_eq!(receipt.pages_attempted(), 1);
            assert!(terminal_error.is_none());
            assert_eq!(needs_tick, next.query_work_pending());
            update = next;
        }
        if observed == ConformanceEffect::None && resolved_probe {
            ConformanceEffect::ProbeConnection
        } else {
            observed
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeHostLifecycle {
    Open,
    Closed,
}

struct MinimalNativeHost {
    runtime: GraphRuntime,
    view: ViewSession,
    bridge: RuntimeInteractionBridge,
    surface: NativeSurface,
    lifecycle: NativeHostLifecycle,
    ids: FixtureIds,
    anchors: FixtureAnchors,
    baseline_revision: u64,
    excluded_revision_delta: u64,
    prefix_held: bool,
    last_frame: u64,
    scenario: ConformanceScenarioId,
    baseline_source_position: CanvasPoint,
    baseline_source_bounds: CanvasRect,
}

impl MinimalNativeHost {
    fn new(scenario: ConformanceScenarioId) -> Self {
        let ids = FixtureIds::new();
        let anchors = FixtureAnchors::new();
        let runtime = GraphRuntime::builder(fixture_catalog())
            .build(fixture_graph(ids, anchors))
            .expect("external conformance runtime starts");
        let runtime = match scenario_command_outcome(scenario) {
            Some(ConformanceCommandOutcome::Rejected) => {
                runtime.with_middleware(RejectConformanceCommand)
            }
            Some(
                ConformanceCommandOutcome::Applied
                | ConformanceCommandOutcome::Conflict
                | ConformanceCommandOutcome::NotDispatched,
            )
            | None => runtime,
        };
        let baseline_revision = runtime
            .snapshot()
            .expect("external baseline snapshot")
            .revision();
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("external view attaches");
        publish_geometry(&view, ids, anchors);
        let baseline_source_position = runtime
            .snapshot()
            .expect("external source baseline")
            .graph()
            .node(&ids.source_node)
            .expect("external source node")
            .pos;
        let baseline_source_bounds = view
            .geometry_snapshot()
            .expect("external geometry baseline")
            .node(ids.source_node)
            .expect("external source geometry")
            .bounds();
        if matches!(
            scenario,
            ConformanceScenarioId::ReconnectSource(_)
                | ConformanceScenarioId::ReconnectTarget(_)
                | ConformanceScenarioId::ReconnectSourcePrefixWait
                | ConformanceScenarioId::ReconnectTargetPrefixWait
        ) {
            let edge = view
                .edge_instance_key(ids.reconnect_edge)
                .expect("reconnect edge identity query")
                .expect("reconnect edge is current");
            view.set_selection(Vec::new(), vec![edge], Vec::new())
                .expect("seed external reconnect selection");
        }
        let pointer = normalized_position(&view, anchors.source);
        let bridge = RuntimeInteractionBridge::new(&view, pointer);
        Self {
            runtime,
            view,
            bridge,
            surface: NativeSurface::new(),
            lifecycle: NativeHostLifecycle::Open,
            ids,
            anchors,
            baseline_revision,
            excluded_revision_delta: 0,
            prefix_held: false,
            last_frame: 0,
            scenario,
            baseline_source_position,
            baseline_source_bounds,
        }
    }

    fn native_frame(&mut self, event: RawNativeEvent) -> NativeFrame {
        self.surface.frame(event)
    }

    fn dispatch(&mut self, frame: NativeFrame) -> ConformanceEffect {
        assert_eq!(frame.sequence, self.last_frame.saturating_add(1));
        self.last_frame = frame.sequence;
        match frame.event {
            RawNativeEvent::PointerDown(hit) => {
                let effect = self.bridge.press(hit, &self.view, self.ids, self.anchors);
                if !matches!(hit, NativeHitRegion::Control) {
                    self.surface.acquire_adapter();
                }
                effect
            }
            RawNativeEvent::PointerMove(destination) => {
                self.bridge
                    .move_to(destination, &self.view, self.anchors, self.scenario)
            }
            RawNativeEvent::PointerUp => {
                let effect = self.bridge.release();
                self.surface.release_adapter();
                effect
            }
            RawNativeEvent::SelectSearchCandidate => {
                self.bridge.select_search_candidate(&self.view, self.ids)
            }
            RawNativeEvent::QueueCommandPrefix => {
                assert!(!self.prefix_held);
                self.prefix_held = true;
                ConformanceEffect::None
            }
            RawNativeEvent::DrainCommandPrefix => {
                self.drain_prefix();
                ConformanceEffect::None
            }
            RawNativeEvent::Advance => ConformanceEffect::None,
            RawNativeEvent::CommandSettled(settlement) => {
                assert!(!self.prefix_held, "settlement cannot bypass its prefix");
                self.bridge.settle(
                    settlement,
                    &self.view,
                    &self.runtime,
                    &mut self.excluded_revision_delta,
                )
            }
            RawNativeEvent::CaptureLost => {
                self.surface.lose_capture();
                self.bridge.cancel();
                ConformanceEffect::ReleaseNativeOwnership
            }
            RawNativeEvent::CaptureReplaced => {
                self.surface.replace_capture();
                self.bridge.cancel();
                ConformanceEffect::ReleaseNativeOwnership
            }
            RawNativeEvent::PreviousOwnerReleased => {
                self.surface.release_previous_owner();
                ConformanceEffect::None
            }
            RawNativeEvent::Cancel(reason) => {
                assert!(matches!(
                    reason,
                    ConformanceCancelReason::Escape | ConformanceCancelReason::OutsideClick
                ));
                self.bridge.cancel();
                self.surface.release_adapter();
                ConformanceEffect::ReleaseNativeOwnership
            }
            RawNativeEvent::SourceRetired => {
                self.retire_source();
                ConformanceEffect::ReleaseNativeOwnership
            }
            RawNativeEvent::HostClosed => {
                self.bridge.close();
                self.surface.release_adapter();
                let outcome = self.view.close().expect("external host closes its view");
                assert_eq!(outcome.reason(), ViewCloseReason::HostClosed);
                self.lifecycle = NativeHostLifecycle::Closed;
                ConformanceEffect::ReleaseNativeOwnership
            }
        }
    }

    fn drain_prefix(&mut self) {
        assert!(self.prefix_held, "external prefix was queued first");
        let snapshot = self.runtime.snapshot().expect("external prefix snapshot");
        self.runtime
            .execute(GraphCommand::set_node_geometry(
                snapshot.version(),
                [
                    NodeGeometryEdit::new(self.ids.invalid_node).with_position(CanvasPoint {
                        x: self.anchors.invalid.x + 1.0,
                        y: self.anchors.invalid.y,
                    }),
                ],
            ))
            .into_result()
            .expect("external prefix command executes")
            .applied()
            .expect("external prefix mutates the graph");
        self.excluded_revision_delta = self.excluded_revision_delta.saturating_add(1);
        self.prefix_held = false;
        self.bridge.revalidate_after_prefix();
    }

    fn retire_source(&mut self) {
        let snapshot = self
            .runtime
            .snapshot()
            .expect("external retirement snapshot");
        let owner = snapshot
            .node_lifetime_key(self.ids.source_node)
            .expect("external source lifetime is current");
        self.runtime
            .execute(GraphCommand::topology(
                snapshot.version(),
                TopologyEdit::new().remove_node(self.ids.source_node, IncidentEdgePolicy::Reject),
            ))
            .into_result()
            .expect("external source retirement executes")
            .applied()
            .expect("external source retirement applies");
        self.excluded_revision_delta = self.excluded_revision_delta.saturating_add(1);
        self.bridge.owner_retired(InteractionOwner::Node(owner));
        self.surface.release_adapter();
    }

    fn observation(&self, effect: ConformanceEffect) -> NormalizedObservation {
        let presentation = self.bridge.presentation();
        let graph = self.observed_graph();
        let geometry = self.observed_geometry(graph);
        let mut observation = NormalizedObservation::new(
            normalized_phase(&presentation),
            effect,
            self.surface.normalized_ownership(),
            if self.bridge.submission.is_some() {
                ConformanceTicket::Pending
            } else {
                ConformanceTicket::None
            },
            self.runtime
                .snapshot()
                .expect("external runtime remains available")
                .revision()
                .saturating_sub(self.baseline_revision)
                .saturating_sub(self.excluded_revision_delta),
            geometry,
        );
        observation.terminal = presentation.terminal().map(normalized_terminal);
        observation.intent = presentation
            .connection_intent()
            .map_or(ConformanceIntent::None, normalized_intent);
        observation.graph = graph;
        observation.optimistic_presentation = self.bridge.submission.is_some()
            && (presentation.connection().is_some() || presentation.node_drag().is_some());
        observation.selection = normalized_selection(&self.view, self.ids);
        observation.view_status = match self.view.close_reason() {
            None => ConformanceViewStatus::Open,
            Some(ViewCloseReason::HostClosed) => ConformanceViewStatus::HostClosed,
            Some(ViewCloseReason::RuntimeDropped) => ConformanceViewStatus::RuntimeDropped,
        };
        assert_eq!(
            self.lifecycle == NativeHostLifecycle::Closed,
            observation.view_status != ConformanceViewStatus::Open,
        );
        observation
    }

    fn observed_graph(&self) -> ConformanceGraph {
        let snapshot = self
            .runtime
            .snapshot()
            .expect("external graph remains available");
        let graph = snapshot.graph();
        if graph.node(&self.ids.source_node).is_none() {
            assert!(graph.port(&self.ids.source_port).is_none());
            return ConformanceGraph::SourceRetired;
        }
        if let Some(edge) = graph.edge(&EdgeId::from_u128(NEW_EDGE_ID)) {
            return match (edge.from, edge.to) {
                (from, to) if from == self.ids.source_port && to == self.ids.valid_port => {
                    ConformanceGraph::NewConnection
                }
                (from, to) if from == self.ids.source_port && to == self.ids.created_port => {
                    assert!(graph.node(&self.ids.created_node).is_some());
                    assert!(graph.port(&self.ids.created_port).is_some());
                    ConformanceGraph::CreateFromWire
                }
                endpoints => panic!("unexpected external new edge endpoints: {endpoints:?}"),
            };
        }
        let reconnect = graph
            .edge(&self.ids.reconnect_edge)
            .expect("external reconnect edge remains present");
        match (reconnect.from, reconnect.to) {
            (from, to) if from == self.ids.source_port && to == self.ids.reconnect_target_port => {
                ConformanceGraph::ReconnectSource
            }
            (from, to) if from == self.ids.reconnect_source_port && to == self.ids.valid_port => {
                ConformanceGraph::ReconnectTarget
            }
            (from, to)
                if from == self.ids.reconnect_source_port
                    && to == self.ids.reconnect_target_port =>
            {
                match graph.node(&self.ids.source_node) {
                    Some(source) if source.pos != self.baseline_source_position => {
                        let expected = CanvasPoint {
                            x: self.baseline_source_position.x + self.anchors.empty.x
                                - self.anchors.source.x,
                            y: self.baseline_source_position.y + self.anchors.empty.y
                                - self.anchors.source.y,
                        };
                        assert!((source.pos.x - expected.x).abs() < 0.001);
                        assert!((source.pos.y - expected.y).abs() < 0.001);
                        ConformanceGraph::NodeMoved
                    }
                    Some(_) | None => ConformanceGraph::Baseline,
                }
            }
            endpoints => panic!("unexpected external reconnect endpoints: {endpoints:?}"),
        }
    }

    fn observed_geometry(&self, graph: ConformanceGraph) -> ConformanceGeometry {
        let Ok(geometry) = self.view.geometry_snapshot() else {
            return ConformanceGeometry::Unavailable;
        };
        for port in [
            self.ids.valid_port,
            self.ids.reconnect_source_port,
            self.ids.reconnect_target_port,
        ] {
            let published = geometry
                .port(port)
                .unwrap_or_else(|| panic!("external geometry is missing port {port:?}"));
            assert!(published.hit_bounds().is_some());
        }
        match graph {
            ConformanceGraph::Baseline => {
                self.assert_route_matches(
                    &geometry,
                    self.ids.reconnect_edge,
                    self.ids.reconnect_source_port,
                    self.ids.reconnect_target_port,
                );
                ConformanceGeometry::Baseline
            }
            ConformanceGraph::SourceRetired => {
                assert!(geometry.node(self.ids.source_node).is_none());
                assert!(geometry.port(self.ids.source_port).is_none());
                ConformanceGeometry::SourceRetired
            }
            ConformanceGraph::NewConnection => {
                self.assert_route_matches(
                    &geometry,
                    EdgeId::from_u128(NEW_EDGE_ID),
                    self.ids.source_port,
                    self.ids.valid_port,
                );
                ConformanceGeometry::NewConnectionRouted
            }
            ConformanceGraph::ReconnectSource => {
                self.assert_route_matches(
                    &geometry,
                    self.ids.reconnect_edge,
                    self.ids.source_port,
                    self.ids.reconnect_target_port,
                );
                ConformanceGeometry::ReconnectSourceRouted
            }
            ConformanceGraph::ReconnectTarget => {
                self.assert_route_matches(
                    &geometry,
                    self.ids.reconnect_edge,
                    self.ids.reconnect_source_port,
                    self.ids.valid_port,
                );
                ConformanceGeometry::ReconnectTargetRouted
            }
            ConformanceGraph::CreateFromWire => {
                assert!(geometry.port(self.ids.created_port).is_none());
                assert!(geometry.route(EdgeId::from_u128(NEW_EDGE_ID)).is_none());
                ConformanceGeometry::CreateFromWireAwaitingLayout
            }
            ConformanceGraph::NodeMoved => {
                let snapshot = self.runtime.snapshot().unwrap();
                let source_position = snapshot.graph().node(&self.ids.source_node).unwrap().pos;
                let source_bounds = geometry.node(self.ids.source_node).unwrap().bounds();
                let dx = source_position.x - self.baseline_source_position.x;
                let dy = source_position.y - self.baseline_source_position.y;
                assert!(
                    (source_bounds.origin.x - self.baseline_source_bounds.origin.x - dx).abs()
                        < 0.001
                );
                assert!(
                    (source_bounds.origin.y - self.baseline_source_bounds.origin.y - dy).abs()
                        < 0.001
                );
                ConformanceGeometry::NodeMoved
            }
        }
    }

    fn assert_route_matches(
        &self,
        geometry: &GeometrySnapshot,
        edge: EdgeId,
        from: PortId,
        to: PortId,
    ) {
        let route = geometry
            .route(edge)
            .unwrap_or_else(|| panic!("external geometry is missing route {edge:?}"));
        let source = geometry.port(from).expect("external route source geometry");
        let target = geometry.port(to).expect("external route target geometry");
        assert_eq!(route.facts().endpoints.source.point, source.route_anchor());
        assert_eq!(route.facts().endpoints.target.point, target.route_anchor());
    }

    fn shutdown(&mut self) {
        if self.lifecycle == NativeHostLifecycle::Open {
            self.bridge.close();
            self.surface.release_adapter();
            self.view.close().expect("external test host shuts down");
            self.lifecycle = NativeHostLifecycle::Closed;
        }
        if matches!(self.surface.capture, NativeCaptureOwner::Successor(_)) {
            self.surface.capture = NativeCaptureOwner::Released;
        }
    }
}

struct ExternalNativeDriver {
    host: MinimalNativeHost,
}

impl ExternalNativeDriver {
    fn for_scenario(scenario: ConformanceScenarioId) -> Self {
        Self {
            host: MinimalNativeHost::new(scenario),
        }
    }

    fn drive(&mut self, scenario: &ConformanceScenario) -> Vec<NormalizedObservation> {
        scenario
            .steps()
            .iter()
            .map(|step| self.dispatch_action(step.action()))
            .collect()
    }

    fn dispatch_action(&mut self, action: ConformanceAction) -> NormalizedObservation {
        let event = match action {
            ConformanceAction::Press(target) => RawNativeEvent::PointerDown(match target {
                ConformanceTarget::NativeControl => NativeHitRegion::Control,
                ConformanceTarget::NodeDragHandle => NativeHitRegion::Node,
                ConformanceTarget::SourcePort => NativeHitRegion::SourcePort,
                ConformanceTarget::Edge => NativeHitRegion::Edge,
                ConformanceTarget::ReconnectSource => NativeHitRegion::ReconnectSource,
                ConformanceTarget::ReconnectTarget => NativeHitRegion::ReconnectTarget,
            }),
            ConformanceAction::CrossDragThreshold => {
                RawNativeEvent::PointerMove(NativePointerDestination::Empty)
            }
            ConformanceAction::MoveToCandidate(candidate) => {
                RawNativeEvent::PointerMove(match candidate {
                    ConformanceCandidate::None => NativePointerDestination::Empty,
                    ConformanceCandidate::Invalid => NativePointerDestination::InvalidCandidate,
                    ConformanceCandidate::Valid => NativePointerDestination::ValidCandidate,
                })
            }
            ConformanceAction::Release => RawNativeEvent::PointerUp,
            ConformanceAction::SelectSearchCandidate => RawNativeEvent::SelectSearchCandidate,
            ConformanceAction::QueuePrefix => RawNativeEvent::QueueCommandPrefix,
            ConformanceAction::DrainPrefix => RawNativeEvent::DrainCommandPrefix,
            ConformanceAction::AdvanceFrame => RawNativeEvent::Advance,
            ConformanceAction::CompleteCommand(settlement) => RawNativeEvent::CommandSettled(settlement),
            ConformanceAction::LoseCapture => RawNativeEvent::CaptureLost,
            ConformanceAction::ReplaceOwner => RawNativeEvent::CaptureReplaced,
            ConformanceAction::ReleasePreviousOwner => RawNativeEvent::PreviousOwnerReleased,
            ConformanceAction::Cancel(reason) => RawNativeEvent::Cancel(reason),
            ConformanceAction::RetireSource => RawNativeEvent::SourceRetired,
            ConformanceAction::CloseHost => RawNativeEvent::HostClosed,
        };
        let frame = self.host.native_frame(event);
        let effect = self.host.dispatch(frame);
        self.host.observation(effect)
    }
}

fn normalized_phase(presentation: &CanvasInteractionPresentation) -> ConformancePhase {
    match presentation.phase() {
        CanvasInteractionPhase::Idle | CanvasInteractionPhase::Closed => ConformancePhase::Idle,
        CanvasInteractionPhase::Armed => ConformancePhase::Armed,
        CanvasInteractionPhase::NodeDrag => ConformancePhase::NodeDrag,
        CanvasInteractionPhase::ConnectionDrag
            if presentation
                .connection()
                .is_some_and(|connection| connection.probe().is_committable()) =>
        {
            ConformancePhase::Candidate
        }
        CanvasInteractionPhase::ConnectionDrag => ConformancePhase::ConnectionDrag,
        CanvasInteractionPhase::ConnectionSearch => ConformancePhase::Searching,
        CanvasInteractionPhase::Committing => ConformancePhase::Committing,
        CanvasInteractionPhase::EdgeActive => ConformancePhase::EdgeActive,
        CanvasInteractionPhase::Pan | CanvasInteractionPhase::Marquee => {
            panic!("external canonical driver entered a navigation phase")
        }
    }
}

fn normalized_selection(view: &ViewSession, ids: FixtureIds) -> ConformanceSelection {
    match view.selection() {
        Ok(selection)
            if selection.nodes().is_empty()
                && selection.edges().is_empty()
                && selection.groups().is_empty() =>
        {
            ConformanceSelection::Empty
        }
        Ok(selection)
            if selection.nodes() == [ids.source_node]
                && selection.edges().is_empty()
                && selection.groups().is_empty() =>
        {
            ConformanceSelection::SourceNode
        }
        Ok(selection)
            if selection.nodes().is_empty()
                && selection.edges() == [ids.reconnect_edge]
                && selection.groups().is_empty() =>
        {
            ConformanceSelection::CanonicalEdge
        }
        Ok(selection) => panic!("unexpected external selection: {selection:?}"),
        Err(_) => ConformanceSelection::Unavailable,
    }
}

fn normalized_terminal(reason: InteractionTerminalReason) -> ConformanceTerminal {
    match reason {
        InteractionTerminalReason::Applied => ConformanceTerminal::Applied,
        InteractionTerminalReason::Noop => {
            panic!("external conformance does not model a no-op command")
        }
        InteractionTerminalReason::Rejected => ConformanceTerminal::Rejected,
        InteractionTerminalReason::Conflict => ConformanceTerminal::Conflict,
        InteractionTerminalReason::TransportFailure => ConformanceTerminal::NotDispatched,
        InteractionTerminalReason::Cancelled => ConformanceTerminal::Cancelled,
        InteractionTerminalReason::OwnerRetired => ConformanceTerminal::OwnerRetired,
        InteractionTerminalReason::ViewClosed => ConformanceTerminal::HostClosed,
        InteractionTerminalReason::ExecutionFailed
        | InteractionTerminalReason::Uncertain
        | InteractionTerminalReason::BudgetExhausted
        | InteractionTerminalReason::GeometryIndeterminate
        | InteractionTerminalReason::IdentityExhausted => {
            panic!("unexpected external terminal reason: {reason:?}")
        }
    }
}

fn normalized_intent(binding: ConnectionGestureBinding) -> ConformanceIntent {
    if binding.reconnect_edge().is_none() {
        return ConformanceIntent::Connect;
    }
    match binding.fixed_endpoint() {
        EdgeEndpoint::To => ConformanceIntent::ReconnectSource,
        EdgeEndpoint::From => ConformanceIntent::ReconnectTarget,
    }
}

fn scenario_command_outcome(scenario: ConformanceScenarioId) -> Option<ConformanceCommandOutcome> {
    match scenario {
        ConformanceScenarioId::NewConnection(settlement)
        | ConformanceScenarioId::ReconnectSource(settlement)
        | ConformanceScenarioId::ReconnectTarget(settlement)
        | ConformanceScenarioId::CreateFromWire(settlement)
        | ConformanceScenarioId::NodeDrag(settlement) => Some(settlement),
        ConformanceScenarioId::NewConnectionPrefixWait
        | ConformanceScenarioId::NewConnectionProbePrefix
        | ConformanceScenarioId::ReconnectSourcePrefixWait
        | ConformanceScenarioId::ReconnectTargetPrefixWait
        | ConformanceScenarioId::CreateFromWirePrefixWait
        | ConformanceScenarioId::NodeDragPrefixWait
        | ConformanceScenarioId::NodePressBelowThreshold
        | ConformanceScenarioId::NativeControl
        | ConformanceScenarioId::EdgePick
        | ConformanceScenarioId::CaptureLost
        | ConformanceScenarioId::OwnerReplaced
        | ConformanceScenarioId::Escape
        | ConformanceScenarioId::OutsideClick
        | ConformanceScenarioId::SourceRetired
        | ConformanceScenarioId::HostClosed => None,
    }
}

fn normalized_position(view: &ViewSession, point: ViewPoint) -> ViewportPointerSample {
    view.viewport_pointer_sample(point)
        .expect("external pointer is finite")
}

fn canvas_anchor(view: &ViewSession, point: ViewPoint) -> CanvasPoint {
    point
        .to_canvas_point(
            view.viewport_transform()
                .expect("external viewport is available"),
        )
        .expect("external anchor is finite")
}

fn fixture_graph(ids: FixtureIds, anchors: FixtureAnchors) -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xada0));
    for spec in fixture_ports(ids, anchors) {
        graph.insert_node(spec.node, fixture_node(spec.port, spec.anchor));
        graph.insert_port(spec.port, fixture_port(spec.node, spec.direction));
    }
    graph.insert_edge(
        ids.reconnect_edge,
        Edge::new(
            EdgeKind::Data,
            ids.reconnect_source_port,
            ids.reconnect_target_port,
        ),
    );
    graph.build().expect("external conformance graph is valid")
}

fn fixture_catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(CREATED_NODE_KIND)
                .port(NodePortSpec::new(
                    CREATED_PORT_KEY,
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Multi,
                ))
                .build(),
        )
        .expect("external create-from-wire kind registers");
    catalog
}

fn fixture_node(port: PortId, anchor: ViewPoint) -> Node {
    Node {
        kind: NodeKindKey::new("external.adapter-conformance"),
        kind_version: 1,
        pos: CanvasPoint {
            x: anchor.x - 30.0,
            y: anchor.y - 30.0,
        },
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 60.0,
            height: 60.0,
        }),
        hidden: false,
        collapsed: false,
        ports: vec![port],
        data: Default::default(),
    }
}

fn fixture_port(node: NodeId, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new("port"),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: Default::default(),
    }
}

fn fixture_ports(ids: FixtureIds, anchors: FixtureAnchors) -> [FixturePortSpec; 5] {
    [
        FixturePortSpec {
            node: ids.source_node,
            port: ids.source_port,
            direction: PortDirection::Out,
            anchor: anchors.source,
        },
        FixturePortSpec {
            node: ids.invalid_node,
            port: ids.invalid_port,
            direction: PortDirection::Out,
            anchor: anchors.invalid,
        },
        FixturePortSpec {
            node: ids.valid_node,
            port: ids.valid_port,
            direction: PortDirection::In,
            anchor: anchors.valid,
        },
        FixturePortSpec {
            node: ids.reconnect_source_node,
            port: ids.reconnect_source_port,
            direction: PortDirection::Out,
            anchor: anchors.reconnect_source,
        },
        FixturePortSpec {
            node: ids.reconnect_target_node,
            port: ids.reconnect_target_port,
            direction: PortDirection::In,
            anchor: anchors.reconnect_target,
        },
    ]
}

fn publish_geometry(view: &ViewSession, ids: FixtureIds, anchors: FixtureAnchors) {
    let frame = view
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 900.0,
                    height: 500.0,
                },
            },
            RendererStamp::new("external.adapter-conformance", 1),
        ))
        .expect("external fixture frame materializes");
    let collector = frame.collector();
    for spec in fixture_ports(ids, anchors) {
        let frame_node = frame
            .nodes()
            .iter()
            .find(|candidate| candidate.id() == spec.node)
            .expect("external fixture node is admitted");
        let frame_port = frame_node
            .ports()
            .iter()
            .find(|candidate| candidate.id() == spec.port)
            .expect("external fixture port belongs to its node");
        let root = ViewRect::new(
            ViewPoint::new(spec.anchor.x - 30.0, spec.anchor.y - 30.0),
            ViewSize::new(60.0, 60.0),
        );
        let visual = ViewRect::new(
            ViewPoint::new(spec.anchor.x - 5.0, spec.anchor.y - 5.0),
            ViewSize::new(10.0, 10.0),
        );
        let hit = ViewRect::new(
            ViewPoint::new(spec.anchor.x - 12.0, spec.anchor.y - 12.0),
            ViewSize::new(24.0, 24.0),
        );
        let mut layout = frame_node.begin_layout(root);
        layout
            .port_rendered(
                frame_port.layout_target(),
                visual,
                hit,
                spec.anchor,
                match spec.direction {
                    PortDirection::In => HandlePosition::Left,
                    PortDirection::Out => HandlePosition::Right,
                },
            )
            .expect("external fixture port layout is valid");
        collector
            .layout(layout.finish().expect("external node reports every port"))
            .expect("external layout enters feedback");
    }
    view.submit_feedback(collector.finish().expect("external feedback finishes once"))
        .expect("external geometry publishes");
}

#[test]
fn consumer_owned_native_driver_executes_every_public_canonical_scenario() {
    for scenario in canonical_scenarios() {
        let mut driver = ExternalNativeDriver::for_scenario(scenario.id());
        let observations = driver.drive(&scenario);
        compare_observations(&scenario, &observations).unwrap_or_else(|error| {
            panic!(
                "external native scenario {} diverged: {error}",
                scenario.display_name()
            )
        });
        driver.host.shutdown();
    }
}

#[test]
fn consumer_owned_bridge_drives_node_intersection_to_a_complete_page() {
    let ids = FixtureIds::new();
    let anchors = FixtureAnchors::new();
    let runtime = GraphRuntime::builder(fixture_catalog())
        .build(fixture_graph(ids, anchors))
        .expect("external paging runtime starts");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("external paging view attaches");
    publish_geometry(&view, ids, anchors);

    let start = normalized_position(&view, ViewPoint::new(-20.0, 20.0));
    let end = normalized_position(&view, ViewPoint::new(850.0, 180.0));
    let mut bridge = RuntimeInteractionBridge::with_config(
        &view,
        start,
        CanvasInteractionConfig {
            query_work_budget: 1,
            ..CanvasInteractionConfig::default()
        },
    );
    bridge
        .session
        .press(
            CanvasPressTarget::Marquee {
                selection_mode: SelectionMode::Replace,
                baseline: view
                    .selection_baseline()
                    .expect("external marquee owns a selection capability"),
            },
            start,
        )
        .expect("external marquee starts");
    let update = bridge
        .session
        .move_to(end)
        .expect("external marquee requests membership");
    bridge.process_update(update);

    assert!(
        bridge.node_intersection_pages > 1,
        "one-entry query budget must exercise cursor continuation"
    );
    assert_eq!(
        view.selection()
            .expect("external selection remains readable")
            .nodes()
            .len(),
        5
    );
    bridge.close();
    view.close().expect("external paging view closes");
}
