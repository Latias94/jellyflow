use std::collections::VecDeque;

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
    Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_runtime::runtime::adapter_protocol::conformance::{
    ConformanceAction, ConformanceCancelReason, ConformanceCandidate, ConformanceEffect,
    ConformanceGeometry, ConformanceOwnership, ConformancePhase, ConformanceScenario,
    ConformanceScenarioId, ConformanceSettlement, ConformanceTarget, ConformanceTerminal,
    ConformanceTicket, NormalizedObservation, compare_observations, new_connection_scenario,
    terminal_ownership_scenarios,
};
use jellyflow_runtime::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionPhase,
    CanvasInteractionQuery, CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressTarget,
    CommandSettlement, ConnectionInteractionStart, FrameRequest, FreshGraphMutation, GraphRuntime,
    HandlePosition, InteractionPosition, InteractionTerminalReason, InteractionTicketId,
    RendererStamp, ViewPoint, ViewRect, ViewSession, ViewSessionOptions, ViewSize,
};

const NEW_EDGE_ID: u128 = 100;

fn main() {}

#[derive(Clone, Copy)]
struct FixtureIds {
    source_node: NodeId,
    source_port: PortId,
    invalid_node: NodeId,
    invalid_port: PortId,
    valid_node: NodeId,
    valid_port: PortId,
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
        }
    }
}

#[derive(Clone, Copy)]
struct FixtureAnchors {
    source: ViewPoint,
    invalid: ViewPoint,
    valid: ViewPoint,
    empty: ViewPoint,
}

impl FixtureAnchors {
    fn new() -> Self {
        Self {
            source: ViewPoint::new(80.0, 96.0),
            invalid: ViewPoint::new(240.0, 96.0),
            valid: ViewPoint::new(400.0, 96.0),
            empty: ViewPoint::new(700.0, 320.0),
        }
    }
}

#[derive(Clone, Copy)]
enum LocalPointerTarget {
    Empty,
    InvalidCandidate,
    ValidCandidate,
}

#[derive(Clone, Copy)]
enum LocalNativeEvent {
    PointerDown(ConformanceTarget),
    PointerMove(LocalPointerTarget),
    PointerUp,
    Advance,
    CommandSettled(ConformanceSettlement),
    Cancel(ConformanceCancelReason),
}

struct LocalNativeFrame {
    event: LocalNativeEvent,
}

impl LocalNativeFrame {
    fn one(event: LocalNativeEvent) -> Self {
        Self { event }
    }
}

struct ExternalNativeDriver {
    runtime: GraphRuntime,
    view: ViewSession,
    interaction: CanvasInteractionSession,
    ids: FixtureIds,
    anchors: FixtureAnchors,
    pointer: InteractionPosition,
    owns_pointer: bool,
    ticket: Option<InteractionTicketId>,
    mutation: Option<FreshGraphMutation>,
}

impl ExternalNativeDriver {
    fn new() -> Self {
        let ids = FixtureIds::new();
        let anchors = FixtureAnchors::new();
        let mut graph = GraphBuilder::new(GraphId::from_u128(0xada0));
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
        ] {
            graph.insert_node(node, fixture_node(port, anchor));
            graph.insert_port(port, fixture_port(node, direction));
        }
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph.build().expect("external conformance graph is valid"))
            .expect("external conformance runtime starts");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("external conformance view attaches");
        publish_geometry(&view, ids, anchors);
        let interaction = CanvasInteractionSession::new(
            view.id(),
            view.generation().expect("view generation is available"),
            CanvasInteractionConfig::default(),
        )
        .expect("external conformance interaction starts");
        let pointer = normalized_position(&view, anchors.source);
        Self {
            runtime,
            view,
            interaction,
            ids,
            anchors,
            pointer,
            owns_pointer: false,
            ticket: None,
            mutation: None,
        }
    }

    fn drive(&mut self, scenario: &ConformanceScenario) -> Vec<NormalizedObservation> {
        scenario
            .steps
            .iter()
            .map(|step| self.dispatch_action(step.action))
            .collect()
    }

    fn dispatch_action(&mut self, action: ConformanceAction) -> NormalizedObservation {
        let event = match action {
            ConformanceAction::Press(target) => LocalNativeEvent::PointerDown(target),
            ConformanceAction::CrossDragThreshold => {
                LocalNativeEvent::PointerMove(LocalPointerTarget::Empty)
            }
            ConformanceAction::MoveToCandidate(ConformanceCandidate::Invalid) => {
                LocalNativeEvent::PointerMove(LocalPointerTarget::InvalidCandidate)
            }
            ConformanceAction::MoveToCandidate(ConformanceCandidate::Valid) => {
                LocalNativeEvent::PointerMove(LocalPointerTarget::ValidCandidate)
            }
            ConformanceAction::Release => LocalNativeEvent::PointerUp,
            ConformanceAction::AdvanceFrame => LocalNativeEvent::Advance,
            ConformanceAction::Settle(settlement) => LocalNativeEvent::CommandSettled(settlement),
            ConformanceAction::Cancel(reason) => LocalNativeEvent::Cancel(reason),
            ConformanceAction::MoveToCandidate(ConformanceCandidate::None)
            | ConformanceAction::SelectSearchCandidate
            | ConformanceAction::QueuePrefix
            | ConformanceAction::DrainPrefix
            | ConformanceAction::LoseCapture
            | ConformanceAction::ReplaceOwner
            | ConformanceAction::RetireSource
            | ConformanceAction::CloseHost => {
                panic!("the minimal external driver does not implement {action:?}")
            }
        };
        let effect = self.dispatch_frame(LocalNativeFrame::one(event));
        self.observation(effect)
    }

    fn dispatch_frame(&mut self, frame: LocalNativeFrame) -> ConformanceEffect {
        match frame.event {
            LocalNativeEvent::PointerDown(target) => self.pointer_down(target),
            LocalNativeEvent::PointerMove(target) => self.pointer_move(target),
            LocalNativeEvent::PointerUp => self.pointer_up(),
            LocalNativeEvent::Advance => ConformanceEffect::None,
            LocalNativeEvent::CommandSettled(settlement) => self.settle(settlement),
            LocalNativeEvent::Cancel(reason) => self.cancel(reason),
        }
    }

    fn pointer_down(&mut self, target: ConformanceTarget) -> ConformanceEffect {
        let ConformanceTarget::SourcePort = target else {
            panic!("the minimal external driver only exposes its source port")
        };
        self.pointer = normalized_position(&self.view, self.anchors.source);
        let drag_intent = self
            .view
            .port_connection_drag_intent(self.ids.source_port)
            .expect("source port facts are readable")
            .expect("source port can start a connection");
        let start = ConnectionInteractionStart::from_port_drag_intent(
            drag_intent,
            EdgeId::from_u128(NEW_EDGE_ID),
            NodeGraphConnectionMode::Strict,
            canvas_anchor(&self.view, self.anchors.source),
            HandlePosition::Right,
            false,
        )
        .expect("source port produces a connection target");
        let update = self
            .interaction
            .press(CanvasPressTarget::Connection(start), self.pointer)
            .expect("native pointer press reaches the semantic session");
        let effect = self.process_update(update);
        self.owns_pointer = true;
        effect
    }

    fn pointer_move(&mut self, target: LocalPointerTarget) -> ConformanceEffect {
        let point = match target {
            LocalPointerTarget::Empty => self.anchors.empty,
            LocalPointerTarget::InvalidCandidate => self.anchors.invalid,
            LocalPointerTarget::ValidCandidate => self.anchors.valid,
        };
        self.pointer = normalized_position(&self.view, point);
        let update = self
            .interaction
            .move_to(self.pointer)
            .expect("native pointer move reaches the semantic session");
        self.process_update(update)
    }

    fn pointer_up(&mut self) -> ConformanceEffect {
        let update = self
            .interaction
            .release(self.pointer)
            .expect("native pointer release reaches the semantic session");
        let effect = self.process_update(update);
        self.owns_pointer = false;
        effect
    }

    fn settle(&mut self, settlement: ConformanceSettlement) -> ConformanceEffect {
        let ticket = self.ticket.take().expect("one command ticket is pending");
        if settlement == ConformanceSettlement::Applied {
            let mutation = self
                .mutation
                .take()
                .expect("an applied ticket carries a real graph mutation");
            let snapshot = self
                .view
                .graph_snapshot()
                .expect("the command binds against a fresh snapshot");
            let command = mutation
                .into_command(&snapshot)
                .expect("the validated connection still binds")
                .expect("the connection is not a no-op");
            let outcome = self
                .view
                .execute(command)
                .expect("the external transport executes the real command");
            assert!(
                outcome.applied().is_some(),
                "the connection must be installed"
            );
        } else {
            self.mutation = None;
        }
        let update = self
            .interaction
            .settle(ticket, command_settlement(settlement))
            .expect("the command result settles its semantic ticket");
        let effect = self.process_update(update);
        if effect != ConformanceEffect::None {
            return effect;
        }
        match settlement {
            ConformanceSettlement::Applied | ConformanceSettlement::Noop => ConformanceEffect::None,
            ConformanceSettlement::Rejected
            | ConformanceSettlement::Conflict
            | ConformanceSettlement::TransportFailure => ConformanceEffect::RestorePresentation,
        }
    }

    fn cancel(&mut self, _reason: ConformanceCancelReason) -> ConformanceEffect {
        let update = self.interaction.cancel();
        let effect = self.process_update(update);
        let released = self.owns_pointer;
        self.owns_pointer = false;
        if effect != ConformanceEffect::None {
            effect
        } else if released {
            ConformanceEffect::ReleaseNativeOwnership
        } else {
            ConformanceEffect::None
        }
    }

    fn process_update(&mut self, update: CanvasInteractionUpdate) -> ConformanceEffect {
        let mut pending = VecDeque::from([update]);
        let mut observed = ConformanceEffect::None;
        let mut resolved_probe = false;
        while let Some(update) = pending.pop_front() {
            let dispatch = update.into_dispatch();
            for effect in dispatch.pre_query_effects {
                match effect {
                    CanvasInteractionEffect::SubmitGraph { ticket, mutation } => {
                        self.ticket = Some(ticket);
                        observed = match mutation {
                            mutation @ FreshGraphMutation::Connection { .. } => {
                                self.mutation = Some(mutation);
                                ConformanceEffect::SubmitConnection
                            }
                            FreshGraphMutation::MoveNodes(_)
                            | FreshGraphMutation::DeleteEdge(_) => {
                                panic!("the external fixture only drives a connection mutation")
                            }
                        };
                    }
                    CanvasInteractionEffect::Selection(mutation) => self
                        .view
                        .apply_selection_mutation(mutation)
                        .expect("the external driver applies view-local selection"),
                    CanvasInteractionEffect::Viewport(mutation) => self
                        .view
                        .apply_viewport_mutation(mutation)
                        .expect("the external driver applies view-local viewport changes"),
                    CanvasInteractionEffect::Search(_) => {
                        observed = ConformanceEffect::OpenSearch;
                    }
                }
            }
            for query in dispatch.queries {
                let CanvasInteractionQuery::ConnectionProbe { id, query, .. } = query else {
                    panic!("the minimal connection driver received a non-connection query")
                };
                resolved_probe = true;
                let outcome = self
                    .view
                    .connection_probe(query)
                    .expect("the external driver resolves its geometry query");
                pending.push_back(
                    self.interaction
                        .resolve_connection_probe(id, outcome)
                        .expect("the native driver returns the probe result"),
                );
            }
        }
        if observed == ConformanceEffect::None && resolved_probe {
            ConformanceEffect::ProbeConnection
        } else {
            observed
        }
    }

    fn observation(&self, effect: ConformanceEffect) -> NormalizedObservation {
        let presentation = self.interaction.presentation();
        let ownership = if self.owns_pointer {
            ConformanceOwnership::Held
        } else {
            ConformanceOwnership::Released
        };
        let ticket = if self.ticket.is_some() {
            ConformanceTicket::Pending
        } else {
            ConformanceTicket::None
        };
        let geometry = self
            .view
            .geometry_snapshot()
            .ok()
            .filter(|snapshot| !snapshot.nodes().is_empty())
            .map_or(ConformanceGeometry::Unavailable, |_| {
                ConformanceGeometry::Available
            });
        let mut observation = NormalizedObservation::new(
            normalized_phase(presentation.phase(), presentation.connection()),
            effect,
            ownership,
            ticket,
            self.runtime
                .snapshot()
                .expect("external conformance runtime remains available")
                .revision(),
            geometry,
        );
        observation.terminal = presentation.terminal().map(normalized_terminal);
        observation.optimistic_presentation = self.ticket.is_some()
            && (presentation.connection().is_some() || presentation.node_drag().is_some());
        observation
    }
}

fn normalized_phase(
    phase: CanvasInteractionPhase,
    connection: Option<&jellyflow_runtime::ConnectionPresentation>,
) -> ConformancePhase {
    match phase {
        CanvasInteractionPhase::Idle | CanvasInteractionPhase::Closed => ConformancePhase::Idle,
        CanvasInteractionPhase::Armed => ConformancePhase::Armed,
        CanvasInteractionPhase::NodeDrag => ConformancePhase::NodeDrag,
        CanvasInteractionPhase::ConnectionDrag
            if connection.is_some_and(|connection| connection.probe().is_valid()) =>
        {
            ConformancePhase::Candidate
        }
        CanvasInteractionPhase::ConnectionDrag => ConformancePhase::ConnectionDrag,
        CanvasInteractionPhase::ConnectionSearch => ConformancePhase::Searching,
        CanvasInteractionPhase::Committing => ConformancePhase::Committing,
        CanvasInteractionPhase::EdgeActive => ConformancePhase::EdgeActive,
        CanvasInteractionPhase::Pan | CanvasInteractionPhase::Marquee => {
            panic!("the minimal driver does not expose navigation gestures")
        }
    }
}

fn normalized_terminal(reason: InteractionTerminalReason) -> ConformanceTerminal {
    match reason {
        InteractionTerminalReason::Applied => ConformanceTerminal::Applied,
        InteractionTerminalReason::Noop => ConformanceTerminal::Noop,
        InteractionTerminalReason::Rejected => ConformanceTerminal::Rejected,
        InteractionTerminalReason::Conflict => ConformanceTerminal::Conflict,
        InteractionTerminalReason::TransportFailure => ConformanceTerminal::TransportFailure,
        InteractionTerminalReason::Cancelled => ConformanceTerminal::Cancelled,
        InteractionTerminalReason::OwnerRetired => ConformanceTerminal::OwnerRetired,
        InteractionTerminalReason::ViewClosed => ConformanceTerminal::HostClosed,
        InteractionTerminalReason::BudgetExhausted
        | InteractionTerminalReason::GeometryIndeterminate
        | InteractionTerminalReason::IdentityExhausted => {
            panic!("unexpected terminal reason in the external fixture: {reason:?}")
        }
    }
}

fn command_settlement(settlement: ConformanceSettlement) -> CommandSettlement {
    match settlement {
        ConformanceSettlement::Applied => CommandSettlement::Applied,
        ConformanceSettlement::Noop => CommandSettlement::Noop,
        ConformanceSettlement::Rejected => CommandSettlement::Rejected,
        ConformanceSettlement::Conflict => CommandSettlement::Conflict,
        ConformanceSettlement::TransportFailure => CommandSettlement::TransportFailure,
    }
}

fn normalized_position(view: &ViewSession, point: ViewPoint) -> InteractionPosition {
    InteractionPosition::from_view(
        point,
        view.viewport_transform()
            .expect("external fixture viewport is available"),
    )
    .expect("fixture pointer is finite")
}

fn canvas_anchor(view: &ViewSession, point: ViewPoint) -> CanvasPoint {
    point
        .to_canvas_point(
            view.viewport_transform()
                .expect("external fixture viewport is available"),
        )
        .expect("fixture anchor is finite")
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
    ] {
        let frame_node = frame
            .nodes()
            .iter()
            .find(|candidate| candidate.id() == node)
            .expect("fixture node is admitted");
        let frame_port = frame_node
            .ports()
            .iter()
            .find(|candidate| candidate.id() == port)
            .expect("fixture port belongs to its frame node");
        let root = ViewRect::new(
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
        let mut layout = frame_node.begin_layout(root);
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
            .expect("fixture port layout is valid");
        collector
            .layout(layout.finish().expect("fixture node reports every port"))
            .expect("fixture layout enters feedback");
    }
    view.submit_feedback(collector.finish().expect("fixture feedback finishes once"))
        .expect("fixture geometry publishes");
}

#[test]
fn consumer_owned_driver_executes_connection_and_cancellation_contracts() {
    let connection = new_connection_scenario(ConformanceSettlement::Applied);
    let mut connection_driver = ExternalNativeDriver::new();
    let connection_observations = connection_driver.drive(&connection);
    compare_observations(&connection, &connection_observations)
        .expect("external native connection observations match the public contract");
    assert!(
        connection_driver
            .runtime
            .snapshot()
            .expect("connected graph remains available")
            .graph()
            .edge(&EdgeId::from_u128(NEW_EDGE_ID))
            .is_some(),
        "the local driver must install a real graph edge",
    );

    let escape = terminal_ownership_scenarios()
        .into_iter()
        .find(|scenario| scenario.id == ConformanceScenarioId::Escape)
        .expect("the public contract includes an Escape scenario");
    let mut cancellation_driver = ExternalNativeDriver::new();
    let cancellation_observations = cancellation_driver.drive(&escape);
    compare_observations(&escape, &cancellation_observations)
        .expect("external native cancellation observations match the public contract");
    assert!(!cancellation_driver.owns_pointer);
    assert_eq!(
        cancellation_driver.interaction.presentation().terminal(),
        Some(InteractionTerminalReason::Cancelled),
    );
}
