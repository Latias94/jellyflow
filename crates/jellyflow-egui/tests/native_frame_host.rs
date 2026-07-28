use std::cell::Cell;
use std::rc::Rc;

use egui::{Id, Pos2, Rect, Sense, TextEdit, Ui, UiBuilder, Vec2, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable as _};
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, GraphBuilder, GraphId, Node,
    NodeGraphConnectionMode, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};
use jellyflow_egui::{EguiNativeFrameError, EguiNativeFrameHost, EguiNativeFrameOutcome};
use jellyflow_runtime::commands::{
    CommandExecutionKind, GraphCommand, IncidentEdgePolicy, TopologyEdit, TopologyPort,
};
use jellyflow_runtime::geometry::{HandlePosition, ViewPoint};
use jellyflow_runtime::graph::{GraphRevision, GraphRuntime, HistoryState};
use jellyflow_runtime::interaction::{
    ConnectionGestureBinding, ConnectionInteractionStart, ConnectionProbe,
};
use jellyflow_runtime::view::{
    ConnectionProbeQuery, ConnectionProbeQueryOutcome, ConnectionProbeTargeting,
    FeedbackSubmissionError, FrameRequest, LayoutNodeStatus, PublishedPortOutcome, RendererStamp,
    ViewCloseReason, ViewSession, ViewSessionError, ViewSessionEvent, ViewSessionOptions,
};
use jellyflow_runtime::viewport::ViewportTransform;

const RENDERER_KEY: &str = "test.egui.native-frame";

struct CompleteLayoutState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    node: NodeId,
    port: PortId,
    root_id: Option<Id>,
    child_ui_id: Option<Id>,
    native_widget_id: Option<Id>,
    ui_widget_id: Option<Id>,
    outcome: Option<EguiNativeFrameOutcome>,
}

impl CompleteLayoutState {
    fn new() -> Self {
        let node = NodeId::from_u128(1);
        let port = PortId::from_u128(11);
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph_with_ports(node, [port]))
            .expect("test runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("test view");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            node,
            port,
            root_id: None,
            child_ui_id: None,
            native_widget_id: None,
            ui_widget_id: None,
            outcome: None,
        }
    }
}

fn render_complete_layout(ui: &mut Ui, state: &mut CompleteLayoutState) {
    let port = state.port;
    let root_id = &mut state.root_id;
    let child_ui_id = &mut state.child_ui_id;
    let native_widget_id = &mut state.native_widget_id;
    let ui_widget_id = &mut state.ui_widget_id;
    let outcome = state
        .host
        .show(ui, frame_request(), |ui, node| {
            root_id.get_or_insert(node.root_id());
            child_ui_id.get_or_insert(ui.id());
            native_widget_id.get_or_insert(node.id("identity-probe"));
            ui_widget_id.get_or_insert(ui.make_persistent_id("identity-probe"));
            let root = ui.group(|ui| {
                ui.label("Native node");
                let (visual_rect, visual) =
                    ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                let hit = ui.interact(
                    visual_rect.expand(4.0),
                    node.id(("port-hit", port)),
                    Sense::drag(),
                );
                node.measure_port_with_hit(port, &visual, &hit, HandlePosition::Right)
                    .expect("frame port is measurable");
            });
            root.response
        })
        .expect("native frame submits");
    state.outcome.get_or_insert(outcome);
}

#[test]
fn complete_real_response_layout_is_accepted() {
    let harness = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(render_complete_layout, CompleteLayoutState::new());
    let state = harness.state();
    let outcome = state.outcome.as_ref().expect("frame outcome");

    assert_eq!(outcome.rendered_instances().len(), 1);
    assert_eq!(outcome.rendered_instances()[0].node(), state.node);
    assert_eq!(
        state.root_id,
        Some(Id::new((
            "jellyflow-native-node",
            &outcome.rendered_instances()[0]
        ))),
    );
    assert_eq!(state.child_ui_id, state.root_id);
    assert_eq!(state.ui_widget_id, state.native_widget_id);
    assert_eq!(
        outcome
            .receipt()
            .layout_receipts()
            .next()
            .and_then(|receipt| receipt.status()),
        Some(LayoutNodeStatus::Published),
    );

    let geometry = state.host.view().geometry_snapshot().expect("geometry");
    let node = geometry.node(state.node).expect("published node geometry");
    let port = node
        .ports()
        .get(&state.port)
        .expect("published port geometry");
    assert!(node.is_fresh());
    assert_eq!(port.outward(), HandlePosition::Right);
    assert_eq!(port.route_anchor().x, port.visual_bounds().origin.x + 16.0);
    let hit_bounds = port.hit_bounds().expect("rendered port is interactive");
    assert_eq!(hit_bounds.size.width, 24.0);
    assert_eq!(hit_bounds.size.height, 24.0);
    assert_ne!(hit_bounds, port.visual_bounds());
}

#[derive(Clone, Copy)]
enum PortClip {
    Partial,
    Full,
    Visible,
}

struct ClippedPortState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    source_port: PortId,
    target_node: NodeId,
    target_port: PortId,
    clip: PortClip,
    scene_origin: Pos2,
    declared_hit: Option<Rect>,
    effective_hit: Option<Rect>,
}

impl ClippedPortState {
    fn new() -> Self {
        let source_node = NodeId::from_u128(0xc1);
        let source_port = PortId::from_u128(0xc11);
        let target_node = NodeId::from_u128(0xc2);
        let target_port = PortId::from_u128(0xc21);
        let mut target = test_node(vec![target_port]);
        target.pos = CanvasPoint { x: 280.0, y: 0.0 };
        let mut builder = GraphBuilder::new(GraphId::from_u128(0xc0));
        builder.insert_node(source_node, test_node(vec![source_port]));
        builder.insert_port(source_port, output_port(source_node, "source"));
        builder.insert_node(target_node, target);
        builder.insert_port(target_port, input_port(target_node, "target"));
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().expect("clipped port graph"))
            .expect("clipped port runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("clipped port view");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            source_port,
            target_node,
            target_port,
            clip: PortClip::Partial,
            scene_origin: Pos2::ZERO,
            declared_hit: None,
            effective_hit: None,
        }
    }
}

fn render_clipped_port(ui: &mut Ui, state: &mut ClippedPortState) {
    state.scene_origin = ui.max_rect().min;
    let target_node = state.target_node;
    let target_port = state.target_port;
    let clip = state.clip;
    let declared_hit = &mut state.declared_hit;
    let effective_hit = &mut state.effective_hit;
    state
        .host
        .show(ui, frame_request(), |ui, native| {
            let root = ui.group(|ui| {
                ui.label("Clipped port");
                let (visual_rect, visual) =
                    ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                let hit_rect = visual_rect.expand(8.0);
                let previous_clip = ui.clip_rect();
                let hit_clip = if native.frame_node().id() == target_node {
                    match clip {
                        PortClip::Partial => Rect::from_min_max(
                            hit_rect.min,
                            Pos2::new(hit_rect.center().x, hit_rect.max.y),
                        ),
                        PortClip::Full => {
                            Rect::from_min_size(hit_rect.max + Vec2::splat(8.0), Vec2::splat(4.0))
                        }
                        PortClip::Visible => previous_clip,
                    }
                } else {
                    previous_clip
                };
                ui.set_clip_rect(hit_clip);
                let hit = ui.interact(
                    hit_rect,
                    native.id(("clipped-port-hit", native.frame_node().id())),
                    Sense::drag(),
                );
                ui.set_clip_rect(previous_clip);

                let (port, outward) = if native.frame_node().id() == target_node {
                    *declared_hit = Some(hit.rect);
                    *effective_hit = Some(hit.interact_rect);
                    (target_port, HandlePosition::Left)
                } else {
                    (state.source_port, HandlePosition::Right)
                };
                native
                    .measure_port_with_hit(port, &visual, &hit, outward)
                    .expect("clipped port is measurable");
            });
            root.response
        })
        .expect("clipped port frame submits");
}

#[test]
fn native_clip_controls_effective_port_hit_without_withdrawing_route_geometry() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(720.0, 360.0))
        .build_ui_state(render_clipped_port, ClippedPortState::new());

    let state = harness.state();
    let declared = state.declared_hit.expect("declared target hit");
    let effective = state.effective_hit.expect("effective target hit");
    assert!(effective.is_positive());
    assert_ne!(effective, declared);
    let partial_geometry = state
        .host
        .view()
        .geometry_snapshot()
        .expect("partial geometry");
    let partial_port = partial_geometry
        .port(state.target_port)
        .expect("partially clipped target keeps route geometry");
    let expected_partial = screen_rect_to_canvas(effective, state.scene_origin);
    assert_eq!(partial_port.hit_bounds(), Some(expected_partial));
    let route_anchor = partial_port.route_anchor();
    let clipped_pointer =
        ViewPoint::new(declared.max.x - state.scene_origin.x - 1.0, route_anchor.y);
    assert!(matches!(
        state
            .host
            .view()
            .connection_probe(ConnectionProbeQuery::new(
                connection_binding(
                    state.host.view(),
                    state.source_port,
                    EdgeId::from_u128(0xc001),
                ),
                clipped_pointer,
                ConnectionProbeTargeting::new(1.0, 1.0).expect("valid target radius"),
                64,
            ))
            .expect("partial clip probe"),
        ConnectionProbeQueryOutcome::Complete {
            probe: ConnectionProbe::None,
            ..
        }
    ));

    harness.state_mut().clip = PortClip::Full;
    harness.step();
    let state = harness.state();
    assert!(
        !state
            .effective_hit
            .expect("fully clipped response")
            .is_positive()
    );
    let clipped_geometry = state
        .host
        .view()
        .geometry_snapshot()
        .expect("clipped geometry");
    let clipped_port = clipped_geometry
        .port(state.target_port)
        .expect("fully clipped target keeps route geometry");
    assert_eq!(clipped_port.hit_bounds(), None);
    assert_eq!(clipped_port.route_anchor(), route_anchor);
    assert!(matches!(
        state
            .host
            .view()
            .connection_probe(ConnectionProbeQuery::new(
                connection_binding(
                    state.host.view(),
                    state.source_port,
                    EdgeId::from_u128(0xc002),
                ),
                ViewPoint::new(route_anchor.x, route_anchor.y),
                ConnectionProbeTargeting::new(12.0, 12.0).expect("valid target radius"),
                64,
            ))
            .expect("full clip probe"),
        ConnectionProbeQueryOutcome::Complete {
            probe: ConnectionProbe::None,
            ..
        }
    ));

    harness.state_mut().clip = PortClip::Visible;
    harness.step();
    let state = harness.state();
    let restored_geometry = state
        .host
        .view()
        .geometry_snapshot()
        .expect("restored geometry");
    let restored_port = restored_geometry
        .port(state.target_port)
        .expect("restored target geometry");
    assert_eq!(
        restored_port.hit_bounds(),
        Some(screen_rect_to_canvas(
            state.declared_hit.expect("restored declared hit"),
            state.scene_origin,
        ))
    );
    assert_eq!(restored_port.route_anchor(), route_anchor);
    assert!(matches!(
        state
            .host
            .view()
            .connection_probe(ConnectionProbeQuery::new(
                connection_binding(
                    state.host.view(),
                    state.source_port,
                    EdgeId::from_u128(0xc003),
                ),
                ViewPoint::new(route_anchor.x, route_anchor.y),
                ConnectionProbeTargeting::new(12.0, 12.0).expect("valid target radius"),
                64,
            ))
            .expect("restored probe"),
        ConnectionProbeQueryOutcome::Complete {
            probe: ConnectionProbe::Valid { target, .. },
            ..
        } if target.port() == state.target_port
    ));
}

struct ExplicitPortOutcomesState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    node: NodeId,
    rendered: PortId,
    hidden: PortId,
    culled: PortId,
}

impl ExplicitPortOutcomesState {
    fn new() -> Self {
        let node = NodeId::from_u128(0x51);
        let rendered = PortId::from_u128(0x511);
        let hidden = PortId::from_u128(0x512);
        let culled = PortId::from_u128(0x513);
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph_with_ports(node, [rendered, hidden, culled]))
            .expect("port outcome runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("port outcome view");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            node,
            rendered,
            hidden,
            culled,
        }
    }
}

fn render_explicit_port_outcomes(ui: &mut Ui, state: &mut ExplicitPortOutcomesState) {
    state
        .host
        .show(ui, frame_request(), |ui, native| {
            let root = ui.group(|ui| {
                ui.label("Outcome node");
                let (_rect, response) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::drag());
                native
                    .measure_port(state.rendered, &response, HandlePosition::Right)
                    .expect("rendered port");
                native.mark_port_hidden(state.hidden).expect("hidden port");
                native.mark_port_culled(state.culled).expect("culled port");
            });
            root.response
        })
        .expect("explicit outcomes submit");
}

#[test]
fn only_rendered_port_outcomes_publish_hit_geometry() {
    let harness = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(
            render_explicit_port_outcomes,
            ExplicitPortOutcomesState::new(),
        );
    let state = harness.state();
    let geometry = state.host.view().geometry_snapshot().expect("geometry");
    let node = geometry.node(state.node).expect("node geometry");

    assert!(node.is_fresh());
    assert_eq!(node.ports().len(), 1);
    assert_eq!(node.port_outcomes().len(), 3);
    assert!(matches!(
        geometry.port_outcome(state.rendered),
        Some(PublishedPortOutcome::Rendered(_))
    ));
    assert!(matches!(
        geometry.port_outcome(state.hidden),
        Some(PublishedPortOutcome::Hidden)
    ));
    assert!(matches!(
        geometry.port_outcome(state.culled),
        Some(PublishedPortOutcome::Culled)
    ));
    assert!(geometry.port(state.rendered).is_some());
    assert!(geometry.port(state.hidden).is_none());
    assert!(geometry.port(state.culled).is_none());
}

struct TransformRoundTripState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    node: NodeId,
    port: PortId,
    scene_origin: Pos2,
    root_rect: Option<Rect>,
    port_rect: Option<Rect>,
}

impl TransformRoundTripState {
    fn new() -> Self {
        let node = NodeId::from_u128(7);
        let port = PortId::from_u128(71);
        let mut node_record = test_node(vec![port]);
        node_record.pos = CanvasPoint { x: 30.0, y: 40.0 };
        let mut builder = GraphBuilder::new(GraphId::from_u128(0x8007));
        builder.insert_node(node, node_record);
        builder.insert_port(port, output_port(node, "output"));
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().expect("transform graph"))
            .expect("transform runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("transform view");
        view.set_viewport(
            ViewportTransform::new(CanvasPoint { x: 10.0, y: -5.0 }, 2.0).expect("valid transform"),
        )
        .expect("set transform");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            node,
            port,
            scene_origin: Pos2::new(72.0, 48.0),
            root_rect: None,
            port_rect: None,
        }
    }
}

fn render_transform_round_trip(ui: &mut Ui, state: &mut TransformRoundTripState) {
    let scene_rect = Rect::from_min_size(state.scene_origin, Vec2::new(500.0, 320.0));
    let mut scene = ui.new_child(
        UiBuilder::new()
            .id_salt("transform-scene")
            .max_rect(scene_rect),
    );
    let port = state.port;
    let root_rect = &mut state.root_rect;
    let port_rect = &mut state.port_rect;
    state
        .host
        .show(&mut scene, frame_request(), |ui, native| {
            let root = ui.group(|ui| {
                ui.label("Transformed node");
                let (_rect, response) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::drag());
                *port_rect = Some(response.rect);
                native
                    .measure_port(port, &response, HandlePosition::Bottom)
                    .expect("real frame port is measurable");
            });
            *root_rect = Some(root.response.rect);
            root.response
        })
        .expect("transformed frame submits");
}

#[test]
fn nonzero_scene_origin_pan_and_zoom_round_trip_response_geometry() {
    let harness = Harness::builder()
        .with_size(Vec2::new(720.0, 480.0))
        .build_ui_state(render_transform_round_trip, TransformRoundTripState::new());
    let state = harness.state();
    let root = state.root_rect.expect("native root response");
    let port_response = state.port_rect.expect("native port response");
    let geometry = state.host.view().geometry_snapshot().expect("geometry");
    let node = geometry.node(state.node).expect("node geometry");
    let port = node.ports().get(&state.port).expect("port geometry");
    let to_canvas = |point: Pos2| CanvasPoint {
        x: (point.x - state.scene_origin.x) / 2.0 - 10.0,
        y: (point.y - state.scene_origin.y) / 2.0 + 5.0,
    };
    let expected_root = to_canvas(root.min);
    assert_approx(node.bounds().origin.x, expected_root.x);
    assert_approx(node.bounds().origin.y, expected_root.y);
    assert_approx(node.bounds().size.width, root.width() / 2.0);
    assert_approx(node.bounds().size.height, root.height() / 2.0);
    let expected_port = to_canvas(port_response.min);
    assert_approx(port.visual_bounds().origin.x, expected_port.x);
    assert_approx(port.visual_bounds().origin.y, expected_port.y);
    assert_approx(port.visual_bounds().size.width, port_response.width() / 2.0);
    assert_approx(
        port.visual_bounds().size.height,
        port_response.height() / 2.0,
    );
    assert_eq!(port.outward(), HandlePosition::Bottom);
}

struct OrderedCommandsState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    queue_commands: bool,
    outcome: Option<EguiNativeFrameOutcome>,
}

impl OrderedCommandsState {
    fn new() -> Self {
        let first = NodeId::from_u128(8);
        let second = NodeId::from_u128(9);
        let first_port = PortId::from_u128(81);
        let second_port = PortId::from_u128(91);
        let mut first_node = test_node(vec![first_port]);
        let mut second_node = test_node(vec![second_port]);
        second_node.pos = CanvasPoint { x: 280.0, y: 0.0 };
        first_node.data = serde_json::json!({ "order": 0 });
        second_node.data = serde_json::json!({ "order": 0 });
        let mut builder = GraphBuilder::new(GraphId::from_u128(0x8008));
        builder.insert_node(first, first_node);
        builder.insert_port(first_port, output_port(first, "first"));
        builder.insert_node(second, second_node);
        builder.insert_port(second_port, output_port(second, "second"));
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().expect("ordered graph"))
            .expect("ordered runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("ordered view");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            queue_commands: false,
            outcome: None,
        }
    }
}

fn render_ordered_commands(ui: &mut Ui, state: &mut OrderedCommandsState) {
    let queue_commands = state.queue_commands;
    let command_offset = Cell::new(0_u64);
    let outcome = state
        .host
        .show(ui, frame_request(), |ui, native| {
            if queue_commands {
                let offset = command_offset.get();
                native
                    .command(GraphCommand::set_node_data(
                        native.graph_version(),
                        native.frame_node().id(),
                        serde_json::json!({ "order": offset + 1 }),
                    ))
                    .expect("command is collected");
                command_offset.set(offset + 1);
            }
            let root = ui.group(|ui| {
                ui.label(format!("Node {:?}", native.frame_node().id()));
                for port in native.frame_node().ports() {
                    let (_rect, response) =
                        ui.allocate_exact_size(Vec2::splat(14.0), Sense::drag());
                    native
                        .measure_port(port.id(), &response, HandlePosition::Right)
                        .expect("real frame port is measurable");
                }
            });
            root.response
        })
        .expect("ordered frame submits");
    if queue_commands {
        state.outcome = Some(outcome);
        state.queue_commands = false;
    }
}

#[test]
fn same_frame_node_commands_preserve_callback_order_and_reject_a_stale_followup() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(720.0, 360.0))
        .build_ui_state(render_ordered_commands, OrderedCommandsState::new());
    harness.state_mut().queue_commands = true;
    harness.step();

    let state = harness.state();
    let receipt = state.outcome.as_ref().expect("ordered outcome").receipt();
    assert_eq!(
        receipt
            .command_completions()
            .map(|completion| completion.kind())
            .collect::<Vec<_>>(),
        [
            CommandExecutionKind::Applied,
            CommandExecutionKind::Conflict
        ]
    );
    assert_eq!(
        receipt
            .layout_receipts()
            .map(|receipt| receipt.status())
            .collect::<Vec<_>>(),
        [
            Some(LayoutNodeStatus::Stale),
            Some(LayoutNodeStatus::Unchanged),
        ]
    );
    let graph = state.host.view().graph_snapshot().expect("ordered graph");
    assert_eq!(graph.revision(), 1);
    let ordered_values = graph
        .graph()
        .nodes()
        .values()
        .map(|node| node.data["order"].as_u64().expect("order value"))
        .collect::<Vec<_>>();
    assert_eq!(ordered_values, vec![1, 0]);
}

struct MultiViewState {
    _runtime: GraphRuntime,
    first: EguiNativeFrameHost,
    second: EguiNativeFrameHost,
    node: NodeId,
    first_prompt: String,
    second_prompt: String,
    first_root: Option<Id>,
    second_root: Option<Id>,
    first_text: Option<Id>,
    second_text: Option<Id>,
}

impl MultiViewState {
    fn new() -> Self {
        let node = NodeId::from_u128(10);
        let port = PortId::from_u128(101);
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph_with_ports(node, [port]))
            .expect("multi-view runtime");
        let first = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("first view");
        let second = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("second view");
        Self {
            _runtime: runtime,
            first: EguiNativeFrameHost::new(first),
            second: EguiNativeFrameHost::new(second),
            node,
            first_prompt: String::new(),
            second_prompt: String::new(),
            first_root: None,
            second_root: None,
            first_text: None,
            second_text: None,
        }
    }
}

fn render_multi_view(ui: &mut Ui, state: &mut MultiViewState) {
    show_multi_view_node(
        ui,
        &state.first,
        "First prompt",
        &mut state.first_prompt,
        &mut state.first_root,
        &mut state.first_text,
    );
    show_multi_view_node(
        ui,
        &state.second,
        "Second prompt",
        &mut state.second_prompt,
        &mut state.second_root,
        &mut state.second_text,
    );
}

fn show_multi_view_node(
    ui: &mut Ui,
    host: &EguiNativeFrameHost,
    label: &str,
    prompt: &mut String,
    root_id: &mut Option<Id>,
    text_id: &mut Option<Id>,
) {
    host.show(ui, frame_request(), |ui, native| {
        *root_id = Some(native.root_id());
        let root = ui.group(|ui| {
            let label = ui.label(label);
            let id = native.tracked_state_id::<egui::text_edit::TextEditState>("prompt");
            let text = ui
                .add(TextEdit::singleline(prompt).id(id))
                .labelled_by(label.id);
            text.request_focus();
            *text_id = Some(text.id);
            for port in native.frame_node().ports() {
                let (_rect, response) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::drag());
                native
                    .measure_port(port.id(), &response, HandlePosition::Right)
                    .expect("real frame port is measurable");
            }
        });
        root.response
    })
    .expect("multi-view frame submits");
}

#[test]
fn view_identity_isolates_native_ids_state_geometry_and_close() {
    let harness = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(render_multi_view, MultiViewState::new());
    let state = harness.state();
    let first_root = state.first_root.expect("first root");
    let second_root = state.second_root.expect("second root");
    let first_text = state.first_text.expect("first text");
    let second_text = state.second_text.expect("second text");
    assert_ne!(first_root, second_root);
    assert_ne!(first_text, second_text);
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, first_text).is_some());
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, second_text).is_some());
    assert!(
        state
            .first
            .view()
            .geometry_snapshot()
            .expect("first geometry")
            .node(state.node)
            .is_some()
    );
    assert!(
        state
            .second
            .view()
            .geometry_snapshot()
            .expect("second geometry")
            .node(state.node)
            .is_some()
    );

    state.first.close().expect("first host closes");
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, first_text).is_none());
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, second_text).is_some());
    assert!(state.second.view().close_reason().is_none());
    assert!(
        state
            .second
            .view()
            .geometry_snapshot()
            .expect("second geometry after first close")
            .node(state.node)
            .is_some()
    );
}

struct SharedContextState {
    host: Rc<EguiNativeFrameHost>,
    prompt: String,
    text_id: Option<Id>,
}

fn render_shared_context(ui: &mut Ui, state: &mut SharedContextState) {
    let text_id = &mut state.text_id;
    let prompt = &mut state.prompt;
    state
        .host
        .show(ui, frame_request(), |ui, native| {
            let root = ui.group(|ui| {
                let label = ui.label("Shared context prompt");
                let id = native.tracked_state_id::<egui::text_edit::TextEditState>("prompt");
                let text = ui
                    .add(TextEdit::singleline(prompt).id(id))
                    .labelled_by(label.id);
                *text_id = Some(text.id);
                for port in native.frame_node().ports() {
                    let (_rect, response) =
                        ui.allocate_exact_size(Vec2::splat(14.0), Sense::drag());
                    native
                        .measure_port(port.id(), &response, HandlePosition::Right)
                        .expect("real frame port is measurable");
                }
            });
            root.response
        })
        .expect("shared context frame submits");
}

#[test]
fn hard_retirement_cleans_the_same_instance_from_every_egui_context() {
    let node = NodeId::from_u128(11);
    let port = PortId::from_u128(111);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph_with_ports(node, [port]))
        .expect("shared runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shared view");
    let host = Rc::new(EguiNativeFrameHost::new(view));
    let mut first = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(
            render_shared_context,
            SharedContextState {
                host: Rc::clone(&host),
                prompt: String::new(),
                text_id: None,
            },
        );
    let mut second = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(
            render_shared_context,
            SharedContextState {
                host: Rc::clone(&host),
                prompt: String::new(),
                text_id: None,
            },
        );
    first.step();
    second.step();
    let first_text = first.state().text_id.expect("first context text");
    let second_text = second.state().text_id.expect("second context text");
    assert_eq!(first_text, second_text);
    assert!(egui::text_edit::TextEditState::load(&first.ctx, first_text).is_some());
    assert!(egui::text_edit::TextEditState::load(&second.ctx, second_text).is_some());

    host.close().expect("shared host closes");
    assert!(egui::text_edit::TextEditState::load(&first.ctx, first_text).is_none());
    assert!(egui::text_edit::TextEditState::load(&second.ctx, second_text).is_none());
}

#[test]
fn terminal_view_event_retires_registered_egui_state_without_another_frame() {
    let node = NodeId::from_u128(12);
    let port = PortId::from_u128(121);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph_with_ports(node, [port]))
        .expect("runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("view");
    let host = Rc::new(EguiNativeFrameHost::new(view));
    let harness = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(
            render_shared_context,
            SharedContextState {
                host: Rc::clone(&host),
                prompt: String::new(),
                text_id: None,
            },
        );
    let text_id = harness.state().text_id.expect("text state id");
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, text_id).is_some());

    host.view().close().expect("exposed view closes");

    assert!(egui::text_edit::TextEditState::load(&harness.ctx, text_id).is_none());
}

struct DroppableHostState {
    runtime: GraphRuntime,
    host: Option<EguiNativeFrameHost>,
    prompt: String,
    text_id: Option<Id>,
}

impl DroppableHostState {
    fn new() -> Self {
        let node = NodeId::from_u128(16);
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph_with_ports(node, []))
            .expect("runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("view");
        Self {
            runtime,
            host: Some(EguiNativeFrameHost::new(view)),
            prompt: String::new(),
            text_id: None,
        }
    }
}

fn render_droppable_host(ui: &mut Ui, state: &mut DroppableHostState) {
    let DroppableHostState {
        host,
        prompt,
        text_id,
        ..
    } = state;
    let Some(host) = host.as_ref() else {
        return;
    };
    host.show(ui, frame_request(), |ui, native| {
        let root = ui.group(|ui| {
            let id = native.tracked_state_id::<egui::text_edit::TextEditState>("prompt");
            let text = ui.add(TextEdit::singleline(prompt).id(id));
            text.request_focus();
            *text_id = Some(text.id);
        });
        root.response
    })
    .expect("droppable host frame submits");
}

#[test]
fn dropping_last_host_owner_retires_egui_state_focus_and_view() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(render_droppable_host, DroppableHostState::new());
    let text_id = harness.state().text_id.expect("text state id");
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, text_id).is_some());
    assert!(harness.ctx.memory(|memory| memory.has_focus(text_id)));
    assert_eq!(harness.state().runtime.live_view_count(), 1);

    let host = harness.state_mut().host.take().expect("last host owner");
    drop(host);

    assert!(egui::text_edit::TextEditState::load(&harness.ctx, text_id).is_none());
    assert!(!harness.ctx.memory(|memory| memory.has_focus(text_id)));
    assert_eq!(harness.state().runtime.live_view_count(), 0);
}

#[test]
fn runtime_patch_retires_deleted_egui_state_without_another_frame() {
    let node = NodeId::from_u128(13);
    let port = PortId::from_u128(131);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph_with_ports(node, [port]))
        .expect("runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("view");
    let host = Rc::new(EguiNativeFrameHost::new(view));
    let harness = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(
            render_shared_context,
            SharedContextState {
                host: Rc::clone(&host),
                prompt: String::new(),
                text_id: None,
            },
        );
    let text_id = harness.state().text_id.expect("text state id");
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, text_id).is_some());

    host.view()
        .execute(GraphCommand::topology(
            host.view()
                .graph_snapshot()
                .expect("graph before delete")
                .version(),
            TopologyEdit::new().remove_node(node, IncidentEdgePolicy::Reject),
        ))
        .expect("view accepts delete")
        .into_result()
        .expect("delete executes")
        .applied()
        .expect("delete applies");

    assert!(egui::text_edit::TextEditState::load(&harness.ctx, text_id).is_none());
}

struct DeleteDuringRenderState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    node: NodeId,
    port: PortId,
    prompt: String,
    text_id: Option<Id>,
    focus_acquired: bool,
    deleted: bool,
    frame_results: Vec<Result<EguiNativeFrameOutcome, EguiNativeFrameError>>,
}

impl DeleteDuringRenderState {
    fn new() -> Self {
        let node = NodeId::from_u128(14);
        let port = PortId::from_u128(141);
        let stateless_peer = NodeId::from_u128(15);
        let mut builder = GraphBuilder::new(GraphId::from_u128(0x8014));
        builder.insert_node(node, test_node(vec![port]));
        builder.insert_port(port, output_port(node, "output"));
        let mut peer = test_node(Vec::new());
        peer.pos = CanvasPoint { x: 280.0, y: 0.0 };
        builder.insert_node(stateless_peer, peer);
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().expect("valid deletion graph"))
            .expect("runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("view");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            node,
            port,
            prompt: String::new(),
            text_id: None,
            focus_acquired: false,
            deleted: false,
            frame_results: Vec::new(),
        }
    }
}

fn render_delete_during_render(ui: &mut Ui, state: &mut DeleteDuringRenderState) {
    let DeleteDuringRenderState {
        host,
        node,
        prompt,
        text_id,
        focus_acquired,
        deleted,
        frame_results,
        ..
    } = state;
    let result = host.show(ui, frame_request(), |ui, native| {
        if native.frame_node().id() != *node {
            return ui.label("Stateless peer");
        }
        let root = ui.group(|ui| {
            let label = ui.label("Ephemeral prompt");
            let id = native.tracked_state_id::<egui::text_edit::TextEditState>("prompt");
            let text = ui
                .add(TextEdit::singleline(prompt).id(id))
                .labelled_by(label.id);
            text.request_focus();
            *text_id = Some(text.id);
            *focus_acquired = ui.memory(|memory| memory.has_focus(text.id));
            if !*deleted {
                host.view()
                    .execute(GraphCommand::topology(
                        native.graph_version(),
                        TopologyEdit::new().remove_node(*node, IncidentEdgePolicy::Reject),
                    ))
                    .expect("view accepts delete during render")
                    .into_result()
                    .expect("delete executes during render")
                    .applied()
                    .expect("delete applies during render");
                *deleted = true;
            }
        });
        root.response
    });
    frame_results.push(result);
}

#[test]
fn delete_during_render_stales_only_the_retired_node_and_publishes_its_peer() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(render_delete_during_render, DeleteDuringRenderState::new());
    let state = harness.state();
    let text_id = state.text_id.expect("text state id");
    let initial_result_count = state.frame_results.len();
    let first_result = state.frame_results.first().expect("first frame result");
    let outcome = first_result
        .as_ref()
        .expect("local geometry impact keeps the frame submittable");
    let layouts = outcome.receipt().layout_receipts().collect::<Vec<_>>();
    assert_eq!(
        layouts.len(),
        2,
        "unexpected delete-during-render result: {first_result:?}"
    );
    assert_eq!(layouts[0].node(), state.node);
    assert_eq!(layouts[0].status(), Some(LayoutNodeStatus::Stale));
    assert_eq!(layouts[1].status(), Some(LayoutNodeStatus::Published));
    let publication = outcome
        .receipt()
        .publication()
        .expect("the unaffected peer publishes");
    assert_eq!(publication.published_nodes().len(), 1);
    assert_ne!(publication.published_nodes()[0], state.node);
    assert!(state.deleted);
    assert!(state.focus_acquired);
    assert!(state.host.view().close_reason().is_none());
    let geometry = state.host.view().geometry_snapshot().expect("geometry");
    assert!(geometry.node(state.node).is_none());
    assert!(geometry.port(state.port).is_none());
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, text_id).is_none());
    assert!(!harness.ctx.memory(|memory| memory.has_focus(text_id)));

    harness.step();

    let state = harness.state();
    assert_eq!(state.frame_results.len(), initial_result_count + 1);
    assert!(state.frame_results.last().is_some_and(Result::is_ok));
    assert!(state.host.view().close_reason().is_none());
    let geometry = state.host.view().geometry_snapshot().expect("geometry");
    assert!(geometry.node(state.node).is_none());
    assert!(geometry.port(state.port).is_none());
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, text_id).is_none());
    assert!(!harness.ctx.memory(|memory| memory.has_focus(text_id)));
}

struct CloseDuringRenderState {
    runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    node: NodeId,
    port: PortId,
    prompt: String,
    text_id: Option<Id>,
    focus_acquired: bool,
    closed: bool,
    frame_results: Vec<Result<EguiNativeFrameOutcome, EguiNativeFrameError>>,
    graph_revision_before: GraphRevision,
    history_before: HistoryState,
    feedback_side_effect_events: Rc<Cell<usize>>,
}

impl CloseDuringRenderState {
    fn new() -> Self {
        let node = NodeId::from_u128(16);
        let port = PortId::from_u128(161);
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph_with_ports(node, [port]))
            .expect("test runtime");
        let graph_revision_before = runtime.snapshot().expect("initial graph").revision();
        let history_before = runtime.history_state().expect("initial history");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("test view");
        let feedback_side_effect_events = Rc::new(Cell::new(0));
        let callback_events = Rc::clone(&feedback_side_effect_events);
        view.subscribe(move |event| {
            if !matches!(event, ViewSessionEvent::Closed { .. }) {
                callback_events.set(callback_events.get() + 1);
            }
        })
        .expect("feedback side-effect subscription");
        Self {
            runtime,
            host: EguiNativeFrameHost::new(view),
            node,
            port,
            prompt: String::new(),
            text_id: None,
            focus_acquired: false,
            closed: false,
            frame_results: Vec::new(),
            graph_revision_before,
            history_before,
            feedback_side_effect_events,
        }
    }
}

fn render_close_during_render(ui: &mut Ui, state: &mut CloseDuringRenderState) {
    let CloseDuringRenderState {
        host,
        node,
        port,
        prompt,
        text_id,
        focus_acquired,
        closed,
        frame_results,
        ..
    } = state;
    let result = host.show(ui, frame_request(), |ui, native| {
        assert_eq!(native.frame_node().id(), *node);
        let root = ui.group(|ui| {
            let label = ui.label("Closing prompt");
            let id = native.tracked_state_id::<egui::text_edit::TextEditState>("prompt");
            let text = ui
                .add(TextEdit::singleline(prompt).id(id))
                .labelled_by(label.id);
            text.request_focus();
            *text_id = Some(text.id);
            *focus_acquired = ui.memory(|memory| memory.has_focus(text.id));

            let (_rect, port_response) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::drag());
            native
                .measure_port(*port, &port_response, HandlePosition::Right)
                .expect("real frame port is measurable");
            native
                .command(GraphCommand::set_node_data(
                    native.graph_version(),
                    *node,
                    serde_json::json!({ "should_not_apply": true }),
                ))
                .expect("command is collected before close");

            let outcome = host
                .view()
                .close()
                .expect("exposed view closes during render");
            assert_eq!(outcome.reason(), ViewCloseReason::HostClosed);
            *closed = true;
        });
        root.response
    });
    frame_results.push(result);
}

#[test]
fn close_during_render_reports_closed_submission_without_feedback_side_effects() {
    let harness = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(render_close_during_render, CloseDuringRenderState::new());
    let state = harness.state();
    let text_id = state.text_id.expect("text state id");
    let frame_result = state.frame_results.first().expect("closed frame result");
    assert!(
        matches!(
            frame_result,
            Err(EguiNativeFrameError::Submission(
                FeedbackSubmissionError::Session(ViewSessionError::Closed {
                    reason: ViewCloseReason::HostClosed
                })
            ))
        ),
        "unexpected close-during-render result: {frame_result:?}"
    );
    assert!(state.closed);
    assert!(state.focus_acquired);
    assert_eq!(
        state.host.view().close_reason(),
        Some(ViewCloseReason::HostClosed)
    );
    assert_eq!(state.feedback_side_effect_events.get(), 0);

    let graph = state
        .runtime
        .snapshot()
        .expect("graph after rejected feedback");
    assert_eq!(graph.revision(), state.graph_revision_before);
    assert_eq!(
        graph
            .graph()
            .node(&state.node)
            .expect("unchanged node")
            .data,
        serde_json::Value::Null
    );
    assert_eq!(
        state.runtime.history_state().expect("history after close"),
        state.history_before
    );
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, text_id).is_none());
    assert!(!harness.ctx.memory(|memory| memory.has_focus(text_id)));
}

struct DynamicPortState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    node: NodeId,
    base_port: PortId,
    dynamic_port: PortId,
    prompt: String,
    text_id: Option<Id>,
    root_id: Option<Id>,
    command_outcome: Option<EguiNativeFrameOutcome>,
}

impl DynamicPortState {
    fn new() -> Self {
        let node = NodeId::from_u128(2);
        let base_port = PortId::from_u128(21);
        let dynamic_port = PortId::from_u128(22);
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph_with_ports(node, [base_port]))
            .expect("test runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("test view");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            node,
            base_port,
            dynamic_port,
            prompt: String::new(),
            text_id: None,
            root_id: None,
            command_outcome: None,
        }
    }
}

fn render_dynamic_port(ui: &mut Ui, state: &mut DynamicPortState) {
    let DynamicPortState {
        host,
        node,
        base_port,
        dynamic_port,
        prompt,
        text_id,
        root_id,
        ..
    } = state;
    let outcome = host
        .show(ui, frame_request(), |ui, native| {
            root_id.get_or_insert(native.root_id());
            let root = ui.group(|ui| {
                ui.set_min_width(220.0);
                let prompt_label = ui.label("Prompt");
                let prompt_id = native.tracked_state_id::<egui::text_edit::TextEditState>("prompt");
                let text = ui
                    .add(TextEdit::singleline(prompt).id(prompt_id))
                    .labelled_by(prompt_label.id);
                *text_id = Some(text.id);

                if ui.button("Add output").clicked() {
                    let graph = native.frame_node().graph_snapshot().graph();
                    let base = graph.port(base_port).expect("base frame port").clone();
                    let command = GraphCommand::topology(
                        native.graph_version(),
                        TopologyEdit::new().replace_node_ports(
                            *node,
                            [
                                TopologyPort::new(*base_port, base),
                                TopologyPort::new(*dynamic_port, output_port(*node, "dynamic")),
                            ],
                            IncidentEdgePolicy::Reject,
                        ),
                    );
                    native.command(command).expect("command is collected");
                }

                let ports = native
                    .frame_node()
                    .ports()
                    .iter()
                    .map(|port| port.id())
                    .collect::<Vec<_>>();
                for port in ports {
                    let (_rect, response) =
                        ui.allocate_exact_size(Vec2::splat(16.0), Sense::drag());
                    native
                        .measure_port(port, &response, HandlePosition::Right)
                        .expect("real frame port is measurable");
                }
            });
            root.response
        })
        .expect("native frame submits");
    if outcome.receipt().command_completions().next().is_some() {
        state.command_outcome = Some(outcome);
    }
}

#[test]
fn native_text_state_survives_dynamic_real_port_command_before_layout() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(render_dynamic_port, DynamicPortState::new());

    let text = harness.get_by_role_and_label(Role::TextInput, "Prompt");
    text.focus();
    text.type_text("hello");
    harness.run();
    let text_id = harness.state().text_id.expect("native text id");
    let cursor_before = egui::text_edit::TextEditState::load(&harness.ctx, text_id)
        .expect("native text state")
        .cursor
        .char_range();

    harness.get_by_label("Add output").click();
    harness.step();

    let state = harness.state();
    let command_frame = state
        .command_outcome
        .as_ref()
        .expect("command frame outcome");
    assert_eq!(
        command_frame
            .receipt()
            .command_completions()
            .map(|completion| completion.kind())
            .collect::<Vec<_>>(),
        [CommandExecutionKind::Applied]
    );
    assert_eq!(
        command_frame
            .receipt()
            .layout_receipts()
            .map(|receipt| receipt.status())
            .collect::<Vec<_>>(),
        [Some(LayoutNodeStatus::Stale)]
    );
    assert_eq!(state.prompt, "hello");
    assert_eq!(
        state.root_id,
        Some(Id::new((
            "jellyflow-native-node",
            &command_frame.rendered_instances()[0]
        ))),
    );
    assert_eq!(
        state
            .host
            .view()
            .graph_snapshot()
            .expect("current graph")
            .graph()
            .node(&state.node)
            .expect("dynamic node")
            .ports,
        vec![state.base_port, state.dynamic_port],
    );
    let stale_geometry = state
        .host
        .view()
        .geometry_snapshot()
        .expect("stale geometry");
    assert!(stale_geometry.node(state.node).is_none());
    assert!(stale_geometry.port(state.base_port).is_none());
    assert!(stale_geometry.port(state.dynamic_port).is_none());

    harness.step();
    let state = harness.state();
    let geometry = state.host.view().geometry_snapshot().expect("geometry");
    assert_eq!(
        geometry
            .node(state.node)
            .expect("fresh dynamic node geometry")
            .ports()
            .len(),
        2,
    );
    assert_eq!(state.text_id, Some(text_id));
    assert_eq!(
        egui::text_edit::TextEditState::load(&harness.ctx, text_id)
            .expect("preserved native text state")
            .cursor
            .char_range(),
        cursor_before,
    );
}

struct MissingPortState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    node: NodeId,
    measured_port: PortId,
    omitted_port: PortId,
    omit: bool,
    outcome: Option<EguiNativeFrameOutcome>,
}

impl MissingPortState {
    fn new() -> Self {
        let node = NodeId::from_u128(3);
        let first = PortId::from_u128(31);
        let omitted_port = PortId::from_u128(32);
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph_with_ports(node, [first, omitted_port]))
            .expect("test runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("test view");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            node,
            measured_port: first,
            omitted_port,
            omit: false,
            outcome: None,
        }
    }
}

fn render_missing_port(ui: &mut Ui, state: &mut MissingPortState) {
    let omitted_port = state.omitted_port;
    let omit = state.omit;
    let outcome = state
        .host
        .show(ui, frame_request(), |ui, native| {
            let root = ui.group(|ui| {
                ui.label("Measured node");
                let ports = native
                    .frame_node()
                    .ports()
                    .iter()
                    .map(|port| port.id())
                    .collect::<Vec<_>>();
                for port in ports {
                    let (_rect, response) =
                        ui.allocate_exact_size(Vec2::splat(16.0), Sense::drag());
                    if !omit || port != omitted_port {
                        native
                            .measure_port(port, &response, HandlePosition::Right)
                            .expect("real frame port is measurable");
                    }
                }
            });
            root.response
        })
        .expect("native frame submits");
    state.outcome = Some(outcome);
}

#[test]
fn missing_real_port_publishes_unavailable_without_fabricated_geometry() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(render_missing_port, MissingPortState::new());
    let initial = harness
        .state()
        .host
        .view()
        .geometry_snapshot()
        .expect("initial geometry");
    assert_eq!(
        initial
            .node(harness.state().node)
            .expect("initial node geometry")
            .ports()
            .len(),
        2,
    );

    harness.state_mut().omit = true;
    harness.step();

    let state = harness.state();
    let outcome = state.outcome.as_ref().expect("incomplete frame outcome");
    assert_eq!(
        outcome
            .receipt()
            .layout_receipts()
            .next()
            .and_then(|receipt| receipt.status()),
        Some(LayoutNodeStatus::Published),
    );
    let geometry = state.host.view().geometry_snapshot().expect("geometry");
    let node = geometry
        .node(state.node)
        .expect("complete node outcome remains addressable");
    assert!(node.is_fresh());
    assert_eq!(node.ports().len(), 1);
    assert_eq!(node.port_outcomes().len(), 2);
    assert!(geometry.port(state.measured_port).is_some());
    assert!(geometry.port(state.omitted_port).is_none());
    assert!(matches!(
        geometry.port_outcome(state.measured_port),
        Some(PublishedPortOutcome::Rendered(_))
    ));
    assert!(matches!(
        geometry.port_outcome(state.omitted_port),
        Some(PublishedPortOutcome::Unavailable)
    ));
}

struct RetirementState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    node: NodeId,
    port: PortId,
    renderer_incarnation: u64,
    prompt: String,
    text_id: Option<Id>,
    root_id: Option<Id>,
}

impl RetirementState {
    fn new() -> Self {
        let node = NodeId::from_u128(4);
        let port = PortId::from_u128(41);
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph_with_ports(node, [port]))
            .expect("test runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("test view");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            node,
            port,
            renderer_incarnation: 1,
            prompt: String::new(),
            text_id: None,
            root_id: None,
        }
    }
}

fn render_retirement(ui: &mut Ui, state: &mut RetirementState) {
    let RetirementState {
        host,
        renderer_incarnation,
        prompt,
        text_id,
        root_id,
        ..
    } = state;
    host.show(
        ui,
        frame_request_with_renderer(*renderer_incarnation),
        |ui, native| {
            *root_id = Some(native.root_id());
            let root = ui.group(|ui| {
                let label = ui.label("Retained prompt");
                let id = native.tracked_state_id::<egui::text_edit::TextEditState>("prompt");
                let text = ui
                    .add(TextEdit::singleline(prompt).id(id))
                    .labelled_by(label.id);
                *text_id = Some(text.id);
                for port in native.frame_node().ports() {
                    let (_rect, response) =
                        ui.allocate_exact_size(Vec2::splat(16.0), Sense::drag());
                    native
                        .measure_port(port.id(), &response, HandlePosition::Right)
                        .expect("real frame port is measurable");
                }
            });
            root.response
        },
    )
    .expect("retirement frame submits");
}

#[test]
fn hard_retirement_clears_exact_egui_state_across_replacement_readd_and_close() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(480.0, 360.0))
        .build_ui_state(render_retirement, RetirementState::new());
    let text = harness.get_by_role_and_label(Role::TextInput, "Retained prompt");
    text.focus();
    text.type_text("hello");
    harness.run();
    let first_text_id = harness.state().text_id.expect("first text id");
    let first_root_id = harness.state().root_id.expect("first root id");
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, first_text_id).is_some());

    harness.state_mut().renderer_incarnation = 2;
    harness.step();
    let second_text_id = harness.state().text_id.expect("replacement text id");
    let second_root_id = harness.state().root_id.expect("replacement root id");
    assert_ne!(second_root_id, first_root_id);
    assert_ne!(second_text_id, first_text_id);
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, first_text_id).is_none());
    assert!(!harness.ctx.memory(|memory| memory.has_focus(first_text_id)));
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, second_text_id).is_some());

    let revision = harness
        .state()
        .host
        .view()
        .graph_snapshot()
        .expect("graph before delete")
        .version();
    let node = harness.state().node;
    harness
        .state()
        .host
        .view()
        .execute(GraphCommand::topology(
            revision,
            TopologyEdit::new().remove_node(node, IncidentEdgePolicy::Reject),
        ))
        .expect("view accepts delete")
        .into_result()
        .expect("delete executes")
        .applied()
        .expect("delete applies");
    harness.step();
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, second_text_id).is_none());
    assert!(
        harness
            .state()
            .host
            .view()
            .geometry_snapshot()
            .expect("geometry after delete")
            .node(node)
            .is_none()
    );

    let revision = harness
        .state()
        .host
        .view()
        .graph_snapshot()
        .expect("graph before re-add")
        .version();
    let port = harness.state().port;
    harness
        .state()
        .host
        .view()
        .execute(GraphCommand::topology(
            revision,
            TopologyEdit::new().add_node(
                node,
                test_node(vec![port]),
                [TopologyPort::new(port, output_port(node, "re-added"))],
            ),
        ))
        .expect("view accepts re-add")
        .into_result()
        .expect("re-add executes")
        .applied()
        .expect("re-add applies");
    harness.step();
    let third_text_id = harness.state().text_id.expect("re-added text id");
    let third_root_id = harness.state().root_id.expect("re-added root id");
    assert_ne!(third_root_id, second_root_id);
    assert_ne!(third_text_id, second_text_id);
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, third_text_id).is_some());

    harness.state().host.close().expect("host closes");
    assert!(egui::text_edit::TextEditState::load(&harness.ctx, third_text_id).is_none());
}

fn frame_request() -> FrameRequest {
    frame_request_with_renderer(1)
}

fn frame_request_with_renderer(renderer_incarnation: u64) -> FrameRequest {
    FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint {
                x: -100.0,
                y: -100.0,
            },
            size: CanvasSize {
                width: 600.0,
                height: 400.0,
            },
        },
        RendererStamp::new(RENDERER_KEY, renderer_incarnation),
    )
}

fn connection_binding(view: &ViewSession, port: PortId, edge: EdgeId) -> ConnectionGestureBinding {
    let port = view
        .port_instance_key(port)
        .expect("connection port identity query")
        .expect("source port is current");
    let drag = view
        .port_connection_drag_intent(port)
        .expect("source port can begin a connection")
        .into_outcome()
        .ready()
        .expect("source port remains current");
    ConnectionInteractionStart::binding_from_port_drag(drag, edge, NodeGraphConnectionMode::Strict)
        .expect("source port admits a strict connection")
}

fn graph_with_ports(
    node_id: NodeId,
    ports: impl IntoIterator<Item = PortId>,
) -> jellyflow_core::Graph {
    let ports = ports.into_iter().collect::<Vec<_>>();
    let node = test_node(ports.clone());
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x8001));
    builder.insert_node(node_id, node);
    for (index, port) in ports.into_iter().enumerate() {
        builder.insert_port(port, output_port(node_id, &format!("port-{index}")));
    }
    builder.build().expect("valid native egui graph")
}

fn test_node(ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new("test.egui-native"),
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
            width: 240.0,
            height: 160.0,
        }),
        hidden: false,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}

fn output_port(node: NodeId, key: &str) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: PortDirection::Out,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn input_port(node: NodeId, key: &str) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: PortDirection::In,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn screen_rect_to_canvas(rect: Rect, scene_origin: Pos2) -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: rect.min.x - scene_origin.x,
            y: rect.min.y - scene_origin.y,
        },
        size: CanvasSize {
            width: rect.width(),
            height: rect.height(),
        },
    }
}

fn assert_approx(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 1.0 / 64.0,
        "expected {expected}, got {actual}",
    );
}
