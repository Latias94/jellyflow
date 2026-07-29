use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Graph, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
    Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::authoring::{
    MutationLayoutEffect, MutationLayoutSettlement, MutationRequest, MutationSettlement,
    OpenGpuiManagedMutation, OpenGpuiNodeAuthoring, OpenGpuiNodeBinding, OpenGpuiPortSide,
};
use jellyflow_open_gpui::host::OpenGpuiSceneHost;
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    commands::GraphCommand, commands::IncidentEdgePolicy, commands::TopologyEdit,
    commands::TopologyPort, graph::GraphRuntime, view::AdapterEnvironment,
    view::PublishedPortOutcome, view::RendererStamp, view::ViewSessionEvent,
    view::ViewSessionOptions, view::WeakViewSession,
};
use open_gpui::{
    AppContext, Context, Entity, InteractiveElement, ParentElement, Render, Styled, Window, div, px,
};
use open_gpui_ui_components::{
    Button, ListboxOption, Select, TextInput,
    gpui_adapter::{TextInputController, init_text_input},
};

const TRACER_KIND: &str = "test.adapter-conformance.tracer";
const TRACER_RENDERER: &str = "test.adapter-conformance.native";
const INPUT_SELECTOR: &str = "text-input:adapter-tracer-prompt:root";
const MODE_SELECTOR: &str = "select:adapter-tracer-mode:trigger";
const FAST_MODE_OPTION_SELECTOR: &str = "listbox:adapter-tracer-mode-listbox:option:fast";
const COMMAND_SELECTOR: &str = "adapter-tracer-command-control";

#[derive(Debug, Clone, Copy)]
struct TracerIds {
    graph: GraphId,
    node: NodeId,
    base_port: PortId,
    dynamic_port: PortId,
}

impl TracerIds {
    fn stable() -> Self {
        Self {
            graph: GraphId::from_u128(0xada0),
            node: NodeId::from_u128(0xada1),
            base_port: PortId::from_u128(0xada2),
            dynamic_port: PortId::from_u128(0xada3),
        }
    }
}

struct TracerFixture {
    ids: TracerIds,
    graph: Graph,
    opaque_data: serde_json::Value,
}

impl TracerFixture {
    fn build() -> Self {
        let ids = TracerIds::stable();
        let opaque_data = serde_json::json!({
            "prompt": "trace me",
            "mode": "balanced",
            "plugin_owned": { "sentinel": [1, 3, 5] }
        });
        let node = Node {
            kind: NodeKindKey::new(TRACER_KIND),
            kind_version: 1,
            pos: CanvasPoint { x: 24.0, y: 20.0 },
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
            ports: vec![ids.base_port],
            data: opaque_data.clone(),
        };
        let mut builder = GraphBuilder::new(ids.graph);
        builder.insert_node(ids.node, node);
        builder.insert_port(ids.base_port, base_port(ids.node));
        let graph = builder.build().expect("valid local tracer graph");
        Self {
            ids,
            graph,
            opaque_data,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeFrameOperation {
    CommandApplied,
    LayoutStale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTopologyIntent {
    AddDynamicPort,
    RemoveDynamicPort,
}

struct NativeTracerNode {
    binding: Entity<OpenGpuiNodeBinding>,
    topology_mutation: OpenGpuiManagedMutation<NativeTopologyIntent>,
    prompt: Entity<TextInputController>,
    ids: TracerIds,
    fast_mode: bool,
    observations: Rc<RefCell<VecDeque<MutationSettlement>>>,
}

impl NativeTracerNode {
    fn dispatch_next_topology(&mut self, cx: &mut Context<Self>) {
        let authoring = {
            self.binding
                .read(cx)
                .authoring()
                .expect("command binding stays live")
        };
        let intent = next_topology_intent(&authoring, self.ids);
        let command = next_topology_command(&authoring, self.ids);
        let settlement = self
            .topology_mutation
            .dispatch(
                authoring,
                command,
                MutationRequest::new(intent),
                MutationLayoutEffect::InvalidateWhenSatisfied,
                cx,
            )
            .expect("native topology mutation enters the host journal");
        assert!(
            matches!(
                settlement,
                MutationSettlement::Pending {
                    awaiting_authority: false
                }
            ),
            "a newly admitted topology mutation must await its authoritative result",
        );
    }
}

impl Render for NativeTracerNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let authoring = self
            .binding
            .read(cx)
            .authoring()
            .expect("retained tracer binding stays live");
        let ids = self.ids;
        let settlement = self.topology_mutation.observe(
            &authoring,
            |intent| topology_intent_is_satisfied(&authoring, ids, *intent),
            cx,
        );
        if matches!(
            &settlement,
            MutationSettlement::Satisfied { .. }
                | MutationSettlement::Retryable { .. }
                | MutationSettlement::Uncertain { .. }
                | MutationSettlement::Retired { .. }
                | MutationSettlement::Closed { .. }
        ) {
            self.observations.borrow_mut().push_back(settlement);
        }
        let has_dynamic_port = authoring.port(self.ids.dynamic_port).is_some();
        let expected_port_order = authoring.node().ports.clone();
        let ports = authoring
            .ports()
            .map(|port| {
                assert_eq!(port.facts().node, self.ids.node);
                (port.id(), port.handle().clone())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ports.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            expected_port_order,
            "authoring pairs graph facts and native handles in node port order",
        );

        let mode_node = cx.weak_entity();
        let mode = Select::new("adapter-tracer-mode", "Mode")
            .option(ListboxOption::new("balanced", "Balanced"))
            .option(ListboxOption::new("fast", "Fast"))
            .selected(if self.fast_mode { "fast" } else { "balanced" })
            .on_select(move |selection, _, cx| {
                let _ = mode_node.update(cx, |node, cx| {
                    node.fast_mode = selection.value() == "fast";
                    cx.notify();
                });
            });

        let command_node = cx.weak_entity();
        let command = Button::new(
            "adapter-tracer-command",
            if has_dynamic_port {
                "Undo output"
            } else {
                "Add output"
            },
        )
        .on_click(move |_, _, cx| {
            let _ = command_node.update(cx, |node, cx| node.dispatch_next_topology(cx));
        });

        let mut root = div()
            .relative()
            .size_full()
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                TextInput::new("adapter-tracer-prompt", "Prompt").controller(self.prompt.clone()),
            )
            .child(div().self_start().flex_none().child(mode))
            .child(
                div()
                    .debug_selector(|| COMMAND_SELECTOR.to_owned())
                    .self_start()
                    .flex_none()
                    .child(command),
            );

        for (index, (_, handle)) in ports.into_iter().enumerate() {
            let top = 44.0 + index as f32 * 30.0;
            let marker = handle.measure(div().w(px(14.0)).h(px(14.0)));
            root = if handle.side() == OpenGpuiPortSide::Left {
                root.child(div().absolute().left(px(0.0)).top(px(top)).child(marker))
            } else {
                root.child(div().absolute().right(px(0.0)).top(px(top)).child(marker))
            };
        }
        root
    }
}

#[open_gpui::test]
fn native_vertical_tracer_runs_commands_and_layout_through_one_real_host_frame(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let fixture = TracerFixture::build();
    let ids = fixture.ids;
    let runtime = Rc::new(
        GraphRuntime::builder(catalog())
            .build(fixture.graph.clone())
            .expect("tracer runtime"),
    );
    let host_view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("host tracer view");
    let host_view_handle = host_view.downgrade();
    let untouched_view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("isolated tracer view");

    let observations = Rc::new(RefCell::new(VecDeque::new()));
    let withdrawals = Rc::new(RefCell::new(VecDeque::new()));
    let callback_withdrawals = Rc::clone(&withdrawals);
    let callback_view = host_view_handle.clone();
    let _geometry_subscription = host_view
        .subscribe(move |event| {
            if matches!(event, ViewSessionEvent::RuntimePatched { .. }) {
                callback_withdrawals
                    .borrow_mut()
                    .push_back(geometry_is_withdrawn(&callback_view, ids.node));
            }
        })
        .expect("observe host geometry invalidation");
    let captured_node = Rc::new(RefCell::new(None));
    let captured_prompt = Rc::new(RefCell::new(None));
    let factory_observations = Rc::clone(&observations);
    let factory_node = Rc::clone(&captured_node);
    let factory_prompt = Rc::clone(&captured_prompt);
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(OpenGpuiNodeRendererDescriptor::new(
            TRACER_KIND,
            1,
            RendererStamp::new(TRACER_RENDERER, 1),
            OpenGpuiNodeAdapterPresentation::new("Adapter tracer"),
            move |binding, _window, cx| {
                let prompt = cx.new(|cx| TextInputController::with_value(String::new(), cx));
                let topology_mutation = binding.read(cx).managed_mutation::<NativeTopologyIntent>();
                let node = cx.new(|_| NativeTracerNode {
                    binding,
                    topology_mutation,
                    prompt: prompt.clone(),
                    ids,
                    fast_mode: false,
                    observations: Rc::clone(&factory_observations),
                });
                *factory_node.borrow_mut() = Some(node.clone());
                *factory_prompt.borrow_mut() = Some(prompt);
                Ok(node)
            },
        ))
        .expect("register adapter tracer renderer");
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            host_view,
            registry.seal(&catalog()).expect("valid test registry"),
            viewport(),
        )
    });
    draw(cx);

    let node = captured_node
        .borrow_mut()
        .take()
        .expect("retained native tracer node");
    let prompt = captured_prompt
        .borrow_mut()
        .take()
        .expect("retained text input controller");
    assert_eq!(
        fresh_published_ports(&host_view_handle, ids.node),
        vec![ids.base_port]
    );
    assert_other_view_isolated(&untouched_view);

    let transform_before_controls = host_view_handle
        .viewport_transform()
        .expect("host transform before native controls");
    let selection_before_controls = host_view_handle
        .selection()
        .expect("host selection before native controls");
    let input = cx.debug_bounds(INPUT_SELECTOR).expect("native text input");
    cx.simulate_click(input.center(), Default::default());
    cx.simulate_input("native text");
    let mode = cx.debug_bounds(MODE_SELECTOR).expect("native mode select");
    cx.simulate_click(mode.center(), Default::default());
    draw(cx);
    let fast_mode = cx
        .debug_bounds(FAST_MODE_OPTION_SELECTOR)
        .expect("native fast-mode option");
    cx.simulate_click(fast_mode.center(), Default::default());
    draw(cx);
    assert_native_state(cx, &node, &prompt);
    assert_eq!(
        host_view_handle
            .viewport_transform()
            .expect("host transform after native controls"),
        transform_before_controls,
        "native text/select input must not begin a canvas gesture",
    );
    assert_eq!(
        host_view_handle
            .selection()
            .expect("host selection after native controls"),
        selection_before_controls,
        "native text/select input must not begin a node gesture",
    );

    let topology_revision = runtime
        .snapshot()
        .expect("graph before topology")
        .revision();
    click_command(cx);
    assert_eq!(
        take_command_observation(&observations, &withdrawals),
        [
            NativeFrameOperation::CommandApplied,
            NativeFrameOperation::LayoutStale,
        ],
    );
    assert_eq!(
        runtime.snapshot().expect("topology graph").revision(),
        topology_revision + 1,
    );
    draw(cx);
    assert_eq!(
        fresh_published_ports(&host_view_handle, ids.node),
        vec![ids.base_port, ids.dynamic_port]
    );
    assert_native_state(cx, &node, &prompt);

    let undo_revision = runtime.snapshot().expect("graph before undo").revision();
    click_command(cx);
    assert_eq!(
        take_command_observation(&observations, &withdrawals),
        [
            NativeFrameOperation::CommandApplied,
            NativeFrameOperation::LayoutStale,
        ],
    );
    assert_eq!(
        runtime.snapshot().expect("undone graph").revision(),
        undo_revision + 1,
    );
    draw(cx);
    assert_eq!(
        fresh_published_ports(&host_view_handle, ids.node),
        vec![ids.base_port]
    );
    assert_native_state(cx, &node, &prompt);

    assert_eq!(
        host_view_handle
            .viewport_transform()
            .expect("host transform after command controls"),
        transform_before_controls,
        "native command controls must not begin a canvas gesture",
    );
    assert_eq!(
        host_view_handle
            .selection()
            .expect("host selection after command controls"),
        selection_before_controls,
        "native command controls must not begin a node gesture",
    );

    let layout_revision_before_environment = host_view_handle
        .geometry_snapshot()
        .expect("geometry before environment invalidation")
        .layout_revision();
    let environment_withdrew_geometry = cx.update_entity(&host, |host, cx| {
        assert!(
            host.set_adapter_environment(AdapterEnvironment::new(2, 3, 5, 7), cx)
                .expect("environment advances")
        );
        geometry_is_withdrawn(&host_view_handle, ids.node)
    });
    assert!(environment_withdrew_geometry);
    draw(cx);
    assert_eq!(
        fresh_published_ports(&host_view_handle, ids.node),
        vec![ids.base_port]
    );
    assert!(
        host_view_handle
            .geometry_snapshot()
            .expect("geometry after environment convergence")
            .layout_revision()
            > layout_revision_before_environment
    );

    let snapshot = runtime.snapshot().expect("final graph");
    assert_eq!(
        snapshot.graph().node(&ids.node).expect("tracer node").data,
        fixture.opaque_data,
        "native controls and topology edits must preserve opaque node data",
    );
    assert_other_view_isolated(&untouched_view);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert!(host.last_diagnostic().is_none());
    });
}

fn next_topology_command(authoring: &OpenGpuiNodeAuthoring, ids: TracerIds) -> GraphCommand {
    if authoring.port(ids.dynamic_port).is_some() {
        return GraphCommand::undo(authoring.version());
    }
    let base = authoring
        .port(ids.base_port)
        .expect("tracer base port exists")
        .facts()
        .clone();
    GraphCommand::topology(
        authoring.version(),
        TopologyEdit::new().replace_node_ports(
            ids.node,
            [
                TopologyPort::new(ids.base_port, base),
                TopologyPort::new(ids.dynamic_port, dynamic_port(ids.node)),
            ],
            IncidentEdgePolicy::Reject,
        ),
    )
}

fn next_topology_intent(authoring: &OpenGpuiNodeAuthoring, ids: TracerIds) -> NativeTopologyIntent {
    if authoring.port(ids.dynamic_port).is_some() {
        NativeTopologyIntent::RemoveDynamicPort
    } else {
        NativeTopologyIntent::AddDynamicPort
    }
}

fn topology_intent_is_satisfied(
    authoring: &OpenGpuiNodeAuthoring,
    ids: TracerIds,
    intent: NativeTopologyIntent,
) -> bool {
    let has_dynamic_port = authoring.port(ids.dynamic_port).is_some();
    match intent {
        NativeTopologyIntent::AddDynamicPort => has_dynamic_port,
        NativeTopologyIntent::RemoveDynamicPort => !has_dynamic_port,
    }
}

fn take_command_observation(
    observations: &Rc<RefCell<VecDeque<MutationSettlement>>>,
    withdrawals: &Rc<RefCell<VecDeque<bool>>>,
) -> [NativeFrameOperation; 2] {
    let mut observations = observations.borrow_mut();
    let settlement = observations
        .pop_front()
        .expect("managed mutation records one authoritative settlement");
    assert!(
        observations.is_empty(),
        "one click must produce exactly one terminal mutation observation"
    );
    assert_eq!(
        settlement,
        MutationSettlement::Satisfied {
            layout: MutationLayoutSettlement::Invalidated,
        },
        "the managed mutation must be satisfied by authoritative graph facts and invalidate layout",
    );
    let mut withdrawals = withdrawals.borrow_mut();
    let geometry_withdrawn = withdrawals
        .pop_front()
        .expect("runtime patch records geometry invalidation");
    assert!(
        withdrawals.is_empty(),
        "one command must produce exactly one runtime patch"
    );
    assert!(
        geometry_withdrawn,
        "the same frame's old layout must be stale after its command prefix",
    );
    [
        NativeFrameOperation::CommandApplied,
        NativeFrameOperation::LayoutStale,
    ]
}

fn click_command(cx: &mut open_gpui::VisualTestContext) {
    let command = cx
        .debug_bounds(COMMAND_SELECTOR)
        .expect("native topology command button");
    cx.simulate_click(command.center(), Default::default());
    cx.run_until_parked();
}

fn assert_native_state(
    cx: &open_gpui::VisualTestContext,
    node: &Entity<NativeTracerNode>,
    prompt: &Entity<TextInputController>,
) {
    cx.read_entity(prompt, |prompt, _| {
        assert_eq!(prompt.value(), "native text")
    });
    cx.read_entity(node, |node, _| assert!(node.fast_mode));
}

fn fresh_published_ports(view: &WeakViewSession, node: NodeId) -> Vec<PortId> {
    let snapshot = view.geometry_snapshot().expect("geometry snapshot");
    let geometry = snapshot.node(node).expect("published node geometry");
    assert!(geometry.is_fresh(), "node geometry must have converged");
    let ports = geometry.ports().keys().copied().collect::<Vec<_>>();
    for port in &ports {
        assert!(matches!(
            geometry.port_outcomes().get(port),
            Some(PublishedPortOutcome::Rendered(_)),
        ));
    }
    ports
}

fn geometry_is_withdrawn(view: &WeakViewSession, node: NodeId) -> bool {
    view.geometry_snapshot()
        .expect("geometry snapshot")
        .node(node)
        .is_none_or(|geometry| !geometry.is_fresh() && geometry.ports().is_empty())
}

fn assert_other_view_isolated(view: &jellyflow_runtime::view::ViewSession) {
    assert!(
        view.geometry_snapshot()
            .expect("other view geometry")
            .nodes()
            .is_empty(),
        "an unrendered view must not receive host geometry",
    );
}

fn base_port(node: NodeId) -> Port {
    Port {
        node,
        key: PortKey::new("input"),
        dir: PortDirection::In,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::json!({ "slot": "base" }),
    }
}

fn dynamic_port(node: NodeId) -> Port {
    Port {
        node,
        key: PortKey::new("dynamic"),
        dir: PortDirection::Out,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::json!({ "slot": "dynamic" }),
    }
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(NodeKindSpec::builder(TRACER_KIND).build())
        .expect("tracer catalog entry");
    catalog
}

fn viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 640.0,
            height: 480.0,
        },
    }
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    draw_one_frame(cx);
    cx.run_until_parked();
}

fn draw_one_frame(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
}
