use egui::{ComboBox, Response, Sense, TextEdit, Ui, Vec2, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable as _};
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Graph, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
    Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_egui::{EguiNativeFrameHost, EguiNativeFrameOutcome};
use jellyflow_runtime::commands::{
    CommandExecutionKind, GraphCommand, IncidentEdgePolicy, TopologyEdit, TopologyPort,
};
use jellyflow_runtime::geometry::HandlePosition;
use jellyflow_runtime::graph::{GraphRuntime, GraphSnapshot};
use jellyflow_runtime::view::{
    AdapterEnvironment, FrameBoundsSource, FrameRequest, LayoutNodeStatus, RendererStamp,
    ViewSession, ViewSessionOptions,
};

const TRACER_RENDERER_KEY: &str = "test.adapter-conformance.native";

#[derive(Clone, Copy)]
struct TracerIds {
    node: NodeId,
    base_port: PortId,
    dynamic_port: PortId,
}

struct TracerFixture {
    ids: TracerIds,
    graph: Graph,
    opaque_data: serde_json::Value,
}

impl TracerFixture {
    fn new() -> Self {
        let ids = TracerIds {
            node: NodeId::from_u128(0xada1),
            base_port: PortId::from_u128(0xada2),
            dynamic_port: PortId::from_u128(0xada3),
        };
        let opaque_data = serde_json::json!({
            "prompt": "trace me",
            "mode": "balanced",
            "plugin_owned": { "sentinel": [1, 3, 5] }
        });
        let mut builder = GraphBuilder::new(GraphId::from_u128(0xada0));
        builder.insert_node(
            ids.node,
            test_node(
                "test.adapter-conformance.tracer",
                vec![ids.base_port],
                opaque_data.clone(),
            ),
        );
        builder.insert_port(
            ids.base_port,
            test_port(
                ids.node,
                "input",
                PortDirection::In,
                serde_json::json!({ "slot": "base" }),
            ),
        );
        Self {
            ids,
            graph: builder.build().expect("valid egui tracer graph"),
            opaque_data,
        }
    }

    fn frame_request(&self) -> FrameRequest {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 640.0,
                    height: 480.0,
                },
            },
            RendererStamp::new(TRACER_RENDERER_KEY, 1),
        )
        .with_fallback_size(CanvasSize {
            width: 260.0,
            height: 180.0,
        })
    }

    fn add_dynamic_port_command(&self, snapshot: &GraphSnapshot) -> Option<GraphCommand> {
        let base = snapshot.graph().port(&self.ids.base_port)?.clone();
        Some(GraphCommand::topology(
            snapshot.version(),
            TopologyEdit::new().replace_node_ports(
                self.ids.node,
                [
                    TopologyPort::new(self.ids.base_port, base),
                    TopologyPort::new(
                        self.ids.dynamic_port,
                        test_port(
                            self.ids.node,
                            "dynamic",
                            PortDirection::Out,
                            serde_json::json!({ "slot": "dynamic" }),
                        ),
                    ),
                ],
                IncidentEdgePolicy::Reject,
            ),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NormalizedOperation {
    CommandApplied,
    Layout(LayoutNodeStatus),
}

struct VerticalTracerState {
    _runtime: GraphRuntime,
    fixture: TracerFixture,
    host: EguiNativeFrameHost,
    untouched_view: ViewSession,
    prompt: String,
    selected_mode: usize,
    first_bounds_source: Option<FrameBoundsSource>,
    topology_outcome: Option<EguiNativeFrameOutcome>,
    undo_outcome: Option<EguiNativeFrameOutcome>,
}

impl VerticalTracerState {
    fn new() -> Self {
        let fixture = TracerFixture::new();
        let runtime = GraphRuntime::builder(Default::default())
            .build(fixture.graph.clone())
            .expect("tracer runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("tracer view");
        let untouched_view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("isolated tracer view");
        Self {
            _runtime: runtime,
            fixture,
            host: EguiNativeFrameHost::new(view),
            untouched_view,
            prompt: String::new(),
            selected_mode: 0,
            first_bounds_source: None,
            topology_outcome: None,
            undo_outcome: None,
        }
    }
}

fn render_vertical_tracer(ui: &mut Ui, state: &mut VerticalTracerState) {
    let VerticalTracerState {
        fixture,
        host,
        prompt,
        selected_mode,
        first_bounds_source,
        ..
    } = state;
    let ids = fixture.ids;
    let outcome = host
        .show_with_fallback(ui, fixture.frame_request(), |ui, native| {
            if native.frame_node().renderer().key() != TRACER_RENDERER_KEY {
                return None;
            }
            first_bounds_source.get_or_insert(native.frame_node().bounds_source());
            let root = ui.group(|ui| {
                ui.set_min_width(230.0);
                let prompt_label = ui.label("Prompt");
                let prompt_id = native.tracked_state_id::<egui::text_edit::TextEditState>("prompt");
                ui.add(TextEdit::singleline(prompt).id(prompt_id))
                    .labelled_by(prompt_label.id);

                ComboBox::new(native.id("mode"), "Mode").show_index(
                    ui,
                    selected_mode,
                    2,
                    |index| ["Balanced", "Fast"][index],
                );

                let has_dynamic = native
                    .frame_node()
                    .ports()
                    .iter()
                    .any(|port| port.id() == ids.dynamic_port);
                if has_dynamic {
                    if ui.button("Undo output").clicked() {
                        native
                            .command(GraphCommand::undo(native.graph_version()))
                            .expect("undo command is collected");
                    }
                } else if ui.button("Add output").clicked() {
                    let snapshot = native.frame_node().graph_snapshot();
                    let command = fixture
                        .add_dynamic_port_command(snapshot)
                        .expect("fixture base port exists");
                    native
                        .command(command)
                        .expect("topology command is collected");
                }

                for port in native.frame_node().ports() {
                    let direction = port.port().expect("real frame port").dir;
                    let (_, response) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::drag());
                    let outward = match direction {
                        PortDirection::In => HandlePosition::Left,
                        PortDirection::Out => HandlePosition::Right,
                    };
                    native
                        .measure_port(port.id(), &response, outward)
                        .expect("real port is measured");
                }
            });
            Some(root.response)
        })
        .expect("tracer frame submits");

    if normalized_operations(&outcome)
        .iter()
        .any(|operation| matches!(operation, NormalizedOperation::CommandApplied))
    {
        let dynamic_exists = host
            .view()
            .graph_snapshot()
            .expect("current graph")
            .graph()
            .port(&ids.dynamic_port)
            .is_some();
        if dynamic_exists {
            state.topology_outcome = Some(outcome);
        } else {
            state.undo_outcome = Some(outcome);
        }
    }
}

#[test]
fn native_vertical_tracer_normalizes_protocol_outcomes_without_widget_abstraction() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(720.0, 520.0))
        .build_ui_state(render_vertical_tracer, VerticalTracerState::new());

    let ids = harness.state().fixture.ids;
    let initial_ports = published_ports(&harness.state().host, ids.node);
    assert!(
        harness
            .state()
            .untouched_view
            .geometry_snapshot()
            .expect("other view geometry")
            .nodes()
            .is_empty(),
    );

    let prompt = harness.get_by_role_and_label(Role::TextInput, "Prompt");
    prompt.focus();
    prompt.type_text("native text");
    harness.run();
    let mode = harness.get_by_role_and_label(Role::ComboBox, "Mode");
    mode.click();
    harness.run();
    harness.get_by_role_and_label(Role::Button, "Fast").click();
    harness.run();

    harness.get_by_label("Add output").click();
    harness.step();
    let topology_operations = normalized_operations(
        harness
            .state()
            .topology_outcome
            .as_ref()
            .expect("topology feedback outcome"),
    );
    let topology_withdrew_geometry = geometry_is_withdrawn(&harness.state().host, ids.node);
    assert_eq!(harness.state().prompt, "native text");
    assert_eq!(harness.state().selected_mode, 1);

    harness.step();
    let dynamic_ports = published_ports(&harness.state().host, ids.node);

    harness.get_by_label("Undo output").click();
    harness.step();
    let undo_operations = normalized_operations(
        harness
            .state()
            .undo_outcome
            .as_ref()
            .expect("undo feedback outcome"),
    );
    let undo_withdrew_geometry = geometry_is_withdrawn(&harness.state().host, ids.node);

    harness.step();
    let restored_ports = published_ports(&harness.state().host, ids.node);
    assert_eq!(harness.state().prompt, "native text");
    assert_eq!(harness.state().selected_mode, 1);

    assert!(
        harness
            .state()
            .host
            .view()
            .set_adapter_environment(AdapterEnvironment::new(2, 3, 5, 7))
            .expect("environment advances"),
    );
    let environment_withdrew_geometry = geometry_is_withdrawn(&harness.state().host, ids.node);
    harness.step();
    let environment_ports = published_ports(&harness.state().host, ids.node);

    let snapshot = harness
        .state()
        .host
        .view()
        .graph_snapshot()
        .expect("final graph");
    let opaque_data_preserved = snapshot
        .graph()
        .node(&ids.node)
        .is_some_and(|node| node.data == harness.state().fixture.opaque_data);
    let other_view_isolated = harness
        .state()
        .untouched_view
        .geometry_snapshot()
        .expect("other view geometry")
        .nodes()
        .is_empty();

    let ordered_stale = [
        NormalizedOperation::CommandApplied,
        NormalizedOperation::Layout(LayoutNodeStatus::Stale),
    ];
    assert_eq!(
        harness.state().first_bounds_source,
        Some(FrameBoundsSource::ProtocolFallback),
        "the first frame must use provisional protocol bounds",
    );
    assert_eq!(initial_ports, [ids.base_port]);
    assert_eq!(topology_operations, ordered_stale);
    assert!(topology_withdrew_geometry);
    assert_eq!(dynamic_ports, [ids.base_port, ids.dynamic_port]);
    assert_eq!(undo_operations, ordered_stale);
    assert!(undo_withdrew_geometry);
    assert_eq!(restored_ports, [ids.base_port]);
    assert!(environment_withdrew_geometry);
    assert_eq!(environment_ports, [ids.base_port]);
    assert!(opaque_data_preserved);
    assert!(other_view_isolated);
}

fn normalized_operations(outcome: &EguiNativeFrameOutcome) -> Vec<NormalizedOperation> {
    let mut operations = outcome
        .receipt()
        .command_completions()
        .filter_map(|completion| {
            (completion.kind() == CommandExecutionKind::Applied)
                .then_some(NormalizedOperation::CommandApplied)
        })
        .collect::<Vec<_>>();
    operations.extend(
        outcome
            .receipt()
            .layout_receipts()
            .filter_map(|receipt| receipt.status().map(NormalizedOperation::Layout)),
    );
    operations
}

fn published_ports(host: &EguiNativeFrameHost, node: NodeId) -> Vec<PortId> {
    host.view()
        .geometry_snapshot()
        .expect("geometry snapshot")
        .node(node)
        .expect("published node geometry")
        .ports()
        .keys()
        .copied()
        .collect()
}

fn geometry_is_withdrawn(host: &EguiNativeFrameHost, node: NodeId) -> bool {
    host.view()
        .geometry_snapshot()
        .expect("geometry snapshot")
        .node(node)
        .is_none_or(|geometry| !geometry.is_fresh() && geometry.ports().is_empty())
}

struct MissingRendererState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    node: NodeId,
    ports: Vec<PortId>,
    opaque_data: serde_json::Value,
    observed_source: Option<FrameBoundsSource>,
    outcome: Option<EguiNativeFrameOutcome>,
}

impl MissingRendererState {
    fn new() -> Self {
        let node = NodeId::from_u128(0xf001);
        let ports = vec![
            PortId::from_u128(0xf011),
            PortId::from_u128(0xf012),
            PortId::from_u128(0xf013),
        ];
        let opaque_data = serde_json::json!({
            "future_plugin": { "version": 99, "bytes": [8, 13, 21] }
        });
        let mut builder = GraphBuilder::new(GraphId::from_u128(0xf000));
        builder.insert_node(
            node,
            test_node("plugin.future.unknown", ports.clone(), opaque_data.clone()),
        );
        builder.insert_port(
            ports[0],
            test_port(node, "input", PortDirection::In, serde_json::json!("a")),
        );
        builder.insert_port(
            ports[1],
            test_port(
                node,
                "output-a",
                PortDirection::Out,
                serde_json::json!({ "opaque": 1 }),
            ),
        );
        builder.insert_port(
            ports[2],
            test_port(
                node,
                "output-b",
                PortDirection::Out,
                serde_json::json!(["opaque", 2]),
            ),
        );
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().expect("unknown-kind graph"))
            .expect("unknown-kind runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("unknown-kind view");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            node,
            ports,
            opaque_data,
            observed_source: None,
            outcome: None,
        }
    }
}

fn render_missing_renderer(ui: &mut Ui, state: &mut MissingRendererState) {
    let source = &mut state.observed_source;
    state.outcome = Some(
        state
            .host
            .show_with_fallback(ui, missing_renderer_request(), |_ui, native| {
                source.get_or_insert(native.frame_node().bounds_source());
                None::<Response>
            })
            .expect("diagnostic fallback submits complete geometry"),
    );
}

#[test]
fn unknown_kind_fallback_is_diagnostic_and_measures_every_opaque_real_port() {
    let harness = Harness::builder()
        .with_size(Vec2::new(640.0, 480.0))
        .build_ui_state(render_missing_renderer, MissingRendererState::new());
    let state = harness.state();
    let outcome = state.outcome.as_ref().expect("fallback outcome");
    let diagnostic = outcome
        .diagnostics()
        .first()
        .expect("missing renderer diagnostic");

    assert_eq!(outcome.diagnostics().len(), 1);
    assert_eq!(diagnostic.node(), state.node);
    assert_eq!(
        diagnostic.node_kind(),
        Some(&NodeKindKey::new("plugin.future.unknown")),
    );
    assert_eq!(diagnostic.renderer_key(), "plugin.missing.renderer");
    assert!(diagnostic.message().contains("plugin.future.unknown"));
    assert_eq!(
        state.observed_source,
        Some(FrameBoundsSource::ProtocolFallback)
    );
    assert!(matches!(
        outcome
            .receipt()
            .layout_receipts()
            .next()
            .and_then(|receipt| receipt.status()),
        Some(LayoutNodeStatus::Published | LayoutNodeStatus::Unchanged),
    ));

    let geometry = state
        .host
        .view()
        .geometry_snapshot()
        .expect("fallback geometry");
    let node_geometry = geometry.node(state.node).expect("fallback node geometry");
    assert!(node_geometry.is_fresh());
    assert_eq!(node_geometry.ports().len(), state.ports.len());
    for port in &state.ports {
        assert!(node_geometry.ports().contains_key(port));
    }

    let snapshot = state.host.view().graph_snapshot().expect("opaque graph");
    let node = snapshot.graph().node(&state.node).expect("unknown node");
    assert_eq!(node.data, state.opaque_data);
    assert_eq!(node.ports, state.ports);
    assert_eq!(
        snapshot
            .graph()
            .port(&state.ports[2])
            .expect("opaque output")
            .data,
        serde_json::json!(["opaque", 2]),
    );
    harness.get_by_label("Renderer unavailable");
}

fn missing_renderer_request() -> FrameRequest {
    FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 640.0,
                height: 480.0,
            },
        },
        RendererStamp::new("plugin.missing.renderer", 1),
    )
    .with_fallback_size(CanvasSize {
        width: 260.0,
        height: 180.0,
    })
}

fn test_node(kind: &str, ports: Vec<PortId>, data: serde_json::Value) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 99,
        pos: CanvasPoint { x: 20.0, y: 20.0 },
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports,
        data,
    }
}

fn test_port(node: NodeId, key: &str, dir: PortDirection, data: serde_json::Value) -> Port {
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
        data,
    }
}
