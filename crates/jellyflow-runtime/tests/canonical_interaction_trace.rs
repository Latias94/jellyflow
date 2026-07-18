#[path = "support/canonical_interaction_trace.rs"]
mod trace;

use trace::{
    CanonicalInteractionTrace, TraceCancelReason, TraceCandidate, TraceEffect, TraceInput,
    TracePhase, TraceReceipt, TraceReconnectEndpoint, TraceTarget, control_and_drag_handle_traces,
    create_from_wire_trace, edge_pick_trace, new_connection_trace, reconnect_trace,
    terminal_ownership_traces,
};

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node,
    NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_runtime::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionPhase,
    CanvasInteractionQuery, CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressTarget,
    CommandSettlement, ConnectionGestureIntent, ConnectionInteractionStart, ConnectionProbe,
    ConnectionProbeCursorStaleReason, ConnectionProbeQuery, ConnectionProbeQueryError,
    ConnectionProbeQueryOutcome, EdgeEndpoint, EdgePickCursorStaleReason, EdgePickQuery,
    EdgePickQueryError, EdgePickQueryOutcome, FrameRequest, GraphRuntime, HandlePosition,
    InteractionOwner, InteractionPosition, InteractionTicketId, NodeDragStart, RendererStamp,
    SelectionMode, ViewPoint, ViewRect, ViewSession, ViewSessionOptions, ViewSize,
};

#[test]
fn canonical_trace_vocabulary_is_deterministic_and_covers_terminal_receipts() {
    for receipt in [
        TraceReceipt::Applied,
        TraceReceipt::Noop,
        TraceReceipt::Rejected,
        TraceReceipt::Conflict,
    ] {
        let trace = new_connection_trace(receipt);
        assert_eq!(trace, new_connection_trace(receipt));
        assert_eq!(
            trace.steps.last().expect("terminal step").phase,
            TracePhase::Idle
        );
        assert_eq!(
            trace
                .steps
                .iter()
                .filter(|step| step.effect == TraceEffect::SubmitConnection)
                .count(),
            1,
        );

        for endpoint in [
            TraceReconnectEndpoint::Source,
            TraceReconnectEndpoint::Target,
        ] {
            let reconnect = reconnect_trace(endpoint, receipt);
            assert_eq!(
                reconnect
                    .steps
                    .iter()
                    .filter(|step| step.effect == TraceEffect::SubmitConnection)
                    .count(),
                1,
            );
        }
    }
}

#[test]
fn search_and_native_control_traces_keep_terminal_ownership_explicit() {
    let rejected = create_from_wire_trace(TraceReceipt::Rejected);
    assert_eq!(
        rejected.steps.last().expect("terminal search step").phase,
        TracePhase::Searching,
    );

    let [control, drag] = control_and_drag_handle_traces();
    assert_eq!(control.steps[0].phase, TracePhase::Idle);
    assert!(
        drag.steps
            .iter()
            .any(|step| step.phase == TracePhase::NodeDrag)
    );

    let edge = edge_pick_trace();
    assert_eq!(edge.steps[0].effect, TraceEffect::PickEdge);
    assert_eq!(
        edge.steps.last().expect("edge terminal").phase,
        TracePhase::Idle,
    );

    for trace in terminal_ownership_traces() {
        let terminal = trace.steps.last().expect("ownership terminal");
        assert_eq!(terminal.phase, TracePhase::Idle);
        assert_eq!(terminal.effect, TraceEffect::ReleaseNativeOwnership);
    }
}

#[test]
fn canonical_traces_drive_the_renderer_neutral_session_and_real_view_queries() {
    for receipt in [
        TraceReceipt::Applied,
        TraceReceipt::Noop,
        TraceReceipt::Rejected,
        TraceReceipt::Conflict,
    ] {
        TraceHarness::new().replay(&new_connection_trace(receipt));
        TraceHarness::new().replay(&create_from_wire_trace(receipt));
        for endpoint in [
            TraceReconnectEndpoint::Source,
            TraceReconnectEndpoint::Target,
        ] {
            TraceHarness::new().replay(&reconnect_trace(endpoint, receipt));
        }
    }

    for trace in control_and_drag_handle_traces() {
        TraceHarness::new().replay(&trace);
    }
    TraceHarness::new().replay(&edge_pick_trace());
    for trace in terminal_ownership_traces() {
        TraceHarness::new().replay(&trace);
    }
}

#[test]
fn interaction_query_cursors_are_isolated_per_view() {
    let harness = TraceHarness::new();
    let other = harness
        ._runtime
        .attach_view(ViewSessionOptions::default())
        .unwrap();
    publish_trace_geometry(&other, harness.ids, harness.anchors);

    let intent = ConnectionGestureIntent::connect(
        EdgeId::from_u128(99),
        harness.ids.source_port,
        NodeGraphConnectionMode::Strict,
    );
    let probe_cursor = match harness
        .view
        .connection_probe(ConnectionProbeQuery::new(
            intent,
            harness.anchors.valid,
            12.0,
            0,
        ))
        .unwrap()
    {
        ConnectionProbeQueryOutcome::Incomplete { cursor, .. } => cursor,
        outcome => panic!("zero-budget probe must be resumable: {outcome:?}"),
    };
    assert!(matches!(
        other.connection_probe(ConnectionProbeQuery::resume(probe_cursor, 16)),
        Err(ConnectionProbeQueryError::StaleCursor(
            ConnectionProbeCursorStaleReason::ForeignView
        ))
    ));

    let edge_cursor = match harness
        .view
        .edge_pick(EdgePickQuery::new(ViewPoint::new(400.0, 80.0), 10.0, 0))
        .unwrap()
    {
        EdgePickQueryOutcome::Incomplete { cursor, .. } => cursor,
        outcome => panic!("zero-budget edge pick must be resumable: {outcome:?}"),
    };
    assert!(matches!(
        other.edge_pick(EdgePickQuery::resume(edge_cursor, 16)),
        Err(EdgePickQueryError::StaleCursor(
            EdgePickCursorStaleReason::ForeignView
        ))
    ));
}

struct TraceHarness {
    _runtime: GraphRuntime,
    view: ViewSession,
    session: CanvasInteractionSession,
    ids: TraceGraphIds,
    anchors: TraceAnchors,
    pointer: InteractionPosition,
    pending_probe: Option<(
        jellyflow_runtime::InteractionQueryId,
        jellyflow_runtime::ConnectionProbeQuery,
    )>,
    ticket: Option<InteractionTicketId>,
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

impl TraceHarness {
    fn new() -> Self {
        let ids = TraceGraphIds {
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
        };
        let anchors = TraceAnchors {
            source: ViewPoint::new(60.0, 80.0),
            invalid: ViewPoint::new(160.0, 80.0),
            valid: ViewPoint::new(240.0, 80.0),
            reconnect_source: ViewPoint::new(360.0, 80.0),
            reconnect_target: ViewPoint::new(440.0, 80.0),
            reconnect_alternate: ViewPoint::new(540.0, 80.0),
            empty: ViewPoint::new(700.0, 300.0),
        };
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
            .build(builder.build().unwrap())
            .unwrap();
        let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
        publish_trace_geometry(&view, ids, anchors);
        let session = CanvasInteractionSession::new(
            view.id(),
            view.generation().unwrap(),
            CanvasInteractionConfig::default(),
        )
        .unwrap();
        let pointer = normalized_position(&view, anchors.source);
        Self {
            _runtime: runtime,
            view,
            session,
            ids,
            anchors,
            pointer,
            pending_probe: None,
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
                self.pointer = normalized_position(&self.view, self.anchors.empty);
                let update = self.session.move_to(self.pointer).unwrap();
                self.capture_probe(update)
            }
            TraceInput::Move { .. } => {
                self.pointer = normalized_position(&self.view, self.invalid_anchor());
                let update = self.session.move_to(self.pointer).unwrap();
                self.capture_probe(update)
            }
            TraceInput::Candidate(candidate) => self.resolve_candidate(candidate),
            TraceInput::Release => self.release(),
            TraceInput::SearchSelected { .. } => {
                let generation = self.session.presentation().generation().unwrap();
                self.session.search_ready(generation, 1).unwrap();
                let update = self
                    .session
                    .select_search_result(
                        generation,
                        self.ids.valid_port,
                        self.view.graph_snapshot().unwrap().revision(),
                    )
                    .unwrap();
                self.capture_submit(update)
            }
            TraceInput::Receipt(receipt) => {
                let ticket = self.ticket.take().expect("trace submitted one command");
                self.session.settle(ticket, settlement(receipt)).unwrap();
                match receipt {
                    TraceReceipt::Applied | TraceReceipt::Noop => TraceEffect::None,
                    TraceReceipt::Rejected | TraceReceipt::Conflict => {
                        TraceEffect::RestorePresentation
                    }
                }
            }
            TraceInput::Cancel(reason) => {
                assert!(matches!(
                    reason,
                    TraceCancelReason::Escape | TraceCancelReason::OutsideClick
                ));
                self.session.cancel();
                TraceEffect::ReleaseNativeOwnership
            }
            TraceInput::Retire => {
                self.session
                    .owner_retired(InteractionOwner::Node(self.active_owner_node()));
                TraceEffect::ReleaseNativeOwnership
            }
            TraceInput::Close => {
                self.session.close();
                TraceEffect::ReleaseNativeOwnership
            }
        }
    }

    fn press(&mut self, target: TraceTarget) -> TraceEffect {
        match target {
            TraceTarget::NativeControl(_) => {
                self.session
                    .press(CanvasPressTarget::NativeControl, self.pointer)
                    .unwrap();
                TraceEffect::None
            }
            TraceTarget::NodeDragHandle(_) => {
                let start = NodeDragStart::new(
                    self.ids.source_node,
                    [self.ids.source_node],
                    CanvasPoint { x: 0.0, y: 0.0 },
                    SelectionMode::Replace,
                )
                .unwrap();
                self.session
                    .press(CanvasPressTarget::NodeDrag(start), self.pointer)
                    .unwrap();
                TraceEffect::None
            }
            TraceTarget::Port(_) => {
                let start = ConnectionInteractionStart::new(
                    self.ids.source_node,
                    ConnectionGestureIntent::connect(
                        EdgeId::from_u128(100),
                        self.ids.source_port,
                        NodeGraphConnectionMode::Strict,
                    ),
                    canvas_anchor(&self.view, self.anchors.source),
                    HandlePosition::Right,
                    true,
                )
                .unwrap();
                self.session
                    .press(CanvasPressTarget::Connection(start), self.pointer)
                    .unwrap();
                TraceEffect::None
            }
            TraceTarget::ReconnectHandle { endpoint, .. } => {
                let (runtime_endpoint, detached, fixed, fixed_node, fixed_anchor, outward) =
                    match endpoint {
                        TraceReconnectEndpoint::Source => (
                            EdgeEndpoint::From,
                            self.ids.reconnect_source_port,
                            self.ids.reconnect_target_port,
                            self.ids.reconnect_target_node,
                            self.anchors.reconnect_target,
                            HandlePosition::Left,
                        ),
                        TraceReconnectEndpoint::Target => (
                            EdgeEndpoint::To,
                            self.ids.reconnect_target_port,
                            self.ids.reconnect_source_port,
                            self.ids.reconnect_source_node,
                            self.anchors.reconnect_source,
                            HandlePosition::Right,
                        ),
                    };
                let start = ConnectionInteractionStart::new(
                    fixed_node,
                    ConnectionGestureIntent::reconnect(
                        self.ids.reconnect_edge,
                        runtime_endpoint,
                        detached,
                        fixed,
                        NodeGraphConnectionMode::Strict,
                    ),
                    canvas_anchor(&self.view, fixed_anchor),
                    outward,
                    false,
                )
                .unwrap();
                self.pointer = normalized_position(&self.view, fixed_anchor);
                self.session
                    .press(CanvasPressTarget::Connection(start), self.pointer)
                    .unwrap();
                TraceEffect::None
            }
            TraceTarget::Edge(_) => {
                let midpoint = ViewPoint::new(400.0, 80.0);
                let pointer = normalized_position(&self.view, midpoint);
                let (mut queries, effects, _, _) = self
                    .session
                    .request_edge_pick(pointer)
                    .unwrap()
                    .into_parts();
                assert!(effects.is_empty());
                let CanvasInteractionQuery::EdgePick { id, query } = queries.remove(0) else {
                    panic!("edge trace requested the wrong query");
                };
                let outcome = self.view.edge_pick(query).unwrap();
                let picked = match &outcome {
                    EdgePickQueryOutcome::Complete {
                        pick: Some(pick), ..
                    } => pick.edge(),
                    other => panic!("edge trace must pick the routed edge: {other:?}"),
                };
                self.session.resolve_edge_pick(id, outcome).unwrap();
                self.session
                    .press(
                        CanvasPressTarget::Edge {
                            edge: picked,
                            selection_mode: SelectionMode::Replace,
                        },
                        pointer,
                    )
                    .unwrap();
                TraceEffect::PickEdge
            }
            TraceTarget::Canvas => panic!("canonical trace has no unresolved canvas press"),
        }
    }

    fn resolve_candidate(&mut self, candidate: TraceCandidate) -> TraceEffect {
        if matches!(candidate, TraceCandidate::Valid { .. }) {
            self.pointer = normalized_position(&self.view, self.valid_anchor());
            let update = self.session.move_to(self.pointer).unwrap();
            self.capture_probe(update);
        }
        let (id, query) = self
            .pending_probe
            .take()
            .expect("candidate follows a probe");
        let outcome = self.view.connection_probe(query).unwrap();
        match (&candidate, &outcome) {
            (
                TraceCandidate::None,
                jellyflow_runtime::ConnectionProbeQueryOutcome::Complete {
                    probe: ConnectionProbe::None,
                    ..
                },
            ) => {}
            (
                TraceCandidate::Invalid { .. },
                jellyflow_runtime::ConnectionProbeQueryOutcome::Complete {
                    probe: ConnectionProbe::Invalid { .. },
                    ..
                },
            ) => {}
            (
                TraceCandidate::Valid { .. },
                jellyflow_runtime::ConnectionProbeQueryOutcome::Complete {
                    probe: ConnectionProbe::Valid { .. },
                    ..
                },
            ) => {}
            _ => panic!("trace candidate disagrees with the real view query: {outcome:?}"),
        }
        self.session.resolve_connection_probe(id, outcome).unwrap();
        TraceEffect::None
    }

    fn release(&mut self) -> TraceEffect {
        let was_connection = self.session.presentation().connection().is_some();
        let update = self.session.release(self.pointer).unwrap();
        if matches!(
            self.session.presentation().phase(),
            CanvasInteractionPhase::Committing
        ) {
            return self.capture_submit(update);
        }
        if !was_connection {
            return self.capture_submit(update);
        }
        if let Some((id, query)) = take_probe(update) {
            let outcome = self.view.connection_probe(query).unwrap();
            let terminal = self.session.resolve_connection_probe(id, outcome).unwrap();
            if terminal
                .effects()
                .iter()
                .any(|effect| matches!(effect, CanvasInteractionEffect::Search(_)))
            {
                return TraceEffect::OpenSearch;
            }
            return self.capture_submit(terminal);
        }
        TraceEffect::None
    }

    fn capture_probe(&mut self, update: CanvasInteractionUpdate) -> TraceEffect {
        self.pending_probe = take_probe(update);
        if self.pending_probe.is_some() {
            TraceEffect::ProbeConnection
        } else {
            TraceEffect::None
        }
    }

    fn capture_submit(&mut self, update: CanvasInteractionUpdate) -> TraceEffect {
        let (_, effects, _, _) = update.into_parts();
        for effect in effects {
            if let CanvasInteractionEffect::SubmitGraph { ticket, mutation } = effect {
                self.ticket = Some(ticket);
                return match mutation {
                    jellyflow_runtime::FreshGraphMutation::Connection { .. } => {
                        TraceEffect::SubmitConnection
                    }
                    jellyflow_runtime::FreshGraphMutation::MoveNodes(_) => {
                        TraceEffect::SubmitNodeMove
                    }
                };
            }
        }
        TraceEffect::None
    }

    fn observed_phase(&self) -> TracePhase {
        let presentation = self.session.presentation();
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
        match self.session.presentation().connection().map(|c| c.intent()) {
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::From,
                ..
            }) => self.anchors.valid,
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::To,
                ..
            })
            | Some(ConnectionGestureIntent::Connect { .. })
            | None => self.anchors.invalid,
        }
    }

    fn valid_anchor(&self) -> ViewPoint {
        match self.session.presentation().connection().map(|c| c.intent()) {
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::From,
                ..
            }) => self.anchors.source,
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::To,
                ..
            }) => self.anchors.reconnect_alternate,
            Some(ConnectionGestureIntent::Connect { .. }) | None => self.anchors.valid,
        }
    }

    fn active_owner_node(&self) -> NodeId {
        self.session
            .presentation()
            .connection()
            .map_or(self.ids.source_node, |connection| {
                match connection.intent() {
                    ConnectionGestureIntent::Connect { .. } => self.ids.source_node,
                    ConnectionGestureIntent::Reconnect {
                        endpoint: EdgeEndpoint::From,
                        ..
                    } => self.ids.reconnect_target_node,
                    ConnectionGestureIntent::Reconnect {
                        endpoint: EdgeEndpoint::To,
                        ..
                    } => self.ids.reconnect_source_node,
                }
            })
    }
}

fn take_probe(
    update: CanvasInteractionUpdate,
) -> Option<(
    jellyflow_runtime::InteractionQueryId,
    jellyflow_runtime::ConnectionProbeQuery,
)> {
    let (queries, _, _, _) = update.into_parts();
    queries.into_iter().find_map(|query| match query {
        CanvasInteractionQuery::ConnectionProbe { id, query, .. } => Some((id, query)),
        CanvasInteractionQuery::EdgePick { .. } => None,
    })
}

fn settlement(receipt: TraceReceipt) -> CommandSettlement {
    match receipt {
        TraceReceipt::Applied => CommandSettlement::Applied,
        TraceReceipt::Noop => CommandSettlement::Noop,
        TraceReceipt::Rejected => CommandSettlement::Rejected,
        TraceReceipt::Conflict => CommandSettlement::Conflict,
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
        .unwrap();
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
            .unwrap();
        let frame_port = frame_node
            .ports()
            .iter()
            .find(|candidate| candidate.id() == port)
            .unwrap();
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
            .unwrap();
        collector.layout(layout.finish().unwrap()).unwrap();
    }
    view.submit_feedback(collector.finish().unwrap()).unwrap();
}

fn normalized_position(view: &ViewSession, point: ViewPoint) -> InteractionPosition {
    let canvas = canvas_anchor(view, point);
    InteractionPosition::new(point, canvas).unwrap()
}

fn canvas_anchor(view: &ViewSession, point: ViewPoint) -> CanvasPoint {
    view.viewport_transform()
        .unwrap()
        .canvas_point_at_screen(CanvasPoint {
            x: point.x,
            y: point.y,
        })
}

fn trace_node(port: PortId) -> Node {
    Node {
        kind: NodeKindKey::new("trace.node"),
        kind_version: 1,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
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
