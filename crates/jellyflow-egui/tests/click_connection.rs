use egui::{Context, Event, Id, Modifiers, PointerButton, Pos2, Rect, Sense, Ui, Vec2};
use egui_kittest::Harness;
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node,
    NodeGraphConnectionMode, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};
use jellyflow_egui::{EguiCanvasInteraction, EguiInteractionOutput, EguiNativeFrameHost};
use jellyflow_runtime::geometry::{HandlePosition, ViewPoint};
use jellyflow_runtime::graph::{EdgeEndpoint, GraphRuntime};
use jellyflow_runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionPhase, CanvasPressTarget,
    InteractionGraphSubmission, InteractionSubmissionSettlementEvidence, SelectionMode,
};
use jellyflow_runtime::view::{FrameRequest, RendererStamp, ViewSession, ViewSessionOptions};

#[test]
fn egui_non_drag_port_activation_uses_runtime_source_target_authority() {
    let source_node = NodeId::from_u128(1);
    let target_node = NodeId::from_u128(2);
    let source_port = PortId::from_u128(11);
    let target_port = PortId::from_u128(12);
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xE601));
    graph.insert_node(source_node, test_node(vec![source_port], 0.0));
    graph.insert_node(target_node, test_node(vec![target_port], 240.0));
    graph.insert_port(source_port, test_port(source_node, PortDirection::Out));
    graph.insert_port(target_port, test_port(target_node, PortDirection::In));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph.build().expect("valid click-connect graph"))
        .expect("runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("view");
    let mut interaction =
        EguiCanvasInteraction::new(&view, CanvasInteractionConfig::default()).unwrap();
    let ctx = Context::default();
    let edge = EdgeId::from_u128(21);
    let source_instance = view
        .port_instance_key(source_port)
        .unwrap()
        .expect("current source port");
    let target_instance = view
        .port_instance_key(target_port)
        .unwrap()
        .expect("current target port");

    let armed = interaction
        .activate_connection_port(&ctx, source_instance, edge, NodeGraphConnectionMode::Strict)
        .unwrap();
    assert!(armed.effects().is_empty());
    let [work] = armed.port_connection_queries() else {
        panic!("source activation must retain its exact authoritative query work");
    };
    assert!(work.fact_reads() > 0);
    assert_eq!(work.incident_bucket_reads(), 0);
    assert_eq!(work.incident_entries_read(), 0);
    assert_eq!(
        interaction.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag
    );

    let mut submitted = interaction
        .activate_connection_port(&ctx, target_instance, edge, NodeGraphConnectionMode::Strict)
        .unwrap();
    assert!(submitted.port_connection_queries().is_empty());
    let mut effects = submitted.take_effects();
    assert_eq!(effects.len(), 1);
    let CanvasInteractionEffect::SubmitGraph { submission } =
        effects.pop().expect("one connection submission")
    else {
        unreachable!("non-drag target activation cannot emit search")
    };
    let mut bound = interaction.bind_submission(submission).unwrap();
    let command = bound.take_command().expect("fresh connection command");
    let completion = runtime.execute(command);
    assert!(completion.applied_revision().is_some());
    let settled = interaction
        .settle_submission(
            &ctx,
            bound,
            InteractionSubmissionSettlementEvidence::Command(completion),
        )
        .unwrap();
    assert!(!settled.renderer_work_pending());
    assert!(settled.terminal_query_error().is_none());

    let graph = runtime.snapshot().unwrap();
    let edge = graph.graph().edge(&edge).expect("committed edge");
    assert_eq!(edge.from, source_port);
    assert_eq!(edge.to, target_port);
}

#[test]
fn native_release_at_new_position_commits_the_armed_node_drag() {
    let mut harness = pointer_harness();
    let start = harness.state().anchors.source_pos();
    let release = harness.state().anchors.empty_pos();
    let before = harness
        .state()
        .runtime
        .snapshot()
        .expect("node-drag baseline")
        .graph()
        .node(&harness.state().ids.source_node)
        .expect("source node")
        .pos;

    press_pointer_response(&mut harness, PointerResponse::NodeDrag);
    release_pointer(&mut harness, release, false);
    execute_pointer_submission(&mut harness);

    let after = harness
        .state()
        .runtime
        .snapshot()
        .expect("node-drag result")
        .graph()
        .node(&harness.state().ids.source_node)
        .expect("moved source node")
        .pos;
    let expected = CanvasPoint {
        x: before.x + release.x - start.x,
        y: before.y + release.y - start.y,
    };
    assert!((after.x - expected.x).abs() < 0.001);
    assert!((after.y - expected.y).abs() < 0.001);
}

#[test]
fn native_source_reconnect_commits_only_the_detached_source_endpoint() {
    assert_native_reconnect(
        PointerResponse::ReconnectSource,
        |ids| ids.alternate_source_port,
        |ids| (ids.alternate_source_port, ids.target_port),
    );
}

#[test]
fn native_target_reconnect_commits_only_the_detached_target_endpoint() {
    assert_native_reconnect(
        PointerResponse::ReconnectTarget,
        |ids| ids.alternate_target_port,
        |ids| (ids.source_port, ids.alternate_target_port),
    );
}

fn assert_native_reconnect(
    response: PointerResponse,
    candidate: impl FnOnce(PointerIds) -> PortId,
    expected: impl FnOnce(PointerIds) -> (PortId, PortId),
) {
    let mut harness = pointer_harness();
    let ids = harness.state().ids;
    let candidate = candidate(ids);
    let release = harness.state().anchors.port(candidate);

    press_pointer_response(&mut harness, response);
    release_pointer(&mut harness, release, true);
    execute_pointer_submission(&mut harness);

    let graph = harness
        .state()
        .runtime
        .snapshot()
        .expect("reconnect result");
    let edge = graph
        .graph()
        .edge(&ids.edge)
        .expect("reconnected edge keeps its identity");
    let (from, to) = expected(ids);
    assert_eq!((edge.from, edge.to), (from, to));
}

#[derive(Clone, Copy)]
struct PointerIds {
    source_node: NodeId,
    source_port: PortId,
    target_node: NodeId,
    target_port: PortId,
    alternate_source_node: NodeId,
    alternate_source_port: PortId,
    alternate_target_node: NodeId,
    alternate_target_port: PortId,
    edge: EdgeId,
}

impl PointerIds {
    fn stable() -> Self {
        Self {
            source_node: NodeId::from_u128(0xE610),
            source_port: PortId::from_u128(0xE611),
            target_node: NodeId::from_u128(0xE620),
            target_port: PortId::from_u128(0xE621),
            alternate_source_node: NodeId::from_u128(0xE630),
            alternate_source_port: PortId::from_u128(0xE631),
            alternate_target_node: NodeId::from_u128(0xE640),
            alternate_target_port: PortId::from_u128(0xE641),
            edge: EdgeId::from_u128(0xE650),
        }
    }
}

#[derive(Clone, Copy)]
struct PointerAnchors {
    source: ViewPoint,
    target: ViewPoint,
    alternate_source: ViewPoint,
    alternate_target: ViewPoint,
    empty: ViewPoint,
}

impl PointerAnchors {
    fn stable() -> Self {
        Self {
            source: ViewPoint::new(120.0, 90.0),
            target: ViewPoint::new(360.0, 90.0),
            alternate_source: ViewPoint::new(120.0, 250.0),
            alternate_target: ViewPoint::new(360.0, 250.0),
            empty: ViewPoint::new(620.0, 340.0),
        }
    }

    fn port(self, port: PortId) -> Pos2 {
        let ids = PointerIds::stable();
        let point = if port == ids.source_port {
            self.source
        } else if port == ids.target_port {
            self.target
        } else if port == ids.alternate_source_port {
            self.alternate_source
        } else if port == ids.alternate_target_port {
            self.alternate_target
        } else {
            panic!("unknown pointer-fixture port {port:?}")
        };
        Pos2::new(point.x, point.y)
    }

    fn source_pos(self) -> Pos2 {
        Pos2::new(self.source.x, self.source.y)
    }

    fn empty_pos(self) -> Pos2 {
        Pos2::new(self.empty.x, self.empty.y)
    }
}

#[derive(Clone, Copy)]
enum PointerResponse {
    NodeDrag,
    ReconnectSource,
    ReconnectTarget,
}

struct PointerState {
    runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    interaction: EguiCanvasInteraction,
    ids: PointerIds,
    anchors: PointerAnchors,
    response: Option<PointerResponse>,
    output: Option<EguiInteractionOutput>,
}

impl PointerState {
    fn new() -> Self {
        let ids = PointerIds::stable();
        let anchors = PointerAnchors::stable();
        let mut graph = GraphBuilder::new(GraphId::from_u128(0xE600));
        for (node, port, direction, anchor) in [
            (
                ids.source_node,
                ids.source_port,
                PortDirection::Out,
                anchors.source,
            ),
            (
                ids.target_node,
                ids.target_port,
                PortDirection::In,
                anchors.target,
            ),
            (
                ids.alternate_source_node,
                ids.alternate_source_port,
                PortDirection::Out,
                anchors.alternate_source,
            ),
            (
                ids.alternate_target_node,
                ids.alternate_target_port,
                PortDirection::In,
                anchors.alternate_target,
            ),
        ] {
            graph.insert_node(node, pointer_node(port, anchor));
            graph.insert_port(port, test_port(node, direction));
        }
        graph.insert_edge(
            ids.edge,
            Edge::new(EdgeKind::Data, ids.source_port, ids.target_port),
        );
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph.build().expect("valid pointer-interaction graph"))
            .expect("pointer-interaction runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("pointer-interaction view");
        let interaction = EguiCanvasInteraction::new(&view, CanvasInteractionConfig::default())
            .expect("pointer interaction");
        Self {
            runtime,
            host: EguiNativeFrameHost::new(view),
            interaction,
            ids,
            anchors,
            response: None,
            output: None,
        }
    }

    fn view(&self) -> &ViewSession {
        self.host.view()
    }

    fn response_center(&self, response: PointerResponse) -> Pos2 {
        match response {
            PointerResponse::NodeDrag | PointerResponse::ReconnectSource => {
                self.anchors.source_pos()
            }
            PointerResponse::ReconnectTarget => self.anchors.port(self.ids.target_port),
        }
    }
}

fn pointer_harness() -> Harness<'static, PointerState> {
    Harness::builder()
        .with_size(Vec2::new(760.0, 460.0))
        .build_ui_state(render_pointer_interaction, PointerState::new())
}

fn render_pointer_interaction(ui: &mut Ui, state: &mut PointerState) {
    render_pointer_frame(ui, state);
    state
        .interaction
        .begin_frame(ui.ctx(), ui.max_rect())
        .expect("begin pointer-interaction frame");
    if let Some(kind) = state.response {
        let response = ui.interact(
            Rect::from_center_size(state.response_center(kind), Vec2::splat(28.0)),
            Id::new((
                "jellyflow-egui-pointer-response",
                pointer_response_key(kind),
            )),
            Sense::drag(),
        );
        let target = if response.ctx.input(|input| {
            input.pointer.button_pressed(PointerButton::Primary)
                && response.is_pointer_button_down_on()
        }) {
            pointer_press_target(state, kind)
        } else {
            CanvasPressTarget::Pan
        };
        state
            .interaction
            .handle_response(&response, target)
            .expect("map native pointer response");
    }
    state.output = Some(
        state
            .interaction
            .end_frame(ui.ctx())
            .expect("end pointer-interaction frame"),
    );
}

fn render_pointer_frame(ui: &mut Ui, state: &PointerState) {
    let ids = state.ids;
    let anchors = state.anchors;
    state
        .host
        .show(ui, pointer_frame_request(), |ui, native| {
            let (port, outward, anchor) =
                pointer_native_port(ids, anchors, native.frame_node().id());
            let visual = ui.interact(
                Rect::from_center_size(Pos2::new(anchor.x, anchor.y), Vec2::splat(10.0)),
                native.id("port-visual"),
                Sense::hover(),
            );
            let hit = ui.interact(
                Rect::from_center_size(Pos2::new(anchor.x, anchor.y), Vec2::splat(24.0)),
                native.id("port-hit"),
                Sense::hover(),
            );
            native
                .measure_port_with_hit(port, &visual, &hit, outward)
                .expect("measure pointer-fixture port");
            ui.interact(ui.max_rect(), native.id("root"), Sense::hover())
        })
        .expect("submit pointer-fixture frame");
}

fn pointer_native_port(
    ids: PointerIds,
    anchors: PointerAnchors,
    node: NodeId,
) -> (PortId, HandlePosition, ViewPoint) {
    if node == ids.source_node {
        (ids.source_port, HandlePosition::Right, anchors.source)
    } else if node == ids.target_node {
        (ids.target_port, HandlePosition::Left, anchors.target)
    } else if node == ids.alternate_source_node {
        (
            ids.alternate_source_port,
            HandlePosition::Right,
            anchors.alternate_source,
        )
    } else if node == ids.alternate_target_node {
        (
            ids.alternate_target_port,
            HandlePosition::Left,
            anchors.alternate_target,
        )
    } else {
        panic!("unknown pointer-fixture node {node:?}")
    }
}

fn pointer_press_target(state: &PointerState, response: PointerResponse) -> CanvasPressTarget {
    match response {
        PointerResponse::NodeDrag => state
            .interaction
            .node_drag_target(
                state
                    .view()
                    .node_lifetime_key(state.ids.source_node)
                    .expect("source node identity query")
                    .expect("source node remains current"),
                SelectionMode::Replace,
            )
            .expect("resolve native node drag")
            .expect("source node is draggable"),
        PointerResponse::ReconnectSource | PointerResponse::ReconnectTarget => {
            let geometry = state
                .view()
                .geometry_snapshot()
                .expect("reconnect geometry");
            let route = geometry
                .route(state.ids.edge)
                .expect("published reconnect route");
            let endpoint = match response {
                PointerResponse::ReconnectSource => EdgeEndpoint::From,
                PointerResponse::ReconnectTarget => EdgeEndpoint::To,
                PointerResponse::NodeDrag => unreachable!(),
            };
            let fixed = match endpoint {
                EdgeEndpoint::From => route.facts().endpoints.target,
                EdgeEndpoint::To => route.facts().endpoints.source,
            };
            CanvasPressTarget::Connection(
                state
                    .view()
                    .connection_start_from_route(
                        route,
                        endpoint,
                        fixed,
                        NodeGraphConnectionMode::Strict,
                        false,
                    )
                    .expect("resolve reconnect facts")
                    .expect("reconnect endpoint is enabled"),
            )
        }
    }
}

fn press_pointer_response(harness: &mut Harness<'static, PointerState>, response: PointerResponse) {
    harness.state_mut().response = Some(response);
    harness.step();
    let start = harness.state().response_center(response);
    harness.hover_at(start);
    harness.step();
    harness.drag_at(start);
    harness.step();
    assert!(harness.state().interaction.owns_native_pointer());
}

fn release_pointer(harness: &mut Harness<'static, PointerState>, position: Pos2, move_first: bool) {
    if move_first {
        harness.hover_at(position);
        harness.step();
    }
    harness.event(Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

fn execute_pointer_submission(harness: &mut Harness<'static, PointerState>) {
    let submission = take_pointer_submission(harness);
    let ctx = harness.ctx.clone();
    let state = harness.state_mut();
    let mut bound = state
        .interaction
        .bind_submission(submission)
        .expect("bind native pointer submission");
    let command = bound
        .take_command()
        .expect("native pointer interaction emits a command");
    let completion = state.runtime.execute(command);
    assert!(completion.applied_revision().is_some());
    let settled = state
        .interaction
        .settle_submission(
            &ctx,
            bound,
            InteractionSubmissionSettlementEvidence::Command(completion),
        )
        .expect("settle native pointer submission");
    assert!(!settled.renderer_work_pending());
    assert!(settled.terminal_query_error().is_none());
}

fn take_pointer_submission(
    harness: &mut Harness<'static, PointerState>,
) -> InteractionGraphSubmission {
    for _ in 0..16 {
        let mut output = harness
            .state_mut()
            .output
            .take()
            .expect("each native frame records pointer output");
        let pending = output.renderer_work_pending();
        let mut submission = None;
        for effect in output.take_effects() {
            match effect {
                CanvasInteractionEffect::SubmitGraph {
                    submission: candidate,
                } => {
                    assert!(submission.replace(candidate).is_none());
                }
                CanvasInteractionEffect::Search(_) => {
                    panic!("focused reconnect and node-drag cases cannot enter search")
                }
            }
        }
        if let Some(submission) = submission {
            return submission;
        }
        assert!(
            pending,
            "native release must submit or retain bounded query work"
        );
        harness.step();
    }
    panic!("native pointer submission did not settle within its bounded frame grant")
}

const fn pointer_response_key(response: PointerResponse) -> u8 {
    match response {
        PointerResponse::NodeDrag => 0,
        PointerResponse::ReconnectSource => 1,
        PointerResponse::ReconnectTarget => 2,
    }
}

fn pointer_frame_request() -> FrameRequest {
    FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint { x: -40.0, y: -40.0 },
            size: CanvasSize {
                width: 800.0,
                height: 520.0,
            },
        },
        RendererStamp::new("test.egui-pointer-interaction", 1),
    )
}

fn pointer_node(port: PortId, anchor: ViewPoint) -> Node {
    let mut node = test_node(vec![port], anchor.x - 30.0);
    node.pos.y = anchor.y - 30.0;
    node.size = Some(CanvasSize {
        width: 60.0,
        height: 60.0,
    });
    node.kind = NodeKindKey::new("test.egui-pointer-interaction");
    node
}

fn test_node(ports: Vec<PortId>, x: f32) -> Node {
    Node {
        kind: NodeKindKey::new("test.egui-click-connect"),
        kind_version: 1,
        pos: CanvasPoint { x, y: 0.0 },
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
        ports,
        data: serde_json::Value::Null,
    }
}

fn test_port(node: NodeId, direction: PortDirection) -> Port {
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
