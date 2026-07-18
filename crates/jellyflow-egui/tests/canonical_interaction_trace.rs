#[path = "../../jellyflow-runtime/tests/support/canonical_interaction_trace.rs"]
mod trace;

use egui::{
    Color32, Event, Id, Key, Modifiers, PointerButton, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};
use egui_kittest::Harness;
use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node,
    NodeGraphConnectionMode, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};
use jellyflow::runtime::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionPhase, CanvasPressFallback,
    CanvasPressTarget, CommandSettlement, ConnectionGestureIntent, ConnectionInteractionStart,
    ConnectionProbe, ConnectionSearchSelection, EdgeEndpoint, FrameRequest, FreshGraphMutation,
    GraphRuntime, HandlePosition, InteractionTerminalReason, InteractionTicketId, RendererStamp,
    SelectionMode, TopologyEdit, ViewPoint, ViewRect, ViewSession, ViewSessionOptions, ViewSize,
};
use jellyflow_egui::{EguiCanvasInteraction, EguiInteractionOutput, EguiInteractionQueryKind};
use trace::{
    CanonicalInteractionTrace, TraceCancelReason, TraceCandidate, TraceEffect, TraceInput,
    TracePhase, TraceReceipt, TraceReconnectEndpoint, TraceTarget, control_and_drag_handle_traces,
    create_from_wire_trace, edge_pick_trace, new_connection_trace, reconnect_trace,
    terminal_ownership_traces,
};

#[test]
fn egui_native_events_replay_every_canonical_interaction_trace() {
    for receipt in [
        TraceReceipt::Applied,
        TraceReceipt::Noop,
        TraceReceipt::Rejected,
        TraceReceipt::Conflict,
    ] {
        EguiTraceRunner::new().replay(&new_connection_trace(receipt));
        EguiTraceRunner::new().replay(&create_from_wire_trace(receipt));
        for endpoint in [
            TraceReconnectEndpoint::Source,
            TraceReconnectEndpoint::Target,
        ] {
            EguiTraceRunner::new().replay(&reconnect_trace(endpoint, receipt));
        }
    }

    for trace in control_and_drag_handle_traces() {
        EguiTraceRunner::new().replay(&trace);
    }
    EguiTraceRunner::new().replay(&edge_pick_trace());
    for trace in terminal_ownership_traces() {
        EguiTraceRunner::new().replay(&trace);
    }
}

#[test]
fn stolen_egui_drag_ownership_cancels_the_semantic_session() {
    let mut runner = EguiTraceRunner::new();
    runner.press(TraceTarget::Port(1));
    runner.move_pointer(runner.state().anchors.empty);
    assert_eq!(runner.observed_phase(), TracePhase::ConnectionDrag);

    runner.state_mut().steal_capture = true;
    runner.harness.step();
    let output = runner.take_output();

    assert!(output.released_native_ownership());
    assert_eq!(runner.observed_phase(), TracePhase::Idle);
    assert_eq!(
        runner.state().interaction.presentation().terminal(),
        Some(InteractionTerminalReason::Cancelled)
    );
}

#[test]
fn egui_interaction_selection_remains_local_to_its_view() {
    let mut runner = EguiTraceRunner::new();
    let untouched = runner
        .state()
        ._runtime
        .attach_view(ViewSessionOptions::default())
        .expect("second view");

    assert_eq!(
        runner.press(TraceTarget::NodeDragHandle(1)),
        TraceEffect::None
    );
    assert_eq!(runner.release(), TraceEffect::None);

    assert_eq!(
        runner.state().view.selection().unwrap().nodes(),
        [runner.state().ids.source_node]
    );
    assert!(untouched.selection().unwrap().nodes().is_empty());
    assert!(untouched.geometry_snapshot().unwrap().nodes().is_empty());
}

struct EguiTraceRunner {
    harness: Harness<'static, EguiTraceState>,
    pointer: Pos2,
    ticket: Option<InteractionTicketId>,
}

impl EguiTraceRunner {
    fn new() -> Self {
        let state = EguiTraceState::new();
        let pointer = state.anchors.source_pos();
        let harness = Harness::builder()
            .with_size(Vec2::new(900.0, 500.0))
            .build_ui_state(render_trace, state);
        Self {
            harness,
            pointer,
            ticket: None,
        }
    }

    fn replay(&mut self, trace: &CanonicalInteractionTrace) {
        for step in &trace.steps {
            let effect = self.apply(step.input);
            assert_eq!(
                self.observed_phase(),
                step.phase,
                "{} after {:?}",
                trace.name,
                step.input
            );
            assert_eq!(effect, step.effect, "{} after {:?}", trace.name, step.input);
        }
    }

    fn apply(&mut self, input: TraceInput) -> TraceEffect {
        match input {
            TraceInput::Press { target } => self.press(target),
            TraceInput::ThresholdCrossed => {
                let output = self.move_pointer(self.state().anchors.empty);
                semantic_effect(&output, true, &mut self.ticket)
            }
            TraceInput::Move { .. } => {
                let point = self.invalid_anchor();
                let output = self.move_pointer(point);
                semantic_effect(&output, true, &mut self.ticket)
            }
            TraceInput::Candidate(candidate) => {
                if matches!(candidate, TraceCandidate::Valid { .. }) {
                    let point = self.valid_anchor();
                    self.move_pointer(point);
                }
                self.assert_candidate(candidate);
                TraceEffect::None
            }
            TraceInput::Release => self.release(),
            TraceInput::SearchSelected { .. } => self.select_search_result(),
            TraceInput::Receipt(receipt) => self.settle(receipt),
            TraceInput::Cancel(reason) => self.cancel(reason),
            TraceInput::Retire => {
                self.state_mut().response_visible = false;
                self.harness.step();
                let output = self.take_output();
                assert!(output.released_native_ownership());
                TraceEffect::ReleaseNativeOwnership
            }
            TraceInput::Close => {
                let ctx = self.harness.ctx.clone();
                let output = self.state_mut().interaction.close(&ctx);
                assert!(output.released_native_ownership());
                TraceEffect::ReleaseNativeOwnership
            }
        }
    }

    fn press(&mut self, target: TraceTarget) -> TraceEffect {
        let response = match target {
            TraceTarget::NativeControl(_) => TraceResponse::NativeControl,
            TraceTarget::NodeDragHandle(_) => TraceResponse::NodeDrag,
            TraceTarget::Port(_) => TraceResponse::Connection,
            TraceTarget::Edge(_) => TraceResponse::Edge,
            TraceTarget::ReconnectHandle { endpoint, .. } => TraceResponse::Reconnect(endpoint),
            TraceTarget::Canvas => panic!("canonical trace has no unresolved canvas press"),
        };
        self.state_mut().response = Some(response);
        self.state_mut().response_visible = true;
        self.harness.step();
        self.pointer = self.state().response_center(response);
        self.harness.hover_at(self.pointer);
        self.harness.step();
        self.harness.drag_at(self.pointer);
        self.harness.step();
        let output = self.take_output();
        semantic_effect(&output, false, &mut self.ticket)
    }

    fn move_pointer(&mut self, point: ViewPoint) -> EguiInteractionOutput {
        self.pointer = Pos2::new(point.x, point.y);
        self.harness.hover_at(self.pointer);
        self.harness.step();
        self.take_output()
    }

    fn release(&mut self) -> TraceEffect {
        self.harness.event(Event::PointerButton {
            pos: self.pointer,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        self.harness.step();
        let output = self.take_output();
        assert!(output.released_native_ownership());
        semantic_effect(&output, false, &mut self.ticket)
    }

    fn select_search_result(&mut self) -> TraceEffect {
        let ctx = self.harness.ctx.clone();
        let state = self.harness.state_mut();
        let generation = state
            .interaction
            .presentation()
            .generation()
            .expect("search owns a generation");
        state
            .interaction
            .search_ready(&state.view, &ctx, generation, 1)
            .expect("search result count applies");
        let selection = ConnectionSearchSelection::new(
            state.ids.valid_port,
            state.view.graph_snapshot().unwrap().revision(),
            TopologyEdit::new().set_node_data(
                state.ids.valid_node,
                serde_json::json!({ "created_from_wire": true }),
            ),
        )
        .expect("create-from-wire selection carries topology");
        let output = state
            .interaction
            .select_search_result(&state.view, &ctx, generation, selection)
            .expect("search selection applies");
        semantic_effect(&output, false, &mut self.ticket)
    }

    fn settle(&mut self, receipt: TraceReceipt) -> TraceEffect {
        let ticket = self.ticket.take().expect("trace submitted one command");
        let ctx = self.harness.ctx.clone();
        let state = self.harness.state_mut();
        state
            .interaction
            .settle(&state.view, &ctx, ticket, settlement(receipt))
            .expect("receipt settles");
        match receipt {
            TraceReceipt::Applied | TraceReceipt::Noop => TraceEffect::None,
            TraceReceipt::Rejected | TraceReceipt::Conflict => TraceEffect::RestorePresentation,
        }
    }

    fn cancel(&mut self, reason: TraceCancelReason) -> TraceEffect {
        let output = match reason {
            TraceCancelReason::Escape => {
                self.harness.key_down(Key::Escape);
                self.harness.step();
                self.take_output()
            }
            TraceCancelReason::OutsideClick => {
                let ctx = self.harness.ctx.clone();
                let state = self.harness.state_mut();
                state
                    .interaction
                    .cancel(&state.view, &ctx)
                    .expect("outside click cancels")
            }
            TraceCancelReason::SourceRetired | TraceCancelReason::HostClosed => {
                panic!("retirement and close use dedicated trace inputs")
            }
        };
        assert!(output.released_native_ownership());
        TraceEffect::ReleaseNativeOwnership
    }

    fn assert_candidate(&self, candidate: TraceCandidate) {
        let presentation = self.state().interaction.presentation();
        let probe = presentation
            .connection()
            .expect("candidate assertion owns a connection")
            .probe();
        match (candidate, probe) {
            (TraceCandidate::None, ConnectionProbe::None) => {}
            (TraceCandidate::Invalid { .. }, ConnectionProbe::Invalid { diagnostics, .. }) => {
                assert!(!diagnostics.is_empty())
            }
            (TraceCandidate::Valid { .. }, ConnectionProbe::Valid { .. }) => {}
            (candidate, probe) => panic!("trace candidate {candidate:?} disagrees with {probe:?}"),
        }
    }

    fn observed_phase(&self) -> TracePhase {
        let presentation = self.state().interaction.presentation();
        match presentation.phase() {
            CanvasInteractionPhase::Idle | CanvasInteractionPhase::Closed => TracePhase::Idle,
            CanvasInteractionPhase::Armed => TracePhase::Armed,
            CanvasInteractionPhase::NodeDrag => TracePhase::NodeDrag,
            CanvasInteractionPhase::ConnectionDrag
                if presentation
                    .connection()
                    .is_some_and(|connection| connection.probe().is_valid()) =>
            {
                TracePhase::Candidate
            }
            CanvasInteractionPhase::ConnectionDrag => TracePhase::ConnectionDrag,
            CanvasInteractionPhase::ConnectionSearch => TracePhase::Searching,
            CanvasInteractionPhase::Committing => TracePhase::Committing,
            CanvasInteractionPhase::EdgeActive => TracePhase::EdgeActive,
            CanvasInteractionPhase::Pan | CanvasInteractionPhase::Marquee => {
                panic!("canonical fixture did not request this phase")
            }
        }
    }

    fn invalid_anchor(&self) -> ViewPoint {
        match self
            .state()
            .interaction
            .presentation()
            .connection()
            .map(|connection| connection.intent())
        {
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::From,
                ..
            }) => self.state().anchors.valid,
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::To,
                ..
            })
            | Some(ConnectionGestureIntent::Connect { .. })
            | None => self.state().anchors.invalid,
        }
    }

    fn valid_anchor(&self) -> ViewPoint {
        match self
            .state()
            .interaction
            .presentation()
            .connection()
            .map(|connection| connection.intent())
        {
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::From,
                ..
            }) => self.state().anchors.source,
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::To,
                ..
            }) => self.state().anchors.reconnect_alternate,
            Some(ConnectionGestureIntent::Connect { .. }) | None => self.state().anchors.valid,
        }
    }

    fn take_output(&mut self) -> EguiInteractionOutput {
        self.state_mut()
            .last_output
            .take()
            .expect("each egui frame emits adapter output")
    }

    fn state(&self) -> &EguiTraceState {
        self.harness.state()
    }

    fn state_mut(&mut self) -> &mut EguiTraceState {
        self.harness.state_mut()
    }
}

struct EguiTraceState {
    _runtime: GraphRuntime,
    view: ViewSession,
    interaction: EguiCanvasInteraction,
    ids: TraceGraphIds,
    anchors: TraceAnchors,
    response: Option<TraceResponse>,
    response_visible: bool,
    steal_capture: bool,
    last_output: Option<EguiInteractionOutput>,
}

impl EguiTraceState {
    fn new() -> Self {
        let ids = TraceGraphIds::new();
        let anchors = TraceAnchors::new();
        let mut builder = GraphBuilder::new(GraphId::from_u128(1));
        for (node, port, direction) in [
            (ids.source_node, ids.source_port, PortDirection::Out),
            (ids.invalid_node, ids.invalid_port, PortDirection::Out),
            (ids.valid_node, ids.valid_port, PortDirection::In),
            (
                ids.reconnect_source_node,
                ids.reconnect_source_port,
                PortDirection::Out,
            ),
            (
                ids.reconnect_target_node,
                ids.reconnect_target_port,
                PortDirection::In,
            ),
            (
                ids.reconnect_alternate_node,
                ids.reconnect_alternate_port,
                PortDirection::In,
            ),
        ] {
            builder.insert_node(node, trace_node(port));
            builder.insert_port(port, trace_port(node, direction));
        }
        builder.insert_edge(
            ids.reconnect_edge,
            Edge::new(
                EdgeKind::Data,
                ids.reconnect_source_port,
                ids.reconnect_target_port,
            ),
        );
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().expect("trace graph"))
            .expect("trace runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("trace view");
        publish_trace_geometry(&view, ids, anchors);
        let interaction = EguiCanvasInteraction::new(&view, CanvasInteractionConfig::default())
            .expect("egui interaction");
        Self {
            _runtime: runtime,
            view,
            interaction,
            ids,
            anchors,
            response: None,
            response_visible: true,
            steal_capture: false,
            last_output: None,
        }
    }

    fn response_center(&self, response: TraceResponse) -> Pos2 {
        match response {
            TraceResponse::NativeControl | TraceResponse::NodeDrag | TraceResponse::Connection => {
                self.anchors.source_pos()
            }
            TraceResponse::Reconnect(TraceReconnectEndpoint::Source) => {
                self.anchors.reconnect_source_pos()
            }
            TraceResponse::Reconnect(TraceReconnectEndpoint::Target) => {
                self.anchors.reconnect_target_pos()
            }
            TraceResponse::Edge => Pos2::new(400.0, 80.0),
        }
    }
}

#[derive(Clone, Copy)]
enum TraceResponse {
    NativeControl,
    NodeDrag,
    Connection,
    Reconnect(TraceReconnectEndpoint),
    Edge,
}

fn render_trace(ui: &mut Ui, state: &mut EguiTraceState) {
    let scene_rect = ui.max_rect();
    state
        .interaction
        .begin_frame(&state.view, ui.ctx(), scene_rect)
        .expect("begin egui interaction frame");
    if let Some(kind) = state.response.filter(|_| state.response_visible) {
        let center = state.response_center(kind);
        let response = ui.interact(
            Rect::from_center_size(center, Vec2::splat(28.0)),
            Id::new(("canonical-egui-response", response_key(kind))),
            match kind {
                TraceResponse::NativeControl => Sense::click(),
                TraceResponse::NodeDrag
                | TraceResponse::Connection
                | TraceResponse::Reconnect(_)
                | TraceResponse::Edge => Sense::drag(),
            },
        );
        let target = press_target(&state.view, &state.interaction, state.ids, kind);
        state
            .interaction
            .handle_response(&state.view, &response, target)
            .expect("egui response maps to semantic input");
        if state.steal_capture {
            ui.ctx().set_dragged_id(Id::new("foreign-egui-owner"));
            state.steal_capture = false;
        }
    }
    paint_native_evidence(ui, state);
    state.last_output = Some(
        state
            .interaction
            .end_frame(&state.view, ui.ctx())
            .expect("end egui interaction frame"),
    );
}

fn press_target(
    view: &ViewSession,
    interaction: &EguiCanvasInteraction,
    ids: TraceGraphIds,
    response: TraceResponse,
) -> CanvasPressTarget {
    match response {
        TraceResponse::NativeControl => CanvasPressTarget::NativeControl,
        TraceResponse::NodeDrag => interaction
            .node_drag_target(view, ids.source_node, SelectionMode::Replace)
            .expect("node target query")
            .expect("source node is draggable"),
        TraceResponse::Connection => interaction
            .connection_target(
                view,
                ids.source_port,
                EdgeId::from_u128(100),
                NodeGraphConnectionMode::Strict,
                true,
            )
            .expect("connection target query")
            .expect("source port starts a connection"),
        TraceResponse::Reconnect(endpoint) => {
            let graph = view.graph_snapshot().expect("reconnect graph");
            let geometry = view.geometry_snapshot().expect("reconnect geometry");
            let route = geometry
                .route(ids.reconnect_edge)
                .expect("published reconnect route");
            let endpoint = match endpoint {
                TraceReconnectEndpoint::Source => EdgeEndpoint::From,
                TraceReconnectEndpoint::Target => EdgeEndpoint::To,
            };
            CanvasPressTarget::Connection(
                ConnectionInteractionStart::reconnect_from_graph_route(
                    graph.graph(),
                    route,
                    endpoint,
                    NodeGraphConnectionMode::Strict,
                    false,
                )
                .expect("reconnect endpoint is enabled"),
            )
        }
        TraceResponse::Edge => CanvasPressTarget::Canvas(CanvasPressFallback::Pan {
            selection_mode: SelectionMode::Replace,
        }),
    }
}

fn paint_native_evidence(ui: &Ui, state: &EguiTraceState) {
    let presentation = state.interaction.presentation();
    if let Some(connection) = presentation.connection() {
        let (center, color) = match connection.probe() {
            ConnectionProbe::Valid { target, .. } => {
                (target.route_anchor(), Color32::from_rgb(70, 190, 120))
            }
            ConnectionProbe::Invalid { target, .. } => {
                (target.route_anchor(), Color32::from_rgb(220, 85, 80))
            }
            ConnectionProbe::None => return,
        };
        ui.painter().circle_stroke(
            Pos2::new(center.x, center.y),
            9.0,
            Stroke::new(2.0_f32, color),
        );
    }
    if presentation.active_edge().is_some() {
        ui.painter().line_segment(
            [Pos2::new(360.0, 80.0), Pos2::new(440.0, 80.0)],
            Stroke::new(3.0_f32, Color32::from_rgb(85, 145, 235)),
        );
    }
}

fn semantic_effect(
    output: &EguiInteractionOutput,
    query_is_probe: bool,
    ticket: &mut Option<InteractionTicketId>,
) -> TraceEffect {
    match output.effects() {
        [] => {}
        [effect] => match effect {
            CanvasInteractionEffect::Search(_) => return TraceEffect::OpenSearch,
            CanvasInteractionEffect::SubmitGraph {
                ticket: emitted,
                mutation,
            } => {
                *ticket = Some(*emitted);
                return match mutation {
                    FreshGraphMutation::Connection { .. } => TraceEffect::SubmitConnection,
                    FreshGraphMutation::MoveNodes(_) => TraceEffect::SubmitNodeMove,
                    FreshGraphMutation::DeleteEdge(_) => {
                        panic!("canonical traces do not delete an edge")
                    }
                };
            }
            CanvasInteractionEffect::Selection(_) | CanvasInteractionEffect::Viewport(_) => {
                panic!("view-local effects must be consumed inside the egui bridge")
            }
        },
        effects => panic!("one interaction transition emitted unordered effects: {effects:?}"),
    }
    if output
        .resolved_queries()
        .contains(&EguiInteractionQueryKind::EdgePick)
    {
        return TraceEffect::PickEdge;
    }
    if query_is_probe
        && output
            .resolved_queries()
            .contains(&EguiInteractionQueryKind::ConnectionProbe)
    {
        return TraceEffect::ProbeConnection;
    }
    TraceEffect::None
}

fn settlement(receipt: TraceReceipt) -> CommandSettlement {
    match receipt {
        TraceReceipt::Applied => CommandSettlement::Applied,
        TraceReceipt::Noop => CommandSettlement::Noop,
        TraceReceipt::Rejected => CommandSettlement::Rejected,
        TraceReceipt::Conflict => CommandSettlement::Conflict,
    }
}

fn response_key(response: TraceResponse) -> u8 {
    match response {
        TraceResponse::NativeControl => 0,
        TraceResponse::NodeDrag => 1,
        TraceResponse::Connection => 2,
        TraceResponse::Reconnect(TraceReconnectEndpoint::Source) => 3,
        TraceResponse::Reconnect(TraceReconnectEndpoint::Target) => 4,
        TraceResponse::Edge => 5,
    }
}

#[derive(Clone, Copy)]
struct TraceGraphIds {
    source_node: NodeId,
    source_port: PortId,
    invalid_node: NodeId,
    invalid_port: PortId,
    valid_node: NodeId,
    valid_port: PortId,
    reconnect_source_node: NodeId,
    reconnect_source_port: PortId,
    reconnect_target_node: NodeId,
    reconnect_target_port: PortId,
    reconnect_alternate_node: NodeId,
    reconnect_alternate_port: PortId,
    reconnect_edge: EdgeId,
}

impl TraceGraphIds {
    fn new() -> Self {
        Self {
            source_node: NodeId::from_u128(1),
            source_port: PortId::from_u128(1),
            invalid_node: NodeId::from_u128(2),
            invalid_port: PortId::from_u128(2),
            valid_node: NodeId::from_u128(3),
            valid_port: PortId::from_u128(3),
            reconnect_source_node: NodeId::from_u128(4),
            reconnect_source_port: PortId::from_u128(4),
            reconnect_target_node: NodeId::from_u128(5),
            reconnect_target_port: PortId::from_u128(5),
            reconnect_alternate_node: NodeId::from_u128(6),
            reconnect_alternate_port: PortId::from_u128(6),
            reconnect_edge: EdgeId::from_u128(9),
        }
    }
}

#[derive(Clone, Copy)]
struct TraceAnchors {
    source: ViewPoint,
    invalid: ViewPoint,
    valid: ViewPoint,
    reconnect_source: ViewPoint,
    reconnect_target: ViewPoint,
    reconnect_alternate: ViewPoint,
    empty: ViewPoint,
}

impl TraceAnchors {
    fn new() -> Self {
        Self {
            source: ViewPoint::new(60.0, 80.0),
            invalid: ViewPoint::new(160.0, 80.0),
            valid: ViewPoint::new(240.0, 80.0),
            reconnect_source: ViewPoint::new(360.0, 80.0),
            reconnect_target: ViewPoint::new(440.0, 80.0),
            reconnect_alternate: ViewPoint::new(540.0, 80.0),
            empty: ViewPoint::new(700.0, 300.0),
        }
    }

    fn source_pos(self) -> Pos2 {
        Pos2::new(self.source.x, self.source.y)
    }

    fn reconnect_source_pos(self) -> Pos2 {
        Pos2::new(self.reconnect_source.x, self.reconnect_source.y)
    }

    fn reconnect_target_pos(self) -> Pos2 {
        Pos2::new(self.reconnect_target.x, self.reconnect_target.y)
    }
}

fn publish_trace_geometry(view: &ViewSession, ids: TraceGraphIds, anchors: TraceAnchors) {
    let frame = view
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint { x: -50.0, y: -50.0 },
                size: CanvasSize {
                    width: 900.0,
                    height: 500.0,
                },
            },
            RendererStamp::new("trace.node", 1),
        ))
        .expect("trace frame");
    let collector = frame.collector();
    for (node, port, direction, anchor) in [
        (
            ids.source_node,
            ids.source_port,
            PortDirection::Out,
            anchors.source,
        ),
        (
            ids.invalid_node,
            ids.invalid_port,
            PortDirection::Out,
            anchors.invalid,
        ),
        (
            ids.valid_node,
            ids.valid_port,
            PortDirection::In,
            anchors.valid,
        ),
        (
            ids.reconnect_source_node,
            ids.reconnect_source_port,
            PortDirection::Out,
            anchors.reconnect_source,
        ),
        (
            ids.reconnect_target_node,
            ids.reconnect_target_port,
            PortDirection::In,
            anchors.reconnect_target,
        ),
        (
            ids.reconnect_alternate_node,
            ids.reconnect_alternate_port,
            PortDirection::In,
            anchors.reconnect_alternate,
        ),
    ] {
        let frame_node = frame
            .nodes()
            .iter()
            .find(|candidate| candidate.id() == node)
            .expect("trace frame node");
        let frame_port = frame_node
            .ports()
            .iter()
            .find(|candidate| candidate.id() == port)
            .expect("trace frame port");
        let node_bounds = ViewRect::new(
            ViewPoint::new(anchor.x - 30.0, anchor.y - 30.0),
            ViewSize::new(60.0, 60.0),
        );
        let visual = ViewRect::new(
            ViewPoint::new(anchor.x - 5.0, anchor.y - 5.0),
            ViewSize::new(10.0, 10.0),
        );
        let hit = ViewRect::new(
            ViewPoint::new(anchor.x - 12.0, anchor.y - 12.0),
            ViewSize::new(24.0, 24.0),
        );
        let mut layout = frame_node.begin_layout(node_bounds);
        layout
            .port_rendered(
                frame_port.layout_target(),
                visual,
                hit,
                anchor,
                match direction {
                    PortDirection::In => HandlePosition::Left,
                    PortDirection::Out => HandlePosition::Right,
                },
            )
            .expect("trace port layout");
        collector
            .layout(layout.finish().expect("trace node layout"))
            .expect("collect trace layout");
    }
    view.submit_feedback(collector.finish().expect("trace feedback"))
        .expect("publish trace geometry");
}

fn trace_node(port: PortId) -> Node {
    Node {
        kind: NodeKindKey::new("trace.node"),
        kind_version: 1,
        pos: CanvasPoint::default(),
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
        data: serde_json::Value::Null,
    }
}

fn trace_port(node: NodeId, direction: PortDirection) -> Port {
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
        data: serde_json::Value::Null,
    }
}
