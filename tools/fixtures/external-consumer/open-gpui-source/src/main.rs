use std::cell::Cell;
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, EdgeViewDescriptor, GraphBuilder,
    GraphId, NodeId, NodeKindKey, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec, NodePortSpec};
use jellyflow_runtime::{
    GraphCommand, GraphRuntime, IncidentEdgePolicy, PublishedPortOutcome, TopologyEdit,
    TopologyPort, ViewSessionOptions,
};
use jellyflow_open_gpui::activity::OpenGpuiActivityKind;
use jellyflow_open_gpui::authoring::{OpenGpuiNodeBinding, OpenGpuiPortHandle};
use jellyflow_open_gpui::chrome::{
    OpenGpuiEdgeLabelAppearance, OpenGpuiEdgeMarkerShape, OpenGpuiEdgePainterRecipe,
    OpenGpuiEdgePainterRegistry, OpenGpuiEdgeStrokePattern, OpenGpuiEdgeTone,
};
use jellyflow_open_gpui::{
    OpenGpuiContentHeightPolicy, OpenGpuiGraphTheme, OpenGpuiNodeRegistrationPolicy,
    OpenGpuiNodeRegistry, OpenGpuiNodeSizePolicy, OpenGpuiRendererRegistrationStatus,
    OpenGpuiSceneHost,
};
use open_gpui::{
    AnyElement, AppContext, Context, Entity, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, Styled, Task, Window, div, px, rgb,
};

const KIND: &str = "external.dynamic-workflow";
const RENDERER: &str = "external.native.dynamic-workflow";
const SINK_KIND: &str = "external.sink";
const SINK_RENDERER: &str = "external.native.missing-sink";
const EDGE_RENDERER: &str = "external.edge.data";
const NODE_SELECTOR: &str = "external-product-node";
const CONTROL_SELECTOR: &str = "external-product-control";
const BASE_PORT_SELECTOR: &str = "external-product-base-port";
const DYNAMIC_PORT_SELECTOR: &str = "external-product-dynamic-port";

struct ProductNodeState {
    title: String,
    mount_count: Cell<u64>,
    render_count: Cell<u64>,
    click_count: Cell<u64>,
    command_count: Cell<u64>,
    connected_state_seen: Cell<bool>,
}

struct ProductNode {
    binding: Entity<OpenGpuiNodeBinding>,
    state: Rc<ProductNodeState>,
    command_task: Option<Task<()>>,
}

impl Render for ProductNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.state
            .render_count
            .set(self.state.render_count.get().saturating_add(1));
        let binding = self.binding.read(cx);
        let authoring = binding
            .authoring()
            .expect("retained node opens one coherent authoring view");
        let base = authoring
            .port(base_port_id())
            .expect("catalog-created real port belongs to the retained node")
            .handle()
            .clone();
        let dynamic = authoring
            .port(dynamic_port_id())
            .map(|port| port.handle().clone());
        if base.presentation().connected() {
            self.state.connected_state_seen.set(true);
        }
        let content_height = if dynamic.is_some() { 144.0 } else { 108.0 };
        let command_binding = self.binding.clone();
        let product_node = cx.weak_entity();
        let clicks = Rc::clone(&self.state);
        let drag_header = binding.drag_handle().wrap(
            div()
                .absolute()
                .left(px(16.0))
                .top(px(4.0))
                .w(px(156.0))
                .h(px(8.0)),
        );

        let mut root = div()
            .debug_selector(|| NODE_SELECTOR.to_owned())
            .relative()
            .w_full()
            .h(px(content_height))
            .child(drag_header)
            .child(
                div().absolute().left(px(16.0)).top(px(16.0)).child(
                    div()
                        .debug_selector(|| CONTROL_SELECTOR.to_owned())
                        .w(px(156.0))
                        .h(px(28.0))
                        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                            clicks
                                .click_count
                                .set(clicks.click_count.get().saturating_add(1));
                            let (authoring, task_activity) = {
                                let binding = command_binding.read(cx);
                                (
                                    binding
                                        .authoring()
                                        .expect("command callback opens a current authoring view"),
                                    binding
                                        .activity()
                                        .acquire(OpenGpuiActivityKind::Task)
                                        .expect("retained command task acquires an activity lease"),
                                )
                            };
                            let base = authoring
                                .port(base_port_id())
                                .expect("base port remains in the authoring view")
                                .facts()
                                .clone();
                            let mut dynamic = base.clone();
                            dynamic.key = PortKey::new("dynamic-result");
                            let command = GraphCommand::topology(
                                authoring.revision(),
                                TopologyEdit::new().replace_node_ports(
                                    node_id(),
                                    [
                                        TopologyPort::new(base_port_id(), base),
                                        TopologyPort::new(dynamic_port_id(), dynamic),
                                    ],
                                    IncidentEdgePolicy::Reject,
                                ),
                            )
                            .with_label("add external dynamic port");
                            let ticket = authoring
                                .enqueue(command, cx)
                                .expect("native command enters the host feedback journal");
                            let command_state = Rc::clone(&clicks);
                            let completion_node = product_node.clone();
                            let invalidate_binding = command_binding.clone();
                            let task = cx.spawn(async move |cx| {
                                let _task_activity = task_activity;
                                let applied = ticket
                                    .await
                                    .is_ok_and(|outcome| outcome.applied().is_some());
                                if applied {
                                    let _ = invalidate_binding
                                        .update(cx, |binding, cx| binding.invalidate_layout(cx));
                                }
                                let _ = completion_node.update(cx, |node, cx| {
                                    node.command_task = None;
                                    if applied {
                                        command_state.command_count.set(
                                            command_state.command_count.get().saturating_add(1),
                                        );
                                    }
                                    cx.notify();
                                });
                            });
                            let _ = product_node.update(cx, |node, _| {
                                node.command_task = Some(task);
                            });
                        })
                        .child(self.state.title.clone()),
                ),
            )
            .child(
                div()
                    .absolute()
                    .right(px(0.0))
                    .top(px(72.0))
                    .child(custom_port_marker(base, BASE_PORT_SELECTOR)),
            );
        if let Some(dynamic) = dynamic {
            root = root.child(
                div()
                    .absolute()
                    .right(px(0.0))
                    .top(px(108.0))
                    .child(custom_port_marker(dynamic, DYNAMIC_PORT_SELECTOR)),
            );
        }
        binding.measure_content(root)
    }
}

fn custom_port_marker(handle: OpenGpuiPortHandle, selector: &'static str) -> AnyElement {
    let presentation = handle.presentation();
    let fill = if presentation.candidate().is_valid_target() {
        rgb(0x4dd6a2)
    } else if presentation.candidate().is_invalid_target() {
        rgb(0xff716a)
    } else if presentation.connected() {
        rgb(0x67a6ff)
    } else if presentation.hovered() {
        rgb(0xe7edf6)
    } else {
        rgb(0x66717f)
    };
    handle
        .with_minimum_hit_diameter(24.0)
        .interactive(
            div()
                .debug_selector(move || selector.to_owned())
                .w(px(14.0))
                .h(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.0))
                .border_2()
                .border_color(fill)
                .child(div().w(px(4.0)).h(px(4.0)).rounded_full().bg(fill)),
        )
        .into_any_element()
}

fn external_graph_theme() -> OpenGpuiGraphTheme {
    let mut theme = OpenGpuiGraphTheme::default();
    theme.palette.selection = rgb(0x8fc7ff).into();
    theme.palette.edge_data = rgb(0x5ed0ae).into();
    theme.metrics.port_hit_diameter = 24.0;
    theme
}

fn external_edge_painters() -> OpenGpuiEdgePainterRegistry {
    let mut painters = OpenGpuiEdgePainterRegistry::builder();
    painters
        .register(
            EDGE_RENDERER,
            OpenGpuiEdgePainterRecipe::new(
                OpenGpuiEdgeStrokePattern::Dotted,
                OpenGpuiEdgeTone::Data,
                OpenGpuiEdgeMarkerShape::Circle,
                OpenGpuiEdgeMarkerShape::Diamond,
                OpenGpuiEdgeLabelAppearance::Badge,
            ),
        )
        .expect("external edge renderer registration");
    painters.build()
}

fn node_id() -> NodeId {
    NodeId::from_u128(20)
}

fn base_port_id() -> PortId {
    PortId::from_u128(21)
}

fn dynamic_port_id() -> PortId {
    PortId::from_u128(22)
}

fn sink_node_id() -> NodeId {
    NodeId::from_u128(30)
}

fn sink_port_id() -> PortId {
    PortId::from_u128(31)
}

fn edge_id() -> EdgeId {
    EdgeId::from_u128(40)
}

#[open_gpui::test]
fn external_retained_node_mounts_and_measures_real_ports(cx: &mut open_gpui::TestAppContext) {
    let kind = NodeKindKey::new(KIND);
    let node = node_id();
    let base_port = base_port_id();
    let dynamic_port = dynamic_port_id();
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(kind.clone(), "Dynamic workflow", RENDERER)
                .default_size(CanvasSize {
                    width: 260.0,
                    height: 180.0,
                })
                .port(NodePortSpec::new(
                    "base",
                    PortDirection::Out,
                    PortKind::Data,
                    PortCapacity::Multi,
                ))
                .build(),
        )
        .expect("register external dynamic node kind");
    let sink_kind = NodeKindKey::new(SINK_KIND);
    catalog
        .register(
            NodeKindSpec::builder(sink_kind.clone(), "Sink", SINK_RENDERER)
                .default_size(CanvasSize {
                    width: 160.0,
                    height: 96.0,
                })
                .port(NodePortSpec::new(
                    "input",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Single,
                ))
                .build(),
        )
        .expect("register external sink kind");

    let creation = catalog
        .instantiate_node_with_ids(&kind, node, CanvasPoint { x: 40.0, y: 48.0 }, [base_port])
        .expect("instantiate external dynamic node");
    let (created_id, created_node, created_ports) = creation.into_parts();
    let sink_creation = catalog
        .instantiate_node_with_ids(
            &sink_kind,
            sink_node_id(),
            CanvasPoint { x: 480.0, y: 48.0 },
            [sink_port_id()],
        )
        .expect("instantiate external sink node");
    let mut graph = GraphBuilder::new(GraphId::from_u128(4));
    graph.insert_node(created_id, created_node);
    for (port_id, port) in created_ports {
        graph.insert_port(port_id, port);
    }
    let (sink_id, sink_node, sink_ports) = sink_creation.into_parts();
    graph.insert_node(sink_id, sink_node);
    for (port_id, port) in sink_ports {
        graph.insert_port(port_id, port);
    }
    let mut edge = Edge::new(EdgeKind::Data, base_port, sink_port_id());
    edge.view = EdgeViewDescriptor::new()
        .with_renderer_key(EDGE_RENDERER)
        .with_label("payload")
        .with_style_token("data.primary");
    graph.insert_edge(edge_id(), edge);
    let runtime = GraphRuntime::builder(catalog)
        .build(graph.build().expect("external fixture graph must validate"))
        .expect("create external graph runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach external view");
    let weak_view = view.downgrade();

    let state = Rc::new(ProductNodeState {
        title: "Application-owned dynamic node".to_owned(),
        mount_count: Cell::new(0),
        render_count: Cell::new(0),
        click_count: Cell::new(0),
        command_count: Cell::new(0),
        connected_state_seen: Cell::new(false),
    });
    let factory_state = Rc::clone(&state);
    let mut registry = OpenGpuiNodeRegistry::builder();
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(108.0, 160.0).expect("external content-height policy"),
        ),
    );
    registry.register_with_policy(RENDERER, policy, move |binding, _window, cx| {
        let state = Rc::clone(&factory_state);
        state
            .mount_count
            .set(state.mount_count.get().saturating_add(1));
        cx.new(|_| ProductNode {
            binding,
            state,
            command_task: None,
        })
    });
    let registry = registry
        .seal()
        .expect("source renderer registrations must be valid");
    assert_eq!(
        registry.status(RENDERER),
        OpenGpuiRendererRegistrationStatus::Registered
    );

    let graph_theme = external_graph_theme();
    let expected_theme = graph_theme;
    let edge_painters = external_edge_painters();
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry,
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 1024.0,
                    height: 768.0,
                },
            },
        )
        .with_graph_theme(graph_theme)
        .with_edge_painter_registry(edge_painters)
    });
    draw(cx);
    draw(cx);

    assert_eq!(state.mount_count.get(), 1);
    assert!(state.render_count.get() > 0);
    assert!(state.connected_state_seen.get());
    assert!(cx.debug_bounds(NODE_SELECTOR).is_some());
    let control = cx
        .debug_bounds(CONTROL_SELECTOR)
        .expect("application-owned native control rendered");
    assert!(cx.debug_bounds(BASE_PORT_SELECTOR).is_some());
    assert!(cx.debug_bounds(DYNAMIC_PORT_SELECTOR).is_none());

    let geometry = weak_view
        .geometry_snapshot()
        .expect("external native geometry published");
    let node_geometry = geometry.node(node).expect("external node geometry");
    assert!(node_geometry.is_fresh());
    assert_eq!(node_geometry.port_outcomes().len(), 1);
    assert!(matches!(
        geometry.port_outcome(base_port),
        Some(PublishedPortOutcome::Rendered(_))
    ));
    assert!(geometry.route(edge_id()).is_some());
    let initial_size = runtime
        .snapshot()
        .expect("content-sized external graph")
        .graph()
        .node(&node)
        .and_then(|node| node.size)
        .expect("external node converged size");
    assert_eq!(initial_size.height, 108.0);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);

    cx.simulate_click(control.center(), Default::default());
    draw(cx);
    draw(cx);
    assert_eq!(state.click_count.get(), 1);
    assert_eq!(state.command_count.get(), 1);
    assert!(cx.debug_bounds(DYNAMIC_PORT_SELECTOR).is_some());

    let geometry = weak_view
        .geometry_snapshot()
        .expect("dynamic native geometry published");
    let node_geometry = geometry.node(node).expect("dynamic node geometry");
    assert!(node_geometry.is_fresh());
    assert_eq!(node_geometry.port_outcomes().len(), 2);
    for port in [base_port, dynamic_port] {
        assert!(matches!(
            geometry.port_outcome(port),
            Some(PublishedPortOutcome::Rendered(_))
        ));
        let layout = geometry.port(port).expect("real port geometry");
        assert_eq!(layout.node(), node);
        assert!(layout.visual_bounds().size.is_positive_finite());
    }
    let dynamic_size = runtime
        .snapshot()
        .expect("dynamic external graph")
        .graph()
        .node(&node)
        .and_then(|node| node.size)
        .expect("dynamic external node size");
    assert_eq!(dynamic_size.height, 144.0);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().mounts(), 2);
        assert_eq!(host.stats().active_nodes(), 2);
        assert!(host.stats().feedback_submissions() >= 1);
        assert_eq!(host.stats().feedback_failures(), 0);
        assert_eq!(host.graph_theme(), expected_theme);
        assert_eq!(host.edge_painter_registry().len(), 1);
    });
    assert_eq!(
        runtime
            .snapshot()
            .expect("runtime snapshot after native render")
            .graph()
            .node(&node)
            .expect("dynamic node remains in runtime")
            .ports,
        vec![base_port, dynamic_port]
    );
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn main() {}
